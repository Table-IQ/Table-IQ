//! Redis KvDriver against real servers (docker/redis/docker-compose.yml):
//!   docker compose -f docker/redis/docker-compose.yml up -d --wait
//!   TIQ_REDIS_URL=redis://127.0.0.1:56379/0                      # Redis Stack: RedisJSON, seeded
//!   TIQ_REDIS_ACL_URL=redis://agent:agentpw@127.0.0.1:56380/0    # ACL user, only `agent:*`
//!   TIQ_REDIS_ADMIN_URL=redis://:tableiq@127.0.0.1:56380/0       # the same server's default user
//!   cargo test --test redis_browse
//!
//! The seeded keys (demo:*, list:*, user:*, …) are read-only here. Every test that writes uses a
//! key prefix of its own, which a guard deletes afterwards (also on a failed assertion). Nothing
//! flushes the shared database; the one FLUSHDB runs in database 15, which only it uses.

mod common;
use common::live_url;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use redis::aio::MultiplexedConnection;
use serde_json::{json, Value};
use tableiq_lib::drivers::kv_redis::{check_command, split_command, Gate, RedisDriver};
use tableiq_lib::drivers::{KvBytes, KvChange, KvDriver, KvItem, KvReadOpts, KvReply, KV_CHANGED};
use tableiq_lib::sql_log::{SqlLog, CURRENT};

async fn open(url: &str) -> RedisDriver {
    RedisDriver::connect(url).await.unwrap()
}

fn change(v: Value) -> KvChange {
    serde_json::from_value(v).unwrap()
}

fn t(s: &str) -> Value {
    json!({ "text": s })
}

fn opts(cursor: Option<String>, count: usize) -> KvReadOpts {
    KvReadOpts { cursor, count: Some(count), path: None }
}

fn text(i: &KvItem) -> String {
    i.value.as_ref().unwrap().text.clone()
}

/// The URL with another database number.
fn in_db(url: &str, db: u32) -> String {
    format!("{}/{db}", url.rsplit_once('/').unwrap().0)
}

/// A unique key prefix, whose keys are deleted on drop.
struct Scratch {
    url: String,
    prefix: String,
}

impl Scratch {
    fn new(url: &str, tag: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        Self { url: url.into(), prefix: format!("tiq_{tag}_{nanos}:") }
    }
    /// `agent:`-rooted, for the ACL user.
    fn under(url: &str, root: &str, tag: &str) -> Self {
        let mut s = Self::new(url, tag);
        s.prefix = format!("{root}{}", s.prefix);
        s
    }
    fn k(&self, name: &str) -> String {
        format!("{}{name}", self.prefix)
    }
    async fn raw(&self) -> MultiplexedConnection {
        redis::Client::open(self.url.as_str()).unwrap().get_multiplexed_async_connection().await.unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let (url, prefix) = (self.url.clone(), self.prefix.clone());
        let _ = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
                let mut con = redis::Client::open(url).unwrap().get_multiplexed_async_connection().await.unwrap();
                let keys: Vec<Vec<u8>> =
                    redis::cmd("KEYS").arg(format!("{prefix}*")).query_async(&mut con).await.unwrap();
                for k in keys {
                    let _: i64 = redis::cmd("DEL").arg(k.as_slice()).query_async(&mut con).await.unwrap();
                }
            })
        })
        .join();
    }
}

/// The seeded keys (every test's scratch keys are `tiq_…`, and come and go in parallel).
async fn seeded(drv: &RedisDriver) -> usize {
    let (mut cursor, mut n) = ("0".to_string(), 0);
    loop {
        let p = drv.scan_page("*", &cursor, 1000).await.unwrap();
        n += p.keys.iter().filter(|k| !k.key.starts_with("tiq_")).count();
        cursor = p.next_cursor;
        if cursor == "0" {
            return n;
        }
    }
}

async fn q<T: redis::FromRedisValue>(con: &mut MultiplexedConnection, args: &[&str]) -> T {
    let mut c = redis::cmd(args[0]);
    for a in &args[1..] {
        c.arg(*a);
    }
    c.query_async(con).await.unwrap()
}

#[tokio::test]
async fn scan_and_get_keep_working() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;

    let keys = drv.scan("demo:*", 100).await.unwrap();
    assert!(keys.iter().any(|k| k.key == "demo:1" && k.kind == "string" && k.ttl == -1 && !k.binary));
    assert!(keys.iter().any(|k| k.key == "demo:2" && k.ttl > 0));

    let v = drv.get("demo:1").await.unwrap();
    assert_eq!((v.kind.as_str(), v.rows[0][0].as_str()), ("string", "hello"));

    let list = drv.get("list:a").await.unwrap();
    assert_eq!(list.columns, vec!["#", "value"]);
    assert_eq!(list.rows, vec![vec!["0", "x"], vec!["1", "y"], vec!["2", "z"]]);
    assert_eq!(drv.get("zset:a").await.unwrap().rows[0], vec!["one", "1"]);
    assert_eq!(drv.get("stream:a").await.unwrap().rows[0], vec!["1700000000000-0", "type=login user=ada"]);
    assert_eq!(drv.get("list:big").await.unwrap().rows.len(), 500, "get is bounded now");
    assert_eq!(drv.get("tiq_no_such_key").await.unwrap().rows, vec![vec!["none"]]);
    assert!(drv.dbsize().await.unwrap() >= 62);
    assert_eq!(drv.list_databases().await.unwrap().len(), 16);
}

