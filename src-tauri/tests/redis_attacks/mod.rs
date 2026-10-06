//! The Redis MCP guard's attack tables, shared by `redis_guard.rs` (the verdicts, no server) and
//! `redis_mcp_live.rs` (the same texts over real HTTP against a real server, with the server's own
//! counters compared before and after).
//!
//! Adding a case: an attack goes in a REFUSE table; ordinary data that merely looks like one goes in
//! an ALLOW table. A refused ALLOW row is a bug to investigate — never a reason to relax the guard.
//! Each text is thrown at every place it can go: TEXT_REFUSE as a key (`redis_get`) AND as a glob
//! (`redis_scan`), the cursors as `redis_scan` and `redis_get` cursors, and so on.

/// Refused as a key and as a glob: (class, text).
pub const TEXT_REFUSE: &[(&str, &str)] = &[
    // --- RESP / inline-protocol injection: a second command smuggled behind a line ending ---
    ("resp-injection", "foo\r\nFLUSHALL\r\n"),
    ("resp-injection", "k\nCONFIG SET dir /tmp"),
    ("resp-injection", "k\r\nCONFIG SET dbfilename pwned.rdb\r\n"),
    ("resp-injection", "k\r\nEVAL \"redis.call('flushall')\" 0\r\n"),
    ("resp-injection", "*1\r\n$8\r\nFLUSHALL\r\n"),
    ("resp-injection", "k\r\n*3\r\n$3\r\nSET\r\n$1\r\na\r\n$1\r\nb\r\n"),
    ("resp-injection", "$5\r\nhello\r\n"),
    ("resp-injection", "k\r\nDEBUG SLEEP 5\r\n"),
    ("resp-injection", "k\r\nSHUTDOWN NOSAVE\r\n"),
    ("resp-injection", "k\r\nSLAVEOF 10.0.0.1 6379\r\n"),
    ("resp-injection", "k\r\nSCRIPT FLUSH\r\n"),
    ("resp-injection", "k\r\nMODULE LOAD /tmp/evil.so\r\n"),
    ("resp-injection", "k\r\nACL SETUSER default on nopass ~* +@all\r\n"),
    ("resp-injection", "k\r\nSELECT 5\r\n"),
    ("resp-injection", "k\r\nMIGRATE 10.0.0.1 6379 k 0 5000\r\n"),
    ("resp-injection", "\r\nFLUSHALL"),
    ("resp-injection", "FLUSHALL\r\n"),
    ("resp-injection", "PING\nPING\nSET pwned 1"),
    ("line-ending", "a\nb"),
    ("line-ending", "a\rb"),
    ("line-ending", "\n"),
    ("line-ending", "\r"),
    ("line-ending", "\r\n"),
    ("line-ending", "a\n"),
    ("line-ending", "\na"),
    // --- NUL: the terminator of every C string the server or a module might use ---
    ("nul", "a\0b"),
    ("nul", "\0"),
    ("nul", "k\0FLUSHALL"),
    ("nul", "k\0"),
    ("nul", "\0\0\0"),
    // --- other control characters (log forging, terminal escapes, protocol framing) ---
    ("control", "a\tb"),
    ("control", "\x1b[31mred"),
    ("control", "\x1b]0;title\x07"),
    ("control", "\x07"),
    ("control", "\x08"),
    ("control", "\x0b"),
    ("control", "\x0c"),
    ("control", "a\x7fb"),
    ("control", "a\u{85}b"),
    ("control", "a\u{9f}b"),
    // --- nothing at all ---
    ("empty", ""),
];

/// Refused as a glob only: (class, text). A key may be any of these.
pub const PATTERN_REFUSE: &[(&str, &str)] = &[
    ("many-stars", "*a*a*a*a*a"),
    ("many-stars", "**********"),
    ("many-stars", "*****"),
    ("many-stars", "a*b*c*d*e*"),
    ("many-stars", "*a*b*c*d*e*f*g*"),
    ("many-stars", "\\*\\*\\*\\*\\*"),
    ("many-stars", "*[a-z]*[a-z]*[a-z]*[a-z]*[a-z]"),
    ("many-stars", "user:*:*:*:*:*"),
    ("many-stars", "*?*?*?*?*?"),
];

