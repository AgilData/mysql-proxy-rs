# mysql-proxy-rs

[![Version](https://img.shields.io/crates/v/mysql-proxy.svg)](https://crates.io/crates/mysql-proxy)
[![Docs](https://docs.rs/mysql-proxy/badge.svg)](https://docs.rs/mysql-proxy)

An implementation of a MySQL proxy server built on top of `tokio-core`. Tested with `rustc 1.13.0-nightly (cbe4de78e 2016-09-05)`.

## Usage

The proxy uses the following interface for defining a handler for handling request and response packets:

```rust
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

pub trait PacketHandler {
    fn handle_request(&self, p: &Packet) -> Action;
    fn handle_response(&self, p: &Packet) -> Action;
}
```

This allows the proxy to forward packets unmodified, mutate individual packets, or take over handling of a packet completely.

## Example

The example proxy passes all queries to MySQL except for queries containing the word 'avocado'.

```
$ cargo run --example proxy
   ...
Listening for MySQL proxy connections on 127.0.0.1:3307
```

Then in a separate window you can test out the proxy. The proxy currently assumes that a MySQL server is running on port 3306.

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
