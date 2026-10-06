//! MongoDB DocumentDriver against real servers (docker/mongo/docker-compose.yml):
//!   docker compose -f docker/mongo/docker-compose.yml up -d --wait
//!   TIQ_MONGO_URL='mongodb://127.0.0.1:57017/tableiq?directConnection=true'   # replica set
//!   TIQ_MONGO_STANDALONE_URL=mongodb://127.0.0.1:57018/tableiq               # no transactions
//!   cargo test --test mongo_browse
//!
//! `users` and `orders` are the shared seed: read-only here. Every test that writes uses a
//! collection of its own, which a guard drops afterwards (also on a failed assertion).

mod common;
use common::live_url;

use mongodb::bson::{doc, Document};
use mongodb::Client;
use serde_json::{json, Value};
use tableiq_lib::drivers::{
    doc_mongo::MongoDriver, DocChange, DocumentDriver, FindOpts, CHANGED_SINCE_LOADED, ROW_GONE,
};

const DB: &str = "tableiq";

fn opts(filter: &str, sort: &str, skip: u64, limit: i64) -> FindOpts {
    FindOpts { filter: filter.into(), sort: sort.into(), skip, limit, ..Default::default() }
}

fn parse(docs: &[String]) -> Vec<Value> {
    docs.iter().map(|d| serde_json::from_str(d).unwrap()).collect()
}

fn names(docs: &[String]) -> Vec<String> {
    parse(docs).iter().map(|d| d["name"].as_str().unwrap_or("").to_string()).collect()
}

fn update(id: Value, set: Value, expect: Value) -> DocChange {
    serde_json::from_value(json!({ "kind": "update", "_id": id, "set": set, "expect": expect })).unwrap()
}
fn delete(id: Value) -> DocChange {
    serde_json::from_value(json!({ "kind": "delete", "_id": id })).unwrap()
}
fn insert(d: Value) -> DocChange {
    serde_json::from_value(json!({ "kind": "insert", "doc": d })).unwrap()
}

/// A scratch collection: unique name, dropped on drop (the runtime is the test's, so the drop
/// is spawned onto a throwaway one).
struct Scratch {
    url: String,
    name: String,
}

impl Scratch {
    fn new(url: &str, tag: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        Self { url: url.into(), name: format!("tiq_{tag}_{nanos}") }
    }
    /// The raw driver-free view, to check what is really stored.
    async fn raw(&self) -> mongodb::Collection<Document> {
        Client::with_uri_str(&self.url).await.unwrap().database(DB).collection(&self.name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let (url, name) = (self.url.clone(), self.name.clone());
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
                let _ = Client::with_uri_str(&url).await.unwrap().database(DB).collection::<Document>(&name).drop().await;
            })
        })
        .join()
        .ok();
    }
}

async fn replica() -> Option<(MongoDriver, String)> {
    let url = live_url("TIQ_MONGO_URL")?;
    Some((MongoDriver::connect(&url, DB).await.unwrap(), url))
}

async fn standalone() -> Option<(MongoDriver, String)> {
    let url = live_url("TIQ_MONGO_STANDALONE_URL")?;
    Some((MongoDriver::connect(&url, DB).await.unwrap(), url))
}

#[tokio::test]
async fn collections_and_find() {
    let Some((drv, _)) = replica().await else { return };
    let colls = drv.list_collections().await.unwrap();
    assert!(colls.iter().any(|c| c == "users") && colls.iter().any(|c| c == "orders"), "{colls:?}");

    let docs = drv.find("users", &FindOpts::default()).await.unwrap();
    assert!(docs.len() >= 3);
    assert!(docs.iter().any(|d| d.contains("Ada")), "docs: {docs:?}");
}

#[tokio::test]
async fn url_database_is_the_default() {
    let Some(url) = live_url("TIQ_MONGO_URL") else { return };
    // No database argument: the URL's path names it.
    let drv = MongoDriver::connect(&url, "").await.unwrap();
    assert!(drv.list_collections().await.unwrap().iter().any(|c| c == "users"));
}

