//! The MongoDB MCP guard's adversarial suite (no server needed): every attack in
//! `mongo_attacks` is refused wherever it can be put — as a filter, a sort, a projection, a
//! count filter and (for objects) inside a pipeline's `$match` — and the ordinary reads next
//! to them are not. Every wrong verdict is collected and reported at once.

mod mongo_attacks;

use mongo_attacks::*;
use tableiq_lib::mcp::mongo_guard::{collection, object, pipeline, MAX_DEPTH, MAX_JSON_BYTES};

#[test]
fn every_attack_is_refused_and_ordinary_reads_are_not() {
    let mut wrong = Vec::new();
    for (class, text) in OBJECT_REFUSE {
        for what in ["filter", "sort", "projection"] {
            if object(what, text).is_ok() {
                wrong.push(format!("[{class}] {what} ALLOWED {text}"));
            }
        }
        // The same object as a `$match`'s argument. (A non-object there is the server's error to
        // give: `$match` of a number reads nothing, writes nothing.)
        if *class == "not-an-object" {
            continue;
        }
        let in_match = format!(r#"[{{"$match":{text}}}]"#);
        if pipeline(&in_match).is_ok() {
            wrong.push(format!("[{class}] pipeline $match ALLOWED {text}"));
        }
    }
    for (class, text) in PIPELINE_REFUSE {
        if let Ok(v) = pipeline(text) {
            wrong.push(format!("[{class}] pipeline ALLOWED {text} -> {v}"));
        }
    }
    for name in COLLECTION_REFUSE {
        if collection(name).is_ok() {
            wrong.push(format!("[collection] ALLOWED {name:?}"));
        }
    }
    for text in OBJECT_ALLOW {
        for what in ["filter", "sort", "projection"] {
            if let Err(m) = object(what, text) {
                wrong.push(format!("[allow] {what} REFUSED {text}: {m}"));
            }
        }
        let in_match = format!(r#"[{{"$match":{text}}}]"#);
        if let Err(m) = pipeline(&in_match) {
            wrong.push(format!("[allow] $match REFUSED {text}: {m}"));
        }
    }
    for text in PIPELINE_ALLOW {
        if let Err(m) = pipeline(text) {
            wrong.push(format!("[allow] pipeline REFUSED {text}: {m}"));
        }
    }
    assert!(wrong.is_empty(), "{} wrong verdicts:\n{}", wrong.len(), wrong.join("\n"));
}

/// The table is the contract: a refactor that quietly empties it must fail here.
#[test]
fn the_attack_table_is_not_shrunk() {
    let n = OBJECT_REFUSE.len() + PIPELINE_REFUSE.len() + COLLECTION_REFUSE.len();
    assert!(n >= 150, "{n} attacks");
}

/// What the guard hands the driver is the tree it checked: re-parsing it gives the same value, and
/// there is nothing left in it a second parser could read differently.
#[test]
fn the_text_returned_parses_to_what_was_checked() {
    for text in OBJECT_ALLOW {
        let out = object("filter", text).unwrap();
        let (a, b): (serde_json::Value, serde_json::Value) =
            (serde_json::from_str(text).unwrap(), serde_json::from_str(&out).unwrap());
        assert_eq!(a, b, "{text}");
    }
    for text in PIPELINE_ALLOW {
        let out = pipeline(text).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(text).unwrap(), serde_json::from_str::<serde_json::Value>(&out).unwrap());
    }
    // Key order is semantic (a sort, a $group): kept.
    assert_eq!(object("sort", r#"{"z":1,"a":-1}"#).unwrap(), r#"{"z":1,"a":-1}"#);
}

#[test]
fn size_and_depth_limits() {
    let deep = |n: usize| format!("{}1{}", r#"{"a":"#.repeat(n), "}".repeat(n));
    assert!(object("filter", &deep(MAX_DEPTH - 1)).is_ok());
    assert!(object("filter", &deep(MAX_DEPTH + 1)).is_err());
    assert!(object("filter", &deep(5_000)).is_err(), "past serde_json's recursion limit: an error, not a crash");
    let arrays = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
    assert!(pipeline(&arrays(MAX_DEPTH + 1)).is_err());
    assert!(pipeline(&arrays(200_000)).is_err());
    // Exactly at the cap is fine, one over is not.
    let pad = |n: usize| format!(r#"{{"a":"{}"}}"#, "x".repeat(n - r#"{"a":""}"#.len()));
    assert!(object("filter", &pad(MAX_JSON_BYTES)).is_ok());
    assert!(object("filter", &pad(MAX_JSON_BYTES + 1)).is_err());
    // A huge `$in` is only an array: bounded by the byte cap, not by a number of elements.
    let ins = format!(r#"{{"n":{{"$in":[{}]}}}}"#, "0,".repeat(400_000) + "0");
    assert!(ins.len() < MAX_JSON_BYTES && object("filter", &ins).is_ok());
    let too_many = format!(r#"{{"n":{{"$in":[{}]}}}}"#, "0,".repeat(600_000) + "0");
    assert!(object("filter", &too_many).unwrap_err().contains("over 1024 KiB"));
}

/// The guard runs inside the MCP call's task, so a worst-case input must not make it slow.
#[test]
fn a_maximal_input_is_checked_quickly() {
    let wide = format!("[{}]", r#"{"$match":{"a":{"$in":[1,2,3]}}},"#.repeat(20_000) + r#"{"$limit":1}"#);
    assert!(wide.len() < MAX_JSON_BYTES);
    let started = std::time::Instant::now();
    assert!(pipeline(&wide).is_ok());
    assert!(started.elapsed() < std::time::Duration::from_secs(2), "took {:?}", started.elapsed());
}

/// Messages name what was refused and why, without echoing a secret from elsewhere.
#[test]
fn a_refusal_says_what_and_why() {
    let m = object("filter", r#"{"$where":"1"}"#).unwrap_err();
    assert!(m.contains("filter") && m.contains("$where") && m.contains("JavaScript"), "{m}");
    let m = pipeline(r#"[{"$out":"x"}]"#).unwrap_err();
    assert!(m.contains("$out"), "{m}");
    let m = pipeline(r#"[{"$currentOp":{}}]"#).unwrap_err();
    assert!(m.contains("$currentOp") && m.contains("not allowed"), "{m}");
    let m = pipeline(r#"[{"$lookup":{"from":{"db":"a","coll":"b"},"pipeline":[],"as":"x"}}]"#).unwrap_err();
    assert!(m.contains("this database only"), "{m}");
    assert!(collection("system.users").unwrap_err().contains("system collections"));
}
