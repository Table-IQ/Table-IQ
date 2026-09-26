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

/// Mask password literals: `PASSWORD [=] [N]'…'` (Postgres, SQL Server — `OLD_PASSWORD` too
/// — and MySQL's `SET PASSWORD [FOR u] = '…'`) and MySQL's `IDENTIFIED [WITH plugin] BY|AS '…'`.
/// A scan, not a parse: literals and quoted identifiers are skipped, so `'password'` or
/// `"password"` never trigger it, and a `password` column only does when a literal follows.
pub fn redact(sql: &str) -> Cow<'_, str> {
    let b = sql.as_bytes();
    let mut masks: Vec<(usize, usize)> = Vec::new(); // literal bodies, quotes excluded
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' => i = lit_end(b, i) + 1,
            q @ (b'"' | b'`') => {
                i = b[i + 1..].iter().position(|&c| c == q).map_or(b.len(), |p| i + p + 2);
            }
            c if is_word(c) => {
                let start = i;
                while i < b.len() && is_word(b[i]) {
                    i += 1;
                }
                let w = &b[start..i];
                let lit = if w.eq_ignore_ascii_case(b"password")
                    || (w.len() > 9 && w[w.len() - 9..].eq_ignore_ascii_case(b"_password"))
                {
                    password_literal(b, i)
                } else if w.eq_ignore_ascii_case(b"identified") {
                    identified_literal(b, i)
                } else {
                    None
                };
                if let Some((s, e)) = lit {
                    masks.push((s, e));
                    i = e + 1;
                }
            }
            _ => i += 1,
        }
    }
    if masks.is_empty() {
        return Cow::Borrowed(sql);
    }
    let mut out = String::with_capacity(sql.len());
    let mut at = 0;
    for (s, e) in masks {
        out.push_str(&sql[at..s]);
        out.push_str("***");
        at = e;
    }
    out.push_str(&sql[at..]);
    Cow::Owned(out)
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// `word` (lowercase) starts at `i` as a whole word.
fn word_at(b: &[u8], i: usize, word: &[u8]) -> bool {
    b.get(i..i + word.len()).is_some_and(|w| w.eq_ignore_ascii_case(word))
        && !b.get(i + word.len()).is_some_and(|&c| is_word(c))
}

/// Index of the quote closing the literal opened at `open` (`b.len()` if unterminated).
/// Both escapes count: a doubled `''` and MySQL's backslash.
fn lit_end(b: &[u8], open: usize) -> usize {
    let mut j = open + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'\'' if b.get(j + 1) == Some(&b'\'') => j += 2,
            b'\'' => return j,
            _ => j += 1,
        }
    }
    b.len()
}

/// The body of a `'…'` or `N'…'` literal starting at `i`.
fn literal_at(b: &[u8], mut i: usize) -> Option<(usize, usize)> {
    if matches!(b.get(i), Some(b'N' | b'n')) && b.get(i + 1) == Some(&b'\'') {
        i += 1;
    }
    (b.get(i) == Some(&b'\'')).then(|| (i + 1, lit_end(b, i)))
}

/// After `PASSWORD`: `[FOR account] [=] '…'`.
fn password_literal(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut j = skip_ws(b, i);
    if word_at(b, j, b"for") {
        // The account is itself quoted (`'u'@'h'`), so walk to the `=` past its literals.
        j += 3;
        loop {
            match b.get(j) {
                Some(b'\'') => j = lit_end(b, j) + 1,
                Some(b'=') => break,
                Some(b';') | None => return None,
                _ => j += 1,
            }
        }
    }
    if b.get(j) == Some(&b'=') {
        j = skip_ws(b, j + 1);
    }
    literal_at(b, j)
}

/// After `IDENTIFIED`: `[WITH plugin] BY|AS '…'`.
fn identified_literal(b: &[u8], i: usize) -> Option<(usize, usize)> {
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
        return None;
    }
    literal_at(b, skip_ws(b, j + 2))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn leaves_other_literals_and_password_columns_alone() {
        for sql in [
            "SELECT 'password' FROM t WHERE name = 'x'",
            "SELECT password, password_hash FROM users",
            "SELECT 'PASSWORD ''x''' AS s",
            r#"SELECT "password" FROM t WHERE a = 'b'"#,
        ] {
            assert!(matches!(redact(sql), Cow::Borrowed(_)), "{sql} must be untouched");
        }
    }
}
