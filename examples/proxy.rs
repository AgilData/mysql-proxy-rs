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
    drop(env_logger::init());

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:3307".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    let mut lp = Loop::new().unwrap();
    let pool = CpuPool::new(4);
    let buffer = Rc::new(RefCell::new(vec![0; 64 * 1024]));
    let handle = lp.handle();
    let listener = lp.run(handle.clone().tcp_listen(&addr)).unwrap();
    let pin = lp.pin();

    println!("Listening for MySQL proxy connections on {}", addr);
    let clients = listener.incoming().map(move |(socket, addr)| {
        (Client {
            pool: pool.clone(),
            handle: handle.clone(),
        }.serve(socket,
                MyHandler {} // our packet handler
        ), addr)
    });
    let server = clients.for_each(|(client, addr)| {
        pin.spawn(client.then(move |res| {
            match res {
                Ok((a, b)) => {
                    println!("proxied {}/{} bytes for {}", a, b, addr)
                }
                Err(e) => println!("error for {}: {}", addr, e),
            }
            futures::finished(())
        }));
        Ok(())
    });

    lp.run(server).unwrap();
}

struct MyHandler {}

impl PacketHandler for MyHandler {

    fn handle_request(&self, p: &Packet) -> Action {
        println!("Request:");
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
        println!("Response:");
        print_packet_chars(&p.bytes);
        Action::Forward
    }

}

impl Drop for MyHandler {
    fn drop(&mut self) {
        println!("Dropping handler");
    }
}


