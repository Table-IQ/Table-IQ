//! The per-connection SQL log behind the bottom panel: every statement the app runs on a
//! connection — the user's own, and the catalog, count and page queries behind the grid.
//!
//! Capture is task-scoped. `drivers::logged::Logged` runs each driver call inside
//! [`CURRENT`]`.scope(log)`, and [`note`] appends to whichever log is in scope (or nothing).
//! sqlx already emits every statement as a `tracing` event on the `sqlx::query` target;
//! [`SqlxCapture`] turns that into a `note`. tiberius (SQL Server) and DuckDB log nothing, so
//! those drivers call `note` themselves.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::lk;

/// Entries kept per connection; the oldest goes first.
const CAP: usize = 2000;
/// Longest statement kept, in bytes. A multi-megabyte import script is not worth holding.
const MAX_SQL: usize = 16 << 10;

/// One sequence across every connection, so a poller's `after_seq` never goes backwards.
static SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Clone, Debug)]
pub struct Entry {
    /// Starts at 1, so `since(0)` is everything.
    pub seq: u64,
    /// Unix microseconds.
    pub ts_us: i64,
    pub sql: String,
}

#[derive(Default)]
pub struct SqlLog(Mutex<VecDeque<Entry>>);

impl SqlLog {
    pub fn push(&self, sql: &str) {
        let sql = if sql.len() > MAX_SQL {
            let mut end = MAX_SQL;
            while !sql.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}…", &sql[..end])
        } else {
            sql.to_string()
        };
        let ts_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as i64)
            .unwrap_or(0);
        let mut q = lk(&self.0);
        // Taken under the lock, so this log's seqs rise in insertion order.
        let seq = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
        if q.len() == CAP {
            q.pop_front();
        }
        q.push_back(Entry { seq, ts_us, sql });
    }

    /// Entries newer than `after`, oldest first.
    pub fn since(&self, after: u64) -> Vec<Entry> {
        let q = lk(&self.0);
        let start = q.partition_point(|e| e.seq <= after);
        q.range(start..).cloned().collect()
    }

    pub fn clear(&self) {
        lk(&self.0).clear();
    }
}

tokio::task_local! {
    /// The log of the connection whose driver call is running on this task.
    pub static CURRENT: Arc<SqlLog>;
}

/// Record `sql` (redacted) in the log in scope; a no-op outside one.
pub fn note(sql: &str) {
    let _ = CURRENT.try_with(|l| l.push(&redact(sql)));
}

/// sqlx's per-statement event (`sqlx_core::logger::QueryLogger`) → [`note`]. The full text is
/// in `db.statement`; sqlx leaves that empty when the statement is its own `summary` (at most
/// four words), so `summary` is the fallback.
struct SqlxCapture;

#[derive(Default)]
struct Statement {
    full: String,
    summary: String,
}

impl tracing::field::Visit for Statement {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "db.statement" => self.full = value.to_string(),
            "summary" => self.summary = value.to_string(),
            _ => {}
        }
    }
    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for SqlxCapture {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        // Nothing in scope (a caller outside `Logged`, or sqlx-sqlite's worker thread): skip
        // the field walk.
        if CURRENT.try_with(|_| ()).is_err() {
            return;
        }
        let mut st = Statement::default();
        event.record(&mut st);
        let sql = match st.full.trim() {
            "" => st.summary.trim(),
            full => full,
        };
        if !sql.is_empty() {
            note(sql);
        }
    }
}

