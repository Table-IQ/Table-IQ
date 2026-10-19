//! Engine profiles: products that speak the MySQL or Postgres wire protocol (TiDB, CockroachDB ...).
//! profile: a named product on top of a base engine; `ConnConfig.engine` stays "postgres" or "mysql".
//! product: what the server's version text says it is; `real`: the base engine itself under a vendor
//! name (Neon is PostgreSQL), so every catalog query and the MCP guard apply unchanged.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct Profile {
    pub base: String,
    pub label: String,
    pub real: bool,
    pub transactions: bool,
    pub backup: bool,
    #[serde(rename = "userMgmt")]
    pub user_mgmt: bool,
    #[serde(default)]
    pub detect: Vec<String>,
    pub tested: bool,
    /// May agents reach it over MCP? Absent: as `real`. Separate from `real`, which also tells
    /// the drivers to treat the server as the base engine.
    #[serde(default)]
    agent: Option<bool>,
    /// The opening of this product to agents (a build's `agent_rev`): an allowlist entry
    /// confirmed under an earlier one is not served until the user re-shares the connection.
    #[serde(default, rename = "agentSince")]
    pub agent_since: u32,
}

impl Profile {
    pub fn agent(&self) -> bool {
        self.agent.unwrap_or(self.real)
    }
}

/// The generation of agent access a new allowlist entry is confirmed under: the latest opening
/// of any product. An entry from before it (`McpConn::rev` lower) is not served for a product
/// opened since ([`reconfirm_block`]).
pub fn agent_rev() -> u32 {
    PROFILES.values().map(|p| p.agent_since).max().unwrap_or(0).max(MONGODB_AGENT_SINCE).max(REDIS_AGENT_SINCE)
}

/// The generation that opened MongoDB to agents (mirrored in `profiles.ts`). MongoDB has no
/// profile to carry an `agentSince`, so an allowlist entry for a `mongodb` connection is served
/// only from this generation on ([`mongodb_reconfirm_block`]): an id that was SQL when it was
/// shared (the engine can be edited) must not turn into a served MongoDB silently.
pub const MONGODB_AGENT_SINCE: u32 = 2;

/// Why an allowlist entry confirmed under generation `rev` cannot serve a MongoDB connection.
pub fn mongodb_reconfirm_block(rev: u32) -> Option<String> {
    (rev < MONGODB_AGENT_SINCE).then(|| {
        "This connection was shared before TableIQ supported MongoDB over MCP. Turn sharing off and on again for it in TableIQ → Settings → MCP to confirm it (MongoDB is always read-only for agents).".to_string()
    })
}

/// The generation that opened Redis to agents (mirrored in `profiles.ts`). A Redis connection could
/// never be shared before, so no entry for one is older than this unless its id was another engine's
/// when it was shared (the engine can be edited): such an entry is not served ([`redis_reconfirm_block`]).
pub const REDIS_AGENT_SINCE: u32 = 3;

/// Why an allowlist entry confirmed under generation `rev` cannot serve a Redis connection.
pub fn redis_reconfirm_block(rev: u32) -> Option<String> {
    (rev < REDIS_AGENT_SINCE).then(|| {
        "This connection was shared before TableIQ supported Redis over MCP. Turn sharing off and on again for it in TableIQ → Settings → MCP to confirm it (Redis is always read-only for agents).".to_string()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Caps {
    pub transactions: bool,
    pub backup: bool,
    pub user_mgmt: bool,
}

static PROFILES: LazyLock<BTreeMap<String, Profile>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../src/lib/profiles.json")).expect("profiles.json is valid")
});

pub fn lookup(id: &str) -> Option<&'static Profile> {
    PROFILES.get(id)
}

/// The profile's label, or the id itself when unknown.
pub fn label(id: &str) -> String {
    lookup(id).map_or_else(|| id.to_string(), |p| p.label.clone())
}