/// Sizes (bytes) over the cap for a key (512) and for a glob (256).
pub const KEY_TOO_LONG: &[usize] = &[513, 1000, 4096, 65_536, 1 << 20];
pub const PATTERN_TOO_LONG: &[usize] = &[257, 300, 1000, 65_536, 1 << 20];

/// Cursors refused by `redis_scan` (and by `redis_get`, which may take a stream id besides).
pub const CURSOR_REFUSE: &[(&str, &str)] = &[
    ("negative", "-1"),
    ("negative", "-0"),
    ("negative", "-9223372036854775808"),
    ("sign", "+1"),
    ("not-decimal", "1.5"),
    ("not-decimal", "1e3"),
    ("not-decimal", "0x1f"),
    ("not-decimal", "0b11"),
    ("not-decimal", "1,2"),
    ("not-decimal", "1_000"),
    ("not-decimal", "NaN"),
    ("not-decimal", "inf"),
    ("not-decimal", "abc"),
    ("not-decimal", "$"),
    ("not-decimal", "*"),
    ("not-decimal", ".."),
    ("not-decimal", "١٢٣"),
    ("not-decimal", "１２"),
    ("space", " 1"),
    ("space", "1 "),
    ("space", "0 MATCH x"),
    ("space", "1 COUNT 1000000"),
    ("huge", "4611686018427387905"),
    ("huge", "9223372036854775807"),
    ("huge", "9223372036854775808"),
    ("huge", "18446744073709551615"),
    ("huge", "18446744073709551616"),
    ("huge", "99999999999999999999999"),
    ("huge", "000000000000000000001"),
    ("injection", "1;FLUSHALL"),
    ("injection", "1\r\nFLUSHALL"),
    ("injection", "1\nFLUSHALL\n"),
    ("injection", "0\0"),
    ("injection", "1 MATCH * TYPE string"),
];

/// Refused only where a stream entry id is not a cursor (`redis_scan`), or never.
pub const SCAN_CURSOR_ONLY_REFUSE: &[(&str, &str)] = &[("stream-id", "1-2"), ("stream-id", "1700000000000-0")];

/// Refused as a `redis_get` cursor too, besides CURSOR_REFUSE: malformed stream ids.
pub const STREAM_CURSOR_REFUSE: &[(&str, &str)] = &[
    ("stream-id", "1-"),
    ("stream-id", "-1"),
    ("stream-id", "1-2-3"),
    ("stream-id", "1--2"),
    ("stream-id", "a-b"),
    ("stream-id", "1-4611686018427387905"),
    ("stream-id", "(1-0"),
    ("stream-id", "+"),
    ("stream-id", "1-2\r\nFLUSHALL"),
    ("stream-id", "$"),
];

/// `count` values refused (the range is 1 to 1000).
pub const COUNT_REFUSE: &[i64] =
    &[0, -1, -2, -1000, i64::MIN, i64::MIN + 1, 1001, 1024, 10_000, 1_000_000, 2_147_483_648, 4_294_967_297, i64::MAX];

