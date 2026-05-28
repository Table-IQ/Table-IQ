//! MongoDB `DocumentDriver` — collection + document browse.

use async_trait::async_trait;
use mongodb::bson::Document;
use mongodb::{Client, Database};

use super::{DocumentDriver, DResult, DriverError};

fn e<E: std::fmt::Display>(err: E) -> DriverError {
    DriverError::Db(err.to_string())
}

pub struct MongoDriver {
    client: Client,
    db: Database,
}

impl MongoDriver {
    pub async fn connect(url: &str, database: &str) -> DResult<Self> {
        let client = Client::with_uri_str(url).await.map_err(e)?;
        let name = if database.is_empty() { "test" } else { database };
        let db = client.database(name);
        Ok(Self { client, db })
    }
}

#[async_trait]
impl DocumentDriver for MongoDriver {
    async fn list_collections(&self) -> DResult<Vec<String>> {
        self.db.list_collection_names().await.map_err(e)
    }

    async fn find(&self, collection: &str, limit: i64) -> DResult<Vec<String>> {
        let coll = self.db.collection::<Document>(collection);
        let mut cursor = coll.find(Document::new()).limit(limit).await.map_err(e)?;
        let mut out = Vec::new();
        // mongodb's native cursor API (no external futures dependency).
        while cursor.advance().await.map_err(e)? {
            let doc = cursor.deserialize_current().map_err(e)?;
            out.push(serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".into()));
        }
        Ok(out)
    }

    async fn list_databases(&self) -> DResult<Vec<String>> {
        // listDatabaseNames hits the admin command and returns names visible to
        // the authenticated user. We keep the client (not just the active db) on
        // the driver specifically so this call doesn't have to reconnect.
        self.client.list_database_names().await.map_err(e)
    }
}
