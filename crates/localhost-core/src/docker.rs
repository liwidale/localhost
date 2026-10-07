use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;

use crate::logs::{LogBuffer, Stream};

const TIMEOUT: Duration = Duration::from_secs(4);

trait Conn: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Conn for T {}

#[derive(Clone, Debug)]
enum Endpoint {
    #[cfg(unix)]
    Unix(PathBuf),
    #[cfg(windows)]
    Pipe(String),
}

#[derive(Clone, Debug)]
pub struct Docker {
    endpoint: Endpoint,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Container {
    pub id: String,
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub ports: Vec<ContainerPort>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub created: u64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct ContainerPort {
    #[serde(rename = "IP", default)]
    pub ip: Option<String>,
    #[serde(rename = "PrivatePort")]
    pub private: u16,
    #[serde(rename = "PublicPort", default)]
    pub public: Option<u16>,
    #[serde(rename = "Type", default)]
    pub kind: String,
}

impl Container {
    pub fn name(&self) -> String {
        self.names
            .first()
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_else(|| self.id.chars().take(12).collect())
    }

    pub fn label(&self, key: &str) -> Option<&str> {
        self.labels.get(key).map(String::as_str)
    }

    pub fn compose_dir(&self) -> Option<PathBuf> {
        self.label("com.docker.compose.project.working_dir")
            .map(PathBuf::from)
    }

    pub fn compose_service(&self) -> Option<&str> {
        self.label("com.docker.compose.service")
    }

    pub fn published(&self) -> Vec<(String, u16)> {
        let mut out: Vec<(String, u16)> = self
            .ports
            .iter()
            .filter(|p| p.kind == "tcp")
            .filter_map(|p| Some((p.ip.clone().unwrap_or_else(|| "0.0.0.0".into()), p.public?)))
            .collect();
        out.sort_by_key(|(ip, port)| (*port, ip.contains(':')));
        out.dedup_by_key(|(_, port)| *port);
        out
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Inspect {
    config: InspectConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectConfig {
    #[serde(default)]
    env: Option<Vec<String>>,
}

impl Docker {
    pub fn detect() -> Option<Self> {
        if let Ok(host) = std::env::var("DOCKER_HOST") {
            #[cfg(unix)]
            if let Some(path) = host.strip_prefix("unix://") {
                return Some(Self {
                    endpoint: Endpoint::Unix(PathBuf::from(path)),
                });
            }
            #[cfg(windows)]
            if let Some(path) = host.strip_prefix("npipe://") {
                return Some(Self {
                    endpoint: Endpoint::Pipe(path.replace('/', "\\")),
                });
            }
        }
        #[cfg(unix)]
        {
            let mut candidates = vec![PathBuf::from("/var/run/docker.sock")];
            if let Some(home) = std::env::home_dir() {
                candidates.push(home.join(".docker/run/docker.sock"));
                candidates.push(home.join(".orbstack/run/docker.sock"));
                candidates.push(home.join(".colima/default/docker.sock"));
                candidates.push(home.join(".rd/docker.sock"));
            }
            if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
                candidates.push(PathBuf::from(runtime).join("docker.sock"));
            }
            candidates.into_iter().find(|p| p.exists()).map(|p| Self {
                endpoint: Endpoint::Unix(p),
            })
        }
        #[cfg(windows)]
        {
            let pipe = r"\\.\pipe\docker_engine";
            std::path::Path::new(pipe).exists().then(|| Self {
                endpoint: Endpoint::Pipe(pipe.to_string()),
            })
        }
        #[cfg(not(any(unix, windows)))]
        None
    }

    async fn connect(&self) -> io::Result<Box<dyn Conn>> {
        match &self.endpoint {
            #[cfg(unix)]
            Endpoint::Unix(path) => Ok(Box::new(tokio::net::UnixStream::connect(path).await?)),
            #[cfg(windows)]
            Endpoint::Pipe(name) => {
                use tokio::net::windows::named_pipe::ClientOptions;
                for _ in 0..10 {
                    match ClientOptions::new().open(name) {
                        Ok(client) => return Ok(Box::new(client)),
                        Err(e) if e.raw_os_error() == Some(231) => {
                            tokio::time::sleep(Duration::from_millis(50)).await
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(io::Error::new(io::ErrorKind::TimedOut, "pipe busy"))
            }
        }
    }

    async fn open(&self, method: &str, path: &str) -> io::Result<Box<dyn Conn>> {
        let mut conn = self.connect().await?;
        let request = format!(
            "{method} {path} HTTP/1.0\r\nHost: docker\r\nUser-Agent: Localhost\r\nContent-Length: 0\r\n\r\n"
        );
        conn.write_all(request.as_bytes()).await?;
        Ok(conn)
    }

    async fn request(&self, method: &str, path: &str) -> io::Result<(u16, Vec<u8>)> {
        let fut = async {
            let mut conn = self.open(method, path).await?;
            let mut raw = Vec::new();
            conn.read_to_end(&mut raw).await?;
            split_response(&raw)
                .map(|(status, body)| (status, body.to_vec()))
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad response"))
        };
        timeout(TIMEOUT, fut)
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "docker timeout"))?
    }

    pub async fn containers(&self) -> io::Result<Vec<Container>> {
        let (status, body) = self.request("GET", "/containers/json").await?;
        if status != 200 {
            return Err(io::Error::other(format!("status {status}")));
        }
        serde_json::from_slice(&body).map_err(io::Error::other)
    }

    pub async fn action(&self, id: &str, action: &str) -> io::Result<()> {
        if !valid_id(id) || !matches!(action, "start" | "stop" | "restart" | "kill") {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid"));
        }
        let fut = async {
            let mut conn = self
                .open("POST", &format!("/containers/{id}/{action}"))
                .await?;
            let mut raw = Vec::new();
            conn.read_to_end(&mut raw).await?;
            Ok::<_, io::Error>(split_response(&raw).map(|(s, _)| s))
        };
        match timeout(Duration::from_secs(30), fut).await {
            Ok(Ok(Some(204 | 304))) => Ok(()),
            Ok(Ok(_)) => Err(io::Error::other("docker refused")),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "docker timeout")),
        }
    }

    pub async fn environment(&self, id: &str) -> io::Result<Vec<(String, String)>> {
        if !valid_id(id) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid"));
        }
        let (status, body) = self
            .request("GET", &format!("/containers/{id}/json"))
            .await?;
        if status != 200 {
            return Err(io::Error::other(format!("status {status}")));
        }
        let inspect: Inspect = serde_json::from_slice(&body).map_err(io::Error::other)?;
        Ok(inspect
            .config
            .env
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| {
                let (k, v) = e.split_once('=')?;
                Some((k.to_string(), v.to_string()))
            })
            .collect())
    }

    pub async fn follow_logs(&self, id: &str, buffer: Arc<LogBuffer>) -> io::Result<()> {
        if !valid_id(id) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid"));
        }
        let path =
            format!("/containers/{id}/logs?follow=1&stdout=1&stderr=1&tail=500&timestamps=1");
        let mut conn = timeout(TIMEOUT, self.open("GET", &path))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "docker timeout"))??;
        let mut pending = Vec::new();
        let mut header_done = false;
        let mut demux = Demux::default();
        let mut chunk = vec![0u8; 16 * 1024];
        loop {
            let n = conn.read(&mut chunk).await?;
            if n == 0 {
                return Ok(());
            }
            pending.extend_from_slice(&chunk[..n]);
            if !header_done {
                let Some(end) = find(&pending, b"\r\n\r\n") else {
                    continue;
                };
                pending.drain(..end + 4);
                header_done = true;
            }
            for (stream, line) in demux.feed(&mut pending) {
                let (ts, text) = split_timestamp(&line);
                buffer.push_at(ts, stream, text);
            }
        }
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn split_response(raw: &[u8]) -> Option<(u16, &[u8])> {
    let end = find(raw, b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..end]).ok()?;
    let status = head.split_whitespace().nth(1)?.parse().ok()?;
    Some((status, &raw[end + 4..]))
}

