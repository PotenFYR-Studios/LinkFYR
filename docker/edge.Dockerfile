# LinkFYR Edge node (self-hosted Fusion relay).
# Run: docker run -p 7443:7443 linkfyr/edge
FROM rust:1-slim-bookworm AS builder
WORKDIR /build
COPY . .
RUN cargo build --release -p linkfyr-edge -p linkfyr-daemon

FROM debian:bookworm-slim
COPY --from=builder /build/target/release/linkfyrd /usr/local/bin/
COPY --from=builder /build/target/release/linkfyr-edge /usr/local/bin/ 2>/dev/null || true
EXPOSE 7443 58008
CMD ["linkfyrd"]