#[tokio::test]
async fn every_type_reads_with_its_metadata() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let rd = |k: &'static str| {
        let drv = &drv;
        async move { drv.read(k, &KvReadOpts::default()).await.unwrap() }
    };

    let s = rd("demo:1").await;
    assert_eq!((s.kind.as_str(), s.ttl, s.size, s.next.clone(), s.truncated), ("string", -1, 5, None, false));
    assert_eq!(text(&s.items[0]), "hello");
    assert!((86_400..=31_536_000).contains(&rd("demo:2").await.ttl), "the seeded TTL is a year, so it cannot run out under a test");

    let l = rd("list:a").await;
    assert_eq!((l.kind.as_str(), l.size, l.next.clone()), ("list", 3, None));
    assert_eq!(l.items.iter().map(|i| (i.index.unwrap(), text(i))).collect::<Vec<_>>(), [(0, "x".into()), (1, "y".into()), (2, "z".into())]);

    let set = rd("set:a").await;
    let mut members: Vec<String> = set.items.iter().map(text).collect();
    members.sort();
    assert_eq!((set.kind.as_str(), set.size, members), ("set", 3, vec!["blue".to_string(), "green".into(), "red".into()]));

    let h = rd("hash:a").await;
    let fields: HashMap<String, String> =
        h.items.iter().map(|i| (i.field.as_ref().unwrap().text.clone(), text(i))).collect();
    assert_eq!((h.kind.as_str(), h.size), ("hash", 3));
    assert_eq!((fields["name"].as_str(), fields["lang"].as_str()), ("Ada", "rust"));

    let z = rd("zset:a").await;
    assert_eq!(z.kind, "zset");
    assert_eq!(z.items.iter().map(|i| (text(i), i.score.unwrap())).collect::<Vec<_>>(), [("one".into(), 1.0), ("two".into(), 2.0), ("three".into(), 3.0)]);

    let x = rd("stream:a").await;
    assert_eq!((x.kind.as_str(), x.size, x.next.clone()), ("stream", 3, None));
    assert_eq!(x.items[0].id.as_deref(), Some("1700000000000-0"));
    let f: Vec<(String, String)> =
        x.items[2].fields.as_ref().unwrap().iter().map(|p| (p.field.text.clone(), p.value.text.clone())).collect();
    assert_eq!(f, [("type".into(), "login".into()), ("user".into(), "linus".into())], "field order is the entry's");

    let j = rd("json:a").await;
    assert_eq!(j.kind, "ReJSON-RL");
    let doc: Value = serde_json::from_str(&text(&j.items[0])).unwrap();
    assert_eq!(doc["address"]["city"], "London");
    let city = drv.read("json:a", &KvReadOpts { path: Some(".address.city".into()), ..Default::default() }).await.unwrap();
    assert_eq!(text(&city.items[0]), "\"London\"");

    let none = rd("tiq_no_such_key").await;
    assert_eq!((none.kind.as_str(), none.ttl, none.items.len()), ("none", -2, 0));
}

#[tokio::test]
async fn a_list_of_5000_pages_exactly() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;

    let (mut cursor, mut got, mut pages) = (None, Vec::new(), 0);
    loop {
        let r = drv.read("list:big", &opts(cursor.clone(), 100)).await.unwrap();
        assert_eq!(r.size, 5000);
        assert_eq!(r.items.len(), 100, "page {pages}");
        pages += 1;
        got.extend(r.items);
        match r.next {
            Some(n) => cursor = Some(n),
            None => break,
        }
    }
    assert_eq!(pages, 50);
    assert_eq!(got.len(), 5000);
    for (i, it) in got.iter().enumerate() {
        assert_eq!(it.index, Some(i as u64));
        assert_eq!(text(it), format!("item-{}", i + 1));
    }

    let big = drv.read("list:big", &opts(None, 5000)).await.unwrap();
    assert_eq!((big.items.len(), big.next.as_deref()), (1000, Some("1000")), "count is capped at 1000");
    let tail = drv.read("list:big", &opts(Some("4950".into()), 100)).await.unwrap();
    assert_eq!((tail.items.len(), tail.next), (50, None));
    let past = drv.read("list:big", &opts(Some("5000".into()), 100)).await.unwrap();
    assert_eq!((past.items.len(), past.next), (0, None));
    assert!(drv.read("list:big", &opts(Some("abc".into()), 10)).await.is_err());
}

#[tokio::test]
async fn scan_pages_cover_the_keyspace_exactly_once() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;

    let (mut cursor, mut seen, mut pages) = ("0".to_string(), HashSet::new(), 0);
    loop {
        let p = drv.scan_page("user:*", &cursor, 7).await.unwrap();
        pages += 1;
        for k in p.keys {
            assert!(k.key.starts_with("user:") && k.kind == "string" && k.ttl == -1, "{k:?}");
            seen.insert(k.key);
        }
        cursor = p.next_cursor;
        if cursor == "0" {
            break;
        }
    }
    assert!(pages > 1, "50 keys in pages of 7");
    let want: HashSet<String> =
        (1..=25).flat_map(|i| [format!("user:{i}:name"), format!("user:{i}:email")]).collect();
    assert_eq!(seen, want);

    assert_eq!(drv.scan("user:*", 20).await.unwrap().len(), 20);
    assert!(!drv.scan_page("", "0", 10).await.unwrap().keys.is_empty(), "an empty pattern is *");
    assert!(drv.scan_page("*", "not-a-cursor", 10).await.is_err());
}

#[tokio::test]
async fn set_hash_zset_and_stream_page_through() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let s = Scratch::new(&url, "pg");
    let mut con = s.raw().await;
    let mut p = redis::pipe();
    for i in 0..2500 {
        p.cmd("SADD").arg(s.k("set")).arg(format!("m{i}")).ignore();
    }
    for i in 0..1200 {
        p.cmd("HSET").arg(s.k("hash")).arg(format!("f{i}")).arg(i).ignore();
    }
    for i in 0..250 {
        p.cmd("ZADD").arg(s.k("zset")).arg(i).arg(format!("z{i:03}")).ignore();
        p.cmd("XADD").arg(s.k("stream")).arg(format!("{}-0", i + 1)).arg("n").arg(i).ignore();
    }
    let _: () = p.query_async(&mut con).await.unwrap();

    // SSCAN / HSCAN: a page is about `count`; every member turns up by the end.
    let mut members = HashSet::new();
    let (mut cursor, mut pages) = (None, 0);
    loop {
        let r = drv.read(&s.k("set"), &opts(cursor.clone(), 100)).await.unwrap();
        assert_eq!(r.size, 2500);
        members.extend(r.items.iter().map(text));
        pages += 1;
        assert!(pages < 200);
        match r.next {
            Some(n) => cursor = Some(n),
            None => break,
        }
    }
    assert_eq!(members.len(), 2500);

    let mut fields = HashMap::new();
    let mut cursor = None;
    loop {
        let r = drv.read(&s.k("hash"), &opts(cursor.clone(), 100)).await.unwrap();
        for i in &r.items {
            fields.insert(i.field.as_ref().unwrap().text.clone(), text(i));
        }
        match r.next {
            Some(n) => cursor = Some(n),
            None => break,
        }
    }
    assert_eq!((fields.len(), fields["f1199"].as_str()), (1200, "1199"));

    // ZRANGE / XRANGE: exact pages, in order.
    for key in ["zset", "stream"] {
        let sizes: Vec<usize> = {
            let (mut cursor, mut sizes, mut seen) = (None, Vec::new(), Vec::new());
            loop {
                let r = drv.read(&s.k(key), &opts(cursor.clone(), 100)).await.unwrap();
                assert_eq!(r.size, 250);
                sizes.push(r.items.len());
                for i in &r.items {
                    seen.push(if key == "zset" { text(i) } else { i.id.clone().unwrap() });
                }
                match r.next {
                    Some(n) => cursor = Some(n),
                    None => break,
                }
            }
            let want: Vec<String> =
                (0..250).map(|i| if key == "zset" { format!("z{i:03}") } else { format!("{}-0", i + 1) }).collect();
            assert_eq!(seen, want, "{key}");
            sizes
        };
        assert_eq!(sizes, [100, 100, 50], "{key}");
    }
}

