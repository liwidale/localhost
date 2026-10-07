use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::logs::now_ms;
use crate::model::{Health, HealthState, Protocol};

const CONNECT: Duration = Duration::from_millis(600);
const RESPONSE: Duration = Duration::from_millis(2500);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    pub health: Health,
    pub protocol: Protocol,
}

pub fn target(bound: IpAddr, port: u16) -> Option<SocketAddr> {
    let ip = match bound {
        IpAddr::V4(v4) if v4.is_unspecified() || v4.is_loopback() => {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        }
        IpAddr::V6(v6) if v6.is_unspecified() || v6.is_loopback() => {
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        }
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) if v4.is_loopback() || v4.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
            _ => return None,
        },
        IpAddr::V4(_) => return None,
    };
    Some(SocketAddr::new(ip, port))
}

pub async fn probe(addr: SocketAddr, protocol: Protocol) -> Probe {
    let started = Instant::now();
    let finish = |state, protocol, http_status| Probe {
        health: Health {
            state,
            http_status,
            latency_ms: Some(started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32),
            checked_at: Some(now_ms()),
        },
        protocol,
    };
    let Ok(Ok(mut stream)) = timeout(CONNECT, TcpStream::connect(addr)).await else {
        return finish(HealthState::Unreachable, protocol, None);
    };
    let _ = stream.set_nodelay(true);
    let request: Option<Vec<u8>> = match protocol {
        Protocol::Http | Protocol::Https => Some(
            format!(
                "HEAD / HTTP/1.1\r\nHost: localhost:{}\r\nUser-Agent: Localhost\r\nAccept: */*\r\nConnection: close\r\n\r\n",
                addr.port()
            )
            .into_bytes(),
        ),
        Protocol::Redis => Some(b"PING\r\n".to_vec()),
        Protocol::MySql | Protocol::Smtp => Some(Vec::new()),
        _ => None,
    };
    let Some(request) = request else {
        return finish(HealthState::Responding, protocol, None);
    };
    if !request.is_empty() && stream.write_all(&request).await.is_err() {
        return finish(HealthState::Error, protocol, None);
    }
    let mut buf = [0u8; 256];
    let read = match timeout(RESPONSE, stream.read(&mut buf)).await {
        Ok(Ok(n)) if n > 0 => n,
        Ok(Ok(_)) => return finish(HealthState::Listening, protocol, None),
        Ok(Err(_)) => return finish(HealthState::Error, protocol, None),
        Err(_) => return finish(HealthState::Unreachable, protocol, None),
    };
    let reply = &buf[..read];
    match protocol {
        Protocol::Http | Protocol::Https => {
            if reply.first() == Some(&0x15) || reply.starts_with(&[0x16, 0x03]) {
                return finish(HealthState::Responding, Protocol::Https, None);
            }
            match http_status(reply) {
                Some(code) if code >= 500 => finish(HealthState::Error, Protocol::Http, Some(code)),
                Some(400) if looks_like_tls_complaint(reply) => {
                    finish(HealthState::Responding, Protocol::Https, Some(400))
                }
                Some(code) => finish(HealthState::Responding, Protocol::Http, Some(code)),
                None => finish(HealthState::Responding, Protocol::Tcp, None),
            }
        }
        Protocol::Redis => {
            let ok = reply.starts_with(b"+PONG") || reply.first() == Some(&b'-');
            finish(
                if ok {
                    HealthState::Responding
                } else {
                    HealthState::Error
                },
                protocol,
                None,
            )
        }
        _ => finish(HealthState::Responding, protocol, None),
    }
}

fn http_status(reply: &[u8]) -> Option<u16> {
    let text = std::str::from_utf8(&reply[..reply.len().min(32)]).ok()?;
    let rest = text.strip_prefix("HTTP/")?;
    rest.split_whitespace().nth(1)?.parse().ok()
}

fn looks_like_tls_complaint(reply: &[u8]) -> bool {
    let text = String::from_utf8_lossy(reply).to_lowercase();
    text.contains("https") || text.contains("ssl") || text.contains("tls")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn maps_bind_addresses() {
        assert_eq!(
            target("0.0.0.0".parse().unwrap(), 80),
            Some("127.0.0.1:80".parse().unwrap())
        );
        assert_eq!(
            target("::".parse().unwrap(), 80),
            Some("[::1]:80".parse().unwrap())
        );
        assert_eq!(target("192.168.1.4".parse().unwrap(), 80), None);
    }

    #[test]
    fn parses_status() {
        assert_eq!(http_status(b"HTTP/1.1 404 Not Found\r\n"), Some(404));
        assert_eq!(http_status(b"SSH-2.0-OpenSSH"), None);
    }

    #[tokio::test]
    async fn probes_http_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 512];
            let _ = s.read(&mut buf).await;
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        let p = probe(addr, Protocol::Http).await;
        assert_eq!(p.health.state, HealthState::Responding);
        assert_eq!(p.health.http_status, Some(200));
        assert_eq!(p.protocol, Protocol::Http);
    }

    #[tokio::test]
    async fn reports_closed_port() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let p = probe(addr, Protocol::Http).await;
        assert_eq!(p.health.state, HealthState::Unreachable);
    }
}
