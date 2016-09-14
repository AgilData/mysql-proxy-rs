# mysql-proxy-rs

[![CircleCI](https://circleci.com/gh/AgilData/mysql-proxy-rs/tree/master.svg?style=svg)](https://circleci.com/gh/AgilData/mysql-proxy-rs/tree/master)
[![Version](https://img.shields.io/crates/v/mysql-proxy.svg)](https://crates.io/crates/mysql-proxy)
[![Docs](https://docs.rs/mysql-proxy/badge.svg)](https://docs.rs/mysql-proxy)

An implementation of a MySQL proxy server built on top of `tokio-core`. 

## Overview

This crate provides a MySQL proxy server that you can extend with your own custom logic. Here are some examples of use cases for a proxy:

- Capture a log of SQL queries issued by an application
- Profiling of query execution time
- Monitor query patterns e.g. threat detection
- Record SQL traffic for later playback for automated testing

## Usage

The proxy defines the following trait for defining a packet handler for handling request and response packets:

```rust
pub trait PacketHandler {
    fn handle_request(&self, p: &Packet) -> Action;
    fn handle_response(&self, p: &Packet) -> Action;
}

/// Handlers return a variant of this enum to indicate how the proxy should handle the packet.
pub enum Action {
    /// drop the packet
    Drop,
    /// forward the packet unmodified
    Forward,
    /// forward a mutated packet
    Mutate(Packet),
    /// respond to the packet without forwarding
    Respond(Vec<Packet>),
    /// respond with an error packet
    Error { code: u16, state: [u8; 5], msg: String },
}

```

## Example

The example proxy passes all queries to MySQL except for queries containing the word 'avocado'. Use the following command to run the example.

Note that we have tested the proxy with `rustc 1.13.0-nightly (cbe4de78e 2016-09-05)`.

```
$ cargo run --example proxy
```

Then in a separate window you can test out the proxy. The proxy binds to port 3307 and assumes that a MySQL server is running on port 3306.

```
$ mysql -u root -p -h 127.0.0.1 -P 3307
```

```sql
mysql> select 'banana';
+--------+
| banana |
+--------+
| banana |
+--------+
1 row in set (0.00 sec)

mysql> select 'avocado';
ERROR 1064 (12345): Proxy rejecting any avocado-related queries
```

# License

`mysql-proxy-rs` is  distributed under the terms of the Apache License (Version 2.0).

See LICENSE-APACHE for details.
