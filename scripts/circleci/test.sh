#!/bin/bash
#
# Runs the test suite

source ~/.cargo/env
RUST_BACKTRACE=1 cargo test
