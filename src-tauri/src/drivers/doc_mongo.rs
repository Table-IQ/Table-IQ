//! MongoDB `DocumentDriver` — browse, edit, index, aggregate.
//!
//! Every query/pipeline/change value arrives as MongoDB Extended JSON and leaves the same way,
//! canonical (see [`DocumentDriver`]). Anything that is not valid Extended JSON is refused
//! before it reaches the server.

use std::time::Duration;

use async_trait::async_trait;
use mongodb::action::Action;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::IndexOptions;
use mongodb::{Client, ClientSession, Collection, Cursor, Database, IndexModel};
use serde_json::Value;

use super::{
    change_err, AggResult, ChangesResult, DocChange, DocChangesResult, DocumentDriver, DResult, DriverError,
    FindOpts, DocIndex, CHANGED_SINCE_LOADED, CONNECT_TIMEOUT, ROW_GONE,
};
use crate::sql_log::note;

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 5000;
const MAX_AGG_ROWS: i64 = 1000;
/// Server-side time budget for a find / count / aggregate.
const MAX_TIME: Duration = Duration::from_secs(30);
/// Longest filter / sort / projection / pipeline / key document accepted.
const MAX_JSON_IN: usize = 1 << 20;
/// Most documents' JSON one call returns: a page of 16 MiB documents must not fill the RAM.
const MAX_OUT_BYTES: usize = 64 << 20;
const MAX_CHANGES: usize = 1000;

fn e<E: std::fmt::Display>(err: E) -> DriverError {
    DriverError::Db(err.to_string())
}

fn bad(what: &str, why: impl std::fmt::Display) -> DriverError {
    DriverError::Db(format!("{what}: {why}"))
}

/// A JSON object in Extended JSON → a `Document`. Empty text is the empty document.
fn parse_doc(what: &str, json: &str) -> DResult<Document> {
    if json.trim().is_empty() {
        return Ok(Document::new());
    }
    if json.len() > MAX_JSON_IN {
        return Err(bad(what, format!("over {} KiB", MAX_JSON_IN >> 10)));
    }
    match serde_json::from_str::<Value>(json) {
        Ok(Value::Object(m)) => Document::try_from(m).map_err(|x| bad(what, format!("invalid Extended JSON: {x}"))),
        Ok(_) => Err(bad(what, "must be a JSON object")),
        Err(x) => Err(bad(what, format!("invalid JSON: {x}"))),
    }
}

fn to_bson(what: &str, v: &Value) -> DResult<Bson> {
    Bson::try_from(v.clone()).map_err(|x| bad(what, format!("invalid Extended JSON: {x}")))
}

/// Canonical Extended JSON, pretty, keys in document order.
fn canonical(d: Document) -> String {
    serde_json::to_string_pretty(&Bson::Document(d).into_canonical_extjson()).unwrap_or_else(|_| "{}".into())
}

/// A document as one line of relaxed Extended JSON, for the SQL log.
fn ej(d: &Document) -> String {
    Bson::Document(d.clone()).into_relaxed_extjson().to_string()
}

/// `$out` / `$merge` as a key at any depth (they also hide inside `$facet` / `$lookup` / `$unionWith`).
fn has_write_stage(v: &Value) -> bool {
    match v {
        Value::Object(m) => m.iter().any(|(k, x)| k == "$out" || k == "$merge" || has_write_stage(x)),
        Value::Array(a) => a.iter().any(has_write_stage),
        _ => false,
    }
}

/// Up to `max` documents of `cursor` as canonical JSON, refusing a result over [`MAX_OUT_BYTES`].
async fn drain(mut cursor: Cursor<Document>, max: usize) -> DResult<Vec<String>> {
    let (mut out, mut bytes) = (Vec::new(), 0usize);
    while out.len() < max && cursor.advance().await.map_err(e)? {
        let s = canonical(cursor.deserialize_current().map_err(e)?);
        bytes += s.len();
        if bytes > MAX_OUT_BYTES {
            return Err(bad("result", format!("over {} MiB — lower the limit or add a projection", MAX_OUT_BYTES >> 20)));
        }
        out.push(s);
    }
    Ok(out)
}

