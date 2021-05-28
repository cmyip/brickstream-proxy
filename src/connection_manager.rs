use std::sync::mpsc::{Sender, Receiver, channel};
use std::net::{TcpStream, TcpListener, SocketAddrV4, Ipv4Addr};
use std::sync::{Arc, Mutex, RwLock};
use std::collections::HashMap;
use std::io::{Write, Read};
use regex::RegexBuilder;
use std::clone::Clone;
use serde::Serialize;
use rand::prelude::*;
use std::thread::{JoinHandle};
use std::thread;
use crate::config;
use std::time::Duration;
use std::borrow::BorrowMut;
use std::ops::DerefMut;

pub static mut SENDER: Option<Sender<Action>> = None;
pub static mut MANAGER: Option<Arc<ConnectionManager>> = None;

#[derive(Clone, Serialize)]
pub struct BsCamera {
    pub ip_address: String,
    pub site_id: String,
    pub site_name: String,
    pub device_id: String,
    pub device_name: String,
    pub mac_address: String,
    pub serial_num: String,
    pub packet_count: i32
}

pub enum ProxyAction {
    ClosePort()
}

#[derive(Clone, Serialize)]
pub struct ProxySessionDto {
    pub port_number: u16,
    pub mac_address: String,
}

#[derive(Clone)]
pub struct ProxySessions {
    pub port_number: u16,
    pub mac_address: String,
    pub tcp_connection: Arc<TcpListener>,
    pub thread: Arc<JoinHandle<()>>,
    pub command: Sender<ProxyAction>
}

pub enum Action {
    RegisterCamera(Box<BsCamera>, TcpStream),
    SendPostRequest(TcpStream, Box<String>, Box<String>),
    UnregisterCamera(TcpStream)
}

#[derive(Clone)]
pub struct ConnectionManager {
    pub sender: Sender<Action>,
    receiver: Arc<Receiver<Action>>,
    connected_cameras: Arc<Mutex<Vec<(Box<BsCamera>, TcpStream)>>>,
    browser_proxies: Arc<Mutex<Vec<ProxySessions>>>,
    packet_counter: Arc<Mutex<HashMap<String, i32>>>,
    port_start: u16,
    port_end: u16,
    camera_list: Arc<RwLock<Vec<Box<BsCamera>>>>
}

const MSG_SIZE: usize = 81960;

unsafe impl Send for ConnectionManager {
}

impl ConnectionManager {
    pub fn new() -> ConnectionManager {
        let (sender, receiver) = channel();
        let connected_cameras = Arc::new(Mutex::new(vec![]));
        let browser_proxies = Arc::new(Mutex::new(vec![]));
        let packet_counter: Arc<Mutex<HashMap<String, i32>>> = Arc::new(Mutex::new(HashMap::new()));
        let port_start = config::get_port_start();
        let port_end = config::get_port_end();
        let camera_list = Arc::new(RwLock::new(vec![]));

        unsafe {
            SENDER = Option::from(sender.clone());
        }

        let manager = ConnectionManager {
            sender,
            receiver: Arc::from(receiver),
            connected_cameras,
            browser_proxies,
            packet_counter,
            port_start,
            port_end,
            camera_list
        };

        return manager;
    }
    /*pub fn get_connections(&self) -> Vec<ProxySessionDto> {
        let mut proxy_dtos : Vec<ProxySessionDto> = vec![];
        let proxy_sessions = self.browser_proxies.clone();
        let proxy_sessions = proxy_sessions.lock().unwrap();
        let proxy_sessions = proxy_sessions.clone();
        for proxy in proxy_sessions {
            proxy_dtos.push(ProxySessionDto {
                mac_address: proxy.mac_address.clone(),
                port_number: proxy.port_number
            })
        }
        return proxy_dtos;
    }*/

