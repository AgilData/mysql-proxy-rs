#!/bin/bash
#
# Build script to build

source ~/.cargo/env

echo
echo "Setting default to nightly build of Rust."
rustup default nightly
rustup override set nightly

echo
echo "Building Project."
RUST_BACKTRACE=1 cargo build