pub struct MongoDriver {
    client: Client,
    db: Database,
    /// A replica set or mongos: change batches run in a transaction.
    transactional: bool,
}

impl MongoDriver {
    /// `url` is any MongoDB connection string (`mongodb://`, `mongodb+srv://`, with
    /// authSource / replicaSet / tls options). `database` empty → the URL's path, else `test`.
    pub async fn connect(url: &str, database: &str) -> DResult<Self> {
        let client = Client::with_uri_str(url).await.map_err(e)?;
        let name = match (database.is_empty(), client.default_database()) {
            (false, _) => database.to_string(),
            (true, Some(d)) => d.name().to_string(),
            (true, None) => "test".to_string(),
        };
        // The first server round trip: it proves reachability + auth (the client itself
        // connects lazily) and tells replica set / mongos from a standalone.
        let hello = tokio::time::timeout(CONNECT_TIMEOUT, client.database("admin").run_command(doc! { "hello": 1 }))
            .await
            .map_err(|_| DriverError::Db(format!("MongoDB server did not answer within {}s", CONNECT_TIMEOUT.as_secs())))?
            .map_err(e)?;
        let transactional = hello.contains_key("setName") || hello.get_str("msg") == Ok("isdbgrid");
        let db = client.database(&name);
        Ok(Self { client, db, transactional })
    }

    fn coll(&self, collection: &str) -> Collection<Document> {
        self.db.collection(collection)
    }
}

/// One validated change, ready to send.
enum Op {
    Update { filter: Document, update: Document },
    Delete { filter: Document },
    Insert(Document),
}

/// `{_id: {$eq: id}}` — `$eq` so an `_id` / expected value shaped like `{"$gt": 0}` is compared
/// literally and never read as an operator.
fn match_filter(id: &Value, expect: &serde_json::Map<String, Value>) -> Result<Document, String> {
    let mut f = doc! { "_id": { "$eq": to_bson("_id", id).map_err(|x| x.to_string())? } };
    for (k, v) in expect {
        if k.is_empty() || k.starts_with('$') {
            return Err(format!("expect: bad field name '{k}'"));
        }
        f.insert(k.clone(), doc! { "$eq": to_bson("expect", v).map_err(|x| x.to_string())? });
    }
    Ok(f)
}

fn plan(c: &DocChange) -> Result<Op, String> {
    match c {
        DocChange::Update { id, set, unset, expect } => {
            let mut update = Document::new();
            let mut names = set.keys().chain(unset.iter());
            if let Some(k) = names.find(|k| k.is_empty() || k.starts_with('$')) {
                return Err(format!("bad field name '{k}'"));
            }
            if !set.is_empty() {
                let d = Document::try_from(set.clone()).map_err(|x| format!("set: invalid Extended JSON: {x}"))?;
                update.insert("$set", d);
            }
            if !unset.is_empty() {
                update.insert("$unset", unset.iter().map(|k| (k.clone(), Bson::from(""))).collect::<Document>());
            }
            if update.is_empty() {
                return Err("nothing to update".into());
            }
            Ok(Op::Update { filter: match_filter(id, expect)?, update })
        }
        DocChange::Delete { id } => Ok(Op::Delete { filter: match_filter(id, &Default::default())? }),
        DocChange::Insert { doc } => match doc {
            Value::Object(m) => Ok(Op::Insert(Document::try_from(m.clone()).map_err(|x| format!("invalid Extended JSON: {x}"))?)),
            _ => Err("a document must be a JSON object".into()),
        },
    }
}

