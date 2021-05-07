#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

mod proxy_server;
mod bs_proxy_config;
mod camera_listener;
mod connection_manager;
mod web_api;
mod config;


use crate::camera_listener::CameraListener;
use crate::connection_manager::{ConnectionManager, MANAGER};
use std::thread;
use std::time::Duration;
use std::net::{SocketAddrV4, Ipv4Addr, IpAddr};
use std::sync::{Arc, Mutex};
use web_api::WebApi;


fn main() {
    // proxy_server::start();
    let mut camera_listener = CameraListener::new(IpAddr::from(Ipv4Addr::new(0, 0, 0, 0)), 2395);
    let connection_manager = ConnectionManager::new();

    let mut web_api = WebApi::new();

    let mut sender = connection_manager.sender.clone();
    camera_listener.set_sender(sender);

    unsafe {
        let connection_manager = connection_manager.clone();
        MANAGER = Option::from(Arc::from(Mutex::from(connection_manager)));
    }

    thread::spawn( move || loop {
        connection_manager.process();
        thread::sleep(Duration::from_millis(200));
    });

    thread::spawn(move || loop {
        camera_listener.process();
    });
    web_api.run();



}
