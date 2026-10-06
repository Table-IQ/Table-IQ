//! The Redis MCP guard's adversarial suite (no server needed): every attack in `redis_attacks` is
//! refused wherever it can be put — a text as a key AND as a glob, a cursor on both tools, a count, a
//! JSON path — and the ordinary input next to them is not. Every wrong verdict is collected and
//! reported at once, and a refusal names the argument it refused.

mod redis_attacks;

use redis_attacks::*;
use tableiq_lib::mcp::redis_guard::{count, cursor, key, path, pattern, MAX_COUNT, MAX_KEY_BYTES, MAX_PATH_BYTES, MAX_PATTERN_BYTES};

/// `r` is a refusal that starts with the argument's name.
fn refused(r: Result<impl std::fmt::Debug, String>, arg: &str) -> Result<(), String> {
    match r {
        Ok(v) => Err(format!("ALLOWED -> {v:?}")),
        Err(m) if m.starts_with(&format!("{arg}:")) => Ok(()),
        Err(m) => Err(format!("refused, but not naming `{arg}`: {m}")),
    }
}

#[test]
fn every_attack_is_refused_and_ordinary_input_is_not() {
    let mut wrong = Vec::new();
    let mut check = |what: String, r: Result<(), String>| {
        if let Err(m) = r {
            wrong.push(format!("{what}: {m}"));
        }
    };
    for (class, t) in TEXT_REFUSE {
        check(format!("[{class}] key {t:?}"), refused(key(t), "key"));
        check(format!("[{class}] pattern {t:?}"), refused(pattern(Some(t)), "pattern"));
    }
    for (class, t) in PATTERN_REFUSE {
        check(format!("[{class}] pattern {t:?}"), refused(pattern(Some(t)), "pattern"));
        check(format!("[{class}] as a key"), key(t).map_err(|m| format!("a key may be this, but: {m}")));
    }
    for n in KEY_TOO_LONG {
        check(format!("key of {n} bytes"), refused(key(&long(*n)), "key"));
    }
    for n in PATTERN_TOO_LONG {
        check(format!("pattern of {n} bytes"), refused(pattern(Some(&long(*n))), "pattern"));
    }
    for (class, t) in CURSOR_REFUSE.iter().chain(SCAN_CURSOR_ONLY_REFUSE) {
        check(format!("[{class}] scan cursor {t:?}"), refused(cursor(Some(t), false), "cursor"));
    }
    for (class, t) in CURSOR_REFUSE.iter().chain(STREAM_CURSOR_REFUSE) {
        check(format!("[{class}] get cursor {t:?}"), refused(cursor(Some(t), true), "cursor"));
    }
    for n in COUNT_REFUSE {
        check(format!("count {n}"), refused(count(Some(*n), 500), "count"));
    }
    for (class, t) in PATH_REFUSE {
        check(format!("[{class}] path {t:?}"), refused(path(Some(t)), "path"));
    }
    for n in PATH_TOO_LONG {
        check(format!("path of {n} bytes"), refused(path(Some(&"a".repeat(*n))), "path"));
    }

    for t in TEXT_ALLOW {
        check(format!("[allow] key {t:?}"), key(t));
        check(format!("[allow] pattern {t:?}"), pattern(Some(t)).map(|_| ()));
    }
    for t in PATTERN_ALLOW {
        check(format!("[allow] pattern {t:?}"), pattern(Some(t)).map(|_| ()));
    }
    for t in CURSOR_ALLOW {
        check(format!("[allow] scan cursor {t:?}"), cursor(Some(t), false).map(|_| ()));
        check(format!("[allow] get cursor {t:?}"), cursor(Some(t), true).map(|_| ()));
    }
    for t in STREAM_CURSOR_ALLOW {
        check(format!("[allow] stream cursor {t:?}"), cursor(Some(t), true).map(|_| ()));
    }
    for n in COUNT_ALLOW {
        check(format!("[allow] count {n}"), count(Some(*n), 10_000).map(|_| ()));
    }
    for t in PATH_ALLOW {
        check(format!("[allow] path {t:?}"), path(Some(t)).map(|_| ()));
    }
    assert!(wrong.is_empty(), "{} wrong verdicts:\n{}", wrong.len(), wrong.join("\n"));
}

