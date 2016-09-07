//! MySQL Proxy Server

#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate futures;
#[macro_use]
extern crate tokio_core;
extern crate futures_cpupool;

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


fn parse_packet_length(header: &[u8]) -> usize {
    (((header[2] as u32) << 16) |
    ((header[1] as u32) << 8) |
    header[0] as u32) as usize
}

pub struct Client {
    pub pool: CpuPool,
    pub handle: LoopHandle,
}

impl Client {

    pub fn serve(self, conn: TcpStream) -> Box<Future<Item = (u64, u64), Error = io::Error>> {

        let addr = Ipv4Addr::new(127, 0, 0, 1);
        let port = 3306;
        let addr = SocketAddr::V4(SocketAddrV4::new(addr, port));

        let handle = self.handle.clone();
        let connected = handle.tcp_connect(&addr);

        // perform handshake
        let pair = connected.and_then(move |server| {
            read_exact(server, [0u8; 4]).and_then(move |(s,hdr)| {
                let l = parse_packet_length(&hdr);
                read_exact(s, vec![0u8; l]).and_then(move |(s2,payload)| {
                    write_all(conn, hdr).and_then(move |(c2,_)| {
                        write_all(c2, payload).and_then(move |(c3,_)| {
                            Ok((c3, s2))
                        })
                    })
                })
            })
        });

        Box::new(pair.and_then(|(c1, c2)| {
            let c1 = Rc::new(c1);
            let c2 = Rc::new(c2);

            let half1 = Transfer::new(c1.clone(), c2.clone(), Direction::Request);
            let half2 = Transfer::new(c2, c1, Direction::Response);
            half1.join(half2)
        }))
    }
}

struct Transfer {
    reader: Rc<TcpStream>,
    writer: Rc<TcpStream>,
    direction: Direction,
    buf: Vec<u8>,
    pos: usize,
    amt: u64,
}

#[derive(Debug)]
enum Direction {
    Request, Response
}

impl Transfer {
    fn new(reader: Rc<TcpStream>,
           writer: Rc<TcpStream>,
    direction: Direction) -> Transfer {
        Transfer {
            reader: reader,
            writer: writer,
            direction: direction,
            buf: vec![0u8; 4096],
            pos: 0,
            amt: 0,
        }
    }
}

impl Future for Transfer {
    type Item = u64;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<u64, io::Error> {

        loop {
            try_ready!(self.reader.poll_read());
            try_ready!(self.writer.poll_write());

            //TODO: ensure there is capacity in the buffer for further reads
            let n = try_nb!((&*self.reader).read(&mut self.buf[self.pos..]));
            if n == 0 {
                println!("{:?} EOF", self.direction);
                try!(self.writer.shutdown(Shutdown::Write));
                return Ok(self.amt.into())
            }
            self.amt += n as u64;
            self.pos += n;

            // do we have a header
            while self.pos > 3 {
                let l = parse_packet_length(&self.buf);
                println!("{:?} packet_len = {}", self.direction, l);

                // do we have the whole packet?
                let s = 4 + l;
                if self.pos >= s {

                    println!("{:?} Writing MySQL packet: ", self.direction);
                    print_packet_bytes(&self.buf[0..s]);
                    print_packet_chars(&self.buf[0..s]);

                    let m = try!((&*self.writer).write(&self.buf[0..s]));
                    assert_eq!(s, m);

                    //TODO: must be more efficient way to do this
                    for _ in 0..s {
                        self.buf.remove(0);
                    }

                    self.pos -= s;
                } else {
                    println!("not enough bytes for packet");
                    break;
                }
            }
        }
    }
}

#[allow(dead_code)]
fn print_packet_chars(buf: &[u8]) {
    print!("[");
    for i in 0..buf.len() {
        print!("{} ", buf[i] as char);
    }
    println!("]");
}

#[allow(dead_code)]
fn print_packet_bytes(buf: &[u8]) {
    print!("[");
    for i in 0..buf.len() {
        if i%8==0 { println!(""); }
        print!("{:#04x} ",buf[i]);
    }
    println!("]");
}


