# mysql-proxy-rs

An implementation of a MySQL proxy server built on top of `tokio-core`.

## Usage

First, run the server

```
$ cargo run
   ...
Listening for MySQL proxy connections on 127.0.0.1:3307
```

Then in a separate window you can test out the proxy:

```
$ mysql -u root -p -h 127.0.0.1 -P 3307
```

# License

`mysql-proxy-rs` is  distributed under the terms of the Apache License (Version 2.0).

See LICENSE-APACHE for details.