/// Run one change; `Err` is the reason it did not apply. `session` is the transaction, if any.
async fn run_op(coll: &Collection<Document>, op: &Op, session: Option<&mut ClientSession>) -> Result<(), String> {
    let name = coll.name();
    match op {
        Op::Update { filter, update } => {
            note(&format!("db.getCollection({name:?}).updateOne({}, {})", ej(filter), ej(update)));
            let r = coll
                .update_one(filter.clone(), update.clone())
                .optional(session, |a, s| a.session(s))
                .await
                .map_err(|x| x.to_string())?;
            // Matched, not modified: re-saving an unchanged value is not a conflict.
            if r.matched_count == 0 { Err(CHANGED_SINCE_LOADED.into()) } else { Ok(()) }
        }
        Op::Delete { filter } => {
            note(&format!("db.getCollection({name:?}).deleteOne({})", ej(filter)));
            let r = coll
                .delete_one(filter.clone())
                .optional(session, |a, s| a.session(s))
                .await
                .map_err(|x| x.to_string())?;
            if r.deleted_count == 0 { Err(ROW_GONE.into()) } else { Ok(()) }
        }
        Op::Insert(d) => {
            note(&format!("db.getCollection({name:?}).insertOne({})", ej(d)));
            coll.insert_one(d).optional(session, |a, s| a.session(s)).await.map(|_| ()).map_err(|x| x.to_string())
        }
    }
}

#[async_trait]
impl DocumentDriver for MongoDriver {
    async fn list_collections(&self) -> DResult<Vec<String>> {
        note("db.getCollectionNames()");
        self.db.list_collection_names().await.map_err(e)
    }

    async fn find(&self, collection: &str, o: &FindOpts) -> DResult<Vec<String>> {
        let (filter, sort, projection) =
            (parse_doc("filter", &o.filter)?, parse_doc("sort", &o.sort)?, parse_doc("projection", &o.projection)?);
        let limit = match o.limit {
            0 => DEFAULT_LIMIT,
            1..=MAX_LIMIT => o.limit,
            _ => return Err(bad("limit", format!("must be 1..={MAX_LIMIT}"))),
        };
        note(&format!(
            "db.getCollection({collection:?}).find({}, {}).sort({}).skip({}).limit({limit})",
            ej(&filter),
            ej(&projection),
            ej(&sort),
            o.skip
        ));
        let coll = self.coll(collection);
        let mut f = coll.find(filter).skip(o.skip).limit(limit).max_time(MAX_TIME);
        if !sort.is_empty() {
            f = f.sort(sort);
        }
        if !projection.is_empty() {
            f = f.projection(projection);
        }
        drain(f.await.map_err(e)?, limit as usize).await
    }

    async fn count(&self, collection: &str, filter: &str) -> DResult<u64> {
        let filter = parse_doc("filter", filter)?;
        let coll = self.coll(collection);
        if filter.is_empty() {
            note(&format!("db.getCollection({collection:?}).estimatedDocumentCount()"));
            return coll.estimated_document_count().max_time(MAX_TIME).await.map_err(e);
        }
        note(&format!("db.getCollection({collection:?}).countDocuments({})", ej(&filter)));
        coll.count_documents(filter).max_time(MAX_TIME).await.map_err(e)
    }

    async fn apply_changes(&self, collection: &str, changes: &[DocChange]) -> DResult<DocChangesResult> {
        if changes.len() > MAX_CHANGES {
            return Err(bad("changes", format!("at most {MAX_CHANGES} per batch")));
        }
        // Validate everything first: a malformed change must not leave a best-effort batch half-written.
        let ops = changes
            .iter()
            .enumerate()
            .map(|(i, c)| plan(c).map_err(|why| change_err(i, why)))
            .collect::<DResult<Vec<_>>>()?;
        let coll = self.coll(collection);
        let mut out = DocChangesResult { transactional: self.transactional, counts: ChangesResult::default(), errors: vec![None; ops.len()] };
        let tally = |counts: &mut ChangesResult, op: &Op| match op {
            Op::Update { .. } => counts.updated += 1,
            Op::Delete { .. } => counts.deleted += 1,
            Op::Insert(_) => counts.inserted += 1,
        };

        if !self.transactional {
            for (i, op) in ops.iter().enumerate() {
                match run_op(&coll, op, None).await {
                    Ok(()) => tally(&mut out.counts, op),
                    Err(why) => out.errors[i] = Some(why),
                }
            }
            return Ok(out);
        }

        let mut session = self.client.start_session().await.map_err(e)?;
        session.start_transaction().await.map_err(e)?;
        note("session.startTransaction()");
        for (i, op) in ops.iter().enumerate() {
            if let Err(why) = run_op(&coll, op, Some(&mut session)).await {
                note("session.abortTransaction()");
                let _ = session.abort_transaction().await; // the server may have aborted it already
                return Err(change_err(i, why));
            }
            tally(&mut out.counts, op);
        }
        note("session.commitTransaction()");
        session.commit_transaction().await.map_err(e)?;
        Ok(out)
    }

