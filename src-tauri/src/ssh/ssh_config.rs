//! Minimal `~/.ssh/config` Host-alias resolution.
//!
//! Intentionally small: parses `Host <alias>` blocks and reads `HostName`, `Port`, `User`,
//! and `IdentityFile`. No `Match`, no `Include`, no token expansion beyond a leading `~`.
//! Anything unmatched falls back to the literal values the caller already had.

/// What a Host alias resolves to. Any field may be absent if the block didn't specify it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostAlias {
    pub hostname: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub identity_file: Option<String>,
}

/// Resolve a Host alias from the text of an ssh config file.
///
/// Returns `None` if no `Host` line matches `alias` exactly (no glob matching in v1).
pub fn resolve_host_alias(config_text: &str, alias: &str) -> Option<HostAlias> {
    let mut in_block = false;
    let mut out = HostAlias::default();
    let mut matched = false;

    for raw in config_text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Tokens are separated by whitespace (and optionally an '='); first token is the keyword.
        let (key, value) = split_kv(line);
        let key_lc = key.to_ascii_lowercase();

        if key_lc == "host" {
            // A new Host block begins. Match if any of its patterns equals our alias exactly.
            in_block = value.split_whitespace().any(|p| p == alias);
            if in_block {
                matched = true;
            }
            continue;
        }
        if !in_block {
            continue;
        }
        match key_lc.as_str() {
            "hostname" => out.hostname = Some(value.to_string()),
            "port" => out.port = value.parse().ok(),
            "user" => out.user = Some(value.to_string()),
            "identityfile" => out.identity_file = Some(expand_tilde(value)),
            _ => {}
        }
    }

    if matched {
        Some(out)
    } else {
        None
    }
}

/// Split a config line into (keyword, value), tolerating `key value` and `key = value`.
fn split_kv(line: &str) -> (&str, &str) {
    if let Some(idx) = line.find(|c: char| c.is_whitespace() || c == '=') {
        let key = &line[..idx];
        let rest = line[idx..].trim_start_matches(|c: char| c.is_whitespace() || c == '=');
        (key, rest.trim())
    } else {
        (line, "")
    }
}

/// Expand a leading `~` to the user's home directory; otherwise return as-is.
pub fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            let mut p = home;
            p.push(rest);
            return p.to_string_lossy().into_owned();
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
# a comment
Host bastion
    HostName 203.0.113.5
    User deploy
    Port 2222
    IdentityFile ~/.ssh/bastion_ed25519

Host other
    HostName 198.51.100.9
    User root
";

    #[test]
    fn resolves_alias_fields() {
        let a = resolve_host_alias(FIXTURE, "bastion").expect("alias present");
        assert_eq!(a.hostname.as_deref(), Some("203.0.113.5"));
        assert_eq!(a.user.as_deref(), Some("deploy"));
        assert_eq!(a.port, Some(2222));
        assert!(a.identity_file.as_deref().unwrap().ends_with(".ssh/bastion_ed25519"));
        // tilde expanded (no literal ~ left)
        assert!(!a.identity_file.as_deref().unwrap().starts_with('~'));
    }

    #[test]
    fn resolves_second_block_independently() {
        let a = resolve_host_alias(FIXTURE, "other").expect("alias present");
        assert_eq!(a.hostname.as_deref(), Some("198.51.100.9"));
        assert_eq!(a.user.as_deref(), Some("root"));
        assert_eq!(a.port, None);
    }

    #[test]
    fn missing_alias_returns_none() {
        assert!(resolve_host_alias(FIXTURE, "nope").is_none());
    }

    #[test]
    fn tolerates_equals_separator() {
        let txt = "Host x\nHostName=10.0.0.1\nPort = 22\n";
        let a = resolve_host_alias(txt, "x").unwrap();
        assert_eq!(a.hostname.as_deref(), Some("10.0.0.1"));
        assert_eq!(a.port, Some(22));
    }
}
