//! TableIQ sample plugin (Extism PDK). ABI: two exports.
//!   info()        -> JSON `{name, description, kind}`  (for the plugin list)
//!   transform(s)  -> string  (input is a result set `{columns:[...], rows:[[...]]}`)
//! This one renders a GitHub-flavored Markdown table.

use extism_pdk::*;

#[plugin_fn]
pub fn info(_: ()) -> FnResult<String> {
    Ok(r#"{"name":"Markdown export","description":"Result set → GitHub-flavored Markdown table","kind":"exporter"}"#.to_string())
}

fn cell(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.replace('|', "\\|"),
        other => other.to_string(),
    }
}

#[plugin_fn]
pub fn transform(input: String) -> FnResult<String> {
    let v: serde_json::Value = serde_json::from_str(&input)?;
    let columns: Vec<String> = v["columns"]
        .as_array()
        .map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default();
    let rows = v["rows"].as_array().cloned().unwrap_or_default();

    let mut out = String::new();
    out.push_str(&format!("| {} |\n", columns.join(" | ")));
    out.push_str(&format!("| {} |\n", columns.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")));
    for r in rows {
        let cells: Vec<String> = r
            .as_array()
            .map(|a| a.iter().map(cell).collect())
            .unwrap_or_default();
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    Ok(out)
}
