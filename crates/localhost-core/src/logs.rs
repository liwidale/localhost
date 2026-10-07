use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::broadcast;

use crate::redact;

const CAPACITY: usize = 5000;
const MAX_LINE: usize = 8 * 1024;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Stream {
    Out,
    Err,
    System,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub t: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub b: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub d: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub i: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub u: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub seq: u64,
    pub ts: u64,
    pub stream: Stream,
    pub level: Level,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spans: Option<Vec<Span>>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

pub struct LogBuffer {
    lines: Mutex<(u64, VecDeque<Arc<LogLine>>)>,
    tx: broadcast::Sender<Arc<LogLine>>,
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self {
            lines: Mutex::new((0, VecDeque::new())),
            tx: broadcast::channel(1024).0,
        }
    }
}

impl LogBuffer {
    pub fn push(&self, stream: Stream, raw: &str) -> Arc<LogLine> {
        self.push_at(None, stream, raw)
    }

    pub fn push_at(&self, ts: Option<u64>, stream: Stream, raw: &str) -> Arc<LogLine> {
        let raw = if raw.len() > MAX_LINE {
            &raw[..raw.floor_char_boundary(MAX_LINE)]
        } else {
            raw
        };
        let spans = parse_ansi(raw.trim_end_matches(['\r', '\n']));
        let text: String = spans.iter().map(|s| s.t.as_str()).collect();
        let text = redact::url_credentials(&text);
        let level = level(&text);
        let styled = spans.len() > 1 || spans.first().is_some_and(|s| s != &plain(&s.t));
        let mut guard = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        guard.0 += 1;
        let line = Arc::new(LogLine {
            seq: guard.0,
            ts: ts.unwrap_or_else(now_ms),
            stream,
            level,
            spans: (styled && !text.contains(redact::MASK)).then_some(spans),
            text,
        });
        if guard.1.len() == CAPACITY {
            guard.1.pop_front();
        }
        guard.1.push_back(line.clone());
        drop(guard);
        let _ = self.tx.send(line.clone());
        line
    }

    pub fn snapshot(&self) -> Vec<Arc<LogLine>> {
        let guard = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        guard.1.iter().cloned().collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<LogLine>> {
        self.tx.subscribe()
    }

    pub fn clear(&self) {
        self.lines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .1
            .clear();
    }
}

fn plain(t: &str) -> Span {
    Span {
        t: t.to_string(),
        ..Span::default()
    }
}

pub fn level(text: &str) -> Level {
    let lower = text.to_lowercase();
    let has = |words: &[&str]| {
        words.iter().any(|w| {
            lower.match_indices(w).any(|(i, _)| {
                let before = lower[..i].chars().next_back();
                let after = lower[i + w.len()..].chars().next();
                !before.is_some_and(char::is_alphanumeric)
                    && !after.is_some_and(char::is_alphanumeric)
            })
        })
    };
    if has(&[
        "error",
        "err",
        "fatal",
        "panic",
        "panicked",
        "exception",
        "failed",
        "eaddrinuse",
        "traceback",
    ]) {
        Level::Error
    } else if has(&["warn", "warning", "deprecated"]) {
        Level::Warn
    } else {
        Level::Info
    }
}

pub fn port_in_use(text: &str) -> Option<u16> {
    let lower = text.to_lowercase();
    let markers = [
        "eaddrinuse",
        "address already in use",
        "is already in use",
        "port is already allocated",
        "only one usage of each socket address",
        "is in use",
    ];
    if !markers.iter().any(|m| lower.contains(m)) {
        return None;
    }
    lower
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| (2..=5).contains(&s.len()))
        .filter_map(|s| s.parse::<u16>().ok())
        .rfind(|p| *p >= 80)
}

const BASIC: [&str; 16] = [
    "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9", "a10", "a11", "a12", "a13", "a14",
    "a15",
];

fn color256(n: u8) -> String {
    match n {
        0..=15 => BASIC[n as usize].to_string(),
        16..=231 => {
            let n = n - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            format!(
                "#{:02x}{:02x}{:02x}",
                level(n / 36),
                level((n / 6) % 6),
                level(n % 6)
            )
        }
        _ => {
            let v = 8 + (n - 232) * 10;
            format!("#{v:02x}{v:02x}{v:02x}")
        }
    }
}

