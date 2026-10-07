pub mod project;
pub mod signatures;

use std::collections::HashSet;

use crate::model::{Evidence, EvidenceKind, Identity, Kind, Protocol};
use crate::system::Proc;
use signatures::{FRAMEWORKS, SIGNATURES, Signature};

const THRESHOLD: f32 = 0.5;

pub fn signature(id: &str) -> Option<&'static Signature> {
    SIGNATURES.iter().find(|s| s.id == id)
}

pub fn runtime(proc: &Proc) -> Option<&'static str> {
    let stem = proc.stem();
    let path = proc
        .exe
        .as_deref()
        .map(|p| p.to_string_lossy().replace('\\', "/").to_lowercase())
        .unwrap_or_default();
    Some(match stem.as_str() {
        "node" | "nodejs" => "node",
        "bun" => "bun",
        "deno" => "deno",
        "ruby" | "bundle" | "rails" | "puma" => "ruby",
        "java" | "javaw" => "java",
        "dotnet" => "dotnet",
        "php" | "php-cgi" | "php-fpm" => "php",
        "beam.smp" | "erl" | "beam" => "erlang",
        "com.docker.backend" | "docker-proxy" | "vpnkit" | "com.docker.proxy" => "docker",
        "wslrelay" => "wsl",
        s if s.starts_with("python") || s == "py" || s == "pythonw" => "python",
        _ if path.contains("/go-build") => "go",
        _ if path.contains("/target/debug/") || path.contains("/target/release/") => "rust",
        _ => return None,
    })
}

pub fn tokens(proc: &Proc) -> HashSet<String> {
    let mut out = HashSet::new();
    out.insert(proc.stem());
    for arg in &proc.cmd {
        for word in arg.split_whitespace() {
            add_token(&mut out, word);
        }
    }
    out
}

fn add_token(out: &mut HashSet<String>, word: &str) {
    let word = word.trim_matches(['"', '\'']).to_lowercase();
    if word.is_empty() {
        return;
    }
    let normalized = word.replace('\\', "/");
    if let Some(idx) = normalized.rfind("/node_modules/") {
        let rest = &normalized[idx + "/node_modules/".len()..];
        let mut parts = rest.split('/');
        if let Some(first) = parts.next() {
            let pkg = if first.starts_with('@') {
                parts.next().map(|p| format!("{first}/{p}"))
            } else {
                Some(first.to_string())
            };
            if let Some(pkg) = pkg {
                out.insert(format!("pkg:{pkg}"));
            }
        }
    }
    let file = normalized.rsplit('/').next().unwrap_or(&normalized);
    for ext in [
        ".js", ".cjs", ".mjs", ".ts", ".py", ".exe", ".cmd", ".jar", ".rb",
    ] {
        if let Some(stem) = file.strip_suffix(ext) {
            out.insert(stem.to_string());
        }
    }
    out.insert(file.to_string());
    out.insert(word);
}

struct Match {
    sig: &'static Signature,
    score: f32,
    evidence: Vec<Evidence>,
}

fn score(
    sig: &'static Signature,
    stem: &str,
    tokens: &HashSet<String>,
    ports: &[u16],
) -> Option<Match> {
    let mut best = 0.0f32;
    let mut evidence = Vec::new();
    if sig.exe.contains(&stem) {
        best = 0.7;
        evidence.push(Evidence {
            kind: EvidenceKind::Executable,
            detail: stem.to_string(),
        });
    }
    let command = sig
        .args
        .iter()
        .filter(|pattern| pattern.split(' ').all(|p| tokens.contains(p)))
        .map(|pattern| {
            let weight = if pattern.contains(' ') || !pattern.starts_with("pkg:") {
                0.75
            } else {
                0.6
            };
            (weight, *pattern)
        })
        .fold(None, |acc: Option<(f32, &str)>, m| match acc {
            Some(a) if a.0 >= m.0 => Some(a),
            _ => Some(m),
        });
    if let Some((weight, pattern)) = command {
        best = best.max(weight);
        evidence.push(Evidence {
            kind: EvidenceKind::Command,
            detail: pattern.strip_prefix("pkg:").unwrap_or(pattern).to_string(),
        });
    }
    if best == 0.0 {
        return None;
    }
    if let Some(port) = ports.iter().find(|p| sig.ports.contains(p)) {
        best += 0.1;
        evidence.push(Evidence {
            kind: EvidenceKind::Port,
            detail: port.to_string(),
        });
    }
    Some(Match {
        sig,
        score: best.min(0.95),
        evidence,
    })
}

fn best_match(candidates: impl Iterator<Item = Match>) -> Option<Match> {
    candidates
        .filter(|m| m.score >= THRESHOLD)
        .fold(None, |acc: Option<Match>, m| match acc {
            Some(a) if a.score >= m.score => Some(a),
            _ => Some(m),
        })
}

