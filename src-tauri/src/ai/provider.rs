//! BYOK provider calls. Generated text is never executed by this layer.
//! The calls themselves need a live key, so only the pure bits are unit-tested here; the
//! deterministic scaffolding lives in `ai/mod.rs`.

use serde_json::{json, Value};
use std::time::Duration;

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

fn request_body(provider: &str, model: &str, system: &str, user: &str) -> Result<Value, String> {
    let messages = json!([{"role":"system", "content":system}, {"role":"user", "content":user}]);
    Ok(match provider {
        "ollama" => json!({"model":model, "stream":false, "messages":messages}),
        // These are text-only completions, not tool calls. Current GPT-6 models
        // support this endpoint; function tools would require Responses.
        "openai" => json!({"model":model, "messages":messages, "max_completion_tokens":8192}),
        "openrouter" => json!({"model":model, "messages":messages, "max_tokens":8192}),
        "gemini" => {
            let generation = if model.starts_with("gemini-4-") {
                // Current Gemini 4 combines thinking + answer in the output cap.
                // Medium effort keeps this interactive SQL path bounded without
                // exhausting a small cap before the final query is returned.
                json!({"maxOutputTokens":32768,"thinkingConfig":{"thinkingLevel":"MEDIUM"}})
            } else {
                json!({"maxOutputTokens":8192})
            };
            json!({
                "systemInstruction":{"parts":[{"text":system}]},
                "contents":[{"parts":[{"text":user}]}],
                "generationConfig":generation
            })
        }
        // Claude 5.5 can return thinking blocks before its answer. Give both
        // reasoning and answer room; parse only public text blocks below.
        "anthropic" => {
            json!({"model":model, "max_tokens":8192, "system":system, "messages":[{"role":"user", "content":user}]})
        }
        _ => return Err("Unsupported AI provider. Choose a provider in Settings → AI.".into()),
    })
}

fn public_error(provider: &str, status: u16, value: &Value, key: &str) -> String {
    let raw = value["error"]["message"]
        .as_str()
        .or_else(|| value["error"].as_str())
        .unwrap_or(
            "Request rejected. Check your API key, model access, quota and provider settings.",
        );
    let message = if key.is_empty() {
        raw.to_owned()
    } else {
        raw.replace(key, "[redacted]")
    };
    format!(
        "{provider} returned HTTP {status}: {}",
        message.chars().take(600).collect::<String>()
    )
}

