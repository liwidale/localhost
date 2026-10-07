pub const MASK: &str = "••••••";

const SECRET_WORDS: &[&str] = &[
    "KEY",
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PASSWD",
    "PASS",
    "PWD",
    "AUTH",
    "CREDENTIAL",
    "CREDENTIALS",
    "PRIVATE",
    "SIGNATURE",
    "SALT",
    "DSN",
    "WEBHOOK",
];

const NOT_SECRET: &[&str] = &[
    "PWD",
    "OLDPWD",
    "XAUTHORITY",
    "SSH_AUTH_SOCK",
    "GPG_AGENT_INFO",
];

const VALUE_PREFIXES: &[&str] = &[
    "sk-",
    "sk_live_",
    "sk_test_",
    "rk_live_",
    "rk_test_",
    "whsec_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xapp-",
    "npm_",
    "hf_",
    "AKIA",
    "ASIA",
    "AIza",
    "SG.",
    "shpat_",
    "dop_v1_",
    "-----BEGIN",
];

pub fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    if NOT_SECRET.contains(&upper.as_str()) {
        return false;
    }
    upper
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| {
            SECRET_WORDS.contains(&word)
                || word.ends_with("TOKEN")
                || word.ends_with("SECRET")
                || word.ends_with("PASSWORD")
                || word.ends_with("APIKEY")
        })
}

pub fn is_secret_value(value: &str) -> bool {
    let value = value.trim();
    if value.len() < 8 {
        return false;
    }
    if VALUE_PREFIXES
        .iter()
        .any(|p| value.starts_with(p) && value.len() >= p.len() + 8)
    {
        return true;
    }
    if value.starts_with("eyJ") && value.matches('.').count() == 2 {
        return true;
    }
    looks_random(value)
}

fn looks_random(value: &str) -> bool {
    if value.len() < 32
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '_' | '-'))
    {
        return false;
    }
    let classes = [
        value.chars().any(|c| c.is_ascii_lowercase()),
        value.chars().any(|c| c.is_ascii_uppercase()),
        value.chars().any(|c| c.is_ascii_digit()),
    ];
    if classes.iter().filter(|c| **c).count() < 3 {
        return false;
    }
    let mut counts = [0u32; 128];
    for b in value.bytes() {
        counts[b as usize] += 1;
    }
    let len = value.len() as f64;
    let entropy: f64 = counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = f64::from(c) / len;
            -p * p.log2()
        })
        .sum();
    entropy >= 4.2
}

pub fn url_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(idx) = rest.find("://") {
        let (head, tail) = rest.split_at(idx + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| {
                c == '/' || c == '?' || c == '#' || c.is_whitespace() || c == '"' || c == '\''
            })
            .unwrap_or(tail.len());
        let authority = &tail[..end];
        match authority.rfind('@') {
            Some(at) => {
                let userinfo = &authority[..at];
                match userinfo.split_once(':') {
                    Some((user, _)) => {
                        out.push_str(user);
                        out.push(':');
                        out.push_str(MASK);
                    }
                    None if is_secret_value(userinfo) => out.push_str(MASK),
                    None => out.push_str(userinfo),
                }
                out.push_str(&authority[at..]);
            }
            None => out.push_str(authority),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

pub fn env_value(name: &str, value: &str) -> (String, bool) {
    if value.is_empty() {
        return (String::new(), false);
    }
    if is_secret_name(name) || is_secret_value(value) {
        return (MASK.to_string(), true);
    }
    let cleaned = url_credentials(value);
    let secret = cleaned != value;
    (cleaned, secret)
}

pub fn args(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut hide_next = false;
    for arg in args {
        if hide_next {
            hide_next = false;
            out.push(MASK.to_string());
            continue;
        }
        if let Some(flag) = arg.strip_prefix('-') {
            let flag = flag.trim_start_matches('-');
            if let Some((name, value)) = flag.split_once('=') {
                if is_secret_name(name) || is_secret_value(value) {
                    let prefix = &arg[..arg.len() - value.len()];
                    out.push(format!("{prefix}{MASK}"));
                    continue;
                }
            } else if is_secret_name(flag) && !flag.is_empty() {
                out.push(arg.clone());
                hide_next = true;
                continue;
            }
        }
        if is_secret_value(arg) {
            out.push(MASK.to_string());
        } else {
            out.push(url_credentials(arg));
        }
    }
    out
}

pub fn command(cmd: &[String]) -> String {
    if let [single] = cmd {
        return args(std::slice::from_ref(single)).remove(0);
    }
    args(cmd)
        .into_iter()
        .map(|a| {
            if a.contains(' ') && !a.starts_with('"') {
                format!("\"{a}\"")
            } else {
                a
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_secret_names() {
        assert!(is_secret_name("STRIPE_SECRET_KEY"));
        assert!(is_secret_name("GITHUB_TOKEN"));
        assert!(is_secret_name("db_password"));
        assert!(is_secret_name("NEXTAUTH_SECRET"));
        assert!(is_secret_name("OPENAI_APIKEY"));
        assert!(!is_secret_name("PWD"));
        assert!(!is_secret_name("PATH"));
        assert!(!is_secret_name("NODE_ENV"));
        assert!(!is_secret_name("KEYBOARD_LAYOUT"));
    }

    #[test]
    fn detects_secret_values() {
        assert!(is_secret_value("ghp_abcdefghijklmnopqrstuvwxyz0123456789"));
        assert!(is_secret_value("AKIAIOSFODNN7EXAMPLE"));
        assert!(is_secret_value(
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abc123def456"
        ));
        assert!(is_secret_value("q8Zr2Lm9XpT4vWb7Kc1Nd5Fg3Hj6Ys0Ue"));
        assert!(!is_secret_value("development"));
        assert!(!is_secret_value("/usr/local/bin:/usr/bin"));
        assert!(!is_secret_value("d41d8cd98f00b204e9800998ecf8427e"));
    }

    #[test]
    fn masks_url_passwords() {
        assert_eq!(
            url_credentials("postgres://app:hunter2@localhost:5432/shop"),
            format!("postgres://app:{MASK}@localhost:5432/shop")
        );
        assert_eq!(
            url_credentials("http://localhost:3000/a"),
            "http://localhost:3000/a"
        );
        let (value, secret) = env_value("DATABASE_URL", "redis://:pw@127.0.0.1:6379");
        assert!(secret);
        assert_eq!(value, format!("redis://:{MASK}@127.0.0.1:6379"));
    }

    #[test]
    fn masks_arguments() {
        let cmd: Vec<String> = [
            "server",
            "--password",
            "hunter2",
            "--token=abc",
            "--port",
            "80",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            args(&cmd),
            vec![
                "server",
                "--password",
                MASK,
                &format!("--token={MASK}"),
                "--port",
                "80"
            ]
        );
    }
}
