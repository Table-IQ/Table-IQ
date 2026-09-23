//! BYOK AI: NL→SQL. The deterministic parts (schema card, prompt, egress, SQL extraction)
//! are unit-tested; the live provider call is gated on a user-supplied key.

use serde::{Deserialize, Serialize};

use crate::drivers::SchemaModel;

pub mod provider;

/// Compact text description of the schema sent to the model. THIS is the egress payload —
/// `ai_egress_preview` returns exactly this so the user sees "what gets sent".
pub fn schema_card(model: &SchemaModel) -> String {
    let mut out = String::new();
    for s in &model.schemas {
        for o in &s.objects {
            let cols = o
                .columns
                .iter()
                .map(|c| format!("{} {}{}", c.name, c.data_type, if c.nullable { "" } else { " NOT NULL" }))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("{}.{} ({})\n", s.name, o.name, cols));
        }
    }
    out.trim_end().to_string()
}

pub const SYSTEM_PROMPT: &str =
    "You are a SQL expert. Given a database schema and a request, return ONE SQL query that answers \
     it. Return only SQL — no prose, no markdown fences. Use only tables/columns from the schema.";

/// Build the (system, user) messages. Pure — unit-tested.
pub fn build_prompt(card: &str, nl: &str) -> (String, String) {
    let user = format!("Schema:\n{card}\n\nRequest: {nl}\n\nSQL:");
    (SYSTEM_PROMPT.to_string(), user)
}

pub const EXPLAIN_SYSTEM: &str =
    "You are a SQL expert. Explain what the given SQL query does in clear, plain English for a \
     developer. Be concise. Do not return SQL.";

/// (system, user) for explain-in-plain-English. Pure — unit-tested.
pub fn build_explain_prompt(card: &str, sql: &str) -> (String, String) {
    (EXPLAIN_SYSTEM.to_string(), format!("Schema:\n{card}\n\nExplain this query:\n{sql}"))
}

pub const OPTIMIZE_SYSTEM: &str =
    "You are a SQL performance expert. Given a schema and a query, return an optimized, equivalent \
     SQL query that returns the same result set. Return only SQL — no prose, no markdown fences.";

/// (system, user) for optimize/rewrite. Pure — unit-tested.
pub fn build_optimize_prompt(card: &str, sql: &str) -> (String, String) {
    (OPTIMIZE_SYSTEM.to_string(), format!("Schema:\n{card}\n\nOptimize this query:\n{sql}\n\nSQL:"))
}

pub const FIX_SYSTEM: &str =
    "You are a SQL expert. Given a schema, a failing query, and its error message, return a \
     corrected SQL query. Return only SQL — no prose, no markdown fences.";

/// (system, user) for fix-this-error. Pure — unit-tested.
pub fn build_fix_prompt(card: &str, sql: &str, error: &str) -> (String, String) {
    (
        FIX_SYSTEM.to_string(),
        format!("Schema:\n{card}\n\nThis query failed:\n{sql}\n\nError:\n{error}\n\nCorrected SQL:"),
    )
}

/// Strip ```sql fences / surrounding prose the model may add, returning bare SQL.
pub fn extract_sql(raw: &str) -> String {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // drop an optional language tag on the first line, then take up to the closing fence
        let body = rest.split_once('\n').map_or("", |(_, body)| body);
        let end = body.find("```").unwrap_or(body.len());
        return body[..end].trim().to_string();
    }
    t.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub provider: String, // "anthropic" | "openai" | "gemini"
    pub model: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
        }
    }
}

fn config_path() -> std::path::PathBuf {
    crate::appdir::app_dir().join("ai.json")
}

pub fn load_config() -> AiConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(cfg: &AiConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    crate::appdir::write_atomic(&config_path(), json.as_bytes()).map_err(|e| e.to_string())
}

/// Keychain account for a provider's API key.
pub fn key_account(provider: &str) -> String {
    format!("ai-key:{provider}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::{ColumnInfo, ObjectInfo, ObjectKind, SchemaInfo};

    fn model() -> SchemaModel {
        SchemaModel {
            schemas: vec![SchemaInfo {
                name: "public".into(),
                objects: vec![ObjectInfo {
                    name: "users".into(),
                    kind: ObjectKind::Table,
                    columns: vec![
                        ColumnInfo { name: "id".into(), data_type: "bigint".into(), nullable: false, pk: true },
                        ColumnInfo { name: "email".into(), data_type: "text".into(), nullable: true, pk: false },
                    ],
                }],
            }],
        }
    }

    #[test]
    fn schema_card_lists_tables_and_columns() {
        let card = schema_card(&model());
        assert_eq!(card, "public.users (id bigint NOT NULL, email text)");
    }

    #[test]
    fn prompt_includes_schema_and_request() {
        let (sys, user) = build_prompt(&schema_card(&model()), "all users");
        assert!(sys.contains("SQL expert"));
        assert!(user.contains("public.users"));
        assert!(user.contains("all users"));
    }

    #[test]
    fn explain_optimize_fix_prompts_carry_context() {
        let card = schema_card(&model());
        let (es, eu) = build_explain_prompt(&card, "SELECT 1");
        assert!(es.contains("plain English"));
        assert!(eu.contains("public.users") && eu.contains("SELECT 1"));

        let (os, ou) = build_optimize_prompt(&card, "SELECT * FROM users");
        assert!(os.contains("performance"));
        assert!(ou.contains("Optimize this query") && ou.contains("SELECT * FROM users"));

        let (fs, fu) = build_fix_prompt(&card, "SELCT 1", "syntax error at SELCT");
        assert!(fs.contains("corrected SQL") || fs.contains("corrected"));
        assert!(fu.contains("SELCT 1") && fu.contains("syntax error at SELCT"));
    }

    #[test]
    fn extract_sql_strips_markdown_fences() {
        assert_eq!(extract_sql("```sql\nSELECT 1;\n```"), "SELECT 1;");
        assert_eq!(extract_sql("SELECT 2;"), "SELECT 2;");
        assert_eq!(extract_sql("  SELECT 3;  "), "SELECT 3;");
    }
}
