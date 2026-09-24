//! BYOK provider calls (Anthropic / OpenAI / Gemini) via minimal HTTP. One trait, three impls.
//! The calls themselves need a live key, so only the pure bits are unit-tested here; the
//! deterministic scaffolding lives in `ai/mod.rs`.

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
                let resp = client
                    .post(gemini_url(&self.model))
                    // NOT `?key=…`: reqwest's error Display appends " for url (…)", and every
                    // call here ends in `.map_err(|e| e.to_string())`, so one transport failure
                    // put the API key into the string the UI renders and the frontend's global
                    // handler writes verbatim into the app log.
                    .header("x-goog-api-key", &self.api_key)
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

/// The Gemini `generateContent` endpoint. The API key goes in the `x-goog-api-key` header,
/// never in the URL — see the call site.
fn gemini_url(model: &str) -> String {
    format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `reqwest::Error`'s Display appends " for url (…)" and every provider arm ends in
    /// `.map_err(|e| e.to_string())`, so anything in the URL reaches the UI — and the
    /// frontend's global error handler writes that string verbatim into the app log.
    /// A DNS blip or a dropped connection used to be enough to log the user's Gemini key.
    #[test]
    fn the_gemini_url_carries_no_api_key() {
        let url = gemini_url("gemini-2.5-flash");
        assert!(!url.contains('?'), "the endpoint takes no query string: {url}");
        assert!(!url.contains("key"), "a credential must not be in the URL: {url}");
        assert!(url.ends_with("/models/gemini-2.5-flash:generateContent"), "{url}");
    }
}
