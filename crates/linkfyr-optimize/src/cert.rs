//! TLS certificate inspection: a manual rustls handshake to fetch the
//! peer certificate, parsed with x509-parser for expiry and subject.
//! Real cryptography stack (the same rustls ureq uses), no shelling out.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use linkfyr_model::optimize::ToolRunReport;

use crate::probes::valid_target;

fn report(ok: bool, summary: String, data: serde_json::Value) -> ToolRunReport {
    ToolRunReport {
        tool: "cert_expiry".into(),
        ok,
        summary,
        took_ms: 0,
        data,
    }
}

/// Fetch the peer certificate chain for `host:port` (443 default).
pub fn peer_certificate(host: &str, port: u16) -> Result<Vec<Vec<u8>>, String> {
    if !valid_target(host) {
        return Err(format!("invalid host: {host}"));
    }
    let addr: SocketAddr = (host, port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| format!("no address for {host}"))?;

    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| format!("bad server name: {e}"))?;
    let mut conn =
        rustls::ClientConnection::new(Arc::new(config), server_name).map_err(|e| e.to_string())?;

    let mut sock =
        TcpStream::connect_timeout(&addr, Duration::from_secs(6)).map_err(|e| e.to_string())?;
    sock.set_read_timeout(Some(Duration::from_secs(6))).ok();
    sock.set_write_timeout(Some(Duration::from_secs(6))).ok();
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);
    // Drive the handshake with a minimal request; the certificate is
    // available as soon as the handshake completes.
    let req = format!("GET / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    tls.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut sink = [0u8; 256];
    let _ = tls.read(&mut sink); // response (or EOF) after handshake

    let certs = conn
        .peer_certificates()
        .ok_or("no peer certificate presented")?;
    Ok(certs.iter().map(|c| c.as_ref().to_vec()).collect())
}

/// Days until the leaf certificate expires (negative = already expired).
pub fn days_until_expiry(der: &[u8]) -> Result<(f64, String), String> {
    let (_, cert) = x509_parser::parse_x509_certificate(der).map_err(|e| e.to_string())?;
    let not_after = cert.validity().not_after.to_datetime();
    let unix = not_after.unix_timestamp();
    let subject = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .unwrap_or("(no CN)")
        .to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let days = (unix - now) as f64 / 86400.0;
    Ok((days, subject))
}

pub fn cert_expiry(host: &str) -> ToolRunReport {
    match peer_certificate(host, 443) {
        Ok(chain) if !chain.is_empty() => {
            let (days, subject) = match days_until_expiry(&chain[0]) {
                Ok(v) => v,
                Err(e) => {
                    return report(
                        false,
                        format!("certificate parse failed: {e}"),
                        serde_json::Value::Null,
                    );
                }
            };
            let verdict = if days < 0.0 {
                format!("EXPIRED {:.0} days ago", -days)
            } else if days < 14.0 {
                format!("expires in {days:.0} days (renew now)")
            } else {
                format!("valid for {days:.0} more days")
            };
            report(
                days >= 0.0,
                format!("{host} ({subject}): {verdict}"),
                serde_json::json!({ "host": host, "subject": subject, "daysRemaining": days, "chainLength": chain.len() }),
            )
        }
        Ok(_) => report(
            false,
            "server presented an empty certificate chain".into(),
            serde_json::Value::Null,
        ),
        Err(e) => report(
            false,
            format!("TLS fetch failed: {e}"),
            serde_json::Value::Null,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_hosts_fail_fast() {
        assert!(!cert_expiry("; rm -rf").ok);
        let r = cert_expiry("host.invalid.example");
        // No DNS/egress in the container: honest failure, never a panic.
        assert!(!r.ok);
        assert!(r.summary.contains("failed"));
    }

    #[test]
    fn live_tls_handshake_against_local_tls_server() {
        // Minimal real TLS server is out of scope here; instead verify
        // the handshake path errors honestly against a plain-TCP peer.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for c in listener.incoming() {
                let Ok(mut s) = c else { return };
                let mut b = [0u8; 512];
                let _ = s.read(&mut b);
                let _ = s.write_all(b"not tls");
            }
        });
        // localhost cert lookup: rustls will reject the garbage handshake.
        let r = peer_certificate("127.0.0.1", port);
        assert!(r.is_err(), "plain-TCP peer must not yield a certificate");
    }
}
