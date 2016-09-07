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

pub struct Packet {
    pub bytes: Vec<u8>
}

pub enum Action {
    Forward,                // forward the packet unmodified
    Mutate(Packet),         // mutate the packet
    Respond(Vec<Packet>)    // handle directly and optionally return some packets
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

struct ConnReader {
    stream: Rc<TcpStream>,
    read_buf: Vec<u8>,
    read_pos: usize,
    read_amt: u64,
}

struct ConnWriter {
    stream: Rc<TcpStream>,
    write_buf: Vec<u8>,
    write_pos: usize,
    write_amt: u64,
}

impl ConnReader {
    fn new(stream: Rc<TcpStream>) -> Self {
        ConnReader {
            stream: stream,
            read_buf: vec![0u8; 4096],
            read_pos: 0,
            read_amt: 0,
        }
    }

    fn read(&mut self) -> Poll<Option<Packet>, io::Error> {
        loop {

            // see if there is a packet already in the buffer
            if let Some(p) = self.parse_packet() {
                return Ok(Async::Ready(Some(p)))
            }

            // try reading more data
            try_ready!(self.stream.poll_read());

            //TODO: ensure capacity first
            let n = try_nb!((&*self.stream).read(&mut self.read_buf[self.read_pos..]));
            if n == 0 {
                return Err(Error::new(ErrorKind::Other, "uh oh"));
            }
            self.read_amt += n as u64;
            self.read_pos += n;

            if let Some(p) = self.parse_packet() {
                return Ok(Async::Ready(Some(p)))
            } else {
                return Ok(Async::Ready(None))
            }
        }
    }

    fn parse_packet(&mut self) -> Option<Packet> {
        // do we have a header
        if self.read_pos > 3 {
            let l = parse_packet_length(&self.read_buf);
            // do we have the whole packet?
            let s = 4 + l;
            if self.read_pos >= s {

                //TODO: clean up this hack
                let mut temp : Vec<u8> = vec![];
                temp.extend_from_slice( &self.read_buf[0..s]);
                let p = Packet { bytes: temp };

                // remove this packet from the buffer
                //TODO: must be more efficient way to do this
                for _ in 0..s {
                    self.read_buf.remove(0);
                }
                self.read_pos -= s;

                Some(p)
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl ConnWriter{

    fn new(stream: Rc<TcpStream>) -> Self {
        ConnWriter{ stream: stream,
            write_buf: vec![0u8; 4096],
            write_pos: 0,
            write_amt: 0 }
    }

    /// Write a packet to the write buffer
    fn push(&mut self, p: &Packet) {
        self.write_buf.extend_from_slice(&p.bytes);
        self.write_pos += p.bytes.len();
    }

    /// Writes the contents of the write buffer to the socket
    fn write(&mut self) -> Poll<(), io::Error> {
        while self.write_pos > 0 {
            try_ready!(self.stream.poll_write());
            let m = try!((&*self.stream).write(&self.write_buf[0..self.write_pos]));
            self.write_pos -= m;
        }
        return Ok(Async::Ready(()));
    }
}

struct Pipe<H: PacketHandler + 'static> {
    client_reader: ConnReader,
    client_writer: ConnWriter,
    server_reader: ConnReader,
    server_writer: ConnWriter,
    handler: H,
}

impl<H> Pipe<H> where H: PacketHandler + 'static {
    fn new(client: Rc<TcpStream>,
           server: Rc<TcpStream>,
           handler: H
           ) -> Pipe<H> {

        Pipe {
            client_reader: ConnReader::new(client.clone()),
            client_writer: ConnWriter::new(client),
            server_reader: ConnReader::new(server.clone()),
            server_writer: ConnWriter::new(server),
            handler: handler,
        }
    }
}

impl<H> Future for Pipe<H> where H: PacketHandler + 'static {
    type Item = (u64, u64);
    type Error = Error;

    fn poll(&mut self) -> Poll<(u64, u64), io::Error> {
        loop {
            // try reading from client
            let client_read = match self.client_reader.read() {
                Ok(Async::Ready(None)) => Ok(Async::Ready(())),
                Ok(Async::Ready(Some(p))) => {
                    match self.handler.handle_request(&p) {
                        Action::Forward => self.server_writer.push(&p),
                        Action::Mutate(ref p2) => self.server_writer.push(p2),
                        Action::Respond(ref v) => {
                            for p in v {
                                self.client_writer.push(&p);
                            }
                        }
                    };
                    Ok(Async::Ready(()))
                },
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(e) => {
                    //try!(self.server_writer.stream.shutdown(Shutdown::Write));
                    Err(e)
                }
            };

            // try writing to server
            let server_write = self.server_writer.write();

            // try reading from server
            let server_read = match self.server_reader.read() {
                Ok(Async::Ready(None)) => Ok(Async::Ready(())),
                Ok(Async::Ready(Some(p))) => {
                    match self.handler.handle_request(&p) {
                        Action::Forward => self.server_writer.push(&p),
                        Action::Mutate(ref p2) => self.server_writer.push(p2),
                        Action::Respond(ref v) => {
                            for p in v {
                                self.client_writer.push(&p);
                            }
                        }
                    };
                    Ok(Async::Ready(()))
                },
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(e) => {
                    //try!(self.client_writer.stream.shutdown(Shutdown::Write));
                    Err(e)
                }
            };

            // try writing to client
            let client_write = self.client_writer.write();

            try_ready!(client_read);
            try_ready!(client_write);
            try_ready!(server_read);
            try_ready!(server_write);
        }
    }

}