/// The table is the contract: a refactor that quietly empties it must fail here.
#[test]
fn the_attack_table_is_not_shrunk() {
    assert!(size() >= 140,"{} attacks", size());
}

/// The edges are exactly the documented numbers.
#[test]
fn limits_are_exactly_at_the_documented_edges() {
    assert!(key(&long(MAX_KEY_BYTES)).is_ok() && key(&long(MAX_KEY_BYTES + 1)).is_err());
    assert!(pattern(Some(&long(MAX_PATTERN_BYTES))).is_ok() && pattern(Some(&long(MAX_PATTERN_BYTES + 1))).is_err());
    assert!(path(Some(&"a".repeat(MAX_PATH_BYTES))).is_ok() && path(Some(&"a".repeat(MAX_PATH_BYTES + 1))).is_err());
    assert!(count(Some(MAX_COUNT as i64), 10_000).is_ok() && count(Some(MAX_COUNT as i64 + 1), 10_000).is_err());
    assert!(count(Some(1), 10_000).is_ok() && count(Some(0), 10_000).is_err());
    assert!(cursor(Some("4611686018427387904"), false).is_ok() && cursor(Some("4611686018427387905"), false).is_err());
    // 4 stars pass, 5 do not — escaped or not.
    assert!(pattern(Some("*a*b*c*d")).is_ok() && pattern(Some("*a*b*c*d*")).is_err());
}

/// What the guard returns is the argument itself: it checks and never edits (no trimming, no
/// normalizing — the driver reads exactly what was checked).
#[test]
fn what_is_returned_is_what_was_checked() {
    for t in TEXT_ALLOW.iter().chain(PATTERN_ALLOW) {
        assert_eq!(pattern(Some(t)).as_deref(), Ok(*t));
    }
    for t in CURSOR_ALLOW.iter().chain(STREAM_CURSOR_ALLOW) {
        assert_eq!(cursor(Some(t), true), Ok(Some(t.to_string())));
    }
    for t in PATH_ALLOW {
        assert_eq!(path(Some(t)), Ok(Some(t.to_string())));
    }
    assert_eq!(pattern(None).as_deref(), Ok("*"), "omitted means every key; an empty text is refused");
    assert_eq!((cursor(None, false), cursor(Some(""), true), path(None), path(Some(""))), (Ok(None), Ok(None), Ok(None), Ok(None)));
}

/// A refusal says what and why, and never echoes the text it refused (a key can be a secret).
#[test]
fn a_refusal_says_what_and_why_without_echoing() {
    let m = key("secret-token\r\nFLUSHALL").unwrap_err();
    assert!(m.starts_with("key:") && m.contains("control characters"), "{m}");
    assert!(!m.contains("secret-token"), "{m}");
    let m = pattern(Some("*a*a*a*a*a")).unwrap_err();
    assert!(m.contains("`*`") && !m.contains("a*a*a"), "{m}");
    assert!(cursor(Some("-1"), true).unwrap_err().contains("stream entry id"));
    assert!(!cursor(Some("-1"), false).unwrap_err().contains("stream"));
    assert!(count(Some(0), 5).unwrap_err().contains("1 to 1000"));
    assert!(path(Some("$.a b")).unwrap_err().contains("not allowed"));
}

/// The guard runs inside the MCP call's task, so a worst-case input must not make it slow.
#[test]
fn a_maximal_input_is_checked_quickly() {
    let (k, p, j) = (long(1 << 20), "*".repeat(1 << 20), "$".repeat(1 << 20));
    let started = std::time::Instant::now();
    for _ in 0..1000 {
        assert!(key(&k).is_err() && pattern(Some(&p)).is_err() && path(Some(&j)).is_err());
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(2), "took {:?}", started.elapsed());
}
