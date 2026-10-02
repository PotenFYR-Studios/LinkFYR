# LinkFYR verification image: runs the complete Rust quality gate.
# Tracks the latest stable Rust, matching CI (dtolnay/rust-toolchain@stable)
# and the toolchain that generated Cargo.lock. The workspace MSRV stays
# 1.85; this image is the "full gate", not the MSRV check.
# Heavy system packages are only needed to compile the Tauri desktop
# crate (linkfyr-desktop) so `cargo test --workspace` matches CI exactly.
FROM rust:1-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        iputils-ping \
        iproute2 \
        procps \
        libwebkit2gtk-4.1-dev \
        libgtk-3-dev \
        libayatana-appindicator3-dev \
        librsvg2-dev \
        libxdo-dev \
        libssl-dev \
        mingw-w64 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add rustfmt clippy
# Windows cross-target so the cfg(windows) service code is compile-verified
# in Docker too (cargo check needs no linker).
RUN rustup target add x86_64-pc-windows-gnu

WORKDIR /app
COPY . .

CMD ["bash", "docker/scripts/rust-gate.sh"]
