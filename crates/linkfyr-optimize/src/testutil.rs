//! Test-only helpers: a real local HTTP server (plain TCP) so the
//! bloat/speedtest engines are exercised over genuine sockets in Docker
//! without external network access.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::thread::JoinHandle;

/// A dumb but real HTTP/1.1 server: GET returns an unbounded byte
/// stream, POST discards its body, HEAD returns headers only.
pub fn spawn_http_server() -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local http");
    let addr = listener.local_addr().expect("addr");
    let handle = std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(stream) = conn else { continue };
            std::thread::spawn(move || handle_conn(stream));
        }
    });
    (addr, handle)
}

fn handle_conn(mut stream: std::net::TcpStream) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    // Read until end of headers.
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let first_line = head.lines().next().unwrap_or_default().to_string();
    let content_length: usize = head
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse().ok())?
        })
        .unwrap_or(0);

    let is_get = first_line.starts_with("GET");
    let is_head = first_line.starts_with("HEAD");

    if !is_get && !is_head {
        // Drain the request body (some may already be in the header read).
        let body_already = buf.len() - head_end(&buf);
        let mut remaining = content_length.saturating_sub(body_already);
        while remaining > 0 {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => remaining = remaining.saturating_sub(n),
                Err(_) => return,
            }
        }
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        return;
    }

    let headers = "HTTP/1.1 200 OK\r\nContent-Length: 1000000000\r\nConnection: close\r\n\r\n";
    if stream.write_all(headers.as_bytes()).is_err() {
        return;
    }
    if is_head {
        return;
    }
    // Stream until the client goes away.
    let payload = vec![b'x'; 65536];
    loop {
        if stream.write_all(&payload).is_err() {
            return;
        }
    }
}

fn head_end(buf: &[u8]) -> usize {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map_or(buf.len(), |p| p + 4)
}

/// A TCP listener that accepts and holds connections: a probe target
/// that always accepts instantly (connect-RTT measurement endpoint).
pub fn spawn_sink_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind sink");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            if conn.is_err() {
                return;
            }
            // Hold the connection open; never read.
            std::mem::forget(conn);
        }
    });
    addr
}