#[derive(Default)]
struct Demux {
    raw: Option<bool>,
    out: Vec<u8>,
    err: Vec<u8>,
}

impl Demux {
    fn feed(&mut self, pending: &mut Vec<u8>) -> Vec<(Stream, String)> {
        let raw = *self.raw.get_or_insert_with(|| {
            !(pending.len() >= 8 && pending[0] <= 2 && pending[1..4] == [0, 0, 0])
        });
        if raw {
            self.out.append(pending);
        } else {
            while pending.len() >= 8 {
                let size =
                    u32::from_be_bytes([pending[4], pending[5], pending[6], pending[7]]) as usize;
                if pending.len() < 8 + size {
                    break;
                }
                let target = if pending[0] == 2 {
                    &mut self.err
                } else {
                    &mut self.out
                };
                target.extend_from_slice(&pending[8..8 + size]);
                pending.drain(..8 + size);
            }
        }
        let mut lines = Vec::new();
        for (stream, buf) in [(Stream::Out, &mut self.out), (Stream::Err, &mut self.err)] {
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                lines.push((stream, String::from_utf8_lossy(&line).into_owned()));
            }
        }
        lines
    }
}

fn split_timestamp(line: &str) -> (Option<u64>, &str) {
    match line.split_once(' ') {
        Some((ts, rest)) => match parse_rfc3339(ts) {
            Some(ms) => (Some(ms), rest),
            None => (None, line),
        },
        None => (None, line),
    }
}

