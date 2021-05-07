use rocket::config::Environment;
use rocket::Config;
use std::env;
use rocket_cors::{AllowedHeaders, AllowedOrigins, Error, Cors};
use rocket::http::Method;
use std::net::Ipv4Addr;

pub fn get_port_start() -> u16 {
    env::var("BROWSER_PORT_START")
        .unwrap_or_else(|_| "10030".to_string())
        .parse::<u16>()
        .expect("BROWSER_PORT_START environment variable should parse to an integer")
}

pub fn get_port_end() -> u16 {
    env::var("BROWSER_PORT_END")
        .unwrap_or_else(|_| "10030".to_string())
        .parse::<u16>()
        .expect("BROWSER_PORT_START environment variable should parse to an integer")
}

pub fn get_camera_ip() -> Ipv4Addr {
    let octets: Vec<u8> = env::var("CAMERA_IP").unwrap_or_else(|_| "0.0.0.0".to_string())
        .split(".").map(|octet| octet.parse::<u8>().unwrap_or_else(|_| 0)).collect();
    return Ipv4Addr::from([octets[0], octets[1], octets[2], octets[3]]);
}

pub fn get_camera_port() -> u16 {
    env::var("CAMERA_PORT")
        .unwrap_or_else(|_| "2375".to_string())
        .parse::<u16>()
        .expect("BROWSER_PORT_START environment variable should parse to an integer")
}

pub fn get_cors() -> rocket_cors::Cors {
    let cors_origins = env::var("CORS_HOSTS").unwrap_or_else(|_| "http://localhost:4200".to_string());
    let cors_origins: Vec<&str> = cors_origins.split(",").map(|cors_light| cors_light.trim()).collect();
    let allowed_origins = AllowedOrigins::some_exact(&cors_origins);

    let cors_result = rocket_cors::CorsOptions {
        allowed_origins,
        allowed_methods: vec![Method::Get, Method::Options, Method::Post, Method::Delete, Method::Put].into_iter().map(From::from).collect(),
        allowed_headers: AllowedHeaders::some(&["Authorization", "Accept", "Content-Type"]),
        allow_credentials: true,
        fairing_route_base: "/api".parse().unwrap(),
        ..Default::default()
    }.to_cors().expect("Cannot build cors");

    return cors_result;
}

pub fn from_env() -> Config {
    let environment = Environment::active().expect("No environment found");

    let port = env::var("API_PORT")
        .unwrap_or_else(|_| "8000".to_string())
        .parse::<u16>()
        .expect("PORT environment variable should parse to an integer");

    let api_ip = env::var("API_IP").unwrap_or_else(|_| "0.0.0.0".to_string());


    Config::build(environment)
        .environment(environment)
        .port(port)
        .address(api_ip)
        .finalize()
        .unwrap()
}
