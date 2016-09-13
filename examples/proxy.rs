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
        let future = TcpStream::connect(&mysql_addr, &handle).and_then(move |mysql| {
            Ok((socket, mysql))
        }).and_then(move |(client, server)| {
            Pipe::new(Rc::new(client), Rc::new(server), DemoHandler { counter: 0 })
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

struct DemoHandler {
    counter: usize

}

impl PacketHandler for DemoHandler {

    fn handle_request(&mut self, p: &Packet) -> Action {
        self.counter = 0;
        Action::Forward
//        print_packet_chars(&p.bytes);
//        match p.packet_type() {
//            Ok(PacketType::ComQuery) => {
//                // ComQuery packets just contain a SQL string as the payload
//                let slice = &p.bytes[5..];
//
//                // convert the slice to a String object
//                let sql = String::from_utf8(slice.to_vec()).expect("Invalid UTF-8");
//
//                // log the query
//                println!("SQL: {}", sql);
//
//                // dumb example of conditional proxy behavior
//                if sql.contains("avocado") {
//                    // take over processing of this packet and return an error packet
//                    // to the client
//                    Action::Error(1064, // error code
//                                  [0x31, 0x32, 0x33, 0x34, 0x35], // sql state
//                                  String::from("Proxy rejecting any avocado-related queries"))
//                } else {
//                    // pass the packet to MySQL unmodified
//                    Action::Forward
//                }
//            },
//            _ => Action::Forward
//        }
    }

    fn handle_response(&mut self, p: &Packet) -> Action {
        println!("---------------------- handle_response() ----------------------");
        print_packet_chars(&p.bytes);

        // packets 1-4 column meta
        // packets 5-45 = result row
        // packet 46 = EOF

        let n = 5;
        self.counter += 1;
        let action = if self.counter < n {
            Action::Forward
        } else if self.counter == n {
            Action::Error { code: 1234, state: [31,32,32,34,35], msg: String::from("bad reults")}
        } else /*if self.counter > n*/ {
            Action::Drop
        };
        println!("Response packet {}: action = {:?}", self.counter, action);
        action
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