#[tokio::test]
async fn binary_values_and_keys_round_trip() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;

    let seeded = drv.read("bin:1", &KvReadOpts::default()).await.unwrap();
    let v = seeded.items[0].value.as_ref().unwrap();
    assert!(v.binary && v.text.contains('\u{fffd}'));
    assert_eq!(v.b64.as_deref(), Some("//4AYWJj"), "ff fe 00 'a' 'b' 'c'");

    let s = Scratch::new(&url, "bin");
    let mut con = s.raw().await;
    let bytes: Vec<u8> = vec![0xff, 0xfe, 0x00, b'x', 0x80];
    // Written back from what read returned (binary flag and lossy text included): exact bytes.
    let loaded = serde_json::to_value(v).unwrap();
    let batch = vec![
        change(json!({"kind": "setString", "key": s.k("str"), "value": loaded})),
        change(json!({"kind": "hset", "key": s.k("h"), "field": {"b64": "/w=="}, "value": {"b64": "/v8A"}})),
        change(json!({"kind": "rpush", "key": s.k("l"), "value": {"b64": "/+4="}})),
        change(json!({"kind": "sadd", "key": s.k("s"), "member": {"b64": "gAA="}})),
    ];
    drv.apply_changes(&batch).await.unwrap();
    let got: Vec<u8> = redis::cmd("GET").arg(s.k("str")).query_async(&mut con).await.unwrap();
    assert_eq!(got, [0xff, 0xfe, 0x00, b'a', b'b', b'c']);
    let hv: Vec<u8> = redis::cmd("HGET").arg(s.k("h")).arg(&[0xffu8][..]).query_async(&mut con).await.unwrap();
    assert_eq!(hv, [0xfe, 0xff, 0x00]);
    let lv: Vec<Vec<u8>> = redis::cmd("LRANGE").arg(s.k("l")).arg(0).arg(-1).query_async(&mut con).await.unwrap();
    assert_eq!(lv, [vec![0xffu8, 0xee]]);

    // A binary field / member is shown and can be expected back.
    let h = drv.read(&s.k("h"), &KvReadOpts::default()).await.unwrap();
    assert_eq!(h.items[0].field.as_ref().unwrap().b64.as_deref(), Some("/w=="));
    let ok = change(json!({"kind": "hset", "key": s.k("h"), "field": {"b64": "/w=="}, "value": t("plain"), "expect": {"b64": "/v8A"}}));
    drv.apply_changes(&[ok]).await.unwrap();

    // A key that is not UTF-8 does not break the listing; it is flagged.
    let key: Vec<u8> = [s.k("k").as_bytes(), &bytes].concat();
    let _: () = redis::cmd("SET").arg(key.as_slice()).arg(1).query_async(&mut con).await.unwrap();
    let page = drv.scan_page(&format!("{}*", s.prefix), "0", 100).await.unwrap();
    assert!(page.keys.iter().any(|k| k.binary && k.key.contains('\u{fffd}')), "{page:?}");
    assert!(page.keys.iter().any(|k| !k.binary && k.key == s.k("str")));
}

#[tokio::test]
async fn a_string_over_a_mebibyte_is_cut_and_flagged() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let s = Scratch::new(&url, "big");
    let mut con = s.raw().await;
    let _: () = redis::cmd("SET").arg(s.k("s")).arg("a".repeat(2 << 20)).query_async(&mut con).await.unwrap();
    let r = drv.read(&s.k("s"), &KvReadOpts::default()).await.unwrap();
    assert_eq!((r.size, r.truncated, text(&r.items[0]).len()), (2 << 20, true, 1 << 20));
    let _: () = redis::cmd("SET").arg(s.k("s")).arg("small").query_async(&mut con).await.unwrap();
    assert!(!drv.read(&s.k("s"), &KvReadOpts::default()).await.unwrap().truncated);
}

#[tokio::test]
async fn a_batch_applies_every_type_of_change() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let s = Scratch::new(&url, "w");
    let mut con = s.raw().await;

    let batch = vec![
        change(json!({"kind": "setString", "key": s.k("str"), "value": t("v1")})),
        change(json!({"kind": "expire", "key": s.k("str"), "seconds": 500})),
        change(json!({"kind": "hset", "key": s.k("h"), "field": t("f"), "value": t("1")})),
        change(json!({"kind": "hset", "key": s.k("h"), "field": t("g"), "value": t("2")})),
        change(json!({"kind": "hdel", "key": s.k("h"), "field": t("g")})),
        change(json!({"kind": "lpush", "key": s.k("l"), "value": t("b")})),
        change(json!({"kind": "lpush", "key": s.k("l"), "value": t("a")})),
        change(json!({"kind": "rpush", "key": s.k("l"), "value": t("c")})),
        change(json!({"kind": "lrem", "key": s.k("l"), "value": t("b"), "count": 1})),
        change(json!({"kind": "sadd", "key": s.k("s"), "member": t("x")})),
        change(json!({"kind": "sadd", "key": s.k("s"), "member": t("y")})),
        change(json!({"kind": "srem", "key": s.k("s"), "member": t("x")})),
        change(json!({"kind": "zadd", "key": s.k("z"), "member": t("m"), "score": 1.5})),
        change(json!({"kind": "zadd", "key": s.k("z"), "member": t("n"), "score": 2})),
        change(json!({"kind": "zrem", "key": s.k("z"), "member": t("n")})),
        change(json!({"kind": "xadd", "key": s.k("x"), "id": "5-0", "fields": [{"field": t("k"), "value": t("v")}]})),
        change(json!({"kind": "xadd", "key": s.k("x"), "fields": [{"field": t("k"), "value": t("w")}]})),
        change(json!({"kind": "xdel", "key": s.k("x"), "id": "5-0"})),
    ];
    assert_eq!(drv.apply_changes(&batch).await.unwrap().applied, batch.len());

    let g: String = q(&mut con, &["GET", &s.k("str")]).await;
    let ttl: i64 = q(&mut con, &["TTL", &s.k("str")]).await;
    assert_eq!((g.as_str(), (1..=500).contains(&ttl)), ("v1", true));
    let h: HashMap<String, String> = q(&mut con, &["HGETALL", &s.k("h")]).await;
    assert_eq!(h, HashMap::from([("f".to_string(), "1".to_string())]));
    let l: Vec<String> = q(&mut con, &["LRANGE", &s.k("l"), "0", "-1"]).await;
    assert_eq!(l, ["a", "c"]);
    let m: Vec<String> = q(&mut con, &["SMEMBERS", &s.k("s")]).await;
    assert_eq!(m, ["y"]);
    let z: Vec<(String, f64)> = q(&mut con, &["ZRANGE", &s.k("z"), "0", "-1", "WITHSCORES"]).await;
    assert_eq!(z, [("m".to_string(), 1.5)]);
    let xl: i64 = q(&mut con, &["XLEN", &s.k("x")]).await;
    assert_eq!(xl, 1);

    // Edits keep a TTL (SET ... KEEPTTL); persist drops it; rename and del move / remove the key.
    drv.apply_changes(&[change(json!({"kind": "setString", "key": s.k("str"), "value": t("v2")}))]).await.unwrap();
    let ttl: i64 = q(&mut con, &["TTL", &s.k("str")]).await;
    assert!(ttl > 0, "an edit must not drop the TTL");
    drv.apply_changes(&[change(json!({"kind": "persist", "key": s.k("str")}))]).await.unwrap();
    let ttl: i64 = q(&mut con, &["TTL", &s.k("str")]).await;
    assert_eq!(ttl, -1);
    drv.apply_changes(&[
        change(json!({"kind": "rename", "key": s.k("str"), "newKey": s.k("renamed")})),
        change(json!({"kind": "del", "key": s.k("h")})),
    ])
    .await
    .unwrap();
    let (a, b, c): (i64, i64, i64) = (
        q(&mut con, &["EXISTS", &s.k("str")]).await,
        q(&mut con, &["EXISTS", &s.k("renamed")]).await,
        q(&mut con, &["EXISTS", &s.k("h")]).await,
    );
    assert_eq!((a, b, c), (0, 1, 0));

    // The same through the read side.
    let r = drv.read(&s.k("renamed"), &KvReadOpts::default()).await.unwrap();
    assert_eq!(text(&r.items[0]), "v2");
    assert_eq!(drv.apply_changes(&[]).await.unwrap().applied, 0);
}

