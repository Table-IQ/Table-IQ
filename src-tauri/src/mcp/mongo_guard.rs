//! The MongoDB guard in front of the `mongo_*` MCP tools.
//!
//! There is no read-only MongoDB session to lean on (no read-only transaction; a role would
//! be the user's job), so this guard IS the boundary, and it is fail-closed like the SQL one:
//!
//! * Input is strict JSON text (the driver wants Extended JSON): at most [`MAX_JSON_BYTES`],
//!   at most [`MAX_DEPTH`] levels deep, no trailing text, and no duplicate key anywhere —
//!   `{"a":1,"a":2}` is read differently by different parsers, and "last one wins" is how a
//!   guard and a server end up disagreeing. What is returned is the text re-serialized from
//!   the tree that was checked, so the driver parses exactly that tree.
//! * A pipeline stage must be on an allowlist of reads ([`STAGES`]); anything else — admin,
//!   change-stream, Atlas and diagnostic stages, and every stage that writes — is refused,
//!   including inside `$facet`, `$lookup` / `$unionWith` pipelines. An allowlist, not a
//!   denylist: a stage a newer server adds stays refused until someone reviews it.
//! * Keys that run JavaScript or smuggle code, or that write, are refused at any depth of any
//!   filter, sort, projection or pipeline ([`DENIED_KEYS`], and every key starting `$_`, the
//!   server's internal operators — two of which run JavaScript) — `$expr: {$function: …}` and
//!   `{"$code": …}` included. The expression operators are a denylist, not an allowlist: they
//!   number in the hundreds and the server adds more; only the stages are an allowlist. Compared trimmed and case-insensitively (the server is exact,
//!   so this only ever refuses more).
//! * The database is the connection's and nothing else: no tool takes a database name, a
//!   `$lookup` / `$graphLookup` / `$unionWith` may name a collection of it only (the
//!   `{db, coll}` forms are refused), and `system.*` collections are never readable.
//!
//! What the guard cannot do: stop a read from being expensive (the server's `maxTimeMS`, the
//! tool timeout and the row/byte caps do), or hide data the connection's user can read — share
//! a MongoDB connection through a user that can read only what agents should see.

use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

/// Longest JSON text accepted for one argument.
pub const MAX_JSON_BYTES: usize = 1 << 20;
/// Deepest nesting accepted (arrays and objects).
pub const MAX_DEPTH: usize = 32;

/// Keys refused anywhere: JavaScript (`$where`, `$function`, `$accumulator`), Extended JSON
/// code wrappers (`$code`, `$codeWScope`), the removed `$eval`, and the stages that write.
const DENIED_KEYS: &[&str] =
    &["$where", "$function", "$accumulator", "$code", "$codewscope", "$eval", "$out", "$merge"];

/// The pipeline stages an agent may run — reads that cannot reach beyond the database (see
/// [`stages`] for the three that name a collection). Left out on purpose: `$out`/`$merge`
/// (write), `$currentOp`, `$listSessions`, `$listLocalSessions`, `$planCacheStats`,
/// `$indexStats`, `$collStats` (server and storage internals, host names), `$queryStats`,
/// `$changeStream*`, `$search`/`$searchMeta`/`$vectorSearch` and the other Atlas stages, and
/// the `$list*` catalog stages.
const STAGES: &[&str] = &[
    "$match", "$project", "$group", "$sort", "$limit", "$skip", "$unwind", "$addFields", "$set", "$unset",
    "$replaceRoot", "$replaceWith", "$count", "$sample", "$bucket", "$bucketAuto", "$facet", "$sortByCount",
    "$lookup", "$graphLookup", "$unionWith", "$geoNear", "$redact", "$setWindowFields", "$densify", "$fill",
];

/// A collection of the connection's own database that agents may read: not empty, no `$` or NUL,
/// and never a `system.` one (`system.users`, `system.js`, `system.views`, `system.buckets.*`).
pub fn collection(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 255 {
        return Err("collection: a name of 1 to 255 bytes is required".into());
    }
    if name.contains(['$', '\0']) {
        return Err("collection: names with `$` or NUL are not allowed".into());
    }
    if name.trim().to_ascii_lowercase().starts_with("system.") {
        return Err("collection: system collections cannot be read over MCP".into());
    }
    Ok(())
}

/// A filter, sort or projection: a JSON object (empty text = none, returned as ""). Returns
/// the text to hand the driver.
pub fn object(what: &str, json: &str) -> Result<String, String> {
    if json.trim().is_empty() {
        return Ok(String::new());
    }
    let v = parse(what, json)?;
    if !v.is_object() {
        return Err(format!("{what}: must be a JSON object"));
    }
    scan(what, &v)?;
    Ok(v.to_string())
}