pub fn parse_ansi(input: &str) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut style = Span::default();
    let mut text = String::new();
    let mut chars = input.chars().peekable();
    let flush = |spans: &mut Vec<Span>, text: &mut String, style: &Span| {
        if !text.is_empty() {
            spans.push(Span {
                t: std::mem::take(text),
                ..style.clone()
            });
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.peek() {
                Some('[') => {
                    chars.next();
                    let mut params = String::new();
                    let mut fin = None;
                    for p in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&p) {
                            fin = Some(p);
                            break;
                        }
                        params.push(p);
                    }
                    if fin == Some('m') {
                        flush(&mut spans, &mut text, &style);
                        apply_sgr(&mut style, &params);
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(p) = chars.next() {
                        if p == '\x07' || (p == '\x1b' && chars.peek() == Some(&'\\')) {
                            if p == '\x1b' {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            },
            '\r' => {
                text.clear();
                spans.clear();
            }
            '\t' => text.push_str("    "),
            c if c.is_control() => {}
            c => text.push(c),
        }
    }
    flush(&mut spans, &mut text, &style);
    if spans.is_empty() {
        spans.push(Span::default());
    }
    spans
}

fn apply_sgr(style: &mut Span, params: &str) {
    let codes: Vec<u16> = if params.is_empty() {
        vec![0]
    } else {
        params
            .split([';', ':'])
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let mut i = 0;
    while i < codes.len() {
        match codes[i] {
            0 => *style = Span::default(),
            1 => style.b = true,
            2 => style.d = true,
            3 => style.i = true,
            4 => style.u = true,
            22 => {
                style.b = false;
                style.d = false;
            }
            23 => style.i = false,
            24 => style.u = false,
            c @ 30..=37 => style.fg = Some(BASIC[(c - 30) as usize].into()),
            c @ 90..=97 => style.fg = Some(BASIC[(c - 90 + 8) as usize].into()),
            39 => style.fg = None,
            38 => match codes.get(i + 1) {
                Some(5) => {
                    if let Some(&n) = codes.get(i + 2) {
                        style.fg = Some(color256(n.min(255) as u8));
                    }
                    i += 2;
                }
                Some(2) => {
                    if let (Some(&r), Some(&g), Some(&b)) =
                        (codes.get(i + 2), codes.get(i + 3), codes.get(i + 4))
                    {
                        style.fg = Some(format!(
                            "#{:02x}{:02x}{:02x}",
                            r.min(255),
                            g.min(255),
                            b.min(255)
                        ));
                    }
                    i += 4;
                }
                _ => {}
            },
            48 => match codes.get(i + 1) {
                Some(5) => i += 2,
                Some(2) => i += 4,
                _ => {}
            },
            _ => {}
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colors() {
        let spans = parse_ansi("\x1b[32m✓\x1b[39m Ready in \x1b[1m1.2s\x1b[22m");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].fg.as_deref(), Some("a2"));
        assert_eq!(spans[1].fg, None);
        assert_eq!(spans[2].t, "1.2s");
        assert!(spans[2].b);
    }

    #[test]
    fn strips_cursor_sequences() {
        let spans = parse_ansi("\x1b[2K\x1b[1Gcompiling\x1b]8;;http://x\x07link\x1b]8;;\x07");
        let text: String = spans.iter().map(|s| s.t.as_str()).collect();
        assert_eq!(text, "compilinglink");
    }

    #[test]
    fn detects_levels() {
        assert_eq!(level("Error: listen EADDRINUSE"), Level::Error);
        assert_eq!(level("warn - something"), Level::Warn);
        assert_eq!(level("ready - started server"), Level::Info);
        assert_eq!(level("0 errors found"), Level::Info);
    }

    #[test]
    fn finds_conflicting_port() {
        assert_eq!(
            port_in_use("Error: listen EADDRINUSE: address already in use :::3000"),
            Some(3000)
        );
        assert_eq!(
            port_in_use("Port 5173 is in use, trying another one..."),
            Some(5173)
        );
        assert_eq!(
            port_in_use("Error: Address already in use (os error 98) on 127.0.0.1:8080"),
            Some(8080)
        );
        assert_eq!(port_in_use("Server started on 3000"), None);
    }

    #[test]
    fn buffer_is_bounded_and_redacts() {
        let buf = LogBuffer::default();
        for i in 0..CAPACITY + 10 {
            buf.push(Stream::Out, &format!("line {i}"));
        }
        let lines = buf.snapshot();
        assert_eq!(lines.len(), CAPACITY);
        assert_eq!(lines[0].text, "line 10");
        let l = buf.push(Stream::Out, "connecting to postgres://u:secret@db/x");
        assert!(!l.text.contains("secret"));
    }
}
