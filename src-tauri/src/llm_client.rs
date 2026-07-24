use crate::settings::PostProcessProvider;
use log::debug;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct JsonSchema {
    name: String,
    strict: bool,
    schema: Value,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
    json_schema: JsonSchema,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ReasoningConfig>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

// Anthropic's native Messages API requires `max_tokens`. A dictation cleanup
// returns text of roughly the input's length, so a bounded ceiling is plenty and
// keeps a runaway response from stalling the demo.
const ANTHROPIC_MAX_TOKENS: u32 = 8192;

/// Request body for Anthropic's native Messages API (`POST /v1/messages`).
/// Unlike the OpenAI-compatible providers, `system` is a top-level field and the
/// reply comes back as `content: [{type,text}]`, not `choices[]`.
#[derive(Debug, Serialize)]
struct AnthropicMessagesRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessagesResponse {
    #[serde(default)]
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: Option<String>,
}

/// Concatenate the text of every `text` block, ignoring thinking/tool blocks.
/// Returns None when the reply carries no usable text.
fn extract_anthropic_text(resp: &AnthropicMessagesResponse) -> Option<String> {
    let text: String = resp
        .content
        .iter()
        .filter(|b| b.block_type == "text")
        .filter_map(|b| b.text.as_deref())
        .collect();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Build headers for API requests based on provider type
fn build_headers(provider: &PostProcessProvider, api_key: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();

    // Common headers
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(REFERER, HeaderValue::from_static("https://abrax.app"));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Abrax/1.0 (+https://abrax.app)"),
    );
    headers.insert("X-Title", HeaderValue::from_static("Abrax"));

    // Provider-specific auth headers
    if !api_key.is_empty() {
        if provider.id == "anthropic" {
            headers.insert(
                "x-api-key",
                HeaderValue::from_str(api_key)
                    .map_err(|e| format!("Invalid API key header value: {}", e))?,
            );
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        } else {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", api_key))
                    .map_err(|e| format!("Invalid authorization header value: {}", e))?,
            );
        }
    }

    Ok(headers)
}

/// Create an HTTP client with provider-specific headers
fn create_client(provider: &PostProcessProvider, api_key: &str) -> Result<reqwest::Client, String> {
    let headers = build_headers(provider, api_key)?;
    reqwest::Client::builder()
        .default_headers(headers)
        // Bounded timeouts so post-processing can never leave the pipeline in
        // "Processing" forever when the provider stalls or the network is a
        // black hole (R6). The demo must never hang on the network.
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

/// Send a chat completion request to an OpenAI-compatible API
/// Returns Ok(Some(content)) on success, Ok(None) if response has no content,
/// or Err on actual errors (HTTP, parsing, etc.)
pub async fn send_chat_completion(
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    prompt: String,
    reasoning_effort: Option<String>,
    reasoning: Option<ReasoningConfig>,
) -> Result<Option<String>, String> {
    send_chat_completion_with_schema(
        provider,
        api_key,
        model,
        prompt,
        None,
        None,
        reasoning_effort,
        reasoning,
    )
    .await
}

/// Send a chat completion request with structured output support
/// When json_schema is provided, uses structured outputs mode
/// system_prompt is used as the system message when provided
/// reasoning_effort sets the OpenAI-style top-level field (e.g., "none", "low", "medium", "high")
/// reasoning sets the OpenRouter-style nested object (effort + exclude)
#[allow(clippy::too_many_arguments)]
pub async fn send_chat_completion_with_schema(
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    user_content: String,
    system_prompt: Option<String>,
    json_schema: Option<Value>,
    reasoning_effort: Option<String>,
    reasoning: Option<ReasoningConfig>,
) -> Result<Option<String>, String> {
    // Anthropic's native API is the Messages API (`/v1/messages`), not OpenAI's
    // `/chat/completions`. Route it to the correct endpoint and wire shape.
    // (The `anthropic` provider declares `supports_structured_output = false`, so
    // `json_schema`/`reasoning*` do not apply on this path.)
    if provider.id == "anthropic" {
        return send_anthropic_messages(provider, &api_key, model, user_content, system_prompt)
            .await;
    }

    let base_url = provider.base_url.trim_end_matches('/');
    let url = format!("{}/chat/completions", base_url);

    debug!("Sending chat completion request to: {}", url);

    let client = create_client(provider, &api_key)?;

    // Build messages vector
    let mut messages = Vec::new();

    // Add system prompt if provided
    if let Some(system) = system_prompt {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: system,
        });
    }

    // Add user message
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_content,
    });

    // Build response_format if schema is provided
    let response_format = json_schema.map(|schema| ResponseFormat {
        format_type: "json_schema".to_string(),
        json_schema: JsonSchema {
            name: "transcription_output".to_string(),
            strict: true,
            schema,
        },
    });

    let request_body = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        response_format,
        reasoning_effort,
        reasoning,
    };

    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error response".to_string());
        return Err(format!(
            "API request failed with status {}: {}",
            status, error_text
        ));
    }

    let completion: ChatCompletionResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse API response: {}", e))?;

    Ok(completion
        .choices
        .first()
        .and_then(|choice| choice.message.content.clone()))
}

