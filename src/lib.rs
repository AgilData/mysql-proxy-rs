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
use std::io::{self, Read, Write, Error, ErrorKind};
use std::net::{SocketAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::net::Shutdown;
use std::str;
use std::time::Duration;

use futures::{Future, Poll, Async};
use futures::stream::Stream;
use futures_cpupool::CpuPool;
use tokio_core::{Loop, LoopHandle, TcpStream};
use tokio_core::io::{read_exact, write_all, Window};

pub struct Packet<'a> {
    pub bytes: &'a [u8]
}

impl<'a> Packet<'a> {

    fn packet_type(&self) -> u8 {
        self.bytes[4]
    }
}

pub enum Action<'a> {
    Forward,                // forward the packet unmodified
    Mutate(Packet<'a>),         // mutate the packet
    Respond(Vec<Packet<'a>>)    // handle directly and optionally return some packets
}

pub trait PacketHandler {
    fn handle_request(&self, p: &Packet) -> Action;
    fn handle_response(&self, p: &Packet) -> Action;
}

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

    pub fn serve<H: PacketHandler + 'static>(self, conn: TcpStream,
                                             handler: H,
                                        ) -> Box<Future<Item = (u64, u64), Error = Error>> {


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

        Box::new(pair.and_then(move |(c1, c2)| {
            let c1 = Rc::new(c1);
            let c2 = Rc::new(c2);
            Pipe::new(c1, c2, handler)
        }))
    }
}

struct Conn {
    stream: Rc<TcpStream>,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
    read_pos: usize,
    write_pos: usize,
    read_amt: u64,
    write_amt: u64,
}

impl Conn {

    fn new(stream: Rc<TcpStream>) -> Self {
        Conn { stream: stream,
            read_buf: vec![0u8; 4096],
            write_buf: vec![0u8; 4096],
            read_pos: 0,
            write_pos: 0,
            read_amt: 0,
            write_amt: 0 }
    }

    fn read(&mut self) -> Poll<Option<Packet>, io::Error> {
        loop {
            try_ready!(self.stream.poll_read());

            //TODO: ensure capacity first
            let n = try_nb!((&*self.stream).read(&mut self.read_buf[self.read_pos..]));
            if n == 0 {
                return Err(Error::new(ErrorKind::Other, "uh oh"));
            }
            self.read_amt += n as u64;
            self.read_pos += n;

            //TODO: parse packet from buffer

            return Ok(Async::Ready(None));
        }
    }

    fn write(&self, p: &Packet) -> Poll<(), io::Error> {
        while self.write_pos > 0 {
            try_ready!(self.stream.poll_write());

        }
        return Ok(Async::Ready(()));
    }
}

struct Pipe<H: PacketHandler + 'static> {
    client: Conn,
    server: Conn,
    handler: H,
}

impl<H> Pipe<H> where H: PacketHandler + 'static {
    fn new(client: Rc<TcpStream>,
           server: Rc<TcpStream>,
           handler: H
           ) -> Pipe<H> {

        Pipe {
            client: Conn::new(client),
            server: Conn::new(server),
            handler: handler,
        }
    }
}

impl<H> Future for Pipe<H> where H: PacketHandler + 'static {
    type Item = (u64,u64);
    type Error = Error;

    fn poll(&mut self) -> Poll<(u64,u64), io::Error> {
        loop {

            // try reading from client
            let client_read = match self.client.read() {
                Ok(Async::Ready(None)) => Ok(Async::Ready(())),
                Ok(Async::Ready(Some(p))) => {
                    match self.handler.handle_request(&p) {
                        Action::Forward => self.server.write(&p),
                        Action::Mutate(ref p2) => self.server.write(p2),
                        Action::Respond(v) => {
                            for p in v {
                                //TODO: cannot borrow mutable
                                //try!(self.client.write(&p));
                            }
                            Ok(Async::Ready(()))
                        }
                    }
                },
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(e) => Err(e)
            };

            // try writing to server
//            let server_write = match self.server.write() {
//                Ok(None) => {},
//                Ok(Some(p)) => {
//                    match self.handler.handle_request(&p) {
//                        Action::Forward => {},
//                        Action::Mutate(p2) => {},
//                        Action::Respond(v) => {},
//                    }
//                },
//                Err(e) => {}
//            };

        }
    }


            // just so the code compiles during the refactor
     //       return Ok((123_u64, 456_u64).into())


            //        loop {
//
//            //TODO: there are definitely race conditions here that will need to be fixed soon
//            try_ready!(self.client.poll_read());
//            try_ready!(self.server.poll_write());
//
//            //TODO: ensure there is capacity in the buffer for further reads
//            let n = try_nb!((&*self.reader).read(&mut self.buf[self.pos..]));
//            if n == 0 {
//                try!(self.writer.shutdown(Shutdown::Write));
//                return Ok(self.amt.into())
//            }
//            self.amt += n as u64;
//            self.pos += n;
//
//            // do we have a header
//            while self.pos > 3 {
//                let l = parse_packet_length(&self.buf);
//
//                // do we have the whole packet?
//                let s = 4 + l;
//                if self.pos >= s {
//
//                    // invoke the packet handler
//                    {
//                        let packet = Packet { bytes: &self.buf[0..s] };
//                        self.handler.handle(&packet);
//                    }
//
//                    // write the packet
//                    let m = try!((&*self.writer).write(&self.buf[0..s]));
//                    assert_eq!(s, m);
//
//                    // remove this packet from the buffer
//                    //TODO: must be more efficient way to do this
//                    for _ in 0..s {
//                        self.buf.remove(0);
//                    }
//
//                    self.pos -= s;
//                } else {
//                    println!("not enough bytes for packet");
//                    break;
//                }
//            }
//        }
//    }
}