#[tokio::test]
async fn one_refused_change_writes_nothing() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let s = Scratch::new(&url, "atomic");
    let mut con = s.raw().await;
    let _: () = redis::cmd("SET").arg(s.k("string")).arg("x").query_async(&mut con).await.unwrap();
    let _: () = redis::cmd("RPUSH").arg(s.k("list")).arg("a").arg("b").query_async(&mut con).await.unwrap();
    let _: () = redis::cmd("SET").arg(s.k("taken")).arg("x").query_async(&mut con).await.unwrap();
    let new = |n: &str| change(json!({"kind": "setString", "key": s.k(n), "value": t("v")}));

    let cases: Vec<(&str, KvChange, &str)> = vec![
        ("wrong type", change(json!({"kind": "lpush", "key": s.k("string"), "value": t("z")})), "WRONGTYPE"),
        ("hash on a list", change(json!({"kind": "hset", "key": s.k("list"), "field": t("f"), "value": t("1")})), "WRONGTYPE"),
        ("lset on a missing key", change(json!({"kind": "lset", "key": s.k("nolist"), "index": 0, "value": t("x")})), "does not exist"),
        ("lset out of range", change(json!({"kind": "lset", "key": s.k("list"), "index": 2, "value": t("x")})), "index out of range"),
        ("expire on a missing key", change(json!({"kind": "expire", "key": s.k("ghost"), "seconds": 10})), "does not exist"),
        ("rename onto a key", change(json!({"kind": "rename", "key": s.k("string"), "newKey": s.k("taken")})), "already exists"),
        ("rename of a missing key", change(json!({"kind": "rename", "key": s.k("ghost"), "newKey": s.k("g2")})), "does not exist"),
        ("a zero TTL", change(json!({"kind": "expire", "key": s.k("string"), "seconds": 0})), "at least 1 second"),
        ("a NaN score", KvChange::ZAdd { key: s.k("z"), member: KvBytes { text: "m".into(), ..Default::default() }, score: f64::NAN, expect: None, expect_absent: false }, "NaN"),
        ("an empty stream entry", change(json!({"kind": "xadd", "key": s.k("x"), "fields": []})), "at least one field"),
        ("a bad stream id", change(json!({"kind": "xdel", "key": s.k("x"), "id": "1 2"})), "invalid stream entry id"),
        ("a stream id past 64 bits", change(json!({"kind": "xdel", "key": s.k("x"), "id": "99999999999999999999-0"})), "invalid stream entry id"),
        ("a stream id of hyphens", change(json!({"kind": "xdel", "key": s.k("x"), "id": "---"})), "invalid stream entry id"),
        ("a stream id with two hyphens", change(json!({"kind": "xdel", "key": s.k("x"), "id": "1-2-3"})), "invalid stream entry id"),
        ("a stream id with a signed part", change(json!({"kind": "xdel", "key": s.k("x"), "id": "1--2"})), "invalid stream entry id"),
        ("a stream id without a sequence", change(json!({"kind": "xdel", "key": s.k("x"), "id": "5-"})), "invalid stream entry id"),
        ("bad base64", change(json!({"kind": "setString", "key": s.k("b"), "value": {"b64": "!!"}})), "invalid base64"),
        ("invalid JSON", change(json!({"kind": "jsonSet", "key": s.k("j"), "value": "{nope"})), "invalid JSON"),
        ("a sub-path of a missing document", change(json!({"kind": "jsonSet", "key": s.k("j"), "path": "$.a", "value": "1"})), "does not exist"),
        ("lset after a push", change(json!({"kind": "lset", "key": s.k("list"), "index": 0, "value": t("x")})), "separate batch"),
    ];
    for (name, bad, why) in cases {
        let mut batch = vec![new("first"), new("second")];
        if name == "lset after a push" {
            batch.push(change(json!({"kind": "rpush", "key": s.k("list"), "value": t("c")})));
        }
        batch.push(bad);
        let at = batch.len() - 1;
        let err = drv.apply_changes(&batch).await.unwrap_err().to_string();
        assert!(err.starts_with(&format!("change {at}:")) && err.contains(why), "{name}: {err}");
        let any: i64 = q(&mut con, &["EXISTS", &s.k("first"), &s.k("second")]).await;
        assert_eq!(any, 0, "{name}: the earlier changes must not be written");
    }
    let list: Vec<String> = q(&mut con, &["LRANGE", &s.k("list"), "0", "-1"]).await;
    assert_eq!(list, ["a", "b"], "nothing touched the list");

    // The one thing Redis cannot take back: a command that fails only at EXEC. It is named, and
    // the error says what happened.
    let _: () = redis::cmd("XADD").arg(s.k("top")).arg("9-0").arg("a").arg("b").query_async(&mut con).await.unwrap();
    let late = vec![
        new("applied"),
        change(json!({"kind": "xadd", "key": s.k("top"), "id": "1-0", "fields": [{"field": t("a"), "value": t("b")}]})),
        new("after"),
    ];
    let err = drv.apply_changes(&late).await.unwrap_err().to_string();
    // The UI matches this phrase; the rest says what is true: EXEC ran the commands after it too.
    assert!(err.starts_with("change 1:") && err.contains("Redis cannot roll a failed command back"), "{err}");
    assert!(err.contains("every other command in the batch ran") && !err.contains("changes before it"), "{err}");
    let applied: i64 = q(&mut con, &["EXISTS", &s.k("applied")]).await;
    let after: i64 = q(&mut con, &["EXISTS", &s.k("after")]).await;
    assert_eq!((applied, after), (1, 1), "the commands before AND after the failed one ran");
    // Two failures: the first is named, the other is counted.
    let two = vec![
        change(json!({"kind": "xadd", "key": s.k("top"), "id": "2-0", "fields": [{"field": t("a"), "value": t("b")}]})),
        new("between"),
        change(json!({"kind": "xadd", "key": s.k("top"), "id": "3-0", "fields": [{"field": t("a"), "value": t("b")}]})),
    ];
    let err = drv.apply_changes(&two).await.unwrap_err().to_string();
    assert!(err.starts_with("change 0:") && err.contains("1 more failed (change 2)"), "{err}");
}

