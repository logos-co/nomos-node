// std
use std::net::Ipv4Addr;
// crates
use reqwest::Client;
use serde::de::DeserializeOwned;
// internal
use crate::server::ClientIp;

pub async fn get_config<Config: DeserializeOwned>(
    ip: Ipv4Addr,
    identifier: String,
    url: &str,
) -> Result<Config, String> {
    let client = Client::new();

    let response = client
        .post(url)
        .json(&ClientIp { ip, identifier })
        .send()
        .await
        .map_err(|err| format!("Failed to send IP announcement: {}", err))?;

    if !response.status().is_success() {
        return Err(format!("Server error: {:?}", response.status()));
    }

    response
        .json::<Config>()
        .await
        .map_err(|err| format!("Failed to parse response: {}", err))
}
