//! MySQL Proxy Server
extern crate mysql_proxy;

use mysql_proxy::*;

fn main() {
    Proxy::run();
}
