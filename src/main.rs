use serde::Deserialize;
use bollard::{Docker, system::VersionComponents, models::SystemVersionPlatform};
use core::slice::Iter;
use std::process;
use bollard::system::EventsOptions;
use chrono::Utc;
use chrono::Duration;
use futures_util::TryStreamExt;
use std::collections::{HashMap};
use std::sync::{Arc, Mutex};
use std::fs::{self, OpenOptions};
use std::io::{prelude::*};
use ctrlc;
use futures_retry::{RetryPolicy, FutureRetry};

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

#[tokio::main]
async fn main() {
    ctrlc::set_handler( || {
       eprintln!("Caught signal, exiting...");
       process::exit(130); // same as bash sigint exit code
    }).expect("Couldn't add signal handler");

    let config = envy::from_env::<Config>().expect("Expected config");
    let curr_containers: HashMap<String, ContainerInfo> = HashMap::new();
    let container_mutex = Arc::new(Mutex::new(curr_containers));
    let host_file_location = config.host_file_location;
    let env_var_name: String = config.env_var_name;
    let vhost_ip_addr: String = config.vhost_ip_addr;
    let context = Context {
        env_var_name,
        container_mutex,
        vhost_ip_addr,
        host_file_location,
    };
    println!("Starting with options: host_file_location:{} env_var_name:{} vhost_ip_addr: {}", context.host_file_location, context.env_var_name, context.vhost_ip_addr);

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
    let filters = build_event_filters();
    let mut event_stream = docker.events(Some(EventsOptions::<String> {
        since: Some(Utc::now() - Duration::minutes(1)),
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
            handle_container_start(&id, &docker, &context).await;
        }
    }
    println!("Docker connection terminated, exiting...")
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
        .fold(String::new(), |a, b| format!("{}{}", a, b));
    drop(guard);    
    return result;
}
fn handle_connection_error(_e: bollard::errors::Error) -> RetryPolicy<bollard::errors::Error> {
    RetryPolicy::WaitRetry(Duration::minutes(1).to_std().unwrap())
}

fn format_vhost_entry(ip: &String, ci: &ContainerInfo) -> String {
    ci.vhosts.iter().fold(String::new(),
     |a, b| format!("{} {}", ip, a) + b + "\n")
}

fn update_vhosts(context: &Context) -> Result<(), std::io::Error> {
    println!("Updating vhost file {}", &context.host_file_location);
    const PREFIX: &str = "# docker-vhoster managed block\n";
    const SUFFIX: &str = "# docker-vhoster block end\n";
    let mut contents = fs::read_to_string(&context.host_file_location)?;
    let start = contents.find(PREFIX);
    let end = contents.find(SUFFIX);
    let formatted = format_vhosts(context);
    let curr_text = format!("{}{}{}", PREFIX, formatted, SUFFIX);
    let new_content: String = match (start, end) {
        (Some(s), Some(e)) => {
            let last = e + SUFFIX.len();
            contents.replace_range(s..last, curr_text.as_str());
            contents
        },
        _ => contents + "\n" + curr_text.as_str(),
    };
    let mut file = OpenOptions::new().write(true).open(&context.host_file_location)?;
    file.write(new_content.as_bytes())?;
    println!("Wrote vhost content: \n{}", curr_text);
    return Ok(());
}

async fn handle_container_start(id: &String, docker: &Docker, context: &Context) {
    println!("Container start! {}", id);

    let vhosts_str = get_vhosts_from_docker(id.to_string(), context.env_var_name.to_string(), docker).await;
    let vhosts = split_vhosts_to_vec(vhosts_str);

    let mut guard = context.container_mutex.lock().unwrap();
    let cinfo = ContainerInfo {
       id: id.to_string(),
       vhosts 
    };
    guard.insert(id.to_string(), cinfo);
    drop(guard);
    println!("{:?}", context);
    let result = update_vhosts(context);
    match result {
        Err(e) => println!("Error updating vhosts file {}", e),
        _ => ()
    }
}

fn split_vhosts_to_vec(vhosts: String) -> Vec<String>{
   let vec: Vec<String> = vhosts.split(",").map(|v|String::from(v)).collect();
   return vec;
}

async fn get_vhosts_from_docker(id: String, env_var_name: String, docker: &Docker) -> String {
    let fut = docker.inspect_container(&id, None);
    let result = fut.await.unwrap();
    // println!("Got vhost from docker {:?}", result);
    let name = result.name.unwrap();
    let env_var_with_eq = format!("{}=", env_var_name);
    let vhosts = result.config
        .unwrap().env.unwrap()
        .iter()
        .find(|&x|x.starts_with(&env_var_with_eq))
        .and_then(|s| Some(s.clone()));
    return match vhosts {
        Some(s) => String::from(s).replace(&env_var_with_eq, ""),
        None => format!("{}.local", name.replace("/", ""))
    }
}

/// provides default value for zoom if ZOOM env var is not set
fn default_host_file_location() -> String {
    return String::from("/etc/hosts"); 
}

fn default_env_var_name() -> String {
    return String::from("VIRTUAL_HOST");
}

fn default_vhost_ip_addr() -> String {
    return String::from("127.0.0.1");
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
#[derive(Deserialize, Debug)]
struct Config {
  #[serde(default="default_host_file_location")]
  host_file_location: String,
  #[serde(default="default_env_var_name")]
  env_var_name: String,
  #[serde(default="default_vhost_ip_addr")]
  vhost_ip_addr: String  
}

#[derive(Debug)]
struct ContainerInfo {
    id: String,
    vhosts: Vec<String>,
}

#[derive(Debug)]
struct Context {
    container_mutex: Arc<Mutex<HashMap<String, ContainerInfo>>>,
    host_file_location: String,
    env_var_name: String,
    vhost_ip_addr: String,   
}