#[tokio::test]
async fn filter_sort_skip_limit_projection() {
    let Some((drv, _)) = replica().await else { return };
    let by_age_desc = drv.find("users", &opts("", r#"{"age": -1}"#, 0, 0)).await.unwrap();
    assert_eq!(&names(&by_age_desc)[..3], ["Grace", "Linus", "Ada"]);

    let second = drv.find("users", &opts("", r#"{"age": -1}"#, 1, 1)).await.unwrap();
    assert_eq!(names(&second), ["Linus"]);

    let old = drv.find("users", &opts(r#"{"age": {"$gt": 40}}"#, r#"{"name": 1}"#, 0, 0)).await.unwrap();
    assert_eq!(names(&old), ["Grace", "Linus"]);

    let nested = drv.find("users", &opts(r#"{"address.city": "London"}"#, "", 0, 0)).await.unwrap();
    assert_eq!(names(&nested), ["Ada"]);

    let projected = drv
        .find("users", &FindOpts { filter: r#"{"name":"Ada"}"#.into(), projection: r#"{"name":1,"_id":0}"#.into(), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(parse(&projected), [json!({"name": "Ada"})]);

    // Extended JSON in the filter: an _id of each type the seed has.
    let by_str = drv.find("orders", &opts(r#"{"_id": "ord-string-id"}"#, "", 0, 0)).await.unwrap();
    let by_num = drv.find("orders", &opts(r#"{"_id": 42}"#, "", 0, 0)).await.unwrap();
    assert_eq!((by_str.len(), by_num.len()), (1, 1));
    let by_oid = drv.find("orders", &opts(r#"{"customer": "Ada"}"#, "", 0, 0)).await.unwrap();
    let oid = parse(&by_oid)[0]["_id"]["$oid"].as_str().unwrap().to_string();
    let again = drv.find("orders", &opts(&format!(r#"{{"_id": {{"$oid": "{oid}"}}}}"#), "", 0, 0)).await.unwrap();
    assert_eq!(again.len(), 1);
}

#[tokio::test]
async fn count_estimated_and_filtered() {
    let Some((drv, _)) = replica().await else { return };
    assert!(drv.count("users", "").await.unwrap() >= 3);
    assert_eq!(drv.count("users", r#"{"age": {"$gt": 40}}"#).await.unwrap(), 2);
    assert_eq!(drv.count("users", r#"{"name": "nobody"}"#).await.unwrap(), 0);
}

#[tokio::test]
async fn malformed_and_oversized_input_is_refused_before_the_server() {
    let Some((drv, _)) = replica().await else { return };
    for bad in ["{not json", "[1, 2]", "42", r#"{"_id": {"$oid": "zz"}}"#] {
        let err = drv.find("users", &opts(bad, "", 0, 0)).await.unwrap_err().to_string();
        assert!(err.starts_with("filter:"), "{bad} -> {err}");
        assert!(drv.count("users", bad).await.is_err(), "{bad}");
    }
    assert!(drv.find("users", &opts("", "{bad", 0, 0)).await.unwrap_err().to_string().starts_with("sort:"));
    assert!(drv.find("users", &opts("", "", 0, 5001)).await.unwrap_err().to_string().contains("limit"));
    assert!(drv.find("users", &opts("", "", 0, -1)).await.is_err());
    let huge = format!(r#"{{"a": "{}"}}"#, "x".repeat(2 << 20));
    assert!(drv.count("users", &huge).await.unwrap_err().to_string().contains("KiB"));
}

#[tokio::test]
async fn canonical_json_round_trips_every_type() {
    let Some((drv, url)) = replica().await else { return };
    let s = Scratch::new(&url, "types");
    let oid = "65f1c2a4e4b0a1b2c3d4e5f6";
    let original = json!({
        "_id": {"$oid": oid},
        "name": "typed",
        "i32": {"$numberInt": "7"},
        "i64": {"$numberLong": "9007199254740993"},
        "dbl": {"$numberDouble": "1.5"},
        "dec": {"$numberDecimal": "12345.6789012345678901234"},
        "when": {"$date": {"$numberLong": "1700000000123"}},
        "nested": {"b": {"$numberInt": "1"}, "a": {"list": [{"$numberLong": "1"}, "x", null, {"deep": [true]}]}},
    });
    let r = drv.apply_changes(&s.name, &[insert(original.clone())]).await.unwrap();
    assert_eq!(r.counts.inserted, 1);

    // Stored with the right BSON types…
    let raw = s.raw().await.find_one(doc! {"name": "typed"}).await.unwrap().unwrap();
    assert!(matches!(raw.get("i32"), Some(mongodb::bson::Bson::Int32(7))));
    assert!(matches!(raw.get("i64"), Some(mongodb::bson::Bson::Int64(9007199254740993))));
    assert!(matches!(raw.get("dec"), Some(mongodb::bson::Bson::Decimal128(_))));
    assert!(matches!(raw.get("when"), Some(mongodb::bson::Bson::DateTime(_))));
    assert!(matches!(raw.get("_id"), Some(mongodb::bson::Bson::ObjectId(_))));

    // …and read back as the same canonical JSON, field order included (`b` before `a`).
    let docs = drv.find(&s.name, &FindOpts::default()).await.unwrap();
    let got: Value = serde_json::from_str(&docs[0]).unwrap();
    assert_eq!(got["nested"], original["nested"]);
    for k in ["_id", "i32", "i64", "dbl", "dec"] {
        assert_eq!(got[k], original[k], "{k}");
    }
    assert_eq!(got["when"], json!({"$date": {"$numberLong": "1700000000123"}}));
    let keys: Vec<_> = got["nested"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["b", "a"]);

    // Feeding the returned values back as `expect` keeps matching (the types survive), and a
    // nested field can be edited without disturbing its siblings.
    let r = drv
        .apply_changes(
            &s.name,
            &[update(
                got["_id"].clone(),
                json!({"nested.a.list.0": {"$numberLong": "2"}}),
                json!({"i64": got["i64"], "dec": got["dec"], "when": got["when"], "nested": got["nested"]}),
            )],
        )
        .await
        .unwrap();
    assert_eq!(r.counts.updated, 1);
    let raw = s.raw().await.find_one(doc! {"name": "typed"}).await.unwrap().unwrap();
    assert!(matches!(raw.get("i64"), Some(mongodb::bson::Bson::Int64(9007199254740993))));
    let after: Value = serde_json::from_str(&drv.find(&s.name, &FindOpts::default()).await.unwrap()[0]).unwrap();
    assert_eq!(after["nested"]["a"]["list"][0], json!({"$numberLong": "2"}));
    assert_eq!(after["nested"]["a"]["list"][1], json!("x"));
}

#[tokio::test]
async fn changes_update_insert_delete_with_every_id_type() {
    let Some((drv, url)) = replica().await else { return };
    let s = Scratch::new(&url, "ids");
    let oid = json!({"$oid": "65f1c2a4e4b0a1b2c3d4e5f7"});
    drv.apply_changes(
        &s.name,
        &[
            insert(json!({"_id": oid, "n": 1})),
            insert(json!({"_id": "str-id", "n": 1})),
            insert(json!({"_id": 42, "n": 1})),
            insert(json!({"n": 1})), // server-generated ObjectId
        ],
    )
    .await
    .unwrap();
    assert_eq!(drv.count(&s.name, r#"{"n": 1}"#).await.unwrap(), 4);

    let ids = [oid.clone(), json!("str-id"), json!(42)];
    let changes: Vec<_> = ids.iter().map(|id| update(id.clone(), json!({"n": 2, "extra": {"k": [1]}}), json!({"n": 1}))).collect();
    let r = drv.apply_changes(&s.name, &changes).await.unwrap();
    assert!(r.transactional);
    assert_eq!((r.counts.updated, r.errors.iter().flatten().count()), (3, 0));
    assert_eq!(drv.count(&s.name, r#"{"n": 2, "extra.k": 1}"#).await.unwrap(), 3);

    // `unset` removes a field; deletes by each _id type.
    let unset: DocChange = serde_json::from_value(json!({"kind": "update", "_id": 42, "unset": ["extra"]})).unwrap();
    drv.apply_changes(&s.name, &[unset]).await.unwrap();
    assert_eq!(drv.count(&s.name, r#"{"extra": {"$exists": false}}"#).await.unwrap(), 2); // 42 + the generated one
    let r = drv.apply_changes(&s.name, &ids.map(delete)).await.unwrap();
    assert_eq!(r.counts.deleted, 3);
    assert_eq!(drv.count(&s.name, "").await.unwrap(), 1);

    // Re-deleting a gone document, and updating one: both are conflicts, nothing else changes.
    for bad in [delete(json!("str-id")), update(json!("str-id"), json!({"n": 3}), json!({}))] {
        let want = if matches!(bad, DocChange::Delete { .. }) { ROW_GONE } else { CHANGED_SINCE_LOADED };
        let err = drv.apply_changes(&s.name, &[bad]).await.unwrap_err().to_string();
        assert_eq!(err, format!("change 0: {want}"));
    }
}

#[tokio::test]
async fn optimistic_conflict_rolls_back_the_whole_batch_on_a_replica_set() {
    let Some((drv, url)) = replica().await else { return };
    let s = Scratch::new(&url, "txn");
    drv.apply_changes(&s.name, &[insert(json!({"_id": 1, "v": "a"})), insert(json!({"_id": 2, "v": "b"}))]).await.unwrap();
    // A second editor changes doc 2 after this one loaded it.
    s.raw().await.update_one(doc! {"_id": 2}, doc! {"$set": {"v": "other"}}).await.unwrap();

    let err = drv
        .apply_changes(
            &s.name,
            &[
                update(json!(1), json!({"v": "A"}), json!({"v": "a"})),
                insert(json!({"_id": 3, "v": "c"})),
                update(json!(2), json!({"v": "B"}), json!({"v": "b"})), // stale
            ],
        )
        .await
        .unwrap_err()
        .to_string();
    assert_eq!(err, format!("change 2: {CHANGED_SINCE_LOADED}"));

    let raw = s.raw().await;
    assert_eq!(raw.find_one(doc! {"_id": 1}).await.unwrap().unwrap().get_str("v").unwrap(), "a", "update 0 rolled back");
    assert!(raw.find_one(doc! {"_id": 3}).await.unwrap().is_none(), "insert rolled back");
    assert_eq!(raw.find_one(doc! {"_id": 2}).await.unwrap().unwrap().get_str("v").unwrap(), "other");

    // A duplicate-key insert in the middle of a batch also takes the whole batch back.
    let err = drv
        .apply_changes(&s.name, &[update(json!(1), json!({"v": "A"}), json!({})), insert(json!({"_id": 2}))])
        .await
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("change 1: ") && err.contains("duplicate key"), "{err}");
    assert_eq!(raw.find_one(doc! {"_id": 1}).await.unwrap().unwrap().get_str("v").unwrap(), "a");
}

#[tokio::test]
async fn standalone_is_best_effort_and_says_so() {
    let Some((drv, url)) = standalone().await else { return };
    let s = Scratch::new(&url, "best");
    drv.apply_changes(&s.name, &[insert(json!({"_id": 1, "v": "a"})), insert(json!({"_id": 2, "v": "b"}))]).await.unwrap();
    s.raw().await.update_one(doc! {"_id": 2}, doc! {"$set": {"v": "other"}}).await.unwrap();

    let r = drv
        .apply_changes(
            &s.name,
            &[
                update(json!(1), json!({"v": "A"}), json!({"v": "a"})),
                update(json!(2), json!({"v": "B"}), json!({"v": "b"})), // stale
                delete(json!(99)),                                       // gone
                insert(json!({"_id": 3})),
            ],
        )
        .await
        .unwrap();
    assert!(!r.transactional);
    assert_eq!((r.counts.updated, r.counts.deleted, r.counts.inserted), (1, 0, 1));
    assert_eq!(r.errors, [None, Some(CHANGED_SINCE_LOADED.to_string()), Some(ROW_GONE.to_string()), None]);

    let raw = s.raw().await;
    assert_eq!(raw.find_one(doc! {"_id": 1}).await.unwrap().unwrap().get_str("v").unwrap(), "A");
    assert_eq!(raw.find_one(doc! {"_id": 2}).await.unwrap().unwrap().get_str("v").unwrap(), "other");
    assert!(raw.find_one(doc! {"_id": 3}).await.unwrap().is_some());
}

#[tokio::test]
async fn a_malformed_change_writes_nothing_even_best_effort() {
    let Some((drv, url)) = standalone().await else { return };
    let s = Scratch::new(&url, "bad");
    let bad_cases = [
        (update(json!(1), json!({}), json!({})), "nothing to update"),
        (update(json!(1), json!({"$inc": 1}), json!({})), "bad field name"),
        (update(json!(1), json!({"v": 1}), json!({"$where": "1"})), "bad field name"),
        (update(json!(1), json!({"v": {"$oid": "zz"}}), json!({})), "invalid Extended JSON"),
        (insert(json!([1])), "must be a JSON object"),
    ];
    for (bad, why) in bad_cases {
        let err = drv.apply_changes(&s.name, &[insert(json!({"_id": 1})), bad]).await.unwrap_err().to_string();
        assert!(err.starts_with("change 1: ") && err.contains(why), "{err}");
    }
    assert_eq!(drv.count(&s.name, "").await.unwrap(), 0, "the valid insert must not have run");
}

#[tokio::test]
async fn expect_and_id_are_compared_literally_never_as_operators() {
    let Some((drv, url)) = replica().await else { return };
    let s = Scratch::new(&url, "eq");
    drv.apply_changes(&s.name, &[insert(json!({"_id": 1, "v": 5}))]).await.unwrap();
    // `{"$gt": 0}` as an expected value would match v=5 if it were read as an operator.
    let err = drv
        .apply_changes(&s.name, &[update(json!(1), json!({"v": 6}), json!({"v": {"$gt": 0}}))])
        .await
        .unwrap_err()
        .to_string();
    assert_eq!(err, format!("change 0: {CHANGED_SINCE_LOADED}"));
    let err = drv.apply_changes(&s.name, &[delete(json!({"$gt": 0}))]).await.unwrap_err().to_string();
    assert_eq!(err, format!("change 0: {ROW_GONE}"));
    assert_eq!(drv.count(&s.name, r#"{"v": 5}"#).await.unwrap(), 1);
}

#[tokio::test]
async fn index_create_list_drop() {
    let Some((drv, url)) = replica().await else { return };
    let s = Scratch::new(&url, "idx");
    drv.apply_changes(&s.name, &[insert(json!({"email": "a@x"}))]).await.unwrap();

    let name = drv.create_index(&s.name, r#"{"email": 1, "n": -1}"#, true, None).await.unwrap();
    assert_eq!(name, "email_1_n_-1");
    let named = drv.create_index(&s.name, r#"{"n": 1}"#, false, Some("by_n")).await.unwrap();
    assert_eq!(named, "by_n");

    let list = drv.list_indexes(&s.name).await.unwrap();
    let find = |n: &str| list.iter().find(|i| i.name == n).unwrap_or_else(|| panic!("{n} in {list:?}"));
    assert!(find("_id_").keys.contains("_id"));
    assert!(find(&name).unique && !find("by_n").unique);
    let keys: Value = serde_json::from_str(&find(&name).keys).unwrap();
    assert_eq!(keys.as_object().unwrap().keys().collect::<Vec<_>>(), ["email", "n"], "compound key order kept");

    // The unique index is enforced.
    let dup = drv.apply_changes(&s.name, &[insert(json!({"email": "a@x"}))]).await.unwrap_err().to_string();
    assert!(dup.contains("duplicate key"), "{dup}");

    drv.drop_index(&s.name, &name).await.unwrap();
    assert!(!drv.list_indexes(&s.name).await.unwrap().iter().any(|i| i.name == name));
    assert!(drv.drop_index(&s.name, &name).await.is_err(), "dropping it twice");
    assert!(drv.drop_index(&s.name, "_id_").await.is_err(), "the _id index cannot go");
    assert!(drv.create_index(&s.name, "{}", false, None).await.is_err());
    assert!(drv.create_index(&s.name, "{bad", false, None).await.is_err());
}

#[tokio::test]
async fn aggregate_group_match_and_row_cap() {
    let Some((drv, url)) = replica().await else { return };
    let r = drv
        .aggregate(
            "users",
            r#"[{"$match": {"age": {"$gt": 40}}}, {"$group": {"_id": null, "n": {"$sum": 1}, "oldest": {"$max": "$age"}}}]"#,
            0,
        )
        .await
        .unwrap();
    assert!(!r.truncated);
    assert_eq!(parse(&r.docs), [json!({"_id": null, "n": {"$numberInt": "2"}, "oldest": {"$numberInt": "85"}})]);

    // The cap: 1005 documents, 1000 come back and the rest is reported.
    let s = Scratch::new(&url, "agg");
    s.raw().await.insert_many((0..1005).map(|i| doc! {"i": i})).await.unwrap();
    let all = drv.aggregate(&s.name, r#"[{"$sort": {"i": 1}}]"#, 0).await.unwrap();
    assert_eq!((all.docs.len(), all.truncated), (1000, true));
    let few = drv.aggregate(&s.name, r#"[{"$sort": {"i": 1}}]"#, 5).await.unwrap();
    assert_eq!((few.docs.len(), few.truncated), (5, true));
    let exact = drv.aggregate(&s.name, r#"[{"$limit": 5}]"#, 5).await.unwrap();
    assert_eq!((exact.docs.len(), exact.truncated), (5, false));
    assert!(drv.aggregate(&s.name, "[]", 1001).await.is_err());
}

#[tokio::test]
async fn aggregate_refuses_writes_and_bad_pipelines() {
    let Some((drv, url)) = replica().await else { return };
    let s = Scratch::new(&url, "out");
    for p in [
        r#"[{"$out": "stolen"}]"#,
        r#"[{"$match": {}}, {"$merge": {"into": "stolen"}}]"#,
        r#"[{"$facet": {"a": [{"$out": "stolen"}]}}]"#,
        r#"[{"$lookup": {"from": "users", "pipeline": [{"$merge": "stolen"}], "as": "x"}}]"#,
        r#"[{"$out": {"db": "other", "coll": "stolen"}}]"#,
    ] {
        let err = drv.aggregate("users", p, 0).await.unwrap_err().to_string();
        assert!(err.contains("$out and $merge"), "{p} -> {err}");
    }
    // Nothing was written anywhere.
    let client = Client::with_uri_str(&url).await.unwrap();
    assert!(!client.database(DB).list_collection_names().await.unwrap().iter().any(|c| c == "stolen"));
    let _ = &s;

    for (p, why) in [("{}", "array"), ("[1]", "object"), ("[{bad", "invalid JSON"), (r#"[{"$match": {"_id": {"$oid": "x"}}}]"#, "Extended JSON")] {
        let err = drv.aggregate("users", p, 0).await.unwrap_err().to_string();
        assert!(err.starts_with("pipeline:") && err.contains(why), "{p} -> {err}");
    }
    // A server-side error comes through as text.
    assert!(drv.aggregate("users", r#"[{"$nope": 1}]"#, 0).await.is_err());
}

#[tokio::test]
async fn driver_calls_reach_the_sql_log() {
    use std::sync::Arc;
    use tableiq_lib::sql_log::{SqlLog, CURRENT};
    let Some((drv, url)) = replica().await else { return };
    let s = Scratch::new(&url, "log");
    let log = Arc::new(SqlLog::default());
    CURRENT
        .scope(log.clone(), async {
            drv.find("users", &opts(r#"{"name": "Ada"}"#, r#"{"age": 1}"#, 2, 3)).await.unwrap();
            drv.count("users", "").await.unwrap();
            drv.apply_changes(&s.name, &[insert(json!({"_id": 1}))]).await.unwrap();
            drv.aggregate("users", r#"[{"$match": {}}]"#, 0).await.unwrap();
        })
        .await;
    let lines: Vec<String> = log.since(0).into_iter().map(|e| e.sql).collect();
    let all = lines.join("\n");
    for want in [
        r#"find({"name":"Ada"}"#,
        ".sort({\"age\":1}).skip(2).limit(3)",
        "estimatedDocumentCount()",
        "session.startTransaction()",
        ".insertOne({\"_id\":1})",
        "session.commitTransaction()",
        ".aggregate([",
    ] {
        assert!(all.contains(want), "missing {want:?} in:\n{all}");
    }
}
