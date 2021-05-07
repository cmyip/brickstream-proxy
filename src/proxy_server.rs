use std::net::{SocketAddrV4, Ipv4Addr, Shutdown, TcpStream, TcpListener, SocketAddr};
use std::sync::mpsc::channel;
use std::sync::mpsc::{ Sender, Receiver };
use std::thread;
use std::io::{Read, ErrorKind, Write, Error};
use std::time::Duration;
use std::str::{FromStr, Utf8Error};
use std::cell::UnsafeCell;
// use confy;
use crate::bs_proxy_config::BsProxyConfig;
use regex::{Regex, Captures, RegexBuilder};
use crate::proxy_server::Action::RegisterCamera;
use std::sync::{Arc, Mutex};
use std::pin::Pin;
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::str;
use std::ops::Deref;
use std::cmp::{max, min};
use std::string::FromUtf8Error;
use std::time::{SystemTime, UNIX_EPOCH};

const MSG_SIZE: usize = 81960;

struct BsCamera {
    pub ip_address: String,
    pub site_id: String,
    pub site_name: String,
    pub device_id: String,
    pub device_name: String,
    pub mac_address: String,
    pub serial_num: String,
    pub packet_count: i32
}

enum Action {
    // Add(SocketAddrV4, TcpStream),
    // SendMessage(String, String)
    RegisterCamera(Box<BsCamera>, Box<TcpStream>),
    SendPostRequest(TcpStream, Box<String>, Box<String>),
    RelayToBrowser(TcpStream, Vec<u8>)
}

static mut SENDER: Option<Sender<Action>> = None;

