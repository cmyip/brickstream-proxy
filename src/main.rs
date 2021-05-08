#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_contrib;

mod bs_proxy_config;
mod camera_listener;
mod connection_manager;
mod web_api;
mod config;


use crate::camera_listener::CameraListener;
use crate::connection_manager::{ConnectionManager, MANAGER};
use std::thread;
use std::time::Duration;
use std::net::{IpAddr};
use std::sync::{Arc, Mutex};
use web_api::WebApi;
use dotenv::dotenv;


fn main() {
    dotenv().ok();
    let port_number = config::get_camera_port();
    let camera_ip = config::get_camera_ip();
    let mut camera_listener = CameraListener::new(IpAddr::from(camera_ip), port_number);
    let connection_manager = ConnectionManager::new();
    println!("Camera server listening on port {}:{}", camera_ip, port_number);

    let web_api = WebApi::new();

    let sender = connection_manager.sender.clone();
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
