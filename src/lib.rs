//! MySQL Proxy Server

#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate futures;
#[macro_use]
extern crate tokio_core;
extern crate futures_cpupool;
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
use futures_cpupool::CpuPool;
use tokio_core::{Loop, LoopHandle, TcpStream};
use tokio_core::io::{read_exact, write_all, Window};

use byteorder::*;

#[derive(Copy,Clone)]
pub enum PacketType {
    ComSleep = 0x00,
    ComQuit = 0x01,
    ComInitDb = 0x02,
    ComQuery = 0x03,
    ComFieldList = 0x04,
    ComCreateDb = 0x05,
    ComDropDb = 0x06,
    ComRefresh = 0x07,
    ComShutdown = 0x08,
    ComStatistics = 0x09,
    ComProcessInfo = 0x0a,
    ComConnect = 0x0b,
    ComProcessKill= 0x0c,
    ComDebug = 0x0d,
    ComPing = 0x0e,
    ComTime = 0x0f,
    ComDelayedInsert = 0x10,
    ComChangeUser = 0x11,
    ComBinlogDump = 0x12,
    ComTableDump = 0x13,
    ComConnectOut = 0x14,
    ComRegisterSlave = 0x15,
    ComStmtPrepare = 0x16,
    ComStmtExecute = 0x17,
    ComStmtSendLongData = 0x18,
    ComStmtClose = 0x19,
    ComStmtReset = 0x1a,
    ComDaemon= 0x1d,
    ComBinlogDumpGtid = 0x1e,
    ComResetConnection = 0x1f,
}

pub struct Packet {
    pub bytes: Vec<u8>
}

impl Packet {

    pub fn error_packet(code: u16, state: [u8; 5], msg: String) -> Self {

        // start building payload
        let mut payload: Vec<u8> = Vec::with_capacity(9 + msg.len());
        payload.push(0xff);  // packet type
        payload.write_u16::<LittleEndian>(code).unwrap(); // error code
        payload.extend_from_slice("#".as_bytes()); // sql_state_marker
        payload.extend_from_slice(&state); // SQL STATE
        payload.extend_from_slice(msg.as_bytes());

        // create header with length and sequence id
        let mut header: Vec<u8> = Vec::with_capacity(4 + 9 + msg.len());
        header.write_u32::<LittleEndian>(payload.len() as u32).unwrap();
        header.pop(); // we need 3 byte length, so discard last byte
        header.push(1); // sequence_id

        // combine the vectors
        header.extend_from_slice(&payload);

        // now move the vector into the packet
        Packet { bytes: header }
    }

    pub fn sequence_id(&self) -> u8 {
        self.bytes[3]
    }

    //TODO: should return Result<PacketType, ?>
    pub fn packet_type(&self) -> PacketType {
        match self.bytes[4] {
            0x00 => PacketType::ComSleep,
            0x01 => PacketType::ComQuit,
            0x02 => PacketType::ComInitDb,
            0x03 => PacketType::ComQuery,
            0x04 => PacketType::ComFieldList,
            0x05 => PacketType::ComCreateDb,
            0x06 => PacketType::ComDropDb,
            0x07 => PacketType::ComRefresh,
            0x08 => PacketType::ComShutdown,
            0x09 => PacketType::ComStatistics,
            0x0a => PacketType::ComProcessInfo,
            0x0b => PacketType::ComConnect,
            0x0c => PacketType::ComProcessKill,
            0x0d => PacketType::ComDebug,
            0x0e => PacketType::ComPing,
            0x0f => PacketType::ComTime,
            0x10 => PacketType::ComDelayedInsert,
            0x11 => PacketType::ComChangeUser,
            0x12 => PacketType::ComBinlogDump,
            0x13 => PacketType::ComTableDump,
            0x14 => PacketType::ComConnectOut,
            0x15 => PacketType::ComRegisterSlave,
            0x16 => PacketType::ComStmtPrepare,
            0x17 => PacketType::ComStmtExecute,
            0x18 => PacketType::ComStmtSendLongData,
            0x19 => PacketType::ComStmtClose,
            0x1a => PacketType::ComStmtReset,
            0x1d => PacketType::ComDaemon,
            0x1e => PacketType::ComBinlogDumpGtid,
            0x1f => PacketType::ComResetConnection,
            _ => panic!("Unsupported packet type")
        }
    }

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

        let pair = connected.and_then(move |server| Ok((conn,server)));

        Box::new(pair.and_then(move |(c1, c2)| {
            let c1 = Rc::new(c1);
            let c2 = Rc::new(c2);
            Pipe::new(c1, c2, handler)
        }))
    }
}

#[derive(Debug)]
enum Direction {
    Client,
    Server
}

