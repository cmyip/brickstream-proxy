use crate::config;
use crate::connection_manager::{MANAGER, BsCamera, ProxySessionDto, ConnectionManager};
use rocket_contrib::json::{Json, JsonValue};
use serde::{Serialize, Deserialize};
use rocket::response::status;
use std::sync::{PoisonError, MutexGuard};


pub struct WebApi {
}

#[derive(Deserialize)]
pub struct ConnectionRequest {
    mac_address: String
}

#[derive(Serialize)]
pub struct ConnectionResponse {
    port_number: u16
}

#[get("/cameras")]
fn get_cameras() -> JsonValue {
    unsafe {
        match &MANAGER {
            None => {}
            Some(manager) => {
                let manager = manager.clone();
                let connections = manager.get_cameras_available();
                println!("Found {} connections", connections.len());
                return json!(connections);
                /*let manager_lock = manager.lock();
                match manager_lock {
                    Ok(manager) => {
                        let connections = manager.get_cameras_available();
                        println!("Found {} connections", connections.len());
                        return json!(connections);
                    }
                    Err(_) => {
                        println!("Failed to perform manager_lock");
                        return json!([]);
                    }
                }*/

            }
        }
    }
    let empty_array : Vec<BsCamera> = vec![];
    return json!(empty_array);
}

/*#[get("/cameras/connection")]
fn get_camera_connections() -> JsonValue {
    unsafe {
        match &MANAGER {
            None => {}
            Some(manager) => {
                let manager = manager.clone();
                let manager = manager.lock().unwrap();
                let connections = manager.get_connections();
                return json!(connections);
            }
        }
    }
    let empty_array : Vec<ProxySessionDto> = vec![];
    return json!(empty_array);
}*/

#[post("/cameras/connection", format = "json", data = "<request>")]
fn post_cameras_connection(request: Json<ConnectionRequest>) -> Result<JsonValue, status::BadRequest<String>>  {
    let mut response = ConnectionResponse {
        port_number: 0
    };
    unsafe {
        match &MANAGER {
            None => {
                return Result::Err(status::BadRequest(Some("Manager is not here".parse().unwrap())));
            }
            Some(manager) => {
                /*let manager = manager.clone();
                let manager = manager.lock().unwrap();*/
                let manager = manager.clone();
                let stream = manager.get_stream_by_mac_id((*request.mac_address).to_string());
                match stream {
                    None => {
                        return Result::Err(status::BadRequest(Some("MAC address does not exist".parse().unwrap())))
                    }
                    Some(stream) => {
                        response.port_number = stream.port_number;
                        return Result::Ok(json!(response));
                    }
                }
            }
        }
    }
}

#[delete("/cameras/connection/<port>")]
pub fn delete_connection_port(port: u16) -> Result<JsonValue, status::BadRequest<String>> {
    unsafe {
        match &MANAGER {
            None => {
                return Result::Err(status::BadRequest(Some("Manager is not here".parse().unwrap())));
            }
            Some(manager) => {
                let manager = manager.clone();
                // let manager = manager.lock().unwrap();
                manager.close_stream_by_port_num(port);
                return Result::Ok(json!(()));
            }
        }
    }
}

impl WebApi {
    pub fn new() -> WebApi {
        WebApi {}
    }


    pub fn run(&self) {
        let cors = config::get_cors();
        let rocket = rocket::custom(config::from_env())
            .attach(cors)
            .mount(
                "/api",
                routes![
                                get_cameras,
                                /*get_camera_connections,*/
                                post_cameras_connection,
                                delete_connection_port
                            ],
            );
        rocket.launch();
    }
}
