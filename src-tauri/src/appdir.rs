//! The app's config directory (saved connections, query library, AI config, plugins).
//! `TIQ_CONFIG_DIR` overrides it so QA runs (e.g. the IPC bridge) never touch real user data.

use std::io::Write;
use std::path::{Path, PathBuf};

pub fn app_dir() -> PathBuf {
    match std::env::var_os("TIQ_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ai.rhobots.tableiq"),
    }
}

/// Write `bytes` to `path` atomically: a temp file in the same directory, `sync_all`, then a
/// rename over the target. A crash or a full disk mid-write leaves the previous file intact
/// instead of a truncated one — which for `connections.json` is the user's whole setup.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("write"),
        std::process::id()
    ));
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)
}

/// Move a file that failed to parse aside as `<name>.corrupt-<unix_ts>`. The alternative —
/// loading empty and saving over it — silently destroys a config the user could have fixed
/// by hand. Returns the path it was moved to.
pub fn quarantine(path: &Path) -> std::io::Result<PathBuf> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let base = path.file_name().unwrap_or_default().to_os_string();
    // The stamp only has second granularity, so two quarantines can land on the same name.
    // Renaming over the first would destroy exactly what this function exists to keep.
    for n in 0..1000 {
        let mut name = base.clone();
        name.push(if n == 0 {
            format!(".corrupt-{ts}")
        } else {
            format!(".corrupt-{ts}-{n}")
        });
        let dest = path.with_file_name(name);
        if !dest.exists() {
            std::fs::rename(path, &dest)?;
            return Ok(dest);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!("{} already has 1000 quarantined copies this second", path.display()),
    ))
}

/// Quarantine a config file that failed to parse and log both facts. The caller carries on
/// with defaults; the user's data is still on disk under its new name.
pub fn quarantine_corrupt(path: &Path, err: &str) {
    let outcome = match quarantine(path) {
        Ok(dest) => format!("moved to {}", dest.display()),
        Err(e) => format!("and could not be moved aside: {e}"),
    };
    crate::applog::log(
        "error",
        &format!("{} did not parse ({err}); {outcome} — starting empty", path.display()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_replaces_the_file_and_leaves_no_temp_behind() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("connections.json");
        write_atomic(&p, b"first").unwrap();
        write_atomic(&p, b"second").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "second");
        let left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(left, vec!["connections.json".to_string()], "temp file was not cleaned up");
    }

    #[test]
    fn write_atomic_creates_missing_directories() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nested").join("ai.json");
        write_atomic(&p, b"{}").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{}");
    }

    /// The stamp has second granularity, so two quarantines in the same second collide. An
    /// unconditional rename would destroy the first copy — the very thing quarantine exists
    /// to prevent.
    #[test]
    fn quarantine_never_overwrites_an_earlier_copy() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("connections.json");
        std::fs::write(&p, b"first").unwrap();
        let first = quarantine(&p).unwrap();
        std::fs::write(&p, b"second").unwrap();
        let second = quarantine(&p).unwrap();
        assert_ne!(first, second, "the second quarantine reused the first name");
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second");
    }

    #[test]
    fn quarantine_moves_the_file_aside_with_a_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("connections.json");
        std::fs::write(&p, b"not json").unwrap();
        let moved = quarantine(&p).unwrap();
        assert!(!p.exists(), "the corrupt file must not stay in place");
        assert_eq!(std::fs::read_to_string(&moved).unwrap(), "not json");
        let name = moved.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("connections.json.corrupt-"), "unexpected name {name}");
        assert!(
            name.trim_start_matches("connections.json.corrupt-").parse::<u64>().unwrap() > 1_700_000_000,
            "expected a unix timestamp suffix, got {name}"
        );
    }
}