    pub fn get_stream_by_mac_id(&self, mac_address: String) -> Option<ProxySessions> {
        // check if mac address already opened
        let browser_proxies = self.browser_proxies.clone();
        let mut browser_proxies = browser_proxies.lock().unwrap();
        {
            let mut browser_iterator = browser_proxies.iter();
            let has_matches = browser_iterator.position(|proxy_entry| proxy_entry.mac_address == mac_address);
            match has_matches {
                None => {}
                Some(matching_position) => {
                    let proxy = browser_proxies.get(matching_position).unwrap();
                    println!("Connection for camera already exist");
                    return Some(proxy.clone());
                }
            }
        }

        // check if mac address exists
        let cameras_connected;
        {
            let connected_cameras = self.connected_cameras.clone();
            let connected_cameras = connected_cameras.lock().unwrap();
            let mut connected_cameras_iter = connected_cameras.iter();
            let existing_position = connected_cameras_iter.position( |conn_tuple| conn_tuple.0.mac_address == mac_address);
            cameras_connected = connected_cameras.len();
            match existing_position {
                None => {
                    return None;
                }
                Some(position) => {
                    let camera_info = connected_cameras.get(position);
                    match camera_info {
                        None => {
                            return None;
                        }
                        Some((_info, _stream)) => {
                        }
                    }
                }
            };

        }

        println!("Connected Cameras {}", cameras_connected);

        // get a random port
        let mut rng = rand::thread_rng();
        let mut numbers: Vec<u16> = (self.port_start .. self.port_end).collect();
        numbers.shuffle(&mut rng);
        let random_port_number = numbers.get(0).unwrap();

        // create a new server
        let proxy_ip = Ipv4Addr::from([0,0,0,0]);
        let browser_address = SocketAddrV4::new(proxy_ip, *random_port_number);
        let browser_listener = TcpListener::bind(browser_address).unwrap();
        let sender = self.sender.clone();

        let thread_mac_address = Box::new(mac_address.clone());
        let thread_listener = browser_listener.try_clone().unwrap();
        let (tx, rx): (Sender<ProxyAction>, Receiver<ProxyAction>) = channel();
        // create listener for new ports
        let thread = thread::spawn(move || loop {
            if let Ok((mut socket, addr)) = browser_listener.accept() {
                let mut buff = vec![0; MSG_SIZE];
                let thread_mac_address = thread_mac_address.clone();
                match socket.read(&mut buff) {
                    Ok(0) => {
                        println!("\nBrowser: {} disconnected", addr);
                    }
                    Ok(message_size) => {
                        println!("\nGot messages from browser_listener");
                        let request_body_string = std::str::from_utf8(&buff[0 .. message_size]).expect("Unable to convert request");
                        let request_body = request_body_string.to_string();

                        let request_body = Box::new(request_body);
                        socket.set_read_timeout(Option::from(Duration::from_millis(2500)));
                        let result = sender.send(Action::SendPostRequest(socket.try_clone().unwrap(), thread_mac_address, request_body));
                        match result {
                            Ok(_send_result) => {
                                println!("\nSend Result done");
                            }
                            Err(_) => {}
                        }
                    }
                    Err(error_message) => {
                        println!("\nERROR: {}", error_message);
                    }
                }
            }
            if let Ok(msg) = rx.try_recv() {
                match msg {
                    ProxyAction::ClosePort() => {
                        break;
                    }
                }
            }
        });
        let session = ProxySessions {
            mac_address,
            port_number: *random_port_number,
            tcp_connection: Arc::from(thread_listener.try_clone().unwrap()),
            thread: Arc::from(thread),
            command: tx
        };
        browser_proxies.push(session.clone());
        return Option::from(session);
    }

    pub fn close_stream_by_port_num(&self, port_num: u16) {
        let browser_proxies = self.browser_proxies.clone();
        let mut browser_proxies = browser_proxies.lock().unwrap();
        {
            let mut browser_iterator = browser_proxies.iter();
            let has_matches = browser_iterator.position(|proxy_entry| proxy_entry.port_number == port_num);
            match has_matches {
                None => {}
                Some(position) => {
                    let proxy = browser_proxies.get(position);
                    match proxy {
                        None => {}
                        Some(proxy) => {
                            proxy.tcp_connection.set_nonblocking(true).expect("Unable to close connection");
                            let command = proxy.command.clone();
                            command.send(ProxyAction::ClosePort());
                        }
                    }
                    browser_proxies.remove(position);
                }
            }
        }
    }