fn parse_rfc3339(ts: &str) -> Option<u64> {
    let b = ts.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[10] != b'T' || !ts.ends_with('Z') {
        return None;
    }
    let num = |r: std::ops::Range<usize>| ts.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hh, mm, ss) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let frac = ts[19..ts.len() - 1].trim_start_matches('.');
    let digits: String = frac.chars().chain("000".chars()).take(3).collect();
    let ms = digits.parse::<i64>().unwrap_or(0);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + hh * 3600 + mm * 60 + ss;
    u64::try_from(secs * 1000 + ms).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_timestamps() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:01.5Z"), Some(1500));
        assert_eq!(
            parse_rfc3339("2024-02-29T12:00:00.123456789Z"),
            Some(1_709_208_000_123)
        );
        assert_eq!(parse_rfc3339("nope"), None);
    }

    #[test]
    fn demultiplexes_frames() {
        let mut pending = Vec::new();
        for (kind, text) in [(1u8, "hello\n"), (2u8, "oops\n")] {
            pending.extend_from_slice(&[kind, 0, 0, 0]);
            pending.extend_from_slice(&(text.len() as u32).to_be_bytes());
            pending.extend_from_slice(text.as_bytes());
        }
        let mut demux = Demux::default();
        let lines = demux.feed(&mut pending);
        assert_eq!(
            lines,
            vec![
                (Stream::Out, "hello\n".into()),
                (Stream::Err, "oops\n".into())
            ]
        );
    }

    #[test]
    fn reads_container_list() {
        let json = r#"[{"Id":"abc123","Names":["/shop-db-1"],"Image":"postgres:16","State":"running",
            "Ports":[{"IP":"0.0.0.0","PrivatePort":5432,"PublicPort":5432,"Type":"tcp"},
                     {"IP":"::","PrivatePort":5432,"PublicPort":5432,"Type":"tcp"}],
            "Labels":{"com.docker.compose.service":"db","com.docker.compose.project.working_dir":"/code/shop"},
            "Created":1700000000}]"#;
        let list: Vec<Container> = serde_json::from_str(json).unwrap();
        let c = &list[0];
        assert_eq!(c.name(), "shop-db-1");
        assert_eq!(c.compose_service(), Some("db"));
        assert_eq!(c.published(), vec![("0.0.0.0".to_string(), 5432)]);
        assert!(valid_id(&c.id));
        assert!(!valid_id("../etc"));
    }
}
