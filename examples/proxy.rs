//! MySQL Proxy Server
extern crate mysql_proxy;
use mysql_proxy::*;

#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate futures;
#[macro_use]
extern crate tokio_core;
extern crate byteorder;

use std::cell::RefCell;
use std::rc::Rc;
use std::env;
use std::io::{self, Read, Write, Error, ErrorKind};
use std::net::{SocketAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::net::Shutdown;
use std::str;
use std::time::Duration;

use futures::{Future, Poll, Async};
use futures::stream::Stream;
use tokio_core::net::{TcpStream, TcpStreamNew, TcpListener};
use tokio_core::io::{copy, Io};
use tokio_core::reactor::{Core, Handle};
use byteorder::*;


fn main() {

    env_logger::init().unwrap();

    let bind_addr = env::args().nth(1).unwrap_or("127.0.0.1:3307".to_string());
    let bind_addr = bind_addr.parse::<SocketAddr>().unwrap();

    let mysql_addr = env::args().nth(2).unwrap_or("127.0.0.1:3306".to_string());
    let mysql_addr = mysql_addr.parse::<SocketAddr>().unwrap();

    // Create the event loop that will drive this server
    let mut l = Core::new().unwrap();
    let handle = l.handle();

    // Create a TCP listener which will listen for incoming connections
    let socket = TcpListener::bind(&bind_addr, &l.handle()).unwrap();
    println!("Listening on: {}", bind_addr);

    let done = socket.incoming().for_each(move |(socket, addr)| {

        // connect to MySQL
        let future = TcpStream::connect(&mysql_addr, &handle).and_then(move |mysql| {
            Ok((socket, mysql))
        }).and_then(move |(client, server)| {
            Pipe::new(Rc::new(client), Rc::new(server), DemoHandler {})
        });

        handle.spawn(future.map_err(|err| {
            println!("Oh no! Error {:?}", err);
        }));

        Ok(())

    });
    l.run(done).unwrap();
}

struct DemoHandler {}

impl PacketHandler for DemoHandler {

    fn handle_request(&self, p: &Packet) -> Action {
        print_packet_chars(&p.bytes);
        match p.bytes[4] {
            0x03 => {

                let slice = &p.bytes[5..];
                let sql = String::from_utf8(slice.to_vec()).expect("Invalid UTF-8");

                println!("SQL: {}", sql);

                if sql.contains("avocado") {
                    Action::Respond(vec![Packet::error_packet(
                                                1064, // error code
                                                [0x31,0x32,0x33,0x34,0x35], // sql state
                                                String::from("Proxy rejecting any avocado-related queries"))])
                } else {
                    Action::Forward
                }
            },
            _ => Action::Forward
        }
    }

    fn handle_response(&self, p: &Packet) -> Action {
        // forward all responses to the client
        Action::Forward
    }

}
#[allow(dead_code)]
pub fn print_packet_chars(buf: &[u8]) {
    print!("[");
    for i in 0..buf.len() {
        print!("{} ", buf[i] as char);
    }
    println!("]");
}

#[allow(dead_code)]
pub fn print_packet_bytes(buf: &[u8]) {
    print!("[");
    for i in 0..buf.len() {
        if i%8==0 { println!(""); }
        print!("{:#04x} ",buf[i]);
    }
    println!("]");
}


