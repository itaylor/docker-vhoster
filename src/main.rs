use anyhow::bail;
use anyhow::{Context as _Ctx, Result};
use bollard::{Docker, system::VersionComponents, models::SystemVersionPlatform, container::ListContainersOptions};
use core::slice::Iter;
use std::io::Error;
use std::process;
use bollard::system::EventsOptions;
use futures_util::TryStreamExt;
use std::collections::{HashMap};
use std::sync::{Arc, Mutex};
use std::fs::{self, OpenOptions};
use std::io::{prelude::*};
use ctrlc;
use futures_retry::{RetryPolicy, FutureRetry};
use clap::Parser;
use std::path::Path;
use futures::future;


macro_rules! collection {
    // map-like
    ($($k:expr => $v:expr),* $(,)?) => {{
        core::convert::From::from([$(($k, $v),)*])
    }};
    // set-like
    ($($v:expr),* $(,)?) => {{
        core::convert::From::from([$($v,)*])
    }};
}
const DOCKER_SOCK_LOCATION: &str = "/var/run/docker.sock";

#[tokio::main]
async fn main() -> Result<()> {
    ctrlc::set_handler( || {
       eprintln!("Caught signal, exiting...");
       process::exit(130); // same as bash sigint exit code
    }).with_context(|| "Unable to add signal handler")?;

    let config = Config::parse();
    let curr_containers: HashMap<String, ContainerInfo> = HashMap::new();
    let container_mutex = Arc::new(Mutex::new(curr_containers));
    let writelock_mutex = Arc::new(Mutex::new(false));
    let host_file_location = config.host_file_location;
    let env_var_name: String = config.env_var_name;
    let vhost_ip_addr: String = config.vhost_ip_addr;
    let context = Context {
        env_var_name,
        container_mutex,
        writelock_mutex,
        vhost_ip_addr,
        host_file_location,
    };

    println!("Starting with options: host_file_location:{} env_var_name:{} vhost_ip_addr: {}", context.host_file_location, context.env_var_name, context.vhost_ip_addr);

    check_vhost_file_access(&context.host_file_location).with_context(|| get_file_perms_text(&context.host_file_location))?;
    check_docker_sock()?;

    #[cfg(unix)]
    let docker = Docker::connect_with_socket_defaults().unwrap();
    let retryable = FutureRetry::new(
        || {
            println!("Attempting to connect to docker...");
            docker.version()
        },
     |e| handle_connection_error(e));
    let version = retryable.await.unwrap().0;
    let missing_str = "<Unknown>";
    let ver_name = get_platform_name(version.platform, missing_str);
    let engine_ver = get_engine_ver(version.components, missing_str);

    println!("Connected to {}, {}", ver_name, engine_ver);

    println!("Fetching initial container list");
    handle_containers_change(&docker, &context).await?;
    let filters = build_event_filters();
    let mut event_stream = docker.events(Some(EventsOptions::<String> {
        since: None,
        until: None,
        filters,
    }));
    println!("Waiting for events...");
    while let Some(x) = event_stream.try_next().await.unwrap() {
        let action = x.action.unwrap();
        let id = x.actor.unwrap().id.unwrap();
        if action.eq("die") || action.eq("stop") {
            handle_container_stop(id, &context);
        } else if action.eq("start") {
            handle_container_start(&id, &docker, &context).await?;
        }
    }
    println!("Docker connection terminated, exiting...");
    Ok(())
}