pub fn start() {
    // let config: BsProxyConfig = confy::load("config").expect("Failed to read config file");
    // let proxy_ip = Ipv4Addr::from_str(config.get_proxy_ip()).expect(&*format!("Failed to parse IP {}", config.get_proxy_ip()));
    let proxy_ip = Ipv4Addr::from([0,0,0,0]);
    let address = SocketAddrV4::new(proxy_ip, 2395);
    let browser_address = SocketAddrV4::new(proxy_ip, 2388);
    let listener = TcpListener::bind(address).unwrap();
    let browser_listener = TcpListener::bind(browser_address).unwrap();
    let (sender, receiver) = channel();
    unsafe {
        let sender = sender.clone();
        SENDER = Option::from(sender);
    }
    // browser_listener.set_nonblocking(true).expect("Cannot set non-blocking");
    // listener.set_nonblocking(true).expect("Cannot set non-blocking");

    let mut clients: Vec<TcpStream> = vec![];
    let mut tcp_connections: Arc<Mutex<Vec<(Box<BsCamera>, Box<TcpStream>)>>> = Arc::new(Mutex::new(vec![]));
    println!("Device side listening to {}:{}", address.ip(), address.port());

    {
        let sender = sender.clone();
        let mut device_states: HashMap<SocketAddr, i32> = HashMap::new();
        thread::spawn(move || loop {
            if let Ok((mut socket, addr)) = listener.accept() {
                println!("Client {:?} connected to channel", addr);

                let sender = sender.clone();
                clients.push(socket.try_clone().expect("Failed to clone client"));
                let mut device_states = device_states.clone();
                thread::spawn(move || loop {
                    let mut buff = vec![0; MSG_SIZE];
                    match socket.read(&mut buff) {
                        //a read() syscall on a socket that has been closed on the other end will return 0 bytes read,
                        //but no error, which should translate to Ok(0) in Rust.
                        //But this may only apply when the other end closed the connection cleanly.
                        Ok(0) => {
                            println!("\nCamera {} disconnected", addr);
                            break;
                        }

                        //Handle when we do not read an empty socket
                        Ok(message_size) => {
                            //Set the buffer as an Iretartor and take it's elements while the condition retunrs true. Finally returns a Vec of type T
                            let buf = buff.clone().into_iter().take_while(|&x| x!= 0).collect::<Vec<_>>();

                            let string_payload = String::from_utf8(buf).expect("Cannot read camera packet as string");

                            if string_payload.starts_with("POST / HTTP") {
                                let ip_regex = String::from("IP_ADDRESS=(?P<IPADDR>(?:[0-9]{1,3}\\.){3}[0-9]{1,3})&");
                                let site_id_regex = String::from("SITE_ID=(?P<SITEID>(?:\\w+))&");
                                let site_name_regex = String::from("SITE_NAME=(?P<SITENAME>(?:\\w+))&");
                                let device_id_regex = String::from("DEVICE_ID=(?P<DEVICEID>(?:\\w+))&");
                                let device_name_regex = String::from("DEVICE_NAME=(?P<DEVICENAME>(?:\\w+))&");
                                let mac_address_regex = String::from("MAC_ADDRESS=(?P<MACADDRESS>(?:\\w+))&");
                                let serial_num_regex = String::from("SERIAL_NUM=(?P<SERIALNUM>(?:\\w+))[\\&]?");
                                let ip_address = extract_post_body(&*string_payload, ip_regex, "".parse().unwrap());
                                let site_id = extract_post_body(&*string_payload, site_id_regex, "".parse().unwrap());
                                let site_name = extract_post_body(&*string_payload, site_name_regex, "".parse().unwrap());
                                let device_id = extract_post_body(&*string_payload, device_id_regex, "".parse().unwrap());
                                let device_name = extract_post_body(&*string_payload, device_name_regex, "".parse().unwrap());
                                let mac_address = extract_post_body(&*string_payload, mac_address_regex, "".parse().unwrap());
                                let serial_num = extract_post_body(&*string_payload, serial_num_regex, "".parse().unwrap());

                                println!("IP_ADDRESS: {}", ip_address);
                                println!("SITE_ID: {}", site_id);
                                println!("SITE_NAME: {}", site_name);
                                println!("DEVICE_ID: {}", device_id);
                                println!("DEVICE_NAME: {}", device_name);
                                println!("MAC_ADDRESS: {}", mac_address);
                                println!("SERIAL_NUM: {}", serial_num);

                                let bs_camera = BsCamera {
                                    ip_address,
                                    site_id,
                                    site_name,
                                    device_id,
                                    device_name,
                                    mac_address,
                                    serial_num,
                                    packet_count: 0
                                };

                                let tcp_stream = socket.try_clone().expect("Failed to get stream");
                                sender.send(Action::RegisterCamera(Box::new(bs_camera), Box::new(tcp_stream)));
                                break;
                            }

                            if string_payload.starts_with("HTTP/1.0 200 OK") {
                                println!("\nRelaying start");
                                let mut collector = buff[0 .. message_size].to_vec();
                                let mut local_buffer = vec![0; MSG_SIZE];
                                let mut payload_length = 0;
                                let mut header_length = 0;
                                let mut payload_so_far = 0;
                                loop {
                                    match socket.read(&mut local_buffer) {
                                        Ok(0) => {
                                            socket.flush();
                                            break;
                                        }
                                        Ok(local_buffer_size) => {
                                            // let mut collector = collector.clone();
                                            let mut concat_buffer = &local_buffer.clone()[0 .. local_buffer_size];
                                            collector.append(&mut concat_buffer.to_vec());
                                            payload_so_far += local_buffer_size;
                                            println!("Received a frame, current frame size: {}", payload_so_far);


                                            let local_buffer_string_result = str::from_utf8(&concat_buffer);
                                            match local_buffer_string_result {
                                                Ok(local_buffer_string) => {
                                                    if local_buffer_string.contains("Content-length:") {
                                                        let content_length_regex = String::from("Content-length: (?P<CONTENTLENGTH>(?:\\w+))");
                                                        let payload_length_str = extract_post_body(local_buffer_string, content_length_regex, "".parse().unwrap());
                                                        payload_length = payload_length_str.parse().unwrap();
                                                        let length_to_parse = local_buffer_string.find("\r\n\r\n");
                                                        match (length_to_parse) {
                                                            None => {
                                                                println!("Not finding matching end of header");
                                                            }
                                                            Some(matching_position) => {
                                                                println!("Header position {}", matching_position);
                                                                header_length = matching_position;
                                                            }
                                                        }
                                                        println!("Parsed content-length: {}", payload_length);
                                                    }
                                                }
                                                Err(_) => {}
                                            }


                                            let combined_length = payload_length + header_length;
                                            if combined_length > 0 && collector.len() >= (combined_length) {
                                                println!("Relaying buffer to browser");
                                                println!("payload length is {}", payload_length);
                                                println!("header length is {}", header_length);
                                                println!("content length is {}", collector.len());
                                                sender.send(Action::RelayToBrowser(socket.try_clone().unwrap(), collector));
                                                break;
                                            }
                                        }
                                        Err(_) => {}
                                    }
                                }

                            }

                        },
                        //Handle reading errors!
                        Err(ref err) if err.kind() == ErrorKind::WouldBlock => (),
                        Err(_) => {
                            println!("\nClient: {} left the channel.", addr);
                            break;
                        }
                    }
                    thread::sleep(Duration::from_millis(200));
                });
            }
        });
    }

    {
        let tcp_connections = tcp_connections.clone();
        thread::spawn(move || loop {
            if let Ok((mut socket, addr)) = browser_listener.accept() {
                let sender = sender.clone();
                let tcp_connections = tcp_connections.clone();
                println!("\nBrowser Side {} connected", addr);
                thread::spawn(move || loop {
                    let mut buff = vec![0; MSG_SIZE];
                    match socket.read(&mut buff) {
                        Ok(0) => {
                            println!("\nClient: {} left the channel.", addr);
                            break;
                        }
                        Ok(message_size) => {
                            println!("\nGot messages from browser_listener");
                            let tcp_connections = tcp_connections.lock().unwrap();
                            let first_client_result = tcp_connections.first();
                            let first_client;
                            match first_client_result {
                                None => {
                                    break;
                                }
                                Some(client) => {
                                    println!("\nFound matching client");
                                    first_client = client
                                }
                            };

                            let request_body_string = str::from_utf8(&buff[0 .. message_size]).expect("Unable to convert request");
                            let request_body = request_body_string.to_string();

                            let first_mac = Box::new(first_client.0.mac_address.clone());
                            let request_body = Box::new(request_body);
                            sender.send(Action::SendPostRequest(socket.try_clone().unwrap(), first_mac, request_body));
                        }
                        Err(error_message) => {
                            println!("\nERROR: {}", error_message);
                            break;
                        }
                    }

                });
            }
        });
    }

    let mut camera_to_browser_map: HashMap<Box<String>, Box<TcpStream>> = HashMap::new();
    loop {
        if let Ok(msg) = receiver.try_recv() {
            let tcp_connections = tcp_connections.clone();
            let mut connection_counter: HashMap<String, i32> = HashMap::new();
            match msg {
                /*Action::SendMessage(message ) => {
                    let buff = message.clone().into_bytes();
                    buff.clone().resize(buff.len(), 0);
                    client.write_all(&buff).map(|_| client).ok()
                }*/
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
                }
                Action::SendPostRequest(browser_stream, mac_address, post_body) => {
                    let mut tcp_connections = tcp_connections.lock().unwrap();
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
                                    println!("Found matching MAC {}", &info.mac_address);

                                    let current_count_result = connection_counter.get(&info.mac_address);
                                    let current_packet_count;
                                    match current_count_result {
                                        None => {
                                            current_packet_count = 1;
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
                                    camera_to_browser_map.insert(Box::new(camera_socket_addr.clone()), Box::new(browser_stream.try_clone().unwrap()));
                                    let mut browser_stream_borrowed = browser_stream.try_clone().unwrap();
                                    println!("Sending to camera {} from {}", camera_socket_addr, browser_stream_borrowed.peer_addr().unwrap());
                                    println!("Hashmap size: {}", camera_to_browser_map.len());

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
                Action::RelayToBrowser(camera_socket, payload) => {
                    let camera_socket_addr = camera_socket.peer_addr().unwrap().to_string();
                    println!("Sending from camera {}", camera_socket_addr);
                    println!("Hashmap size: {}", camera_to_browser_map.len());
                    let browser_socket_result = camera_to_browser_map.get(&camera_socket_addr);
                    match browser_socket_result {
                        None => {
                            for camera_string in camera_to_browser_map.keys() {
                                println!("Keys: {}", camera_string);
                            }
                            println!("Not finding matching camera sockets");
                        }
                        Some(socket) => {
                            println!("Found browser {}", socket.peer_addr().unwrap());
                            let socket_result = socket.try_clone();
                            match socket_result {
                                Ok(mut socket) => {
                                    println!("Writing data back to browser");
                                    socket.write(payload.as_slice());
                                    socket.flush().expect("Unable to write to browser");
                                }
                                Err(_) => {}
                            }
                        }
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(200));
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