#[tokio::test]
async fn a_stale_expectation_is_a_conflict() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let s = Scratch::new(&url, "expect");
    let mut con = s.raw().await;
    let _: () = redis::cmd("SET").arg(s.k("str")).arg("v1").query_async(&mut con).await.unwrap();
    let _: () = redis::cmd("HSET").arg(s.k("h")).arg("f").arg("1").query_async(&mut con).await.unwrap();
    let _: () = redis::cmd("RPUSH").arg(s.k("l")).arg("a").arg("b").query_async(&mut con).await.unwrap();

    // Still what was loaded: applied.
    let fresh = vec![
        change(json!({"kind": "setString", "key": s.k("str"), "value": t("v2"), "expect": t("v1")})),
        change(json!({"kind": "hset", "key": s.k("h"), "field": t("f"), "value": t("2"), "expect": t("1")})),
        change(json!({"kind": "lset", "key": s.k("l"), "index": 1, "value": t("B"), "expect": t("b")})),
    ];
    drv.apply_changes(&fresh).await.unwrap();

    // Someone else changed each one since: refused, and the others in the batch are not written.
    for (i, stale) in [
        change(json!({"kind": "setString", "key": s.k("str"), "value": t("v3"), "expect": t("v1")})),
        change(json!({"kind": "hset", "key": s.k("h"), "field": t("f"), "value": t("3"), "expect": t("1")})),
        change(json!({"kind": "hdel", "key": s.k("h"), "field": t("f"), "expect": t("1")})),
        change(json!({"kind": "lset", "key": s.k("l"), "index": 1, "value": t("Z"), "expect": t("b")})),
        change(json!({"kind": "setString", "key": s.k("h"), "value": t("v"), "expect": t("1")})),
    ]
    .into_iter()
    .enumerate()
    {
        let batch = vec![change(json!({"kind": "setString", "key": s.k("other"), "value": t("o")})), stale];
        let err = drv.apply_changes(&batch).await.unwrap_err().to_string();
        assert!(err.starts_with("change 1:") && err.contains(KV_CHANGED), "case {i}: {err}");
    }
    let (a, b, c, o): (String, String, Vec<String>, i64) = (
        q(&mut con, &["GET", &s.k("str")]).await,
        q(&mut con, &["HGET", &s.k("h"), "f"]).await,
        q(&mut con, &["LRANGE", &s.k("l"), "0", "-1"]).await,
        q(&mut con, &["EXISTS", &s.k("other")]).await,
    );
    assert_eq!((a.as_str(), b.as_str(), c, o), ("v2", "2", vec!["a".to_string(), "B".into()], 0));

    // A deleted key no longer matches what was loaded either.
    let _: i64 = q(&mut con, &["DEL", &s.k("str")]).await;
    let gone = change(json!({"kind": "setString", "key": s.k("str"), "value": t("v"), "expect": t("v2")}));
    assert!(drv.apply_changes(&[gone]).await.unwrap_err().to_string().contains(KV_CHANGED));
}

/// A NEW hash field or sorted-set member expects to still be absent, and a score edit expects the score
/// it loaded: someone else's write in between is a conflict, not a silent overwrite.
#[tokio::test]
async fn an_absent_expectation_is_a_conflict_when_it_appeared() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let s = Scratch::new(&url, "absent");
    let mut con = s.raw().await;
    let _: () = redis::cmd("HSET").arg(s.k("h")).arg("old").arg("1").query_async(&mut con).await.unwrap();
    let _: () = redis::cmd("ZADD").arg(s.k("z")).arg("1.5").arg("old").query_async(&mut con).await.unwrap();

    // Still absent: applied, on an existing key and on a key that does not exist yet.
    let ok = vec![
        change(json!({"kind": "hset", "key": s.k("h"), "field": t("new"), "value": t("2"), "expectAbsent": true})),
        change(json!({"kind": "hset", "key": s.k("nohash"), "field": t("f"), "value": t("2"), "expectAbsent": true})),
        change(json!({"kind": "zadd", "key": s.k("z"), "member": t("new"), "score": 3, "expectAbsent": true})),
        change(json!({"kind": "zadd", "key": s.k("noz"), "member": t("m"), "score": 1, "expectAbsent": true})),
        // A score edit that loaded 1.5 (the app renders it so; Redis answers 1.5 as well).
        change(json!({"kind": "zadd", "key": s.k("z"), "member": t("old"), "score": 2, "expect": t("1.5")})),
    ];
    drv.apply_changes(&ok).await.unwrap();
    let (h, z): (String, f64) = (q(&mut con, &["HGET", &s.k("h"), "new"]).await, q(&mut con, &["ZSCORE", &s.k("z"), "old"]).await);
    assert_eq!((h.as_str(), z), ("2", 2.0));

    // Someone else wrote the field / member after this one was loaded: refused, the rest not written.
    let new = |n: &str| change(json!({"kind": "setString", "key": s.k(n), "value": t("v")}));
    for (i, stale) in [
        change(json!({"kind": "hset", "key": s.k("h"), "field": t("new"), "value": t("9"), "expectAbsent": true})),
        change(json!({"kind": "zadd", "key": s.k("z"), "member": t("new"), "score": 9, "expectAbsent": true})),
        change(json!({"kind": "zadd", "key": s.k("z"), "member": t("old"), "score": 9, "expect": t("1.5")})),
        change(json!({"kind": "zadd", "key": s.k("z"), "member": t("ghost"), "score": 9, "expect": t("1")})),
    ]
    .into_iter()
    .enumerate()
    {
        let err = drv.apply_changes(&[new("first"), stale]).await.unwrap_err().to_string();
        assert!(err.starts_with("change 1:") && err.contains(KV_CHANGED), "case {i}: {err}");
    }
    let (h, z, first): (String, f64, i64) = (
        q(&mut con, &["HGET", &s.k("h"), "new"]).await,
        q(&mut con, &["ZSCORE", &s.k("z"), "old"]).await,
        q(&mut con, &["EXISTS", &s.k("first")]).await,
    );
    assert_eq!((h.as_str(), z, first), ("2", 2.0, 0), "nothing was overwritten or written");
    // An edit that asks for both is refused before anything is sent.
    let both = change(json!({"kind": "hset", "key": s.k("h"), "field": t("x"), "value": t("1"), "expect": t("1"), "expectAbsent": true}));
    assert!(drv.apply_changes(&[both]).await.unwrap_err().to_string().contains("cannot both be set"));
}