pub fn classify(members: &[&Proc], ports: &[u16], deps: &[String]) -> Identity {
    let runtime = members.iter().find_map(|p| runtime(p));
    let found = best_match(members.iter().flat_map(|p| {
        let stem = p.stem();
        let tokens = tokens(p);
        SIGNATURES
            .iter()
            .filter_map(move |sig| score(sig, &stem, &tokens, ports))
            .collect::<Vec<_>>()
    }));
    let framework = FRAMEWORKS.iter().find(|f| {
        deps.iter().any(|d| d == f.dependency)
            && match (&found, f.refines) {
                (Some(m), Some(base)) => m.sig.id == base,
                (None, None) => true,
                _ => false,
            }
    });
    let dependency = |name: &str| Evidence {
        kind: EvidenceKind::Dependency,
        detail: name.to_string(),
    };
    match (found, framework) {
        (Some(m), Some(f)) => {
            let mut evidence = m.evidence;
            evidence.push(dependency(f.dependency));
            Identity {
                tool: Some(f.id),
                name: Some(f.name.to_string()),
                kind: m.sig.kind,
                runtime,
                confidence: (m.score + 0.05).min(0.95),
                evidence,
            }
        }
        (Some(m), None) => Identity {
            tool: Some(m.sig.id),
            name: Some(m.sig.name.to_string()),
            kind: m.sig.kind,
            runtime,
            confidence: m.score,
            evidence: m.evidence,
        },
        (None, Some(f)) if !ports.is_empty() => Identity {
            tool: Some(f.id),
            name: Some(f.name.to_string()),
            kind: Kind::DevServer,
            runtime,
            confidence: 0.55,
            evidence: vec![dependency(f.dependency)],
        },
        _ => Identity {
            runtime,
            ..Identity::default()
        },
    }
}

pub fn classify_image(image: &str, ports: &[u16]) -> Identity {
    let base = image.rsplit('/').next().unwrap_or(image);
    let base = base.split(['@', ':']).next().unwrap_or(base).to_lowercase();
    let mut tokens = HashSet::new();
    tokens.insert(base.clone());
    let found = best_match(
        SIGNATURES
            .iter()
            .filter_map(|sig| score(sig, &base, &tokens, ports)),
    );
    match found {
        Some(m) => Identity {
            tool: Some(m.sig.id),
            name: Some(m.sig.name.to_string()),
            kind: m.sig.kind,
            runtime: Some("docker"),
            confidence: m.score,
            evidence: vec![Evidence {
                kind: EvidenceKind::Container,
                detail: image.to_string(),
            }],
        },
        None => Identity {
            runtime: Some("docker"),
            evidence: vec![Evidence {
                kind: EvidenceKind::Container,
                detail: image.to_string(),
            }],
            ..Identity::default()
        },
    }
}

pub fn protocol(identity: &Identity) -> Protocol {
    if let Some(sig) = identity.tool.and_then(signature) {
        return sig.protocol;
    }
    if identity.tool.is_some() {
        return Protocol::Http;
    }
    match identity.runtime {
        Some(
            "node" | "bun" | "deno" | "python" | "ruby" | "php" | "java" | "dotnet" | "go" | "rust",
        ) => Protocol::Http,
        _ => Protocol::Tcp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn proc(exe: &str, cmd: &[&str]) -> Proc {
        Proc {
            pid: 1,
            parent: None,
            name: exe.rsplit(['/', '\\']).next().unwrap().to_string(),
            exe: Some(PathBuf::from(exe)),
            cmd: cmd.iter().map(|s| s.to_string()).collect(),
            cwd: None,
            cpu: 0.0,
            memory: 0,
            start_time: 0,
            owned: true,
        }
    }

    #[test]
    fn recognizes_next_dev() {
        let p = proc(
            "/usr/local/bin/node",
            &["node", "/code/shop/node_modules/next/dist/bin/next", "dev"],
        );
        let id = classify(&[&p], &[3000], &[]);
        assert_eq!(id.tool, Some("nextjs"));
        assert_eq!(id.runtime, Some("node"));
        assert!(id.confidence > 0.8);
    }

    #[test]
    fn recognizes_windows_vite() {
        let p = proc(
            r"C:\Program Files\nodejs\node.exe",
            &[
                r"C:\Program Files\nodejs\node.exe",
                r"C:\dev\site\node_modules\vite\bin\vite.js",
            ],
        );
        let id = classify(&[&p], &[5173], &[]);
        assert_eq!(id.tool, Some("vite"));
    }

    #[test]
    fn refines_vite_with_sveltekit() {
        let p = proc(
            "/usr/bin/node",
            &["node", "/a/node_modules/.bin/vite", "dev"],
        );
        let id = classify(&[&p], &[5173], &["@sveltejs/kit".into()]);
        assert_eq!(id.tool, Some("sveltekit"));
    }

    #[test]
    fn recognizes_postgres_by_executable() {
        let p = proc("/opt/homebrew/bin/postgres", &["postgres", "-D", "/data"]);
        let id = classify(&[&p], &[5432], &[]);
        assert_eq!(id.tool, Some("postgres"));
        assert_eq!(id.kind, Kind::Database);
    }

    #[test]
    fn recognizes_django_runserver() {
        let p = proc(
            "/usr/bin/python3",
            &["python3", "/home/me/blog/manage.py", "runserver"],
        );
        assert_eq!(classify(&[&p], &[8000], &[]).tool, Some("django"));
    }

    #[test]
    fn falls_back_to_dependencies() {
        let p = proc("/usr/bin/node", &["node", "server.js"]);
        let id = classify(&[&p], &[4000], &["express".into()]);
        assert_eq!(id.tool, Some("express"));
        assert!(id.confidence < 0.6);
    }

    #[test]
    fn unknown_process_has_no_tool() {
        let p = proc("/usr/bin/node", &["node", "index.js"]);
        let id = classify(&[&p], &[4000], &[]);
        assert_eq!(id.tool, None);
        assert_eq!(id.runtime, Some("node"));
        assert_eq!(protocol(&id), Protocol::Http);
    }

    #[test]
    fn classifies_images() {
        assert_eq!(
            classify_image("postgres:16-alpine", &[5432]).tool,
            Some("postgres")
        );
        assert_eq!(
            classify_image("docker.io/bitnami/redis:7", &[]).tool,
            Some("redis")
        );
        assert_eq!(classify_image("ghcr.io/me/app:latest", &[]).tool, None);
    }
}
