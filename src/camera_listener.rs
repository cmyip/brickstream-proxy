use std::sync::mpsc::{Sender};
use crate::connection_manager::{Action, BsCamera};
use std::thread;
use std::io::{ErrorKind, Read};
use std::time::Duration;
use regex::RegexBuilder;
use std::net::{TcpListener, SocketAddr, IpAddr};

const MSG_SIZE: usize = 81960;

pub struct CameraListener {
    pub port_number: u16,
    listener: TcpListener,
    sender: Option<Sender<Action>>
}

impl CameraListener {
    pub fn new(ip_address: IpAddr, port_number: u16) -> CameraListener {
        let address = SocketAddr::new(ip_address, port_number);
        let listener = TcpListener::bind(address).unwrap();
        return CameraListener {
            port_number,
            listener,
            sender: None
        };
    }

    pub fn set_sender(&mut self, sender: Sender<Action>) {
        self.sender = Option::from(sender);
    }

    pub fn process(&mut self) {
        let sender;
        match &self.sender {
            None => {
                println!("Sender not initialized, exiting");
                return;
            }
            Some(bsender) => {
                sender = bsender.clone();
            }
        }
        if let Ok((mut socket, addr)) = self.listener.accept() {
            println!("Client {:?} connected to channel", addr);

            let sender = sender.clone();
            // create a thread to keep the socket to keep connection to camera open
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
                    Ok(_message_size) => {
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
                            let result = sender.send(Action::RegisterCamera(Box::new(bs_camera), Box::new(tcp_stream)));
                            match result {
                                Ok(_) => {}
                                Err(_) => {}
                            }
                            break;
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
    }
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