    async fn list_indexes(&self, collection: &str) -> DResult<Vec<DocIndex>> {
        note(&format!("db.getCollection({collection:?}).getIndexes()"));
        let mut cursor = self.coll(collection).list_indexes().await.map_err(e)?;
        let mut out = Vec::new();
        while cursor.advance().await.map_err(e)? {
            let m = cursor.deserialize_current().map_err(e)?;
            let o = m.options.unwrap_or_default();
            out.push(DocIndex {
                name: o.name.unwrap_or_default(),
                keys: canonical(m.keys),
                unique: o.unique.unwrap_or(false),
                expire_after_seconds: o.expire_after.map(|d| d.as_secs()),
            });
        }
        Ok(out)
    }

    async fn create_index(&self, collection: &str, keys: &str, unique: bool, name: Option<&str>) -> DResult<String> {
        let keys = parse_doc("keys", keys)?;
        if keys.is_empty() {
            return Err(bad("keys", "an index needs at least one field"));
        }
        note(&format!(
            "db.getCollection({collection:?}).createIndex({}, {{unique: {unique}{}}})",
            ej(&keys),
            name.map(|n| format!(", name: {n:?}")).unwrap_or_default()
        ));
        let opts = IndexOptions::builder().unique(unique.then_some(true)).name(name.map(str::to_string)).build();
        let model = IndexModel::builder().keys(keys).options(opts).build();
        Ok(self.coll(collection).create_index(model).await.map_err(e)?.index_name)
    }

    async fn drop_index(&self, collection: &str, name: &str) -> DResult<()> {
        note(&format!("db.getCollection({collection:?}).dropIndex({name:?})"));
        self.coll(collection).drop_index(name).await.map_err(e)
    }

    async fn aggregate(&self, collection: &str, pipeline: &str, limit: i64) -> DResult<AggResult> {
        if pipeline.len() > MAX_JSON_IN {
            return Err(bad("pipeline", format!("over {} KiB", MAX_JSON_IN >> 10)));
        }
        let cap = match limit {
            0 => MAX_AGG_ROWS,
            1..=MAX_AGG_ROWS => limit,
            _ => return Err(bad("limit", format!("must be 1..={MAX_AGG_ROWS}"))),
        };
        let stages = match serde_json::from_str::<Value>(pipeline) {
            Ok(Value::Array(a)) => a,
            Ok(_) => return Err(bad("pipeline", "must be a JSON array of stages")),
            Err(x) => return Err(bad("pipeline", format!("invalid JSON: {x}"))),
        };
        if has_write_stage(&Value::Array(stages.clone())) {
            return Err(bad("pipeline", "$out and $merge write to the database and are not allowed here"));
        }
        let mut docs = stages
            .into_iter()
            .map(|s| match s {
                Value::Object(m) => Document::try_from(m).map_err(|x| bad("pipeline", format!("invalid Extended JSON: {x}"))),
                _ => Err(bad("pipeline", "every stage must be a JSON object")),
            })
            .collect::<DResult<Vec<_>>>()?;
        // One past the cap tells a full page from a truncated one.
        docs.push(doc! { "$limit": cap + 1 });
        note(&format!(
            "db.getCollection({collection:?}).aggregate([{}])",
            docs.iter().map(ej).collect::<Vec<_>>().join(", ")
        ));
        let cursor = self.coll(collection).aggregate(docs).max_time(MAX_TIME).await.map_err(e)?;
        let cursor = cursor.with_type::<Document>();
        let mut docs = drain(cursor, cap as usize + 1).await?;
        let truncated = docs.len() > cap as usize;
        docs.truncate(cap as usize);
        Ok(AggResult { docs, truncated })
    }

    async fn list_databases(&self) -> DResult<Vec<String>> {
        // listDatabaseNames hits the admin command and returns names visible to
        // the authenticated user. We keep the client (not just the active db) on
        // the driver specifically so this call doesn't have to reconnect.
        self.client.list_database_names().await.map_err(e)
    }
}
