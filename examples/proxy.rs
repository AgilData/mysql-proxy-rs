//! MySQL Proxy Server

#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate futures;
#[macro_use]
extern crate tokio_core;
extern crate futures_cpupool;
extern crate mysql_proxy;

use mysql_proxy::*;

use std::cell::RefCell;
use std::rc::Rc;
use std::env;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::net::Shutdown;
use std::str;
use std::time::Duration;

use futures::{Future, Poll};
use futures::stream::Stream;
use futures_cpupool::CpuPool;
use tokio_core::{Loop, LoopHandle, TcpStream};
use tokio_core::io::{read_exact, write_all, Window};


fn main() {
    let addr: SocketAddr = "127.0.0.1:3307".parse().unwrap();

    let mut reactor = reactor::Core::new().unwrap();
    let handle = reactor.handle();
    let listener = TcpListener::bind(&addr, &handle).unwrap();

    println!("Running server on {}", addr);

    reactor.run(listener.incoming().for_each(move |(socket, _)| {

        let client = Client {
            handle: handle.clone()
        };

//        handle.spawn(connection.map_err(|err| {
//            println!("Oh no! Error {:?}", err);
//        }));

        Ok(())
    })).unwrap();
}

struct MyHandler {

}

impl MyHandler {

    fn new() -> Self {
        println!("Created new handler");
        MyHandler {}
    }
}

impl PacketHandler for MyHandler {

    fn handle_request(&self, p: &Packet) -> Action {
//        println!("Request:");
//        print_packet_chars(&p.bytes);

//        match p.packet_type() {
//            PacketType::ComQuery => {
//
//                let slice = &p.bytes[5..];
//                let sql = String::from_utf8(slice.to_vec()).expect("Invalid UTF-8");
//
//                println!("SQL: {}", sql);
//
//                if sql.contains("avocado") {
//                    Action::Respond(vec![Packet::error_packet(
//                                                1064, // error code
//                                                [0x31,0x32,0x33,0x34,0x35], // sql state
//                                                String::from("Proxy rejecting any avocado-related queries"))])
//                } else {
//                    Action::Forward
//                }
//            },
//            _ => Action::Forward
//        }

        Action::Forward

    }

    fn handle_response(&self, p: &Packet) -> Action {
//        println!("Response:");
//        print_packet_chars(&p.bytes);
        Action::Forward
    }

}

impl Drop for MyHandler {
    fn drop(&mut self) {
        println!("Dropping handler");
    }
}


