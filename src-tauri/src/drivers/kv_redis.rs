//! Redis `KvDriver` — key browse (SCAN + per-type value read).

use async_trait::async_trait;
use redis::aio::ConnectionManager;

use super::{DResult, DriverError, KeyInfo, KvDriver, KvValue};

fn e(err: redis::RedisError) -> DriverError {
    DriverError::Db(err.to_string())
}

pub struct RedisDriver {
    con: ConnectionManager,
}

impl RedisDriver {
    pub async fn connect(url: &str) -> DResult<Self> {
        let client = redis::Client::open(url).map_err(e)?;
        let con = ConnectionManager::new(client).await.map_err(e)?;
        Ok(Self { con })
    }
}

#[async_trait]
impl KvDriver for RedisDriver {
    async fn scan(&self, pattern: &str, limit: usize) -> DResult<Vec<KeyInfo>> {
        let mut con = self.con.clone();
        let pat = if pattern.is_empty() { "*" } else { pattern };
        let mut cursor: u64 = 0;
        let mut keys: Vec<String> = Vec::new();
        loop {
            let (next, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pat)
                .arg("COUNT")
                .arg(200)
                .query_async(&mut con)
                .await
                .map_err(e)?;
            keys.extend(batch);
            cursor = next;
            if cursor == 0 || keys.len() >= limit {
                break;
            }
        }
        keys.truncate(limit);

        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            let kind: String = redis::cmd("TYPE").arg(&key).query_async(&mut con).await.map_err(e)?;
            let ttl: i64 = redis::cmd("TTL").arg(&key).query_async(&mut con).await.map_err(e)?;
            out.push(KeyInfo { key, kind, ttl });
        }
        Ok(out)
    }

    async fn get(&self, key: &str) -> DResult<KvValue> {
        let mut con = self.con.clone();
        let kind: String = redis::cmd("TYPE").arg(key).query_async(&mut con).await.map_err(e)?;
        let (columns, rows): (Vec<String>, Vec<Vec<String>>) = match kind.as_str() {
            "string" => {
                let v: String = redis::cmd("GET").arg(key).query_async(&mut con).await.map_err(e)?;
                (vec!["value".into()], vec![vec![v]])
            }
            "list" => {
                let items: Vec<String> =
                    redis::cmd("LRANGE").arg(key).arg(0).arg(-1).query_async(&mut con).await.map_err(e)?;
                let rows = items.into_iter().enumerate().map(|(i, v)| vec![i.to_string(), v]).collect();
                (vec!["#".into(), "value".into()], rows)
            }
            "set" => {
                let items: Vec<String> =
                    redis::cmd("SMEMBERS").arg(key).query_async(&mut con).await.map_err(e)?;
                (vec!["member".into()], items.into_iter().map(|m| vec![m]).collect())
            }
            "hash" => {
                let flat: Vec<String> =
                    redis::cmd("HGETALL").arg(key).query_async(&mut con).await.map_err(e)?;
                let rows = flat.chunks(2).map(|c| vec![c[0].clone(), c.get(1).cloned().unwrap_or_default()]).collect();
                (vec!["field".into(), "value".into()], rows)
            }
            "zset" => {
                let flat: Vec<String> = redis::cmd("ZRANGE")
                    .arg(key)
                    .arg(0)
                    .arg(-1)
                    .arg("WITHSCORES")
                    .query_async(&mut con)
                    .await
                    .map_err(e)?;
                let rows = flat.chunks(2).map(|c| vec![c[0].clone(), c.get(1).cloned().unwrap_or_default()]).collect();
                (vec!["member".into(), "score".into()], rows)
            }
            other => (vec!["type".into()], vec![vec![other.to_string()]]),
        };
        Ok(KvValue { kind, columns, rows })
    }

    async fn list_databases(&self) -> DResult<Vec<String>> {
        // Redis exposes a fixed-count databases config (default 16). `CONFIG GET
        // databases` returns the pair `["databases", "16"]`; we enumerate 0..N-1
        // and return them as strings so the same picker UI handles every engine.
        let mut con = self.con.clone();
        let pair: Vec<String> = redis::cmd("CONFIG")
            .arg("GET")
            .arg("databases")
            .query_async(&mut con)
            .await
            .map_err(e)?;
        let n: usize = pair
            .get(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(16);
        Ok((0..n).map(|i| i.to_string()).collect())
    }
}
