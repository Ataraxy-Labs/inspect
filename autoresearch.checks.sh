#!/bin/bash
set -e
cd "$(dirname "$0")"
cargo test -p inspect-core
cargo build --release -p inspect-cli
