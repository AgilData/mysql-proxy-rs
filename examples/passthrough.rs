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

use std::rc::Rc;
use std::env;
use std::net::{SocketAddr};
use std::str;

use futures::{Future};
use futures::stream::Stream;
use tokio_core::net::{TcpStream, TcpListener};
use tokio_core::reactor::{Core};

fn main() {
    env_logger::init().unwrap();

    // determine address for the proxy to bind to
    let bind_addr = env::args().nth(1).unwrap_or("127.0.0.1:3307".to_string());
    let bind_addr = bind_addr.parse::<SocketAddr>().unwrap();

    // determine address of the MySQL instance we are proxying for
    let mysql_addr = env::args().nth(2).unwrap_or("127.0.0.1:3306".to_string());
    let mysql_addr = mysql_addr.parse::<SocketAddr>().unwrap();

    // Create the tokio event loop that will drive this server
    let mut l = Core::new().unwrap();

    // Get a reference to the reactor event loop
    let handle = l.handle();

    // Create a TCP listener which will listen for incoming connections
    let socket = TcpListener::bind(&bind_addr, &l.handle()).unwrap();
    println!("Listening on: {}", bind_addr);

    // for each incoming connection
    let done = socket.incoming().for_each(move |(socket, _)| {

        // create a future to serve requests
        let future = TcpStream::connect(&mysql_addr, &handle)
            .and_then(move |mysql| { Ok((socket, mysql)) })
            .and_then(move |(client, server)|
                { Pipe::new(Rc::new(client), Rc::new(server), PassthroughHandler {})
                });

        // tell the tokio reactor to run the future
        handle.spawn(future.map_err(|err| {
            println!("Failed to spawn future: {:?}", err);
        }));

        // everything is great!
        Ok(())

    });
    l.run(done).unwrap();
}

struct PassthroughHandler {}

impl PacketHandler for PassthroughHandler {

    fn handle_request(&mut self, _: &Packet) -> Action {
        Action::Forward
    }

    fn handle_response(&mut self, _: &Packet) -> Action {
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
