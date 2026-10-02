#!/usr/bin/env bash
# Full Rust gate, run inside docker/test.Dockerfile.
set -euo pipefail
cd /app

echo "== cargo fmt --check =="
cargo fmt --all -- --check

echo "== cargo clippy (deny warnings) =="
cargo clippy --workspace --all-targets -- -D warnings

echo "== windows cross-target check (service code) =="
cargo check -p linkfyr-daemon --target x86_64-pc-windows-gnu

echo "== cargo test --workspace =="
cargo test --workspace

echo "== smoke: CLI status (simulated) =="
cargo run --quiet -p linkfyr-cli -- --simulated status

echo "== smoke: registry list =="
# tail (not head): early-exiting readers SIGPIPE the CLI and fail the gate.
cargo run --quiet -p linkfyr-cli -- --simulated optimize list | tail -3

echo "RUST GATE PASSED"