/// The product a server's version text names, among the profiles of `base`. A product that
/// is not the base engine wins over one that is, so a text naming both is restricted; then
/// the longest marker, ties by id, so the answer never depends on map order.
pub fn detect(base: &str, version_text: &str) -> Option<&'static str> {
    let mut markers: Vec<(bool, &str, &str)> = PROFILES
        .iter()
        .filter(|(_, p)| p.base == base)
        .flat_map(|(id, p)| p.detect.iter().map(move |m| (p.real, m.as_str(), id.as_str())))
        .collect();
    markers.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.len().cmp(&a.1.len())).then(a.2.cmp(b.2)));
    markers.into_iter().find(|(_, m, _)| version_text.contains(m)).map(|(_, _, id)| id)
}

/// The profile a live session runs under: a known saved one on `base`, else the detected one.
pub fn session_profile(base: &str, saved: Option<&str>, detected: Option<&str>) -> Option<String> {
    let on_base = |id: &&str| lookup(id).is_some_and(|p| p.base == base);
    saved.filter(on_base).or(detected.filter(on_base)).map(str::to_string)
}

/// Field-wise AND over the known profiles among saved and detected: the more restrictive wins.
pub fn effective_caps(saved: Option<&str>, detected: Option<&str>) -> Caps {
    [saved, detected].into_iter().flatten().filter_map(lookup).fold(
        Caps { transactions: true, backup: true, user_mgmt: true },
        |c, p| Caps {
            transactions: c.transactions && p.transactions,
            backup: c.backup && p.backup,
            user_mgmt: c.user_mgmt && p.user_mgmt,
        },
    )
}

/// [`effective_caps`], further narrowed by the connect-time `BEGIN` probe: only a server that
/// refused `BEGIN` (`Some(false)`) turns transactions off. A probe that timed out or never ran
/// (`None`) leaves the profile's flag, and a probe never grants what the profile withholds.
pub fn caps_with_probe(saved: Option<&str>, detected: Option<&str>, tx_probe: Option<bool>) -> Caps {
    let c = effective_caps(saved, detected);
    Caps { transactions: c.transactions && tx_probe != Some(false), ..c }
}

/// Why a connection cannot be shared over MCP, or None when it can. An empty saved profile
/// is no profile, as in `capsOf` (`profiles.ts`).
pub fn agent_block(base: &str, saved: Option<&str>, detected: Option<&str>, detect_ok: bool) -> Option<String> {
    let saved = saved.filter(|id| !id.is_empty());
    if let Some(id) = saved.filter(|id| lookup(id).is_none_or(|p| p.base != base)) {
        return Some(format!(
            "This connection cannot be shared over MCP: its profile ({id}) was saved by a newer TableIQ. Update TableIQ to manage it."
        ));
    }
    if let Some(p) = [saved, detected].into_iter().flatten().filter_map(lookup).find(|p| !p.agent()) {
        return Some(format!(
            "{} cannot be shared over MCP yet: {AGENT_NOTE} Ask the user to run this in a TableIQ query tab.",
            p.label
        ));
    }
    (!detect_ok).then(|| {
        "TableIQ could not identify this server (its version query failed), so it cannot be shared over MCP. Ask the user to reconnect, or to run this in a TableIQ query tab.".to_string()
    })
}

/// Why a closed product is not shared. Mirrored in `mcp.ts`.
pub const AGENT_NOTE: &str =
    "TableIQ's agent guard has been reviewed for PostgreSQL, MySQL, MariaDB and CockroachDB only.";

/// Why an allowlist entry confirmed under generation `rev` cannot be served yet: the product
/// behind the connection (saved or detected) was opened to agents after it was shared, so
/// the user never confirmed it for this product. Re-sharing it (off, on) records the current
/// generation and drops its write access. Checked after [`agent_block`].
pub fn reconfirm_block(saved: Option<&str>, detected: Option<&str>, rev: u32) -> Option<String> {
    [saved, detected].into_iter().flatten().filter_map(lookup).find(|p| p.agent() && p.agent_since > rev).map(|p| {
        format!(
            "This connection was shared before TableIQ supported {} over MCP. Turn sharing off and on again for it in TableIQ → Settings → MCP to confirm it (writes start off).",
            p.label
        )
    })
}

