//! The Redis guard in front of the `redis_*` MCP tools.
//!
//! Redis has no read-only session or transaction to lean on, and no tool here takes a command: the
//! tools are `SCAN`, a type-aware read of one key and `DBSIZE`, built on the driver's typed read
//! methods. So no argument can ever become a command, and this guard is the second wall — it
//! keeps what the arguments ARE (a key, a glob, a cursor, a count, a JSON path) plain data:
//!
//! * a key or glob is 1 to [`MAX_KEY_BYTES`] / [`MAX_PATTERN_BYTES`] bytes with no control character
//!   at all (NUL, CR, LF, tab, ESC, DEL, the C1 range): the characters a smuggled second command
//!   (`foo\r\nFLUSHALL\r\n`) or a log-forging line would need. Redis does bulk-encode arguments,
//!   so these would arrive as data anyway; refusing them is a wall behind the wall.
//! * a glob has at most [`MAX_STARS`] `*` (escaped ones counted too): Redis matches globs by
//!   backtracking, and a pattern of many `*` is how a few bytes of input become seconds of server time.
//! * a cursor is a decimal number up to 2^62 (a stream's `ms-seq` id also, where the key is a
//!   stream): `SCAN`'s own cursors, a list's offsets and a stream's ids, never free text.
//! * a count is 1 to [`MAX_COUNT`], and below the user's row cap.
//! * a RedisJSON path uses `[A-Za-z0-9_.$[]'"*-]` only and at most [`MAX_PATH_BYTES`] bytes: no
//!   space, `?`, `(`, `@`, `=` or `:` — so no filter or script expression, and no second argument.
//!
//! What the guard cannot do: hide what the connection's Redis user can read — agents see every
//! key, and every value, that user can. Share a Redis connection through an ACL user limited to
//! the keys agents should see (`~agent:* +@read -@dangerous`); the server then answers `NOPERM` for
//! the rest. Nor can it make a read cheap: the tool timeout, the caps on page and response size, and
//! the server's own limits do.

/// Longest key accepted.
pub const MAX_KEY_BYTES: usize = 512;
/// Longest glob accepted.
pub const MAX_PATTERN_BYTES: usize = 256;
/// Most `*` in a glob.
pub const MAX_STARS: usize = 4;
/// Longest JSON path accepted.
pub const MAX_PATH_BYTES: usize = 128;
/// Largest `count`.
pub const MAX_COUNT: usize = 1000;
/// Largest cursor (or stream id part): far above any `SCAN` cursor or list offset, and low enough that
/// an offset plus a page is still a number the server reads as a signed 64-bit integer.
pub const MAX_CURSOR: u64 = 1 << 62;

/// A key to read. Returned by value of the argument, unchanged: a key is data and is never edited.
pub fn key(s: &str) -> Result<(), String> {
    plain("key", s, MAX_KEY_BYTES)
}

/// A `SCAN` glob; omitted means every key. An empty one is refused rather than guessed at.
pub fn pattern(s: Option<&str>) -> Result<String, String> {
    let Some(s) = s else { return Ok("*".into()) };
    plain("pattern", s, MAX_PATTERN_BYTES)?;
    if s.bytes().filter(|b| *b == b'*').count() > MAX_STARS {
        return Err(format!("pattern: at most {MAX_STARS} `*` (a glob with more is slow for the server to match)"));
    }
    Ok(s.to_string())
}

/// A cursor: omitted or empty is the first page. `stream`: the key may be a stream, whose cursor is an
/// entry id (`1700000000000` or `1700000000000-3`).
pub fn cursor(s: Option<&str>, stream: bool) -> Result<Option<String>, String> {
    let Some(s) = s.filter(|s| !s.is_empty()) else { return Ok(None) };
    let num = |p: &str| p.len() <= 19 && !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    let ok = match s.split_once('-') {
        None => num(s),
        Some((ms, seq)) => stream && num(ms) && num(seq),
    };
    if !ok || s.split('-').any(|p| !p.parse::<u64>().is_ok_and(|n| n <= MAX_CURSOR)) {
        let hint = if stream { " or a stream entry id such as 1700000000000-0" } else { "" };
        return Err(format!("cursor: must be a decimal number up to 2^62{hint}, as the previous page returned it"));
    }
    Ok(Some(s.to_string()))
}

/// The page size: the agent's ask (1 to [`MAX_COUNT`]), within the user's row cap; omitted is the row cap.
pub fn count(ask: Option<i64>, row_cap: usize) -> Result<usize, String> {
    match ask {
        Some(n) if !(1..=MAX_COUNT as i64).contains(&n) => Err(format!("count: must be from 1 to {MAX_COUNT}")),
        Some(n) => Ok((n as usize).min(row_cap)),
        None => Ok(row_cap.min(MAX_COUNT)),
    }
}