    pub fn get_cameras_available(&self) -> Vec<BsCamera> {
        /*let connections = self.connected_cameras.clone();
        let connections = connections.lock().unwrap();
        let mut vec_connections: Vec<BsCamera> = vec![];
        for (camera_info, _connection) in connections.iter() {
            vec_connections.push(*camera_info.clone());
        }
        return vec_connections;*/
        let copy_of_camera_list;
        {
            let borrowed_handle = self.camera_list.read().unwrap();
            copy_of_camera_list = borrowed_handle.iter().map(|cam| *cam.clone()).collect();
        }
        return copy_of_camera_list;
    }
    pub fn process(&mut self) {
        if let Ok(msg) = self.receiver.try_recv() {
            let tcp_connections = self.connected_cameras.clone();
            match msg {
                Action::RegisterCamera(camera, socket) => {
                    println!("Incoming camera {}", camera.ip_address);
                    let mut tcp_connections = tcp_connections.lock().unwrap();
                    let existing_position;
                    {
                        let mut tcp_conn_iterator = tcp_connections.iter();
                        existing_position = tcp_conn_iterator.position( |conn_tuple| conn_tuple.0.mac_address == camera.mac_address);
                    }
                    match existing_position {
                        None => {
                            tcp_connections.push((camera.clone(), socket));
                        }
                        Some(position) => {
                            tcp_connections.remove(position);
                            tcp_connections.push((camera.clone(), socket));
                        }
                    };
                    let new_list: Vec<Box<BsCamera>> = tcp_connections.iter()
                        .map( |conn_tuple| conn_tuple.0.clone() )
                        .collect();
                    {
                        let mut camera_list_mutex = self.camera_list.clone();
                        let mut camera_list = camera_list_mutex.write().unwrap();
                        camera_list.clear();
                        for item in new_list {
                            camera_list.push(item);
                        }
                    }

                    println!("TCP Connections {}", tcp_connections.len());
                    println!("Camera List {}", self.camera_list.read().unwrap().len());
                }
                Action::SendPostRequest(browser_stream, mac_address, post_body) => {
                    // check if camera with mac address is available
                    let tcp_connections = self.connected_cameras.lock().unwrap();
                    let mut tcp_conn_iterator = tcp_connections.iter();
                    let existing_position = tcp_conn_iterator.position( |conn_tuple| conn_tuple.0.mac_address == *mac_address);
                    match existing_position {
                        None => {

                        }
                        Some(position) => {
                            let connection_result = tcp_connections.get(position);
                            match connection_result {
                                None => {}
                                Some(connection_tuple) => {
                                    let (info, stream) = connection_tuple;
                                    let packet_counter = self.packet_counter.clone();
                                    let packet_counter = packet_counter.lock().unwrap();
                                    let mut connection_counter = packet_counter;
                                    println!("Found matching MAC {}", &info.mac_address);

                                    let current_packet_count;
                                    let current_count_result = connection_counter.get(&info.mac_address);
                                    match current_count_result {
                                        None => {
                                            current_packet_count = 0;
                                            connection_counter.insert(info.mac_address.clone(), 1);
                                        }
                                        Some(result) => {
                                            current_packet_count = *result;
                                            let new_value = result + 1;
                                            connection_counter.insert(info.mac_address.clone(), new_value);
                                        }
                                    }
                                    let mut browser_stream_borrowed = browser_stream.try_clone().unwrap();
                                    let mut stream = stream.try_clone().expect("Unable to get handle");
                                    let combined_output = combine_output(&info, &post_body, current_packet_count);
                                    stream.write((&combined_output).as_ref());
                                    let flush_result = stream.flush();
                                    match flush_result {
                                        Ok(_) => {}
                                        Err(_) => {
                                            browser_stream_borrowed.flush();
                                        }
                                    }
                                    let camera_socket_addr = stream.peer_addr().unwrap().to_string();

                                    println!("Sending to camera {} from {}", camera_socket_addr, browser_stream_borrowed.peer_addr().unwrap());
                                    // println!("{}", combined_output);

                                    let mut stream_recv_count = 0;
                                    let mut packet_limit = 0;
                                    let mut current_packet_size = 0;
                                    stream.set_read_timeout(Some(core::time::Duration::from_millis(3000)));
                                    stream.set_write_timeout(Some(core::time::Duration::from_millis(3000)));

                                    loop {
                                        let mut read_buffer = vec![0; MSG_SIZE];
                                        match stream.read(&mut read_buffer) {
                                            Ok(0) => {
                                                println!("Camera connection disconnected");
                                                break;
                                            }
                                            Ok(msg_size) => {
                                                println!("Relaying frame to browser size: {}", msg_size);
                                                // println!("{}", String::from_utf8(Vec::from(&read_buffer[0 .. msg_size])).unwrap());
                                                browser_stream_borrowed.write(&read_buffer[0..msg_size]);
                                                current_packet_size += msg_size;

                                                if stream_recv_count < 2 {
                                                    let mut read_size= msg_size;
                                                    for char_index in 0 .. msg_size {
                                                        let m1 = read_buffer[char_index] == 13;
                                                        let m2 = read_buffer[char_index + 1] == 10;
                                                        if m1 && m2 {
                                                            // camera sent HTTP Header only
                                                            read_size = char_index;
                                                            println!("Found header!");
                                                            break;
                                                        }
                                                    }
                                                    if read_size <= 2 {
                                                        break;
                                                    }
                                                    let read_buffer_str_result = String::from_utf8(read_buffer[0..read_size].to_owned());
                                                    match read_buffer_str_result {
                                                        Ok(read_buffer_str) => {
                                                            // attempt to read headers
                                                            if read_buffer_str == "WWW-Authenticate: Basic realm=\"CAMERA_AUTHENTICATE1\"" {
                                                                let mut token_buffer = vec![0; MSG_SIZE];
                                                                let auth_token_result = browser_stream_borrowed.read(&mut token_buffer);
                                                                match auth_token_result {
                                                                    Ok(token_size) => {
                                                                        stream.write(&token_buffer[0 .. token_size]);
                                                                        browser_stream_borrowed.flush();
                                                                        break;
                                                                    }
                                                                    Err(_) => {}
                                                                }
                                                            }
                                                            println!("Parsing packet length {}", read_buffer_str);
                                                            // just relay data
                                                            let content_length_regex = String::from("Content-length: (?P<CONTENTLENGTH>(?:\\w+))");
                                                            let payload_length_str = extract_post_body(&read_buffer_str, content_length_regex, "".parse().unwrap());
                                                            if payload_length_str.len() > 0 {
                                                                let content_length: usize = payload_length_str.parse().expect("parsed length error");
                                                                let header_length = current_packet_size - msg_size + read_size;
                                                                packet_limit = content_length + header_length;
                                                                println!("Parsed packet length {}", packet_limit);
                                                            }
                                                        }
                                                        Err(_) => {
                                                            // camera sent images, should break
                                                            println!("Cannot convert to UTF8 string");
                                                        }
                                                    }
                                                }
                                                if current_packet_size > packet_limit && packet_limit != 0 {
                                                    println!("Ending http request current_count: {} packet_limit: {}", current_packet_size, packet_limit);
                                                    break;
                                                }
                                                stream_recv_count += 1;
                                            }
                                            Err(error) => {
                                                println!("Cannot read socket {}", error);
                                                break;
                                            }
                                        }
                                    }
                                    browser_stream_borrowed.flush();
                                }
                            };

                        }
                    };
                }
                Action::UnregisterCamera(socket) => {
                    let mut tcp_connections = tcp_connections.lock().unwrap();
                    let existing_position;
                    {
                        let mut tcp_conn_iterator = tcp_connections.iter();
                        existing_position = tcp_conn_iterator.position( |conn_tuple| conn_tuple.1.peer_addr().unwrap().port() == socket.peer_addr().unwrap().port());
                    }
                    match existing_position {
                        None => {
                            println!("No Matching stream {}", socket.peer_addr().unwrap().ip());
                        }
                        Some(position) => {
                            println!("Removing from list {}", socket.peer_addr().unwrap().ip());
                            tcp_connections.remove(position);
                        }
                    };
                    let new_list : Vec<Box<BsCamera>> = tcp_connections.iter()
                        .map( |conn_tuple| conn_tuple.0.clone() )
                        .collect();
                    {
                        let mut camera_list = self.camera_list.clone();
                        let mut camera_list = camera_list.write().unwrap();
                        camera_list.clear();
                        for item in new_list {
                            camera_list.push(item);
                        }
                    }
                }
            }
        }
    }
}


fn combine_output(camera_info: &Box<BsCamera>, http_request: &String, current_packet_count: i32) -> String {
    let combined_data = format!("POST {} url{}.html HTTP/1.1", camera_info.ip_address, current_packet_count);
    let combined_data = format!("{}\r\nContent-Type: text/plain;\r\nConnection: Keep-Alive", combined_data);
    let combined_data = format!("{}\r\nContent-Length: {}", combined_data, http_request.len());
    let combined_data = format!("{}\r\n\r\n{}", combined_data, http_request);
    return combined_data;
}

fn extract_post_body(body: &str, regex_pattern: String, default: String) -> String {
    let re_ip_add = RegexBuilder::new(&*regex_pattern).multi_line(true).build().unwrap();
    let matching_ip_result = re_ip_add.captures(body);
    let matching_text;
    match matching_ip_result {
        None => {
            matching_text = String::from(default);
        }
        Some(captured) => {
            let has_captures = captured.len();
            if has_captures > 1 {
                matching_text = String::from(&captured[1]);
            } else {
                matching_text = String::from(&captured[0]);
            }
        }
    };
    return matching_text;
}