/// The engine the MCP guard runs for: `base`, or `"cockroachdb"` when the saved or the detected
/// profile says so (the guard adds that product's own checks to the Postgres ones).
pub fn guard_engine<'a>(base: &'a str, saved: Option<&str>, detected: Option<&str>) -> &'a str {
    if base == "postgres" && [saved, detected].contains(&Some("cockroachdb")) { "cockroachdb" } else { base }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_sits_on_postgres_or_mysql() {
        assert!(PROFILES.len() >= 23);
        for (id, p) in PROFILES.iter() {
            assert!(p.base == "postgres" || p.base == "mysql", "{id}");
            assert!(!p.label.is_empty(), "{id}");
        }
    }

    #[test]
    fn ci_engines_are_tested() {
        for id in ["tidb", "cockroachdb", "mariadb"] {
            assert!(lookup(id).unwrap().tested, "{id}");
        }
    }

    #[test]
    fn detect_matches_real_version_strings() {
        assert_eq!(detect("mysql", "8.0.11-TiDB-v8.5.1 TiDB Server (Apache License 2.0)"), Some("tidb"));
        assert_eq!(detect("mysql", "11.4.2-MariaDB-ubu2404 mariadb.org binary distribution"), Some("mariadb"));
        assert_eq!(detect("mysql", "8.4.0 MySQL Community Server - GPL"), None);
        assert_eq!(detect("postgres", "CockroachDB CCL v25.2.1 (x86_64-pc-linux-gnu, built …)"), Some("cockroachdb"));
        assert_eq!(detect("postgres", "PostgreSQL 16.4 on aarch64-unknown-linux-gnu, compiled by gcc"), None);
        assert_eq!(detect("postgres", "8.0.11-TiDB-v8.5.1"), None);
    }

    #[test]
    fn no_two_profiles_of_one_base_share_a_marker() {
        let all: Vec<(&String, &String, &String)> = PROFILES
            .iter()
            .flat_map(|(id, p)| p.detect.iter().map(move |m| (id, &p.base, m)))
            .collect();
        for (ia, ba, ma) in &all {
            for (ib, bb, mb) in &all {
                if ia != ib && ba == bb {
                    assert!(!ma.contains(mb.as_str()), "{ia}:{ma} overlaps {ib}:{mb}");
                }
            }
        }
    }

    #[test]
    fn detect_is_deterministic() {
        // MariaDB and TiDB both match. MariaDB is the longer marker, but it is the base
        // engine and TiDB is not: the text must not read as a server agents may reach.
        assert_eq!(detect("mysql", "5.7.0-TiDB-v1 MariaDB"), Some("tidb"));
        assert!(agent_block("mysql", None, detect("mysql", "10.6.0-MariaDB TiDB Server"), true).is_some());
        assert_eq!(detect("mysql", "11.4.2-MariaDB-ubu2404 mariadb.org binary distribution"), Some("mariadb"));
        // OceanBase (9) and TiDB (4), neither the base engine: the longest marker wins.
        assert_eq!(detect("mysql", "x TiDB OceanBase"), Some("oceanbase"));
        // TiDB and Dolt are both 4 characters; the tie goes to the lower id.
        assert_eq!(detect("mysql", "x TiDB Dolt"), Some("dolt"));
    }

    #[test]
    fn session_profile_prefers_a_known_saved_one() {
        let s = |b, sv, d| session_profile(b, sv, d);
        assert_eq!(s("mysql", Some("planetscale"), Some("vitess")).as_deref(), Some("planetscale"));
        assert_eq!(s("mysql", None, Some("tidb")).as_deref(), Some("tidb"));
        assert_eq!(s("mysql", Some("from-the-future"), Some("tidb")).as_deref(), Some("tidb"));
        assert_eq!(s("mysql", Some("from-the-future"), None), None);
        assert_eq!(s("postgres", Some("tidb"), None), None);
    }

    #[test]
    fn effective_caps_is_the_more_restrictive() {
        let all = Caps { transactions: true, backup: true, user_mgmt: true };
        assert_eq!(effective_caps(None, None), all);
        assert!(!effective_caps(Some("tidb"), None).backup);
        let c = effective_caps(Some("mariadb"), Some("doris"));
        assert!(!c.transactions && !c.backup);
        // Either side restricts: a fold where the detected product simply won would allow these.
        assert!(!effective_caps(Some("doris"), Some("tidb")).transactions);
        assert_eq!(effective_caps(Some("doris"), Some("mariadb")), Caps { transactions: false, backup: false, user_mgmt: false });
        assert_eq!(effective_caps(Some("from-the-future"), None), all);
    }

    #[test]
    fn only_a_refused_begin_turns_transactions_off() {
        assert!(caps_with_probe(None, None, Some(true)).transactions);
        assert!(caps_with_probe(None, None, None).transactions, "unknown keeps the static flag");
        assert!(!caps_with_probe(None, None, Some(false)).transactions);
        // The probe never grants: a profile without transactions stays without them.
        assert!(!caps_with_probe(Some("doris"), None, Some(true)).transactions);
        assert_eq!(caps_with_probe(Some("tidb"), None, Some(true)), effective_caps(Some("tidb"), None));
        let c = caps_with_probe(Some("mariadb"), None, Some(false));
        assert!(!c.transactions && c.backup && !c.user_mgmt);
    }

    #[test]
    fn agent_block_matches_the_case_table() {
        #[derive(serde::Deserialize)]
        struct Case { engine: String, profile: Option<String>, allowed: bool }
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../src/lib/agentAccessCases.json")).unwrap();
        for c in cases {
            let blocked = agent_block(&c.engine, c.profile.as_deref(), None, true);
            assert_eq!(blocked.is_none(), c.allowed, "{} {:?}", c.engine, c.profile);
        }
    }

    #[test]
    fn label_names_the_product_or_falls_back_to_the_id() {
        assert_eq!(label("tidb"), "TiDB");
        assert_eq!(label("from-the-future"), "from-the-future");
    }

    /// A known product saved on the other wire protocol is not a profile of this session: it
    /// reads like an id a newer build wrote, and a failed version query only matters where
    /// nothing else has already refused.
    #[test]
    fn agent_block_edges() {
        let m = agent_block("postgres", Some("tidb"), None, true).unwrap();
        assert!(m.contains("(tidb) was saved by a newer TableIQ"), "{m}");
        // A real engine under a vendor name is shared, unless the version query failed.
        assert!(agent_block("postgres", Some("neon"), None, true).is_none());
        let m = agent_block("postgres", Some("neon"), None, false).unwrap();
        assert!(m.contains("could not identify this server"), "{m}");
        // A compatible product is named even when the version query also failed.
        let m = agent_block("mysql", Some("tidb"), None, false).unwrap();
        assert!(m.contains("TiDB cannot be shared over MCP yet"), "{m}");
        // The detected product alone blocks a connection saved as plain MySQL; MariaDB does not.
        assert!(agent_block("mysql", None, Some("mariadb"), true).is_none());
        // CockroachDB is open to agents (its guard is reviewed); a profile that is not stays refused.
        assert!(agent_block("postgres", None, Some("cockroachdb"), true).is_none());
        assert!(agent_block("postgres", Some("neon"), Some("cockroachdb"), true).is_none());
        let m = agent_block("postgres", Some("redshift"), None, true).unwrap();
        assert!(m.contains("Amazon Redshift cannot be shared over MCP yet"), "{m}");
        // An empty saved profile is no profile, and no version failure means nothing to say.
        assert!(agent_block("mysql", Some(""), None, true).is_none());
    }

    /// `agent` is its own flag: absent it follows `real`, and only CockroachDB differs.
    #[test]
    fn agent_follows_real_unless_set() {
        let differing: Vec<&str> = PROFILES.iter().filter(|(_, p)| p.agent() != p.real).map(|(id, _)| id.as_str()).collect();
        assert_eq!(differing, ["cockroachdb"]);
        assert!(!lookup("cockroachdb").unwrap().real, "the drivers still treat it as a compatible product");
        assert!(lookup("neon").unwrap().agent() && !lookup("tidb").unwrap().agent());
    }

    /// A product opened to agents without being the base engine says when, so entries older
    /// than that are re-confirmed; one that is the base engine or closed has no such date.
    #[test]
    fn an_opened_compatible_product_has_a_generation() {
        for (id, p) in PROFILES.iter() {
            assert_eq!(p.agent() && !p.real, p.agent_since > 0, "{id}");
        }
        assert_eq!(agent_rev(), REDIS_AGENT_SINCE);
        assert!(MONGODB_AGENT_SINCE > lookup("cockroachdb").unwrap().agent_since, "a later opening than any product's");
        const { assert!(REDIS_AGENT_SINCE > MONGODB_AGENT_SINCE, "each opening is a generation of its own") };
    }

    #[test]
    fn an_entry_older_than_the_products_opening_is_not_served() {
        let now = agent_rev();
        assert!(reconfirm_block(Some("cockroachdb"), None, 0).unwrap().contains("CockroachDB over MCP"));
        assert!(reconfirm_block(None, Some("cockroachdb"), 0).is_some(), "found by detection alone");
        assert!(reconfirm_block(Some("neon"), Some("cockroachdb"), 0).is_some(), "a wrong label lifts nothing");
        assert!(reconfirm_block(Some("cockroachdb"), None, now).is_none(), "re-shared since");
        // The base engine and real engines under a vendor name were always open.
        for saved in [None, Some("neon"), Some("mariadb"), Some("from-the-future")] {
            assert!(reconfirm_block(saved, None, 0).is_none(), "{saved:?}");
        }
    }

    #[test]
    fn a_redis_entry_older_than_its_opening_is_not_served() {
        assert!(redis_reconfirm_block(REDIS_AGENT_SINCE - 1).unwrap().contains("Redis over MCP"));
        assert!(redis_reconfirm_block(0).is_some());
        assert!(redis_reconfirm_block(REDIS_AGENT_SINCE).is_none());
        assert!(redis_reconfirm_block(agent_rev()).is_none(), "a share made now is served");
    }

    #[test]
    fn the_guard_runs_for_cockroachdb_when_either_side_says_so() {
        assert_eq!(guard_engine("postgres", Some("cockroachdb"), None), "cockroachdb");
        assert_eq!(guard_engine("postgres", Some("neon"), Some("cockroachdb")), "cockroachdb");
        assert_eq!(guard_engine("postgres", None, None), "postgres");
        assert_eq!(guard_engine("postgres", Some("neon"), None), "postgres");
        assert_eq!(guard_engine("mysql", Some("cockroachdb"), None), "mysql");
    }

    #[test]
    fn agent_block_reasons() {
        let m = agent_block("mysql", None, Some("tidb"), true).unwrap();
        assert!(m.contains("TiDB cannot be shared over MCP yet"), "{m}");
        let m = agent_block("mysql", None, None, false).unwrap();
        assert!(m.contains("could not identify this server"), "{m}");
        let m = agent_block("mysql", Some("from-the-future"), None, true).unwrap();
        assert!(m.contains("was saved by a newer TableIQ"), "{m}");
        // An empty saved profile is no profile: what was detected still decides.
        let m = agent_block("mysql", Some(""), Some("tidb"), true).unwrap();
        assert!(m.contains("TiDB cannot be shared over MCP yet"), "{m}");
    }
}
