
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct BsProxyConfig {
    proxy_ip: String,
    web_ip: String,
    proxy_port: u16,
    web_port: u16
}

impl ::std::default::Default for BsProxyConfig {
    fn default() -> Self {
        Self {
            proxy_ip: "0.0.0.0".parse().unwrap(),
            web_ip: "0.0.0.0".parse().unwrap(),
            proxy_port: 2375,
            web_port: 2376
        }
    }
}