fn completion_text(provider: &str, value: &Value) -> Result<String, String> {
    let stop = match provider {
        "anthropic" => value["stop_reason"].as_str(),
        "gemini" => value["candidates"][0]["finishReason"].as_str(),
        "ollama" => value["done_reason"].as_str(),
        _ => value["choices"][0]["finish_reason"].as_str(),
    };
    if matches!(
        stop,
        Some(
            "max_tokens"
                | "model_context_window_exceeded"
                | "MAX_TOKENS"
                | "CONTINUATION"
                | "length"
        )
    ) {
        return Err("The AI response reached its output limit. Partial SQL was not inserted; shorten the request or choose another model.".into());
    }
    if matches!(
        stop,
        Some("refusal" | "content_filter" | "SAFETY" | "RECITATION" | "PROHIBITED_CONTENT")
    ) {
        return Err(
            "The AI provider declined or filtered this request. Your query was not changed.".into(),
        );
    }
    if provider == "gemini" && stop.is_some_and(|reason| reason != "STOP") {
        return Err(
            "Gemini returned an incomplete or filtered answer. Partial SQL was not inserted."
                .into(),
        );
    }
    if matches!(provider, "openai" | "openrouter")
        && (value["choices"][0]
            .get("error")
            .is_some_and(|e| !e.is_null())
            || stop.is_some_and(|reason| reason != "stop"))
    {
        return Err("The AI model did not complete the request. Check model access and quota; partial SQL was not inserted.".into());
    }
    let text = match provider {
        "anthropic" => value["content"]
            .as_array()
            .map(|parts| {
                parts
                    .iter()
                    .filter(|part| part["type"] == "text")
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        "gemini" => value["candidates"][0]["content"]["parts"]
            .as_array()
            .map(|parts| {
                parts
                    .iter()
                    .filter(|part| part["thought"] != true)
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        "openai" | "openrouter" => {
            let message = &value["choices"][0]["message"];
            if message["refusal"].as_str().is_some_and(|r| !r.is_empty()) {
                return Err(
                    "The AI provider declined this request. Your query was not changed.".into(),
                );
            }
            message["content"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    message["content"]
                        .as_array()
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|part| part["text"].as_str())
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default()
                })
        }
        "ollama" => value["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        _ => return Err("Unsupported AI provider".into()),
    };
    if text.trim().is_empty() {
        return Err("The AI provider returned no answer text. Check model access, content restrictions or output limits. Your query was not changed.".into());
    }
    Ok(text)
}

#[async_trait::async_trait]
impl LlmProvider for ByokProvider {
    async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|_| "Could not start the AI request")?;
        let body = request_body(&self.provider, &self.model, system, user)?;
        let request = match self.provider.as_str() {
            "ollama" => client.post(format!("{}/api/chat", ollama_base())),
            "openai" => client
                .post("https://api.openai.com/v1/chat/completions")
                .bearer_auth(&self.api_key),
            "openrouter" => client
                .post("https://openrouter.ai/api/v1/chat/completions")
                .bearer_auth(&self.api_key),
            "gemini" => {
                client
                    .post(gemini_url(&self.model))
                    // Keep credentials out of URLs, transport errors and app logs.
                    .header("x-goog-api-key", &self.api_key)
            }
            "anthropic" => client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01"),
            _ => unreachable!("request_body validates providers before network access"),
        };
        let response = request.json(&body).send().await.map_err(|_| {
            format!(
                "Could not reach {}. Check your connection and provider settings.",
                self.provider
            )
        })?;
        let status = response.status();
        let value: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() || value.get("error").is_some_and(|e| !e.is_null()) {
            return Err(public_error(
                &self.provider,
                status.as_u16(),
                &value,
                &self.api_key,
            ));
        }
        if value.is_null() {
            return Err(format!("{} returned an unreadable response", self.provider));
        }
        completion_text(&self.provider, &value)
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
        assert!(
            !url.contains('?'),
            "the endpoint takes no query string: {url}"
        );
        assert!(
            !url.contains("key"),
            "a credential must not be in the URL: {url}"
        );
        assert!(
            url.ends_with("/models/gemini-2.5-flash:generateContent"),
            "{url}"
        );
    }

    #[test]
    fn reads_claude_answer_after_thinking_without_inserting_reasoning() {
        let value = json!({"content":[{"type":"thinking","thinking":"private reasoning"},{"type":"text","text":"SELECT 1;"},{"type":"text","text":"-- reviewed"}],"stop_reason":"end_turn"});
        assert_eq!(
            completion_text("anthropic", &value).unwrap(),
            "SELECT 1;\n-- reviewed"
        );
        assert!(completion_text(
            "anthropic",
            &json!({"content":[{"type":"thinking","thinking":"reasoning only"}]})
        )
        .is_err());
    }

    #[test]
    fn reads_only_gemini_final_text_parts() {
        let value = json!({"candidates":[{"content":{"parts":[{"thought":true,"text":"reasoning"},{"text":"SELECT 2;"}]},"finishReason":"STOP"}]});
        assert_eq!(completion_text("gemini", &value).unwrap(), "SELECT 2;");
        assert!(completion_text(
            "gemini",
            &json!({"promptFeedback":{"blockReason":"SAFETY"}})
        )
        .is_err());
    }

    #[test]
    fn refuses_truncated_sql_even_when_some_text_was_returned() {
        for provider in ["openai", "openrouter"] {
            assert!(completion_text(provider, &json!({"choices":[{"message":{"content":"SELECT dangerous_partial"},"finish_reason":"length"}]})).unwrap_err().contains("Partial SQL"));
        }
        assert!(completion_text(
            "anthropic",
            &json!({"content":[{"type":"text","text":"SELECT partial"}],"stop_reason":"max_tokens"})
        )
        .is_err());
    }

    #[test]
    fn router_uses_chat_shape_and_unknown_provider_cannot_fall_back_to_anthropic() {
        let body = request_body(
            "openrouter",
            "anthropic/claude-sonnet-5.5",
            "schema",
            "question",
        )
        .unwrap();
        assert_eq!(body["model"], "anthropic/claude-sonnet-5.5");
        assert_eq!(body["messages"][1]["content"], "question");
        assert!(request_body("unknown", "whatever", "s", "u").is_err());
        assert_eq!(
            completion_text(
                "openrouter",
                &json!({"choices":[{"message":{"content":"SELECT 3;"}}]})
            )
            .unwrap(),
            "SELECT 3;"
        );
    }

    #[test]
    fn api_errors_redact_a_key_echo_and_never_look_like_empty_success() {
        let err = public_error(
            "gemini",
            403,
            &json!({"error":{"message":"key fixture-secret rejected"}}),
            "fixture-secret",
        );
        assert!(err.contains("HTTP 403"));
        assert!(!err.contains("fixture-secret"));
        assert!(completion_text(
            "openai",
            &json!({"choices":[{"message":{"refusal":"declined"}}]})
        )
        .is_err());
        assert!(completion_text("openai", &json!({"choices":[]})).is_err());
    }

    #[test]
    fn rejects_router_mid_generation_errors_even_with_partial_content() {
        let value = json!({"choices":[{"message":{"content":"SELECT partial"},"finish_reason":"error","error":{"code":502,"message":"upstream failed"}}]});
        assert!(completion_text("openrouter", &value).is_err());
        let value =
            json!({"choices":[{"message":{"content":"SELECT partial"},"error":{"code":502}}]});
        assert!(completion_text("openrouter", &value).is_err());
    }

    #[test]
    fn rejects_gemini_continuation_and_filtered_partial_text() {
        for reason in [
            "CONTINUATION",
            "BLOCKLIST",
            "SPII",
            "ESCALATION",
            "PUP_LIMITED_DISABLED",
            "OTHER",
        ] {
            let value = json!({"candidates":[{"content":{"parts":[{"text":"SELECT partial"}]},"finishReason":reason}]});
            assert!(completion_text("gemini", &value).is_err(), "{reason}");
        }
    }

    #[test]
    fn gemini_four_uses_supported_medium_effort_without_legacy_thinking_budget() {
        for model in ["gemini-4-hydrogen", "gemini-4-carbon"] {
            let body = request_body("gemini", model, "schema", "question").unwrap();
            assert_eq!(
                body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
                "MEDIUM"
            );
            assert_eq!(body["generationConfig"]["maxOutputTokens"], 32768);
            assert!(body["generationConfig"]["thinkingConfig"]
                .get("thinkingBudget")
                .is_none());
        }
        let legacy = request_body("gemini", "gemini-2.5-flash", "s", "u").unwrap();
        assert!(legacy["generationConfig"].get("thinkingConfig").is_none());
    }
}