struct ConnReader {
    direction: Direction,
    stream: Rc<TcpStream>,
    read_buf: Vec<u8>,
    read_pos: usize,
    read_amt: u64,
}

struct ConnWriter {
    direction: Direction,
    stream: Rc<TcpStream>,
    write_buf: Vec<u8>,
    write_pos: usize,
    write_amt: u64,
}

impl ConnReader {

    fn new(stream: Rc<TcpStream>, direction: Direction) -> Self {
        ConnReader {
            stream: stream,
            direction: direction,
            read_buf: vec![0u8; 4096],
            read_pos: 0,
            read_amt: 0,
        }
    }

    /// Read from the socket until the status is NotReady
    fn read(&mut self) -> Poll<(), io::Error> {
        loop {
            try_ready!(self.stream.poll_read());
            //TODO: ensure capacity first
            let n = try_nb!((&*self.stream).read(&mut self.read_buf[self.read_pos..]));
            if n == 0 {
                return Err(Error::new(ErrorKind::Other, "connection closed"));
            }
            self.read_amt += n as u64;
            self.read_pos += n;
        }
    }

    fn next(&mut self) -> Option<Packet> {
        // do we have a header
        if self.read_pos > 3 {
            let l = parse_packet_length(&self.read_buf);
            // do we have the whole packet?
            let s = 4 + l;
            if self.read_pos >= s {
                let mut temp : Vec<u8> = Vec::with_capacity(s);
                temp.extend_from_slice(&self.read_buf[0..s]);
                let p = Packet { bytes: temp };

                // shift data down
                let mut j = 0;
                for i in s .. self.read_pos {
                    self.read_buf[j] = self.read_buf[i];
                    j += 1;
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

impl ConnWriter {

    fn new(stream: Rc<TcpStream>, direction: Direction) -> Self {
        ConnWriter{
            stream: stream,
            direction: direction,
            write_buf: vec![0u8; 4096],
            write_pos: 0,
            write_amt: 0
        }
    }

    /// Write a packet to the write buffer
    fn push(&mut self, p: &Packet) {
        for i in 0 .. p.bytes.len() {
            self.write_buf[self.write_pos + i] = p.bytes[i];
        }
        self.write_pos += p.bytes.len();
    }

    /// Writes the contents of the write buffer to the socket
    fn write(&mut self) -> Poll<(), io::Error> {
        while self.write_pos > 0 {
            try_ready!(self.stream.poll_write());
            let s = try!((&*self.stream).write(&self.write_buf[0..self.write_pos]));

            let mut j = 0;
            for i in s .. self.write_pos {
                self.write_buf[j] = self.write_buf[i];
                j += 1;
            }
            self.write_pos -= s;

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
            client_reader: ConnReader::new(client.clone(), Direction::Client),
            client_writer: ConnWriter::new(client, Direction::Client),
            server_reader: ConnReader::new(server.clone(), Direction::Server),
            server_writer: ConnWriter::new(server, Direction::Server),
            handler: handler,
        }
    }
}

impl<H> Future for Pipe<H> where H: PacketHandler + 'static {
    type Item = (u64, u64);
    type Error = Error;

    fn poll(&mut self) -> Poll<(u64, u64), io::Error> {
        loop {
            let client_read = self.client_reader.read();

            // if the client connection has closed, close the server connection too
            match &client_read {
                &Err(ref e) => { self.server_writer.stream.shutdown(Shutdown::Write).unwrap(); },
                _ => {}
            }

            // process buffered requests
            while let Some(request) = self.client_reader.next() {
                match self.handler.handle_request(&request) {
                    Action::Forward => self.server_writer.push(&request),
                    Action::Mutate(ref p2) => self.server_writer.push(p2),
                    Action::Respond(ref v) => {
                        for p in v {
                            self.client_writer.push(&p);
                        }
                    }
                };
            }

            // try writing to server
            let server_write = self.server_writer.write();

            // try reading from server
            let server_read = self.server_reader.read();

            // if the server connection has closed, close the client connection too
            match &server_read {
                &Err(ref e) => { self.client_writer.stream.shutdown(Shutdown::Write).unwrap(); },
                _ => {}
            }

            // process buffered responses
            while let Some(response) = self.server_reader.next() {
                match self.handler.handle_response(&response) {
                    Action::Forward => self.client_writer.push(&response),
                    Action::Mutate(ref p2) => self.client_writer.push(p2),
                    Action::Respond(ref v) => {
                        for p in v {
                            self.server_writer.push(&p);
                        }
                    }
                };
            }

            // try writing to client
            let client_write = self.client_writer.write();

            try_ready!(client_read);
            try_ready!(client_write);
            try_ready!(server_read);
            try_ready!(server_write);
        }

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

