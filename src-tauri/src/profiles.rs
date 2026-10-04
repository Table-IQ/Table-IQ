//! Engine profiles: products that speak the MySQL or Postgres wire protocol (TiDB, CockroachDB ...).
//! profile: a named product on top of a base engine; `ConnConfig.engine` stays "postgres" or "mysql".
//! product: what the server's version text says it is; `real`: the base engine itself under a vendor
//! name (Neon is PostgreSQL), so every catalog query and the MCP guard apply unchanged.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::connections::ConnConfig;

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

/// The saved profile, when it is known and sits on the connection's engine.
pub fn of(cfg: &ConnConfig) -> Option<&'static Profile> {
    lookup(cfg.profile.as_deref()?).filter(|p| p.base == cfg.engine)
}

/// The product a server's version text names, among the profiles of `base`. Longest marker
/// first, ties by id, so the answer never depends on map order.
pub fn detect(base: &str, version_text: &str) -> Option<&'static str> {
    let mut markers: Vec<(&str, &str)> = PROFILES
        .iter()
        .filter(|(_, p)| p.base == base)
        .flat_map(|(id, p)| p.detect.iter().map(move |m| (m.as_str(), id.as_str())))
        .collect();
    markers.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.1.cmp(b.1)));
    markers.into_iter().find(|(m, _)| version_text.contains(m)).map(|(_, id)| id)
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

/// Why a connection cannot be shared over MCP, or None when it can. An empty saved profile
/// is no profile, as in `capsOf` (`profiles.ts`).
pub fn agent_block(base: &str, saved: Option<&str>, detected: Option<&str>, detect_ok: bool) -> Option<String> {
    let saved = saved.filter(|id| !id.is_empty());
    if let Some(id) = saved.filter(|id| lookup(id).is_none_or(|p| p.base != base)) {
        return Some(format!(
            "This connection cannot be shared over MCP: its profile ({id}) was saved by a newer TableIQ. Update TableIQ to manage it."
        ));
    }
    if let Some(p) = [saved, detected].into_iter().flatten().filter_map(lookup).find(|p| !p.real) {
        return Some(format!(
            "{} cannot be shared over MCP yet: TableIQ's agent guard has been reviewed for PostgreSQL, MySQL and MariaDB only. Ask the user to run this in a TableIQ query tab.",
            p.label
        ));
    }
    (!detect_ok).then(|| {
        "TableIQ could not identify this server (its version query failed), so it cannot be shared over MCP. Ask the user to reconnect, or to run this in a TableIQ query tab.".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::tests::cfg;

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
    fn a_profile_on_the_wrong_base_is_not_a_profile() {
        let mut c = cfg("postgres");
        c.profile = Some("tidb".into());
        assert!(of(&c).is_none());
        let mut c = cfg("mysql");
        c.profile = Some("tidb".into());
        assert!(of(&c).is_some());
        c.profile = Some("from-the-future".into());
        assert!(of(&c).is_none());
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
        // MariaDB (7) and TiDB (4) both match; the longest marker wins.
        assert_eq!(detect("mysql", "5.7.0-TiDB-v1 MariaDB"), Some("mariadb"));
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
        assert_eq!(effective_caps(Some("from-the-future"), None), all);
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