/// Send a request to Anthropic's native Messages API. Anthropic exposes no
/// OpenAI-compatible `/chat/completions` endpoint, so the `anthropic` provider
/// must be routed here rather than through the OpenAI path.
async fn send_anthropic_messages(
    provider: &PostProcessProvider,
    api_key: &str,
    model: &str,
    user_content: String,
    system_prompt: Option<String>,
) -> Result<Option<String>, String> {
    let base_url = provider.base_url.trim_end_matches('/');
    let url = format!("{}/messages", base_url);

    debug!("Sending Anthropic Messages request to: {}", url);

    // create_client already sets `x-api-key` + `anthropic-version` for this provider.
    let client = create_client(provider, api_key)?;

    let request_body = AnthropicMessagesRequest {
        model: model.to_string(),
        max_tokens: ANTHROPIC_MAX_TOKENS,
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: user_content,
        }],
        system: system_prompt,
    };

    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error response".to_string());
        return Err(format!(
            "API request failed with status {}: {}",
            status, error_text
        ));
    }

    let completion: AnthropicMessagesResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse API response: {}", e))?;

    Ok(extract_anthropic_text(&completion))
}

/// Fetch available models from an OpenAI-compatible API
/// Returns a list of model IDs
pub async fn fetch_models(
    provider: &PostProcessProvider,
    api_key: String,
) -> Result<Vec<String>, String> {
    let base_url = provider.base_url.trim_end_matches('/');
    let url = format!("{}/models", base_url);

    debug!("Fetching models from: {}", url);

    let client = create_client(provider, &api_key)?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!(
            "Model list request failed ({}): {}",
            status, error_text
        ));
    }

    let parsed: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let mut models = Vec::new();

    // Handle OpenAI format: { data: [ { id: "..." }, ... ] }
    if let Some(data) = parsed.get("data").and_then(|d| d.as_array()) {
        for entry in data {
            if let Some(id) = entry.get("id").and_then(|i| i.as_str()) {
                models.push(id.to_string());
            } else if let Some(name) = entry.get("name").and_then(|n| n.as_str()) {
                models.push(name.to_string());
            }
        }
    }
    // Handle array format: [ "model1", "model2", ... ]
    else if let Some(array) = parsed.as_array() {
        for entry in array {
            if let Some(model) = entry.as_str() {
                models.push(model.to_string());
            }
        }
    }

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_request_uses_messages_api_shape() {
        let req = AnthropicMessagesRequest {
            model: "claude-haiku-4-5".to_string(),
            max_tokens: ANTHROPIC_MAX_TOKENS,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "hola".to_string(),
            }],
            system: Some("Eres un corrector de dictado.".to_string()),
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["model"], "claude-haiku-4-5");
        assert_eq!(v["max_tokens"], ANTHROPIC_MAX_TOKENS);
        assert_eq!(v["system"], "Eres un corrector de dictado.");
        assert_eq!(v["messages"][0]["role"], "user");
        assert_eq!(v["messages"][0]["content"], "hola");
        // Nothing OpenAI-shaped leaks into the Anthropic body.
        assert!(v.get("response_format").is_none());
        assert!(v.get("choices").is_none());
    }

    #[test]
    fn anthropic_request_omits_system_when_absent() {
        let req = AnthropicMessagesRequest {
            model: "m".to_string(),
            max_tokens: 8192,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "x".to_string(),
            }],
            system: None,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert!(v.get("system").is_none());
    }

    #[test]
    fn extracts_and_concatenates_text_blocks() {
        let resp: AnthropicMessagesResponse = serde_json::from_str(
            r#"{"content":[{"type":"text","text":"hola"},{"type":"text","text":" mundo"}]}"#,
        )
        .unwrap();
        assert_eq!(extract_anthropic_text(&resp).as_deref(), Some("hola mundo"));
    }

    #[test]
    fn ignores_non_text_blocks() {
        let resp: AnthropicMessagesResponse = serde_json::from_str(
            r#"{"content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"solo esto"}]}"#,
        )
        .unwrap();
        assert_eq!(extract_anthropic_text(&resp).as_deref(), Some("solo esto"));
    }

    #[test]
    fn empty_content_yields_none() {
        let resp: AnthropicMessagesResponse = serde_json::from_str(r#"{"content":[]}"#).unwrap();
        assert!(extract_anthropic_text(&resp).is_none());
    }
}