/// Install the process-wide `tracing` subscriber that feeds sqlx's statements to [`note`].
/// Only the `sqlx::query` target is enabled, at every level (sqlx logs statements at DEBUG,
/// slow ones at WARN), so every other callsite stays disabled. The app had no subscriber
/// before, so nothing else changes. A second call (or another subscriber) is a no-op.
pub fn init_tracing() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Layer;
    let only_sqlx = tracing_subscriber::filter::filter_fn(|m| m.target() == "sqlx::query");
    let subscriber = tracing_subscriber::registry().with(SqlxCapture.with_filter(only_sqlx));
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Mask secrets: the literal after `PASSWORD` or `*_PASSWORD` (`[FOR account] [=] …`), after
/// `SECRET` or `*_SECRET` (`[=] …`), and after MySQL's `IDENTIFIED [WITH plugin] BY|AS …`
/// (plus its `REPLACE …`). A literal is `'…'`, a prefixed `N'…'`/`E'…'`/`X'…'`, `$tag$…$tag$`,
/// or the arguments of a call such as `PASSWORD('…')`; MySQL's `"…"` counts after `=`
/// and `IDENTIFIED BY|AS`. Adjacent literals and unquoted assignment values (conninfo)
/// are masked too. Comments between a keyword and its value are skipped.
///
/// Keyword-anchored and fail-closed: nothing before a keyword is parsed, so no comment,
/// string or odd escape earlier in the text can hide a password. A keyword that sits inside
/// a string or comment may mask the next literal it meets — over-masking a log line is the
/// safe direction — and every keyword is looked at, even one inside another's mask.
pub fn redact(sql: &str) -> Cow<'_, str> {
    let b = sql.as_bytes();
    let mut masks: Vec<(usize, usize)> = Vec::new(); // literal bodies, quotes excluded
    let mut i = 0;
    while i < b.len() {
        if !is_word(b[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && is_word(b[i]) {
            i += 1;
        }
        let w = &b[start..i];
        // The scan goes on from the keyword, never from the end of its mask: a keyword
        // inside an identifier or string can open a fake literal that ends right at the real
        // password's quote, and skipping it would hide the real keyword inside it.
        if is_kw(w, b"password") {
            masks.extend(value_after(b, i, true));
        } else if is_kw(w, b"secret") {
            masks.extend(value_after(b, i, false));
        } else if w.eq_ignore_ascii_case(b"identified") {
            identified(b, i, &mut masks);
        }
    }
    // A keyword inside a value can open a "literal" at the very end of the text (the closing
    // `$$` of `PASSWORD $$secret$$`); there is nothing left there to hide.
    masks.retain(|&(s, _)| s < b.len());
    if masks.is_empty() {
        return Cow::Borrowed(sql);
    }
    // Overlapping (or nested) masks become their union, so the rewrite runs once, in order.
    masks.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(masks.len());
    for (s, e) in masks {
        match merged.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    let mut out = String::with_capacity(sql.len());
    let mut at = 0;
    for (s, e) in merged {
        out.push_str(&sql[at..s]);
        out.push_str("***");
        at = e;
    }
    out.push_str(&sql[at..]);
    Cow::Owned(out)
}

// ASCII-only: every keyword this module looks for (`password`, `secret`, `identified`,
// `with`, `by`, `as`, `replace`, `for`) is ASCII, and a UTF-8 continuation byte is always
// `>= 0x80` too — counting it as a word char used to glue it onto the end of a keyword
// (`PASSWORD<U+00A0>` scanned as one "word"), so the keyword never matched. Non-ASCII bytes
// inside an *identifier* (e.g. a role named `café`) still end the scan correctly: they just
// stop being lumped into a keyword comparison that could never match them anyway.
fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// `w` is `kw` or ends in `_kw` (`OLD_PASSWORD`, `CLIENT_SECRET`), case-insensitively.
fn is_kw(w: &[u8], kw: &[u8]) -> bool {
    w.eq_ignore_ascii_case(kw)
        || (w.len() > kw.len()
            && w[w.len() - kw.len() - 1] == b'_'
            && w[w.len() - kw.len()..].eq_ignore_ascii_case(kw))
}

/// Skip whitespace and comments (`-- …` / `# …` to the line end, nested `/* … */`;
/// unterminated runs to the end). Whitespace is `char::is_whitespace`, not just ASCII — SQL Server accepts a
/// non-breaking space (U+00A0) anywhere plain whitespace is legal — decoded one char at a
/// time so `i` only ever advances by a whole char and stays on a char boundary (`b` is the
/// UTF-8 bytes of the original `&str`, so `&b[i..]` is always valid UTF-8 there).
fn skip_ws(b: &[u8], mut i: usize) -> usize {
    loop {
        while i < b.len() {
            match std::str::from_utf8(&b[i..]).ok().and_then(|s| s.chars().next()) {
                Some(c) if c.is_whitespace() => i += c.len_utf8(),
                _ => break,
            }
        }
        let rest = b.get(i..).unwrap_or_default();
        if rest.starts_with(b"--") || rest.starts_with(b"#") {
            i = rest.iter().position(|&c| c == b'\n').map_or(b.len(), |p| i + p);
        } else if rest.starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < b.len() && depth > 0 {
                if b[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if b[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else {
            return i;
        }
    }
}

/// `word` (lowercase) starts at `i` as a whole word.
fn word_at(b: &[u8], i: usize, word: &[u8]) -> bool {
    b.get(i..i + word.len()).is_some_and(|w| w.eq_ignore_ascii_case(word))
        && !b.get(i + word.len()).is_some_and(|&c| is_word(c))
}

/// Index of the quote closing the literal opened at `open` (`b.len()` if unterminated) —
/// the same quote character as `b[open]`. A doubled quote and a backslash both escape — the
/// backslash only ever makes the mask longer.
fn lit_end(b: &[u8], open: usize) -> usize {
    let quote = b[open];
    let mut j = open + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            c if c == quote && b.get(j + 1) == Some(&quote) => j += 2,
            c if c == quote => return j,
            _ => j += 1,
        }
    }
    b.len()
}

/// The body of the literal starting at `i`: `'…'`, one with a prefix glued on (`N`, `E`,
/// `X`, `_utf8mb4`, …), or `$tag$…$tag$`. With `dq`, also MySQL's `"…"` string — only where
/// a value is certain (after `=` or `IDENTIFIED BY`), since elsewhere `"` quotes an
/// identifier.
fn literal_at(b: &[u8], i: usize, dq: bool) -> Option<(usize, usize)> {
    if dq && b.get(i) == Some(&b'"') {
        return Some((i + 1, lit_end(b, i)));
    }
    let mut q = i;
    while q < b.len() && (b[q].is_ascii_alphanumeric() || b[q] == b'_' || b[q] == b'&') {
        q += 1;
    }
    if b.get(q) == Some(&b'\'') {
        return Some((q + 1, lit_end(b, q)));
    }
    if b.get(i) != Some(&b'$') {
        return None;
    }
    let mut k = i + 1;
    while k < b.len() && (b[k].is_ascii_alphanumeric() || b[k] == b'_') {
        k += 1;
    }
    if b.get(k) != Some(&b'$') {
        return None; // `$1` is a placeholder, not a dollar quote
    }
    let tag = &b[i..=k];
    let end = b[k + 1..].windows(tag.len()).position(|w| w == tag).map_or(b.len(), |p| k + 1 + p);
    Some((k + 1, end))
}

/// Position after a literal's closing delimiter (including a dollar quote's full tag).
fn after_literal(b: &[u8], start: usize, mask: (usize, usize)) -> usize {
    let delimiter = if b.get(start) == Some(&b'$') { mask.0 - start } else { 1 };
    (mask.1 + delimiter).min(b.len())
}

/// The literal at `i` (including SQL's adjacent literal continuation), or a call's
/// complete argument list. A simple single-literal call keeps its quotes in the log;
/// complex expressions are masked in full rather than exposing later arguments.
fn value_at(b: &[u8], i: usize, dq: bool) -> Option<(usize, usize)> {
    if let Some(mut m) = literal_at(b, i, dq) {
        let mut next = after_literal(b, i, m);
        loop {
            let start = skip_ws(b, next);
            if start == next {
                break;
            }
            let Some(tail) = literal_at(b, start, dq) else { break };
            m.1 = tail.1;
            next = after_literal(b, start, tail);
        }
        return Some(m);
    }
    let mut k = i;
    while k < b.len() && is_word(b[k]) {
        k += 1;
    }
    let k = skip_ws(b, k);
    if b.get(k) != Some(&b'(') {
        return None;
    }
    let start = skip_ws(b, k + 1);
    if let Some(m) = literal_at(b, start, dq) {
        if b.get(skip_ws(b, after_literal(b, start, m))) == Some(&b')') {
            return Some(m);
        }
    }
    let mut end = k + 1;
    let mut depth = 1;
    while end < b.len() {
        end = skip_ws(b, end);
        if let Some(m) = literal_at(b, end, true) {
            end = after_literal(b, end, m);
            continue;
        }
        match b.get(end) {
            Some(b'(') => depth += 1,
            Some(b')') => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            None => break,
            _ => {}
        }
        end += 1;
    }
    Some((k + 1, end))
}

/// After a keyword: `[FOR account] [=] value` (`FOR` only after `PASSWORD`).
fn value_after(b: &[u8], i: usize, allow_for: bool) -> Option<(usize, usize)> {
    let mut j = skip_ws(b, i);
    if allow_for && word_at(b, j, b"for") {
        // MySQL `SET PASSWORD FOR 'u'@'h' = …`: straight to the `=`, parsing nothing.
        j += 3;
        while b.get(j).is_some_and(|&c| c != b'=' && c != b';') {
            j += 1;
        }
    }
    let eq = b.get(j) == Some(&b'=');
    if eq {
        j = skip_ws(b, j + 1);
    }
    value_at(b, j, eq).or_else(|| {
        // libpq conninfo can sit inside a SQL string: password=unquoted-value.
        // Leave parameter placeholders alone. Over-mask other assignment expressions.
        if !eq || matches!(b.get(j), None | Some(b'$' | b'?' | b'@' | b':' | b';' | b',' | b')')) {
            return None;
        }
        let mut end = j;
        while end < b.len() {
            let c = std::str::from_utf8(&b[end..]).ok()?.chars().next()?;
            if c.is_whitespace() || matches!(c, '\'' | '"' | ';' | ',' | ')') {
                break;
            }
            // A conninfo backslash escapes the following character, including whitespace.
            end += c.len_utf8();
            if c == '\\' && end < b.len() {
                end += std::str::from_utf8(&b[end..]).ok()?.chars().next()?.len_utf8();
            }
        }
        (end > j).then_some((j, end))
    })
}

/// After `IDENTIFIED`: `[WITH plugin] BY|AS value [REPLACE value]`.
fn identified(b: &[u8], i: usize, masks: &mut Vec<(usize, usize)>) {
    let mut j = skip_ws(b, i);
    if word_at(b, j, b"with") {
        j = skip_ws(b, j + 4);
        if b.get(j) == Some(&b'\'') {
            j = lit_end(b, j) + 1;
        } else {
            while j < b.len() && (is_word(b[j]) || b[j] == b'`') {
                j += 1;
            }
        }
        j = skip_ws(b, j);
    }
    if !(word_at(b, j, b"by") || word_at(b, j, b"as")) {
        return;
    }
    let Some(m) = value_at(b, skip_ws(b, j + 2), true) else { return };
    masks.push(m);
    let k = skip_ws(b, m.1 + 1);
    if word_at(b, k, b"replace") {
        masks.extend(value_at(b, skip_ws(b, k + 7), true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_commented_continued_and_computed_passwords_and_conninfo() {
        for sql in [
            "ALTER ROLE u PASSWORD /* outer /* inner */ still outer */ 'LEAK_NESTED'",
            "SET PASSWORD = # mysql comment\n 'LEAK_HASH'",
            "ALTER ROLE u PASSWORD 'first'\n 'LEAK_CONTINUATION'",
            "SET PASSWORD = CONCAT('first', 'LEAK_CONCAT')",
            "SELECT dblink('host=localhost password=LEAK_CONN dbname=test', 'SELECT 1')",
            "SET PASSWORD = CONCAT('first)', CONCAT('second', 'LEAK_NESTED_CALL'))",
            "ALTER ROLE u PASSWORD $pw$first$pw$\n $pw$LEAK_DOLLAR$pw$",
            "SELECT dblink('password=first\\ LEAK_ESCAPED dbname=test', 'SELECT 1')",
        ] {
            let out = redact(sql);
            assert!(!out.contains("LEAK_"), "secret leaked: {out}");
            for (end, _) in sql.char_indices() {
                let _ = redact(&sql[..end]);
            }
        }
    }

    #[test]
    fn ring_keeps_the_newest_cap_entries() {
        let log = SqlLog::default();
        for i in 0..CAP + 5 {
            log.push(&format!("SELECT {i}"));
        }
        let all = log.since(0);
        assert_eq!(all.len(), CAP);
        assert_eq!(all[0].sql, "SELECT 5", "the oldest five were evicted");
        assert_eq!(all[CAP - 1].sql, format!("SELECT {}", CAP + 4));
        assert!(all.windows(2).all(|w| w[0].seq < w[1].seq), "seq rises with insertion");
    }

    #[test]
    fn since_returns_only_newer_entries_and_clear_empties() {
        let log = SqlLog::default();
        log.push("a");
        log.push("b");
        log.push("c");
        let all = log.since(0);
        let tail = log.since(all[0].seq);
        assert_eq!(tail.iter().map(|e| e.sql.as_str()).collect::<Vec<_>>(), ["b", "c"]);
        assert!(log.since(all[2].seq).is_empty());
        assert!(all[0].ts_us > 1_600_000_000_000_000, "a unix-microsecond stamp");
        log.clear();
        assert!(log.since(0).is_empty());
    }

    #[test]
    fn long_statements_are_cut_on_a_char_boundary() {
        let log = SqlLog::default();
        // 'é' is two bytes, so the byte cap lands mid-character.
        log.push(&format!("x{}", "é".repeat(MAX_SQL)));
        let sql = &log.since(0)[0].sql;
        assert!(sql.len() <= MAX_SQL + "…".len());
        assert!(sql.ends_with('…'));
    }

    #[test]
    fn redacts_postgres_passwords() {
        assert_eq!(redact("ALTER ROLE bob WITH PASSWORD 'hunter2'"), "ALTER ROLE bob WITH PASSWORD '***'");
        assert_eq!(
            redact("create user a encrypted password 'it''s' valid until 'infinity'"),
            "create user a encrypted password '***' valid until 'infinity'"
        );
    }

    /// Task 10 (Database User Management): user-management statements land in the SQL log
    /// like any other statement, so the exact string `userMgmt.ts`'s `createUserSql` builds
    /// for Postgres — `CREATE ROLE "name" LOGIN PASSWORD '…'` — must have its password
    /// redacted too.
    #[test]
    fn redacts_the_generated_pg_create_user_statement() {
        assert_eq!(
            redact(r#"CREATE ROLE "bob" LOGIN PASSWORD 'hunter2';"#),
            r#"CREATE ROLE "bob" LOGIN PASSWORD '***';"#
        );
    }

    #[test]
    fn redacts_sql_server_passwords() {
        assert_eq!(
            redact("CREATE LOGIN a WITH PASSWORD = N'p@ss', CHECK_POLICY = OFF"),
            "CREATE LOGIN a WITH PASSWORD = N'***', CHECK_POLICY = OFF"
        );
        assert_eq!(
            redact("ALTER LOGIN a WITH PASSWORD = 'new' OLD_PASSWORD = 'old'"),
            "ALTER LOGIN a WITH PASSWORD = '***' OLD_PASSWORD = '***'"
        );
    }

    /// SQL Server accepts a non-breaking space (U+00A0) anywhere plain whitespace is legal,
    /// so `PASSWORD` followed by one, or `=` followed by one, must still find its value.
    /// Byte-wise, U+00A0 is `0xC2 0xA0` — both continuation bytes are `>= 0x80`, which used to
    /// make the keyword scanner treat them as *part of* the keyword (`is_word` counted any
    /// high bit as a word char), so `w` never matched `"password"` and nothing was masked.
    #[test]
    fn redacts_sql_server_password_next_to_a_non_breaking_space() {
        assert_eq!(
            redact("ALTER LOGIN a WITH PASSWORD\u{00A0}= N'secret'"),
            "ALTER LOGIN a WITH PASSWORD\u{00A0}= N'***'"
        );
        assert_eq!(
            redact("ALTER LOGIN a WITH PASSWORD =\u{00A0}N'secret'"),
            "ALTER LOGIN a WITH PASSWORD =\u{00A0}N'***'"
        );
    }

    #[test]
    fn redacts_mysql_passwords() {
        assert_eq!(
            redact(r"CREATE USER 'u'@'%' IDENTIFIED BY 'se\'cr''et'"),
            "CREATE USER 'u'@'%' IDENTIFIED BY '***'"
        );
        assert_eq!(
            redact("alter user u identified with caching_sha2_password by 'x'; SELECT 1"),
            "alter user u identified with caching_sha2_password by '***'; SELECT 1"
        );
        assert_eq!(
            redact("SET PASSWORD FOR 'u'@'localhost' = 'x'"),
            "SET PASSWORD FOR 'u'@'localhost' = '***'"
        );
    }

    #[test]
    fn earlier_comments_and_strings_cannot_derail_it() {
        for (sql, want) in [
            (
                "-- don't forget\nALTER ROLE x PASSWORD 'secret1'",
                "-- don't forget\nALTER ROLE x PASSWORD '***'",
            ),
            (
                "/* it's */ CREATE LOGIN a WITH PASSWORD = N'secret7'",
                "/* it's */ CREATE LOGIN a WITH PASSWORD = N'***'",
            ),
            (
                r"SELECT 'C:\' AS p; ALTER ROLE x PASSWORD 'secret2'",
                r"SELECT 'C:\' AS p; ALTER ROLE x PASSWORD '***'",
            ),
        ] {
            assert_eq!(redact(sql), want);
        }
    }

    #[test]
    fn masks_every_literal_form() {
        assert_eq!(redact("ALTER ROLE x PASSWORD E'secret3'"), "ALTER ROLE x PASSWORD E'***'");
        assert_eq!(redact("ALTER ROLE x PASSWORD $$secret$$"), "ALTER ROLE x PASSWORD $$***$$");
        assert_eq!(redact("ALTER ROLE x PASSWORD $pw$se'c$pw$;"), "ALTER ROLE x PASSWORD $pw$***$pw$;");
        assert_eq!(redact("SET PASSWORD = PASSWORD('secret5')"), "SET PASSWORD = PASSWORD('***')");
        assert_eq!(
            redact("CREATE CREDENTIAL c WITH IDENTITY = 'u', SECRET = 'sk'"),
            "CREATE CREDENTIAL c WITH IDENTITY = 'u', SECRET = '***'"
        );
        assert_eq!(
            redact("ALTER USER u IDENTIFIED BY 'new' REPLACE 'old'"),
            "ALTER USER u IDENTIFIED BY '***' REPLACE '***'"
        );
        // Unterminated: fail closed, masking to the end.
        assert_eq!(redact("ALTER ROLE x PASSWORD 'sec"), "ALTER ROLE x PASSWORD '***");
    }

    #[test]
    fn leaves_statements_without_a_password_literal_alone() {
        for sql in [
            "SELECT a, b FROM t WHERE name = 'x' -- it's fine",
            "SELECT password, password_hash FROM users",
            r#"SELECT "password" FROM t WHERE a = 'b'"#,
            "SELECT CAST(`password` AS CHAR) FROM t WHERE password = $1",
            "",
        ] {
            assert!(matches!(redact(sql), Cow::Borrowed(_)), "{sql} must be untouched");
        }
    }

    /// Every truncation of awkward input — unterminated quotes, a lone `$`, a keyword at the
    /// very end, multi-byte text — must come back without a panic.
    #[test]
    fn never_panics_on_a_truncated_statement() {
        let sql = "SET PASSWORD FOR 'u'@'h' = PASSWORD($pw$sé'c$pw$) IDENTIFIED WITH `p` BY N'x\\' \
                   REPLACE E'o''ld' SECRET = $1 client_secret=";
        for (i, _) in sql.char_indices() {
            let _ = redact(&sql[..i]);
        }
        assert!(!redact(sql).contains("sé'c"));
    }

    /// A keyword inside an identifier or string opens a fake "literal" that ends exactly at the
    /// real password's opening quote. The scan must not skip that masked region, or the real
    /// keyword inside it — and so the real password after it — is never seen.
    #[test]
    fn a_fake_keyword_cannot_swallow_the_real_one() {
        for sql in [
            r#"ALTER ROLE "x PASSWORD '" PASSWORD 'LEAK1'"#,
            "ALTER USER 'u'@'h IDENTIFIED BY' IDENTIFIED BY 'LEAK2'",
            "ALTER LOGIN [x PASSWORD '] WITH PASSWORD = N'LEAK3'",
        ] {
            let out = redact(sql);
            assert!(!out.contains("LEAK"), "{sql} leaked: {out}");
        }
    }

    /// Nested masks (a keyword inside a dollar-quoted value) merge into their union.
    #[test]
    fn overlapping_masks_merge() {
        assert_eq!(
            redact("ALTER ROLE x PASSWORD $$a PASSWORD 'b' c$$"),
            "ALTER ROLE x PASSWORD $$***$$"
        );
    }

    #[test]
    fn a_comment_between_keyword_and_value_is_skipped() {
        assert_eq!(redact("ALTER ROLE x PASSWORD /*c*/ 'x1'"), "ALTER ROLE x PASSWORD /*c*/ '***'");
        assert_eq!(
            redact("ALTER ROLE x PASSWORD -- c\n 'x2'"),
            "ALTER ROLE x PASSWORD -- c\n '***'"
        );
    }

    #[test]
    fn masks_mysql_double_quoted_passwords() {
        assert_eq!(
            redact(r#"CREATE USER 'u'@'%' IDENTIFIED BY "pw""#),
            r#"CREATE USER 'u'@'%' IDENTIFIED BY "***""#
        );
        assert_eq!(
            redact(r#"SET PASSWORD FOR 'u'@'h' = "pw2""#),
            r#"SET PASSWORD FOR 'u'@'h' = "***""#
        );
    }

    /// Fail-closed by design: text before a keyword isn't parsed, so a keyword inside a string
    /// masks the next literal it meets. Over-masking a log line is the safe direction.
    #[test]
    fn a_keyword_inside_a_string_over_masks() {
        assert_eq!(
            redact("SELECT 'password' FROM t WHERE name = 'x'"),
            "SELECT 'password'***'x'"
        );
    }
}
