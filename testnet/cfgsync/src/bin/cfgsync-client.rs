// std
use std::{env, fs, net::Ipv4Addr, process};
// crates
use nomos_executor::config::Config as ExecutorConfig;
use nomos_node::Config as ValidatorConfig;
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};
// internal

#[derive(Serialize)]
struct ClientIp {
    ip: Ipv4Addr,
}

fn parse_ip(ip_str: String) -> Ipv4Addr {
    ip_str.parse().unwrap_or_else(|_| {
        eprintln!("Invalid IP format, defaulting to 127.0.0.1");
        Ipv4Addr::new(127, 0, 0, 1)
    })
}

async fn get_config<Config: Serialize + DeserializeOwned>(
    ip: Ipv4Addr,
    url: &str,
    config_file: &str,
) -> Result<(), String> {
    let client = Client::new();

    let response = client
        .post(url)
        .json(&ClientIp { ip })
        .send()
        .await
        .map_err(|err| format!("Failed to send IP announcement: {}", err))?;

    if !response.status().is_success() {
        return Err(format!("Server error: {:?}", response.status()));
    }

    let config = response
        .json::<Config>()
        .await
        .map_err(|err| format!("Failed to parse response: {}", err))?;

    let yaml = serde_yaml::to_string(&config)
        .map_err(|err| format!("Failed to serialize config to YAML: {}", err))?;

    fs::write(config_file, yaml)
        .map_err(|err| format!("Failed to write config to file: {}", err))?;

    println!("Config saved to {}", config_file);
    Ok(())
}

#[tokio::main]
async fn main() {
    let config_file_path = env::var("CFG_FILE_PATH").unwrap_or("config.yaml".to_string());
    let server_addr = env::var("CFG_SERVER_ADDR").unwrap_or("http://127.0.0.1:4400".to_string());
    let ip = parse_ip(env::var("CFG_HOST_IP").unwrap_or_else(|_| "127.0.0.1".to_string()));

    let host_kind = env::var("CFG_HOST_KIND").unwrap_or_else(|_| "validator".to_string());

    let node_config_endpoint = match host_kind.as_str() {
        "executor" => format!("{}/executor", server_addr),
        _ => format!("{}/validator", server_addr),
    };

    let config_result = match host_kind.as_str() {
        "executor" => {
            get_config::<ExecutorConfig>(ip, &node_config_endpoint, &config_file_path).await
        }
        _ => get_config::<ValidatorConfig>(ip, &node_config_endpoint, &config_file_path).await,
    };

    // Handle error if the config request fails
    if let Err(err) = config_result {
        eprintln!("Error: {}", err);
        process::exit(1);
    }
}
