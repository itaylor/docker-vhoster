use serde::Deserialize;
use bollard::{Docker, system::VersionComponents, models::SystemVersionPlatform};
use core::slice::Iter;
use std::collections::HashMap;
use std::future;
use bollard::system::EventsOptions;
use chrono::Utc;
use chrono::Duration;
use futures_util::TryStreamExt;


#[tokio::main]
async fn main() {
    let config = envy::from_env::<Config>().expect("Expected config");
    let host_file_location = config.host_file_location;
    let env_var_name: String = config.env_var_name;
    let vhost_ip_addr: String = config.vhost_ip_addr;
    println!("Starting with options: {}, {}, {}", host_file_location, env_var_name, vhost_ip_addr);

    #[cfg(unix)]
    let docker = Docker::connect_with_socket_defaults().unwrap();
    let version = docker.version().await.unwrap();
    let missing_str = "<Unknown>";
    let ver_name = get_platform_name(version.platform, missing_str);
    let engine_ver = get_engine_ver(version.components, missing_str);

    println!("Connected to {}, {}", ver_name, engine_ver);
    
    let mut filters = HashMap::new();
    let mut events = Vec::new();
    events.push("start".to_string());
    events.push("stop".to_string());
    events.push("die".to_string());

    let mut types = Vec::new();
    types.push("container".to_string());
    filters.insert("event".to_string(), events);
    filters.insert("type".to_string(), types);
    let event_stream = docker.events(Some(EventsOptions::<String> {
        since: Some(Utc::now() - Duration::minutes(1)),
        until: None,
        filters,
    }));
    println!("Waiting for events...");
    let fut =  event_stream.try_for_each(|x| { 
        println!("Got event! {:?}", x);
        return future::ready(Ok(()));
    });
    match fut.await {
        Err(e) => println!("error: {:?}", e),
        Ok(e)  => println!("OK! {:?}", e)
    };
    // event_stream..then(|i| println!("{:?}", i);
}

/// provides default value for zoom if ZOOM env var is not set
fn default_host_file_location() -> String {
    return String::from("/etc/hosts"); 
}

fn default_env_var_name() -> String {
    return String::from("VIRTUAL_HOSTS");
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

#[derive(Deserialize, Debug)]
struct Config {
  #[serde(default="default_host_file_location")]
  host_file_location: String,
  #[serde(default="default_env_var_name")]
  env_var_name: String,
  #[serde(default="default_vhost_ip_addr")]
  vhost_ip_addr: String  
}