/// RedisJSON paths refused: a character outside `[A-Za-z0-9_.$\[\]'"*-]`, or too long.
pub const PATH_REFUSE: &[(&str, &str)] = &[
    ("space", "$.a b"),
    ("space", "$ .a"),
    ("space", "a b"),
    ("space", "$.a "),
    ("second-argument", "$.a $.b"),
    ("second-argument", "$.a\n$.b"),
    ("second-argument", "$.a\r\nFLUSHALL"),
    ("second-argument", "$.a\r\nJSON.DEL k $\r\n"),
    ("second-argument", "$.a\0$.b"),
    ("second-argument", "$.a,$.b"),
    ("second-argument", "$.a|$.b"),
    ("second-argument", "$.a;JSON.DEL k $"),
    ("filter-script", "$.a?(@.b==1)"),
    ("filter-script", "$..[?(@.x)]"),
    ("filter-script", "$[?(@.price<10)]"),
    ("filter-script", "$.a[(@.length-1)]"),
    ("filter-script", "@.a"),
    ("filter-script", "$.a=b"),
    ("filter-script", "$.a+1"),
    ("slice", "$[0:2]"),
    ("slice", "$[::2]"),
    ("encoding", "$.a%20b"),
    ("encoding", "$.é"),
    ("encoding", "$.a\\b"),
    ("encoding", "$.a`b"),
    ("encoding", "$.a{}"),
    ("encoding", "$.a)"),
    ("encoding", "$.a\t"),
    // one character at a time: each of these alone must be enough to refuse
    ("single-char", "$.a?"),
    ("single-char", "$.a("),
    ("single-char", "$.a)"),
    ("single-char", "$.a@"),
    ("single-char", "$.a="),
    ("single-char", "$.a:"),
    ("single-char", "$.a,"),
    ("single-char", "$.a;"),
    ("single-char", "$.a|"),
    ("single-char", "$.a&"),
    ("single-char", "$.a!"),
    ("single-char", "$.a<"),
    ("single-char", "$.a>"),
    ("single-char", "$.a{"),
    ("single-char", "$.a}"),
    ("single-char", "$.a#"),
    ("single-char", "$.a~"),
    ("single-char", "$.a^"),
    ("single-char", "$.a+"),
    ("single-char", "$.a%"),
    ("single-char", "$.a/"),
    ("single-char", "$.a\\"),
    ("single-char", "$.a `"),
    ("single-char", "$.a\r"),
    ("single-char", "$.a\n"),
    ("single-char", "$.a\0"),
    ("single-char", "$.a\u{7f}"),
];

pub const PATH_TOO_LONG: &[usize] = &[129, 200, 4096];

// ── ordinary input that merely looks like an attack ───────────────────────────────

/// Fine as a key (data is data) and as a glob. Commands as key names are keys, and read as such:
/// nothing runs them.
pub const TEXT_ALLOW: &[&str] = &[
    "a",
    "user:42:session",
    "with space",
    "FLUSHALL",
    "FLUSHALL ASYNC",
    "DEL a b c",
    "SET pwned 1",
    "CONFIG SET dir /tmp",
    "EVAL \"redis.call('flushall')\" 0",
    "SCRIPT FLUSH",
    "SHUTDOWN NOSAVE",
    "*1$8FLUSHALL",
    "k;FLUSHALL",
    "k|FLUSHALL",
    "k&&FLUSHALL",
    "$(FLUSHALL)",
    "`FLUSHALL`",
    "../../etc/passwd",
    "日本語",
    "emoji:\u{1f600}",
    "line\u{2028}separator",
    "quote\"and'quote",
    "back\\slash",
    "ünïcödé",
    "a\u{fffd}b",
];

/// Fine as a glob only.
pub const PATTERN_ALLOW: &[&str] = &[
    "*",
    "user:*",
    "user:*:session",
    "user:?:session",
    "user:[0-9]*",
    "user:[^a]*",
    "*:session:*",
    "a*b*c*d",
    "\\*",
    "**",
    "[*]",
];

pub const CURSOR_ALLOW: &[&str] = &["0", "1", "17", "007", "4611686018427387904", "1234567890123456789"];

pub const STREAM_CURSOR_ALLOW: &[&str] = &["1700000000000", "1700000000000-0", "0-1", "4611686018427387904-4611686018427387904"];

pub const COUNT_ALLOW: &[i64] = &[1, 2, 10, 100, 500, 1000];

pub const PATH_ALLOW: &[&str] = &["$", ".", "$.a.b", "$.items[0].name", "$..id", "$.a[*]", "$['a-b']", "$[\"k\"]", "$.a[-1]", ".a.b", "$.a_b.c9"];

/// `text` as a key too long by `n` bytes.
pub fn long(n: usize) -> String {
    "k".repeat(n)
}

/// How many attack texts the tables hold (the live suite and the guard suite both check a floor).
pub fn size() -> usize {
    TEXT_REFUSE.len()
        + PATTERN_REFUSE.len()
        + KEY_TOO_LONG.len()
        + PATTERN_TOO_LONG.len()
        + CURSOR_REFUSE.len()
        + SCAN_CURSOR_ONLY_REFUSE.len()
        + STREAM_CURSOR_REFUSE.len()
        + COUNT_REFUSE.len()
        + PATH_REFUSE.len()
        + PATH_TOO_LONG.len()
}
