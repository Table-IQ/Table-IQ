//! BYOK provider calls (Anthropic / OpenAI / Gemini) via minimal HTTP. One trait, three impls.
//! Not unit-tested (needs a live key); the deterministic scaffolding lives in `ai/mod.rs`.

use serde_json::json;

/// Ollama base URL (override with TIQ_OLLAMA_URL).
pub fn ollama_base() -> String {
    std::env::var("TIQ_OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".into())
}

/// Is a local Ollama server reachable (and which models does it have)?
pub async fn ollama_available() -> bool {
    reqwest::Client::new()
        .get(format!("{}/api/tags", ollama_base()))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, system: &str, user: &str) -> Result<String, String>;
}

pub struct ByokProvider {
    pub provider: String,
    pub model: String,
    pub api_key: String,
}

impl ByokProvider {
    pub fn new(provider: &str, model: &str, api_key: &str) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            api_key: api_key.into(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for ByokProvider {
    async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        let client = reqwest::Client::new();
        match self.provider.as_str() {
            "ollama" => {
                // Local, zero-egress. No API key. Default endpoint :11434.
                let resp = client
                    .post(format!("{}/api/chat", ollama_base()))
                    .json(&json!({
                        "model": self.model,
                        "stream": false,
                        "messages": [
                            {"role": "system", "content": system},
                            {"role": "user", "content": user}
                        ]
                    }))
                    .send()
                    .await
                    .map_err(|e| format!("Ollama not reachable ({e}). Is `ollama serve` running?"))?;
                let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                Ok(v["message"]["content"].as_str().unwrap_or("").to_string())
            }
            "openai" => {
                let resp = client
                    .post("https://api.openai.com/v1/chat/completions")
                    .bearer_auth(&self.api_key)
                    .json(&json!({
                        "model": self.model,
                        "messages": [
                            {"role": "system", "content": system},
                            {"role": "user", "content": user}
                        ]
                    }))
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                Ok(v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string())
            }
            "gemini" => {
                let url = format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                    self.model, self.api_key
                );
                let resp = client
                    .post(url)
                    .json(&json!({
                        "systemInstruction": {"parts": [{"text": system}]},
                        "contents": [{"parts": [{"text": user}]}]
                    }))
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                Ok(v["candidates"][0]["content"]["parts"][0]["text"].as_str().unwrap_or("").to_string())
            }
            _ => {
                // anthropic (default)
                let resp = client
                    .post("https://api.anthropic.com/v1/messages")
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&json!({
                        "model": self.model,
                        "max_tokens": 1024,
                        "system": system,
                        "messages": [{"role": "user", "content": user}]
                    }))
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
                Ok(v["content"][0]["text"].as_str().unwrap_or("").to_string())
            }
        }
    }
}