/// An aggregation pipeline: a JSON array of stage objects, every stage allowed and every
/// collection it names the database's own. Returns the text to hand the driver.
pub fn pipeline(json: &str) -> Result<String, String> {
    let v = parse("pipeline", json)?;
    let Value::Array(a) = &v else { return Err("pipeline: must be a JSON array of stages".into()) };
    scan("pipeline", &v)?;
    stages(a)?;
    Ok(v.to_string())
}

// ── parsing ───────────────────────────────────────────────────────────────

fn parse(what: &str, json: &str) -> Result<Value, String> {
    if json.len() > MAX_JSON_BYTES {
        return Err(format!("{what}: over {} KiB", MAX_JSON_BYTES >> 10));
    }
    let mut de = serde_json::Deserializer::from_str(json);
    let v = Strict(0).deserialize(&mut de).map_err(|e| format!("{what}: invalid JSON: {e}"))?;
    de.end().map_err(|e| format!("{what}: invalid JSON: {e}"))?;
    Ok(v)
}

/// `serde_json::Value`, but a duplicate key or nesting past [`MAX_DEPTH`] is an error. The
/// number is the depth of the value being read.
struct Strict(usize);

impl<'de> DeserializeSeed<'de> for Strict {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Strict {
    type Value = Value;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v).map(Value::Number).ok_or_else(|| E::custom("number out of range"))
    }
    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.into()))
    }
    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        deeper::<A::Error>(self.0)?;
        let mut out = Vec::new();
        while let Some(v) = seq.next_element_seed(Strict(self.0 + 1))? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        deeper::<A::Error>(self.0)?;
        let mut out = Map::new();
        while let Some(k) = map.next_key::<String>()? {
            if out.contains_key(&k) {
                return Err(de::Error::custom(format!("duplicate key {k:?}")));
            }
            let v = map.next_value_seed(Strict(self.0 + 1))?;
            out.insert(k, v);
        }
        Ok(Value::Object(out))
    }
}

fn deeper<E: de::Error>(depth: usize) -> Result<(), E> {
    if depth >= MAX_DEPTH {
        return Err(E::custom(format!("nested deeper than {MAX_DEPTH} levels")));
    }
    Ok(())
}

// ── checks ────────────────────────────────────────────────────────────────

/// A key that runs JavaScript or writes. Every `$_` key is refused as well: the server reserves
/// that prefix for its internal operators, and two of them run JavaScript (`$_internalJsEmit`, an
/// expression, and `$_internalJsReduce`, an accumulator — confirmed on mongod 7, where they sleep
/// and compute like `$function` does). Nothing an agent needs starts with `$_`, so the prefix is
/// refused whole instead of tracking each name the server adds.
fn denied(key: &str) -> bool {
    let k = key.trim();
    k.starts_with("$_") || DENIED_KEYS.iter().any(|d| k.eq_ignore_ascii_case(d))
}

/// Every key and string of `v`, at any depth. `$$USER_ROLES` is the connection's own role list.
fn scan(what: &str, v: &Value) -> Result<(), String> {
    match v {
        Value::Object(m) => m.iter().try_for_each(|(k, x)| {
            if denied(k) {
                return Err(format!("{what}: `{k}` is not allowed over MCP (it runs JavaScript or writes)"));
            }
            scan(what, x)
        }),
        Value::Array(a) => a.iter().try_for_each(|x| scan(what, x)),
        Value::String(s) if s.trim_start().starts_with("$$USER_ROLES") => {
            Err(format!("{what}: `$$USER_ROLES` is not allowed over MCP"))
        }
        _ => Ok(()),
    }
}