fn get_file_perms_text(host_file_location: &String) -> String {
    format!("File permssions to hosts file at `{}` must be set to allow your user to modify them.

Make sure of the following:
  1. The file {} exists (you will need to provide it as volume mount if you're using docker)
  2. You are running as a user who has access to the file
  3. On Mac and Windows, the `etc/hosts` file is protected by ACLs.  You will need to set an ACL setting to allow your user to modify the the file.
     On Mac this can be done by running this command:
     `sudo chmod +a \"user:$(whoami) allow read,write,append,readattr,writeattr,readextattr,writeextattr,readsecurity\" /etc/hosts`
     See the readme.md for windows directions

If you wish to use a different path, set the `HOST_FILE_LOCATION` env var or pass the `-h` argument.
", host_file_location, host_file_location)
}

fn handle_container_stop(id: String, context: &Context) {
    println!("Container stop! {}", id);
    let mut guard = context.container_mutex.lock().unwrap();
    guard.remove(&id);
    drop(guard);
    let result = update_vhosts(context);
    match result {
        Err(e) => println!("Error updating vhosts file {}", e),
        _ => ()
    }
}

fn format_vhosts(context: &Context) -> String {
    let guard = context.container_mutex
        .lock()
        .unwrap();
    let result = guard
        .iter()
        .map(|x| format_vhost_entry(&context.vhost_ip_addr, x.1))
        .collect::<Vec<String>>()
        .join("\n");
    drop(guard);    
    return result;
}
fn handle_connection_error(_e: bollard::errors::Error) -> RetryPolicy<bollard::errors::Error> {
    RetryPolicy::WaitRetry(std::time::Duration::from_secs(60))
}

fn format_vhost_entry(ip: &String, ci: &ContainerInfo) -> String {
    let parts= ci.vhosts.iter()
        .map(|s| format!("{} {}", ip, s))
        .collect::<Vec<String>>()
        .join("\n");
    parts
}

fn update_vhosts(context: &Context) -> Result<(), std::io::Error> {
    // TODO: is a Mutex the "right" Rusty way to synchronize a function?  
    let mut lock = context.writelock_mutex.lock().unwrap();
    println!("Updating vhost file {}", &context.host_file_location);
    const PREFIX: &str = "# docker-vhoster managed block\n";
    const SUFFIX: &str = "# docker-vhoster block end\n";
    let mut contents = fs::read_to_string(&context.host_file_location)?;
    // println!("READ hosts file: \n{}", contents);
    let start = contents.find(PREFIX);
    let end = contents.find(SUFFIX);
    let formatted = format_vhosts(context);
    let curr_text = format!("{}{}\n{}", PREFIX, formatted, SUFFIX);
    let new_content: String = match (start, end) {
        (Some(s), Some(e)) => {
            let last = e + SUFFIX.len();
            contents.replace_range(s..last, curr_text.as_str());
            contents
        },
        _ => contents + "\n" + curr_text.as_str(),
    };
    let mut file = OpenOptions::new().write(true).truncate(true).open(&context.host_file_location)?;
    file.write(new_content.as_bytes())?;
    file.sync_all()?;
    // println!("WROTE hosts file: \n{}", new_content);
    println!("Wrote vhost content: \n{}", curr_text);
    *lock = false;
    return Ok(());
}

// Checks access to the vhost file by attempting to read and write from it.
fn check_vhost_file_access(host_file_location: &String) -> Result<()> {
    let contents = fs::read_to_string(host_file_location)
        .with_context(|| format!("Unable to open {}",  host_file_location))?;
    let mut file = OpenOptions::new().write(true).truncate(true).open(host_file_location)?;
    file.write(contents.as_bytes())
        .with_context(|| format!("Unable to write to file {}", host_file_location))?;
    file.sync_all()?;
    Ok(())
}

// Checks that /var/run/docker.sock file exists 
fn check_docker_sock() -> Result<()> {
    if !Path::new(DOCKER_SOCK_LOCATION).exists() {
        bail!("Unable to find the unix socket `{}`.  You are probably missing the volume mount for it.", DOCKER_SOCK_LOCATION);
    }
    Ok(())
}

async fn handle_containers_change(docker: &Docker, context: &Context) -> Result<(),Error> {
    let options = Some(ListContainersOptions::<String>{
        ..Default::default()
    });
    let containers = docker.list_containers(options).await.unwrap(); 
    let futures = containers.iter().map(|c| get_vhosts_for_container(c.id.as_ref().unwrap(), docker, context)); 
    future::try_join_all(futures).await?;
    update_vhosts(context)?;
    return Ok(());
}

async fn get_vhosts_for_container(id: &String, docker: &Docker, context: &Context) -> Result<(), std::io::Error> {
    let vhosts = get_vhosts_from_docker(id.to_string(), context.env_var_name.to_string(), docker).await;

    let mut guard = context.container_mutex.lock().unwrap();
    let cinfo = ContainerInfo {
       id: id.to_string(),
       vhosts 
    };
    guard.insert(id.to_string(), cinfo);
    drop(guard);
    return Ok(());
}

async fn handle_container_start(id: &String, docker: &Docker, context: &Context) -> Result<(), std::io::Error> {
    println!("Container start! {}", id);
    handle_containers_change(docker, context).await?;
    return Ok(());
}

fn container_config_to_vhost_names(vn: String, iter: Iter<String>) -> Vec<String> {
    iter.filter(|&x| x.starts_with(&vn))
    .flat_map(|x| x.replace(&vn, "")
        .split(",")
        .map(|v|String::from(v))
        .collect::<Vec<String>>()
    ).collect()
}

async fn get_vhosts_from_docker(id: String, env_var_name: String, docker: &Docker) -> Vec<String> {
    let fut = docker.inspect_container(&id, None).await;
    let result = match fut {
        Ok(r) => r,
        Err(_) => return Vec::<String>::new()
    };
    let name = result.name.unwrap();
    let var_names = env_var_name.as_str().split(",").map(|s| format!("{}=", s));
    let env = result.config.unwrap().env.unwrap();
    let vhosts: Vec<String> = var_names.flat_map(|vn| {
        return container_config_to_vhost_names(vn, env.iter());
    }).collect();
    
    match vhosts.len() {
        0 => vec!(format!("{}.local", name.replace("/", ""))),
        _ => vhosts,
    }
}

fn get_platform_name(platform: Option<SystemVersionPlatform>, missing_str: &str) -> String {
    return match platform {
        Some(platform) => platform.name,
        None => String::from(missing_str)
    }
}

fn get_engine_ver(components:Option<Vec<VersionComponents>>, missing_str: &str) -> String {
    return match components {
        Some(comps) => find_engine_ver(comps.iter(), missing_str),
        None => String::from(missing_str)
    };
}

fn find_engine_ver(mut comps: Iter<VersionComponents>, missing_str: &str) -> String {
    let maybe_engine = comps.find(|&x| x.name.eq("Engine"));
    return match maybe_engine {
        Some(engine) => engine.version.clone(),
        None => String::from(missing_str)
    }
}

fn build_event_filters() -> HashMap<String, Vec<String>> {
    let events = collection!(
        String::from("start"),
        String::from("stop"),
        String::from("die")
    );
    let types= collection!(String::from("container"));
    let filters = collection!(
        String::from("event") => events,
        String::from("type") => types
    );
    return filters;
}
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Config {
  #[clap(short, long, env, default_value="/etc/hosts")]
  host_file_location: String,
  #[clap(short, long, env, default_value="VIRTUAL_HOST,ETC_HOST")]
  env_var_name: String,
  #[clap(short, long, env, default_value="127.0.0.1")]
  vhost_ip_addr: String  
}

#[derive(Debug)]
struct ContainerInfo {
    #[allow(dead_code)] // Really hard to debug without the id there...
    id: String,
    vhosts: Vec<String>,
}

#[derive(Debug)]
struct Context {
    container_mutex: Arc<Mutex<HashMap<String, ContainerInfo>>>,
    writelock_mutex: Arc<Mutex<bool>>,
    host_file_location: String,
    env_var_name: String,
    vhost_ip_addr: String,   
}