/// A RedisJSON path; omitted or empty is the whole document.
pub fn path(s: Option<&str>) -> Result<Option<String>, String> {
    let Some(s) = s.filter(|s| !s.is_empty()) else { return Ok(None) };
    if s.len() > MAX_PATH_BYTES {
        return Err(format!("path: over {MAX_PATH_BYTES} bytes"));
    }
    if let Some(c) = s.chars().find(|c| !(c.is_ascii_alphanumeric() || "_.$[]'\"*-".contains(*c))) {
        return Err(format!("path: {c:?} is not allowed (only letters, digits and _ . $ [ ] ' \" * -)"));
    }
    Ok(Some(s.to_string()))
}

/// `s` as one plain argument: 1 to `max` bytes, no control character.
fn plain(what: &str, s: &str, max: usize) -> Result<(), String> {
    if s.is_empty() {
        return Err(format!("{what}: must not be empty"));
    }
    if s.len() > max {
        return Err(format!("{what}: over {max} bytes"));
    }
    if let Some(c) = s.chars().find(|c| c.is_control()) {
        return Err(format!("{what}: control characters are not allowed ({:?})", c.escape_default().to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_plain_data() {
        for ok in ["a", "user:42:session", "with space", "日本語", "a\u{fffd}b", &"k".repeat(MAX_KEY_BYTES)] {
            assert!(key(ok).is_ok(), "{ok:?}");
        }
        for bad in ["", "a\0b", "a\nb", "a\rb", "foo\r\nFLUSHALL\r\n", "a\tb", "\x1b[2J", "a\x7fb", "a\u{85}b", &"k".repeat(MAX_KEY_BYTES + 1)] {
            assert!(key(bad).is_err(), "{bad:?}");
        }
        assert!(key("a\nb").unwrap_err().starts_with("key:"));
    }

    #[test]
    fn globs_are_plain_and_cheap() {
        assert_eq!(pattern(None).unwrap(), "*");
        assert_eq!(pattern(Some("user:*:s[ab]?")).unwrap(), "user:*:s[ab]?");
        assert!(pattern(Some("*a*b*c*d")).is_ok());
        for bad in ["", "a\nb", "*a*a*a*a*a", "\\*\\*\\*\\*\\*", "**********", &"a".repeat(MAX_PATTERN_BYTES + 1)] {
            assert!(pattern(Some(bad)).unwrap_err().starts_with("pattern:"), "{bad:?}");
        }
    }

    #[test]
    fn cursors_are_numbers() {
        assert_eq!(cursor(None, false), Ok(None));
        assert_eq!(cursor(Some(""), false), Ok(None));
        assert_eq!(cursor(Some("0"), false), Ok(Some("0".into())));
        assert_eq!(cursor(Some("4611686018427387904"), false), Ok(Some("4611686018427387904".into())));
        assert_eq!(cursor(Some("1700000000000-3"), true), Ok(Some("1700000000000-3".into())));
        for bad in ["-1", "+1", "1.5", "1e3", "0x10", " 1", "1 ", "4611686018427387905", "9223372036854775807", "18446744073709551615", "18446744073709551616", "abc", "1;FLUSHALL", "1\n2", "1-2", "-"] {
            assert!(cursor(Some(bad), false).is_err(), "{bad:?}");
        }
        for bad in ["1-", "-1", "1-2-3", "1--2", "a-b", "1-4611686018427387905"] {
            assert!(cursor(Some(bad), true).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn counts_are_bounded_by_the_ask_and_the_row_cap() {
        assert_eq!(count(None, 500), Ok(500));
        assert_eq!(count(None, 10_000), Ok(MAX_COUNT));
        assert_eq!(count(Some(10), 500), Ok(10));
        assert_eq!(count(Some(900), 20), Ok(20));
        for bad in [0, -1, i64::MIN, 1001, i64::MAX] {
            assert!(count(Some(bad), 500).unwrap_err().starts_with("count:"), "{bad}");
        }
    }

    #[test]
    fn json_paths_use_a_safe_charset() {
        assert_eq!(path(None), Ok(None));
        assert_eq!(path(Some("")), Ok(None));
        for ok in ["$", ".", "$.a.b", "$.items[0].name", "$['a-b']", "$[\"k\"]", "$..*", "$.a[-1]", ".a.b"] {
            assert_eq!(path(Some(ok)), Ok(Some(ok.into())), "{ok}");
        }
        for bad in ["$.a b", "$.a\n$.b", "$.a\r\nFLUSHALL", "$.a?(@.b==1)", "$.a;x", "$[0:2]", "$.a\0", "$.é", "$.a,$.b", &"a".repeat(MAX_PATH_BYTES + 1)] {
            assert!(path(Some(bad)).unwrap_err().starts_with("path:"), "{bad:?}");
        }
    }
}