/// Stage allowlist, recursively through the pipelines a stage holds, and the collections the
/// stages that read another one name.
fn stages(list: &[Value]) -> Result<(), String> {
    for stage in list {
        let Some((name, arg)) = stage.as_object().filter(|m| m.len() == 1).and_then(|m| m.iter().next())
        else {
            return Err("pipeline: every stage must be an object with exactly one operator".into());
        };
        if !STAGES.contains(&name.as_str()) {
            return Err(format!("pipeline: stage `{name}` is not allowed over MCP (reads of this database only)"));
        }
        match name.as_str() {
            "$lookup" => {
                let m = object_of(name, arg, &["from", "localField", "foreignField", "as", "let", "pipeline"])?;
                from(name, m.get("from"))?;
                nested(m.get("pipeline"))?;
            }
            "$graphLookup" => {
                let keys = [
                    "from", "startWith", "connectFromField", "connectToField", "as", "maxDepth", "depthField",
                    "restrictSearchWithMatch",
                ];
                let m = object_of(name, arg, &keys)?;
                from(name, m.get("from"))?;
            }
            "$unionWith" => match arg {
                Value::String(c) => collection(c).map_err(|e| format!("pipeline: $unionWith {e}"))?,
                Value::Object(_) => {
                    let m = object_of(name, arg, &["coll", "pipeline"])?;
                    from(name, m.get("coll"))?;
                    nested(m.get("pipeline"))?;
                }
                _ => return Err("pipeline: `$unionWith` takes a collection name or an object".into()),
            },
            "$facet" => {
                let m = arg.as_object().ok_or("pipeline: `$facet` takes an object of pipelines")?;
                m.values().try_for_each(|p| nested(Some(p)))?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// `arg` as an object holding only keys the stage is documented to take: a key this guard has not
/// reviewed (`db`, say) is not passed on.
fn object_of<'a>(stage: &str, arg: &'a Value, keys: &[&str]) -> Result<&'a Map<String, Value>, String> {
    let m = arg.as_object().ok_or_else(|| format!("pipeline: `{stage}` takes an object"))?;
    match m.keys().find(|k| !keys.contains(&k.as_str())) {
        Some(k) => Err(format!("pipeline: `{stage}` option `{k}` is not allowed over MCP")),
        None => Ok(m),
    }
}

/// The collection a `$lookup` / `$graphLookup` / `$unionWith` reads: a name in this database. The
/// `{db, coll}` form is how another database would be named.
fn from(stage: &str, v: Option<&Value>) -> Result<(), String> {
    match v {
        None => Ok(()),
        Some(Value::String(c)) => collection(c).map_err(|e| format!("pipeline: {stage} {e}")),
        Some(_) => Err(format!("pipeline: `{stage}` may name a collection of this database only, as a string")),
    }
}

fn nested(v: Option<&Value>) -> Result<(), String> {
    match v {
        None => Ok(()),
        Some(Value::Array(a)) => stages(a),
        Some(_) => Err("pipeline: a nested pipeline must be an array of stages".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_keys_are_refused_at_every_depth() {
        for j in [r#"{"a":1,"a":2}"#, r#"{"a":{"b":1,"b":1}}"#, r#"{"a":[{"x":1,"x":1}]}"#] {
            let e = object("filter", j).unwrap_err();
            assert!(e.contains("duplicate key"), "{j}: {e}");
        }
        assert!(pipeline(r#"[{"$match":{"a":1,"a":1}}]"#).unwrap_err().contains("duplicate key"));
    }

    #[test]
    fn the_returned_text_is_the_checked_tree_in_order() {
        assert_eq!(object("sort", r#" { "b" : -1 , "a" : 1 } "#).unwrap(), r#"{"b":-1,"a":1}"#);
        assert_eq!(object("filter", "  ").unwrap(), "");
        assert_eq!(pipeline("[]").unwrap(), "[]");
    }

    #[test]
    fn malformed_and_oversized_input_is_refused() {
        for j in ["{", "{'a':1}", r#"{"a":1} {"b":2}"#, r#"{"a":1}x"#, "[1]", "5", r#""s""#, "null", r#"{"a":NaN}"#, r#"{"a":1e999}"#] {
            assert!(object("filter", j).is_err(), "{j}");
        }
        let big = format!(r#"{{"a":"{}"}}"#, "x".repeat(MAX_JSON_BYTES));
        assert!(object("filter", &big).unwrap_err().contains("over 1024 KiB"));
        let deep = |n: usize| format!("{}1{}", r#"{"a":"#.repeat(n), "}".repeat(n));
        assert!(object("filter", &deep(MAX_DEPTH - 1)).is_ok());
        assert!(object("filter", &deep(MAX_DEPTH + 1)).unwrap_err().contains("nested deeper"));
        // Far past serde_json's own limit: an error, not a stack overflow.
        assert!(object("filter", &format!("{}{}", "[".repeat(100_000), "]".repeat(100_000))).is_err());
    }

    #[test]
    fn collection_names() {
        for ok in ["users", "a.b", "system", "Users 2", "systems.x"] {
            assert!(collection(ok).is_ok(), "{ok}");
        }
        for bad in ["", "system.js", "system.users", "System.Views", " system.views", "system.buckets.t", "a$b", "$cmd", "a\0b"] {
            assert!(collection(bad).is_err(), "{bad:?}");
        }
        assert!(collection(&"x".repeat(256)).is_err());
    }
}
