use std::sync::mpsc::{Sender, Receiver, channel};
use std::net::{TcpStream, TcpListener, SocketAddrV4, Ipv4Addr};
use std::sync::{Arc, Mutex, MutexGuard};
use std::collections::HashMap;
use std::io::{Write, Read};
use regex::RegexBuilder;
use std::clone::Clone;
use serde::Serialize;
use rand::prelude::*;
use std::thread::{Thread, JoinHandle};
use std::thread;
use std::ops::Deref;
use std::borrow::BorrowMut;

pub static mut SENDER: Option<Sender<Action>> = None;
pub static mut MANAGER: Option<Arc<Mutex<ConnectionManager>>> = None;

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
    RegisterCamera(Box<BsCamera>, Box<TcpStream>),
    SendPostRequest(TcpStream, Box<String>, Box<String>),
    DoNothing(String)
}

#[derive(Clone)]
pub struct ConnectionManager {
    pub sender: Sender<Action>,
    receiver: Arc<Receiver<Action>>,
    tcp_connections: Arc<Mutex<Vec<(Box<BsCamera>, Box<TcpStream>)>>>,
    browser_proxies: Arc<Mutex<Vec<ProxySessions>>>,
    packet_counter: Arc<Mutex<HashMap<String, i32>>>,
    port_start: u16,
    port_end: u16
}

const MSG_SIZE: usize = 81960;

unsafe impl Send for ConnectionManager {

}

/*impl Clone for ConnectionManager {
    fn clone(&self) -> Self {
        return ConnectionManager {
            sender: self.sender.clone(),
            tcp_connections: self.tcp_connections.clone(),
            receiver: self.receiver.clone()
        }
    }
}*/

impl ConnectionManager {
    pub fn new() -> ConnectionManager {
        let (sender, receiver) = channel();
        let tcp_connections = Arc::new(Mutex::new(vec![]));
        let browser_proxies = Arc::new(Mutex::new(vec![]));
        let packet_counter: Arc<Mutex<HashMap<String, i32>>> = Arc::new(Mutex::new(HashMap::new()));

        unsafe {
            SENDER = Option::from(sender.clone());
        }

        let manager = ConnectionManager {
            sender,
            receiver: Arc::from(receiver),
            tcp_connections,
            browser_proxies,
            packet_counter,
            port_start: 10030,
            port_end: 10080
        };

        return manager;
    }
    pub fn get_connections(&self) -> Vec<ProxySessionDto> {
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
    }

    pub fn get_stream_by_mac_id(&self, mac_address: String) -> Option<ProxySessions> {
        // check if mac address already opened
        let browser_proxies = self.browser_proxies.clone();
        let mut browser_proxies = browser_proxies.lock().unwrap();
        {
            let mut browser_iterator = browser_proxies.iter();
            let has_matches = browser_iterator.position(|proxy_entry| proxy_entry.mac_address == mac_address);
            match has_matches {
                None => {}
                Some(_) => {
                    println!("Connection for camera already exist");
                    return None;
                }
            }
        }

        // check if mac address exists
        let tcp_connections = self.tcp_connections.clone();
        let mut tcp_connections = tcp_connections.lock().unwrap();
        let existing_position;
        {
            let mut tcp_conn_iterator = tcp_connections.iter();
            existing_position = tcp_conn_iterator.position( |conn_tuple| conn_tuple.0.mac_address == mac_address);
        }
        println!("TCP Connections {}", tcp_connections.len());
        let tcpstream;
        match existing_position {
            None => {
                return None;
            }
            Some(position) => {
                let camera_info = tcp_connections.get(position);
                match camera_info {
                    None => {
                        return None;
                    }
                    Some((info, stream)) => {
                        tcpstream = stream.try_clone().unwrap();
                    }
                }
            }
        };
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
                        sender.send(Action::SendPostRequest(socket.try_clone().unwrap(), thread_mac_address, request_body));
                    }
                    Err(error_message) => {
                        println!("\nERROR: {}", error_message);
                    }
                }
            }
            if let Ok(msg) = rx.try_recv() {
                match (msg) {
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
                    match (proxy) {
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

    pub fn testing(&self) {
        println!("Testing");
    }
    pub fn get_cameras_available(&self) -> Vec<BsCamera> {
        let connections = self.tcp_connections.clone();
        let mut connections = connections.lock().unwrap();
        let mut vec_connections: Vec<BsCamera> = vec![];
        for (camera_info, connection) in connections.iter() {
            vec_connections.push(*camera_info.clone());
        }
        return vec_connections;
    }
    pub fn process(&self) {
        if let Ok(msg) = self.receiver.try_recv() {
            let tcp_connections = self.tcp_connections.clone();
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
                            tcp_connections.push((camera, socket));
                        }
                        Some(position) => {
                            tcp_connections.remove(position);
                            tcp_connections.push((camera, socket));
                        }
                    };
                    println!("TCP Connections {}", tcp_connections.len());
                }
                Action::SendPostRequest(browser_stream, mac_address, post_body) => {
                    let mut tcp_connections = self.tcp_connections.lock().unwrap();
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
                                    let mut packet_counter = packet_counter.lock().unwrap();
                                    let mut connection_counter = packet_counter;
                                    println!("Found matching MAC {}", &info.mac_address);

                                    let mut current_packet_count = 0;
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

                                    let mut stream = stream.try_clone().expect("Unable to get handle");
                                    let combined_output = combine_output(&info, &post_body, current_packet_count);
                                    stream.write((&combined_output).as_ref());
                                    stream.flush();
                                    let camera_socket_addr = stream.peer_addr().unwrap().to_string();
                                    let mut browser_stream_borrowed = browser_stream.try_clone().unwrap();
                                    println!("Sending to camera {} from {}", camera_socket_addr, browser_stream_borrowed.peer_addr().unwrap());
                                    // println!("{}", combined_output);

                                    let mut stream_recv_count = 0;
                                    let mut packet_limit = 0;
                                    let mut current_packet_size = 0;

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
                                                        if (m1 && m2) {
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
                                                            println!("Parsing packet length {}", read_buffer_str);
                                                            let content_length_regex = String::from("Content-length: (?P<CONTENTLENGTH>(?:\\w+))");
                                                            let payload_length_str = extract_post_body(&read_buffer_str, content_length_regex, "".parse().unwrap());
                                                            if payload_length_str.len() > 0 {
                                                                packet_limit = payload_length_str.parse().expect("parsed length error");
                                                                println!("Parsed packet length {}", packet_limit);
                                                            }
                                                        }
                                                        Err(_) => {
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
                Action::DoNothing(not_string) => {
                    println!("Got a DoNothing message");
                    println!("{}", not_string);
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
    let mut matching_text: String = String::from("");
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