#[tokio::test]
async fn json_documents_read_and_write() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let s = Scratch::new(&url, "json");

    let root = change(json!({"kind": "jsonSet", "key": s.k("doc"), "value": r#"{"n":1,"tags":["a"]}"#}));
    drv.apply_changes(&[root]).await.unwrap();
    let r = drv.read(&s.k("doc"), &KvReadOpts::default()).await.unwrap();
    assert_eq!(r.kind, "ReJSON-RL");
    let doc: Value = serde_json::from_str(&text(&r.items[0])).unwrap();
    assert_eq!(doc, json!({"n": 1, "tags": ["a"]}));

    let sub = |v: &str, expect: &str| {
        change(json!({"kind": "jsonSet", "key": s.k("doc"), "path": "$.n", "value": v, "expect": t(expect)}))
    };
    // `JSON.GET key $.n` is the value in a one-element array.
    drv.apply_changes(&[sub("2", "[1]")]).await.unwrap();
    let err = drv.apply_changes(&[sub("3", "[1]")]).await.unwrap_err().to_string();
    assert!(err.contains(KV_CHANGED), "{err}");
    let n = drv.read(&s.k("doc"), &KvReadOpts { path: Some(".n".into()), ..Default::default() }).await.unwrap();
    assert_eq!(text(&n.items[0]), "2");

    // A string key is not a document.
    let _: () = redis::cmd("SET").arg(s.k("str")).arg("x").query_async(&mut s.raw().await).await.unwrap();
    let wrong = change(json!({"kind": "jsonSet", "key": s.k("str"), "value": "1"}));
    assert!(drv.apply_changes(&[wrong]).await.unwrap_err().to_string().contains("WRONGTYPE"));
}

#[tokio::test]
async fn a_module_missing_on_the_server_is_named() {
    // The ACL server is plain Redis: no RedisJSON.
    let Some(url) = live_url("TIQ_REDIS_ADMIN_URL") else { return };
    let drv = open(&url).await;
    let s = Scratch::new(&url, "nojson");
    let set = change(json!({"kind": "jsonSet", "key": s.k("doc"), "value": "{}"}));
    let err = drv.apply_changes(&[set]).await.unwrap_err().to_string();
    assert!(err.starts_with("change 0:") && err.contains("RedisJSON module is not loaded"), "{err}");
    let KvReply::Error { message } = drv.command("JSON.GET x", false).await.unwrap() else { panic!() };
    assert!(message.contains("RedisJSON module is not loaded"), "{message}");
}

#[tokio::test]
async fn the_console_renders_replies_and_asks_before_deleting() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let s = Scratch::new(&url, "con");
    let run = |line: String, confirmed: bool| {
        let drv = &drv;
        async move { serde_json::to_value(drv.command(&line, confirmed).await.unwrap()).unwrap() }
    };

    assert_eq!(run("GET demo:1".into(), false).await, json!({"type": "bulk", "text": "hello", "binary": false}));
    assert_eq!(run("get tiq_no_such_key".into(), false).await, json!({"type": "nil"}));
    assert_eq!(run("TYPE list:a".into(), false).await, json!({"type": "status", "value": "list"}));
    assert_eq!(run(format!("SET {} 1", s.k("n")), false).await, json!({"type": "status", "value": "OK"}));
    assert_eq!(run(format!("INCR {}", s.k("n")), false).await, json!({"type": "int", "value": 2}));
    let l = run("LRANGE list:a 0 -1".into(), false).await;
    assert_eq!(l["type"], "array");
    assert_eq!(l["items"].as_array().unwrap().len(), 3);
    assert_eq!(l["items"][2], json!({"type": "bulk", "text": "z", "binary": false}));
    let nested = run("XRANGE stream:a - + COUNT 1".into(), false).await;
    assert_eq!(nested["items"][0]["items"][0]["text"], "1700000000000-0");
    assert_eq!(nested["items"][0]["items"][1]["items"][0]["text"], "type");
    assert_eq!(run("GET bin:1".into(), false).await["b64"], "//4AYWJj");

    // Quoting, escapes and the server's own refusals.
    assert_eq!(run(format!(r#"SET "{}" "a b\x41""#, s.k("q")), false).await["type"], "status");
    assert_eq!(run(format!("GET {}", s.k("q")), false).await["text"], "a bA");
    let wrong = run("LPUSH demo:1 x".into(), false).await;
    assert_eq!(wrong["type"], "error");
    assert!(wrong["message"].as_str().unwrap().starts_with("WRONGTYPE"), "{wrong}");
    assert_eq!(run("GET".into(), false).await["type"], "error");

    // DEL / UNLINK / FLUSHDB only once confirmed.
    for line in [format!("DEL {}", s.k("n")), format!("unlink {}", s.k("n")), format!("GetDel {}", s.k("n")), "FLUSHDB".into()] {
        let err = drv.command(&line, false).await.unwrap_err().to_string();
        assert!(err.contains("deletes data"), "{line}: {err}");
    }
    assert_eq!(run(format!("GET {}", s.k("n")), false).await["text"], "2", "refused means not run");
    assert_eq!(run(format!("DEL {}", s.k("n")), true).await, json!({"type": "int", "value": 1}));

    // FLUSHDB, confirmed, in a database nothing else uses: and only that one is emptied.
    let scratch_db = open(&in_db(&url, 15)).await;
    let before = seeded(&drv).await;
    scratch_db.command("SET tiq:f 1", false).await.unwrap();
    assert_eq!(scratch_db.dbsize().await.unwrap(), 1);
    assert_eq!(serde_json::to_value(scratch_db.command("FLUSHDB", true).await.unwrap()).unwrap()["value"], "OK");
    assert_eq!(scratch_db.dbsize().await.unwrap(), 0);
    assert_eq!(seeded(&drv).await, before, "only database 15 was flushed");
}

#[tokio::test]
async fn the_console_refuses_the_dangerous_commands_before_sending() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let mut con = Scratch::new(&url, "deny").raw().await;
    let size = seeded(&drv).await;
    let policy: Vec<String> = q(&mut con, &["CONFIG", "GET", "maxmemory-policy"]).await;

    let denied = [
        "FLUSHALL", "flushall async", "  FlushAll  ", "\"FLUSHALL\"", "\nFLUSHALL", "CONFIG SET maxmemory-policy allkeys-lru",
        "config set save \"\"", "CONFIG REWRITE", "CONFIG RESETSTAT", "DEBUG SLEEP 5", "DEBUG SEGFAULT", "DEBUG RELOAD", "SHUTDOWN",
        "SHUTDOWN NOSAVE", "SLAVEOF 10.0.0.1 6379", "REPLICAOF NO ONE", "MIGRATE 10.0.0.1 6379 demo:1 0 100", "MODULE LOAD /x.so",
        "MODULE UNLOAD ReJSON", "SCRIPT FLUSH", "EVAL \"return redis.call('flushall')\" 0", "EVALSHA abc 0", "EVAL_RO \"return 1\" 0",
        "FUNCTION FLUSH", "FUNCTION LOAD x", "FCALL f 0", "ACL SETUSER default off", "ACL DELUSER agent", "CLUSTER RESET HARD",
        "BGSAVE", "SAVE", "BGREWRITEAOF", "KEYS *", "keys user:*", "CLIENT KILL TYPE normal", "CLIENT PAUSE 100000", "CLIENT NO-EVICT on",
        "CLIENT REPLY OFF", "FAILOVER", "SYNC", "PSYNC ? -1", "SWAPDB 0 1", "SELECT 3", "AUTH nobody", "HELLO 3", "RESET", "QUIT", "MULTI",
        "EXEC", "DISCARD", "WATCH demo:1", "UNWATCH", "READONLY", "MONITOR", "SUBSCRIBE c", "PSUBSCRIBE c*", "UNSUBSCRIBE",
        "BLPOP nothing 0", "BRPOP nothing 0", "BRPOPLPUSH a b 0", "BLMOVE a b LEFT RIGHT 0", "BZPOPMIN z 0", "BLMPOP 0 1 l LEFT",
        "XREAD BLOCK 0 STREAMS stream:a $", "xreadgroup group g c block 0 streams stream:a >", "WAIT 1 0",
        // module administration: settings, scripts, diagnostics, internals
        "FT.CONFIG SET TIMEOUT 1", "ft.config get *", "FT.DEBUG DUMP_TERMS idx", "BF.DEBUG k", "JSON.DEBUG MEMORY json:a",
        "TFUNCTION LOAD \"#!js api_version=1.0 name=x\"", "TFCALL x.f 0", "TFCALLASYNC x.f 0", "_rg_internals.function x",
        "redisgears_2.CLUSTERSET x", "timeseries.INFOCLUSTER", "FT._LIST", "SENTINEL MASTERS",
    ];
    let t0 = std::time::Instant::now();
    for line in denied {
        let err = drv.command(line, true).await.expect_err(line).to_string(); // not even `confirmed` opens them
        assert!(err.contains("not available in the console") || err.contains("empty"), "{line}: {err}");
        let args = split_command(line).unwrap();
        assert!(check_command(&args).is_err(), "{line}");
    }
    assert!(t0.elapsed().as_secs() < 3, "DEBUG SLEEP must not have run");

    // The same words allowed to pass would have been felt: nothing was.
    assert_eq!(seeded(&drv).await, size, "FLUSHALL / DEL never ran");
    assert_eq!(drv.get("demo:1").await.unwrap().rows[0][0], "hello");
    let after: Vec<String> = q(&mut con, &["CONFIG", "GET", "maxmemory-policy"]).await;
    assert_eq!(policy, after, "CONFIG SET never ran");

    // What is fine stays fine, case-insensitively.
    for line in ["get demo:1", "Info keyspace", "SCAN 0 COUNT 5", "JSON.GET json:a", "OBJECT ENCODING list:a", "XREAD COUNT 1 STREAMS stream:a 0", "PING", "DBSIZE"] {
        assert_eq!(check_command(&split_command(line).unwrap()), Ok(Gate::Allow), "{line}");
        assert!(drv.command(line, false).await.is_ok(), "{line}");
    }
    assert!(drv.command("", false).await.is_err());
    assert!(drv.command("SET \"unterminated", false).await.is_err());
}

/// Every console command that removes data its line does not spell out is refused until the user confirms
/// it, and then does what it says: the gate is decided by the name, before anything is sent.
#[tokio::test]
async fn the_console_asks_before_it_removes_data() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let s = Scratch::new(&url, "confirm");
    let mut con = s.raw().await;
    let (str_, list, zset, stream, json) = (s.k("str"), s.k("list"), s.k("zset"), s.k("stream"), s.k("json"));
    let reset = |con: &mut MultiplexedConnection| {
        let (str_, list, zset, stream, json) = (str_.clone(), list.clone(), zset.clone(), stream.clone(), json.clone());
        let con = con.clone();
        async move {
            let mut con = con;
            let _: () = redis::pipe()
                .cmd("SET").arg(&str_).arg("v")
                .cmd("DEL").arg(&list).arg(&zset).arg(&stream).arg(&json)
                .cmd("RPUSH").arg(&list).arg("a").arg("b").arg("c")
                .cmd("ZADD").arg(&zset).arg(1).arg("a").arg(2).arg("b")
                .cmd("XADD").arg(&stream).arg("1-0").arg("f").arg("v")
                .cmd("XADD").arg(&stream).arg("2-0").arg("f").arg("v")
                .cmd("JSON.SET").arg(&json).arg("$").arg("{\"a\":1}")
                .query_async(&mut con)
                .await
                .unwrap();
        }
    };
    let lines = [
        (format!("EXPIRE {str_} 1000"), str_.clone()),
        (format!("pexpire {str_} 1000000"), str_.clone()),
        (format!("EXPIREAT {str_} 4102444800"), str_.clone()),
        (format!("PEXPIREAT {str_} 4102444800000"), str_.clone()),
        (format!("LTRIM {list} 0 0"), list.clone()),
        (format!("LPOP {list}"), list.clone()),
        (format!("RPOP {list} 2"), list.clone()),
        (format!("LMPOP 1 {list} LEFT"), list.clone()),
        (format!("ZPOPMIN {zset}"), zset.clone()),
        (format!("ZREMRANGEBYRANK {zset} 0 -1"), zset.clone()),
        (format!("ZREMRANGEBYSCORE {zset} -inf +inf"), zset.clone()),
        (format!("ZREMRANGEBYLEX {zset} - +"), zset.clone()),
        (format!("XTRIM {stream} MAXLEN 0"), stream.clone()),
        (format!("RENAME {str_} {}", s.k("renamed")), str_.clone()),
        (format!("JSON.DEL {json} $.a"), json.clone()),
        (format!("json.forget {json} $.a"), json.clone()),
        (format!("COPY {str_} {} REPLACE", s.k("copied")), s.k("copied")),
    ];
    for (line, _) in &lines {
        reset(&mut con).await;
        let before: Vec<Vec<u8>> = q(&mut con, &["KEYS", &format!("{}*", s.prefix)]).await;
        let err = drv.command(line, false).await.expect_err(line).to_string();
        assert!(err.contains("deletes data — confirm it to run it"), "{line}: {err}");
        let after: Vec<Vec<u8>> = q(&mut con, &["KEYS", &format!("{}*", s.prefix)]).await;
        assert_eq!(before.len(), after.len(), "{line} ran without being confirmed");
        let ttl: i64 = q(&mut con, &["TTL", &str_]).await;
        assert_eq!(ttl, -1, "{line} ran without being confirmed");
        assert!(drv.command(line, true).await.is_ok(), "{line}");
    }
    // Spelled-out targets and plain edits are not asked about.
    reset(&mut con).await;
    for line in [format!("SET {str_} w"), format!("SREM {}x a", s.prefix), format!("HDEL {}h f", s.prefix), format!("ZREM {zset} a"), format!("XDEL {stream} 1-0"), format!("COPY {str_} {}c2", s.prefix)] {
        assert!(drv.command(&line, false).await.is_ok(), "{line}");
    }
}

#[tokio::test]
async fn commands_are_logged_as_redis_would_take_them() {
    let Some(url) = live_url("TIQ_REDIS_URL") else { return };
    let drv = open(&url).await;
    let log = Arc::new(SqlLog::default());
    CURRENT
        .scope(log.clone(), async {
            drv.scan_page("demo:*", "0", 10).await.unwrap();
            drv.read("list:a", &KvReadOpts::default()).await.unwrap();
            drv.apply_changes(&[change(json!({"kind": "setString", "key": "tiq_log_k", "value": t("a b")}))]).await.unwrap();
            drv.command("DEL tiq_log_k", true).await.unwrap();
        })
        .await;
    let all: Vec<String> = log.since(0).into_iter().map(|e| e.sql).collect();
    for want in [
        "SCAN 0 MATCH demo:* COUNT 10",
        "TYPE list:a",
        "LRANGE list:a 0 99",
        "WATCH tiq_log_k",
        "MULTI",
        "SET tiq_log_k \"a b\" KEEPTTL",
        "EXEC",
        "DEL tiq_log_k",
    ] {
        assert!(all.iter().any(|l| l == want), "{want} not in {all:#?}");
    }
}

#[tokio::test]
async fn an_acl_user_connects_and_is_refused_cleanly() {
    let (Some(acl), Some(admin)) = (live_url("TIQ_REDIS_ACL_URL"), live_url("TIQ_REDIS_ADMIN_URL")) else { return };
    let host = acl.rsplit_once('@').unwrap().1.to_string(); // 127.0.0.1:56380/0

    // Credentials are checked at connect: wrong password, no password, unknown user.
    for (what, url) in [
        ("wrong password", format!("redis://agent:nope@{host}")),
        ("no password", format!("redis://{host}")),
        ("unknown user", format!("redis://ghost:agentpw@{host}")),
    ] {
        let err = RedisDriver::connect(&url).await.err().unwrap_or_else(|| panic!("{what} connected")).to_string();
        assert!(err.contains("WRONGPASS") || err.contains("NOAUTH"), "{what}: {err}");
    }

    let drv = open(&acl).await;
    let s = Scratch::under(&admin, "agent:", "acl");

    // Its own keys work.
    assert_eq!(text(&drv.read("agent:1", &KvReadOpts::default()).await.unwrap().items[0]), "mine");
    assert_eq!(drv.scan_page("agent:*", "0", 10).await.unwrap().keys.len(), 2);
    drv.apply_changes(&[change(json!({"kind": "setString", "key": s.k("ok"), "value": t("1")}))]).await.unwrap();
    assert_eq!(text(&drv.read(&s.k("ok"), &KvReadOpts::default()).await.unwrap().items[0]), "1");
    assert_eq!(drv.list_databases().await.unwrap().len(), 16, "no CONFIG: the default of 16");

    // Anything else is refused with the server's own words, not a crash.
    let err = drv.read("other:1", &KvReadOpts::default()).await.unwrap_err().to_string();
    assert!(err.starts_with("NOPERM"), "{err}");
    let err = drv.scan_page("*", "0", 100).await.unwrap_err().to_string();
    assert!(err.contains("NOPERM"), "{err}");
    let batch = vec![
        change(json!({"kind": "setString", "key": s.k("first"), "value": t("1")})),
        change(json!({"kind": "setString", "key": "other:written", "value": t("1")})),
    ];
    let err = drv.apply_changes(&batch).await.unwrap_err().to_string();
    assert!(err.starts_with("change 1:") && err.contains("NOPERM"), "{err}");
    let mut con = s.raw().await;
    let n: i64 = q(&mut con, &["EXISTS", &s.k("first"), "other:written"]).await;
    assert_eq!(n, 0, "a refused change rolls the batch back");

    let KvReply::Error { message } = drv.command("GET other:1", false).await.unwrap() else { panic!() };
    assert!(message.starts_with("NOPERM"), "{message}");
    let KvReply::Error { message } = drv.command("PUBLISH c x", false).await.unwrap() else { panic!() };
    assert!(message.starts_with("NOPERM"), "{message}");
    assert!(drv.command("FLUSHALL", true).await.unwrap_err().to_string().contains("not available"));
    let other: String = q(&mut con, &["GET", "other:1"]).await;
    assert_eq!(other, "secret");

    // The admin of the same server sees it all (and has CONFIG).
    assert!(open(&admin).await.scan_page("*", "0", 100).await.unwrap().keys.len() >= 3);
}
