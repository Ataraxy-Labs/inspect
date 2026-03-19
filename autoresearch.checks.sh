#!/bin/bash
set -e
cd "$(dirname "$0")"

# Always compile inspect first so test failures aren't hiding build breakage.
cargo build --release -p inspect-cli
cargo test -p inspect-core
