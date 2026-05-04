use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

use crate::model_alias::{
    MODEL_ORDER, model_mappings_with_legacy_aliases, normalize_model_mappings,
    resolve_requested_model_slot,
};
use crate::models::{AppConfig, Provider};

pub(super) fn normalize_anthropic_response(upstream_data: &Value, model: &str) -> Value {
    let Some(object) = upstream_data.as_object() else {
        return upstream_data.clone();
    };
    if object.contains_key("error") {
        return upstream_data.clone();
    }
    if object.get("type").and_then(Value::as_str) == Some("message")
        || object.contains_key("content")
    {
        return normalize_anthropic_message(upstream_data, model);
    }
    upstream_data.clone()
}

pub(super) fn normalize_anthropic_sse_event(event: &Value, model: &str) -> Value {
    let Some(object) = event.as_object() else {
        return event.clone();
    };
    let mut normalized = object.clone();
    match normalized.get("type").and_then(Value::as_str) {
        Some("message_start") => {
            let message = normalized
                .get("message")
                .cloned()
                .unwrap_or_else(|| json!({}));
            normalized.insert(
                "message".to_string(),
                normalize_anthropic_message(&message, model),
            );
        }
        Some("message_delta") => {
            let usage = normalize_usage(normalized.get("usage").unwrap_or(&Value::Null));
            normalized.insert("usage".to_string(), usage);
        }
        _ if normalized.contains_key("usage") => {
            let usage = normalize_usage(normalized.get("usage").unwrap_or(&Value::Null));
            normalized.insert("usage".to_string(), usage);
        }
        _ => {}
    }
    Value::Object(normalized)
}

fn normalize_anthropic_message(message: &Value, model: &str) -> Value {
    let mut object = message.as_object().cloned().unwrap_or_default();
    object
        .entry("id".to_string())
        .or_insert_with(|| Value::String(generated_message_id()));
    object
        .entry("type".to_string())
        .or_insert_with(|| Value::String("message".to_string()));
    object
        .entry("role".to_string())
        .or_insert_with(|| Value::String("assistant".to_string()));
    let response_model = object
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(model)
        .to_string();
    object.insert("model".to_string(), Value::String(response_model));
    let content = normalize_content_blocks(object.get("content").unwrap_or(&Value::Null));
    object.insert("content".to_string(), content);
    let usage = normalize_usage(object.get("usage").unwrap_or(&Value::Null));
    object.insert("usage".to_string(), usage);
    Value::Object(object)
}

fn normalize_content_blocks(content: &Value) -> Value {
    if content.is_array() {
        return content.clone();
    }
    if let Some(text) = content.as_str() {
        return json!([{"type": "text", "text": text}]);
    }
    if content.is_null() {
        return Value::Array(Vec::new());
    }
    json!([{"type": "text", "text": content.to_string()}])
}

pub(super) fn max_unsupported_hint_message(model: &str) -> Value {
    json!({
        "id": "msg_hint",
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [{"type": "text", "text": "该模型不支持 max，请取消勾选。"}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 0, "output_tokens": 0}
    })
}

pub(super) fn is_max_unsupported_error(status_code: u16, error_text: &str) -> bool {
    if !matches!(status_code, 400 | 422) {
        return false;
    }
    let text = error_text.to_lowercase();
    [
        "output_config",
        "thinking",
        "effort",
        "max",
        "not supported",
        "unsupported",
        "invalid parameter",
    ]
    .iter()
    .any(|keyword| text.contains(keyword))
}

pub(super) fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

pub(super) fn error_name(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "TimeoutError"
    } else if error.is_connect() {
        "ConnectError"
    } else if error.is_decode() {
        "DecodeError"
    } else if error.is_request() {
        "RequestError"
    } else {
        "ReqwestError"
    }
}

pub fn gateway_auth_failed(
    gateway_api_key: Option<&str>,
    authorization: Option<&str>,
    x_api_key: Option<&str>,
) -> bool {
    let Some(gateway_api_key) = gateway_api_key.filter(|key| !key.is_empty()) else {
        return true;
    };
    let bearer = authorization
        .unwrap_or("")
        .strip_prefix("Bearer ")
        .unwrap_or(authorization.unwrap_or(""))
        .trim();
    let x_api_key = x_api_key.unwrap_or("").trim();
    gateway_api_key != bearer && gateway_api_key != x_api_key
}

pub fn gateway_models_response(
    provider: Option<&Provider>,
    providers: Option<&[Provider]>,
    expose_all: bool,
) -> Value {
    let entries = if expose_all {
        all_provider_model_entries(providers.unwrap_or_default())
    } else {
        provider_model_ids(provider)
            .into_iter()
            .map(|model_id| ModelEntry {
                name: model_id.clone(),
                display_name: model_id,
                supports_1m: false,
            })
            .collect()
    };
    let data = entries
        .into_iter()
        .map(|item| {
            let mut row = Map::new();
            row.insert("type".to_string(), Value::String("model".to_string()));
            row.insert("id".to_string(), Value::String(item.name.clone()));
            row.insert("display_name".to_string(), Value::String(item.display_name));
            row.insert(
                "created_at".to_string(),
                Value::String("2024-01-01T00:00:00Z".to_string()),
            );
            if item.supports_1m {
                row.insert("supports1m".to_string(), Value::Bool(true));
            }
            Value::Object(row)
        })
        .collect::<Vec<_>>();

    let first_id = data
        .first()
        .and_then(|item| item.get("id"))
        .cloned()
        .unwrap_or(Value::Null);
    let last_id = data
        .last()
        .and_then(|item| item.get("id"))
        .cloned()
        .unwrap_or(Value::Null);

    json!({
        "data": data,
        "has_more": false,
        "first_id": first_id,
        "last_id": last_id
    })
}

pub fn map_model(original_model: &str, provider: Option<&Provider>) -> String {
    let Some(provider) = provider else {
        return original_model.to_string();
    };
    if original_model.is_empty() {
        return original_model.to_string();
    }
    let models_config = model_mappings_with_legacy_aliases(&provider.models);
    if models_config.is_empty() {
        return original_model.to_string();
    }
    if provider_model_ids(Some(provider))
        .iter()
        .any(|model_id| model_id == original_model)
    {
        return original_model.to_string();
    }
    if let Some(slot) = resolve_requested_model_slot(original_model) {
        return models_config
            .get(&slot)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                models_config
                    .get("default")
                    .filter(|value| !value.is_empty())
            })
            .cloned()
            .unwrap_or_else(|| original_model.to_string());
    }
    models_config
        .get("default")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| original_model.to_string())
}

pub fn build_upstream_url(base_url: &str, api_format: &str) -> String {
    let clean = base_url.trim().trim_end_matches('/');
    let api_format = normalize_api_format(api_format);
    let lower = clean.to_lowercase();
    if api_format == "openai_chat" {
        if lower.ends_with("/chat/completions") {
            return clean.to_string();
        }
        return format!("{clean}/chat/completions");
    }
    if lower.ends_with("/v1/messages") {
        return clean.to_string();
    }
    if lower.ends_with("/v1") {
        return format!("{clean}/messages");
    }
    format!("{clean}/v1/messages")
}

pub fn normalize_api_format(value: &str) -> String {
    let normalized = value.trim().to_lowercase().replace('-', "_");
    if matches!(
        normalized.as_str(),
        "openai" | "openai_chat" | "chat_completions"
    ) {
        return "openai_chat".to_string();
    }
    if matches!(normalized.as_str(), "anthropic" | "claude" | "messages") {
        return "anthropic".to_string();
    }
    if normalized.is_empty() {
        "anthropic".to_string()
    } else {
        normalized
    }
}

pub fn get_upstream_headers(provider: &Provider) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::from([
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Accept".to_string(), "application/json".to_string()),
    ]);
    if normalize_api_format(&provider.api_format) == "anthropic" {
        headers.insert("anthropic-version".to_string(), "2023-06-01".to_string());
    }
    if !provider.api_key.is_empty() {
        if provider.auth_scheme == "x-api-key" {
            headers.insert("x-api-key".to_string(), provider.api_key.clone());
        } else {
            headers.insert(
                "Authorization".to_string(),
                format!("Bearer {}", provider.api_key),
            );
        }
    }
    for (key, value) in &provider.extra_headers {
        headers.insert(key.clone(), value.replace("{apiKey}", &provider.api_key));
    }
    headers
}

pub fn anthropic_to_openai_chat_body(body: &Value, stream: bool) -> Value {
    let body_object = body.as_object().cloned().unwrap_or_default();
    let mut messages = body_object
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut system_message = body_object.get("system").cloned();
    if system_message.is_none() {
        if let Some(first) = messages.first().and_then(Value::as_object) {
            if first.get("role").and_then(Value::as_str) == Some("system") {
                system_message = first.get("content").cloned();
                messages.remove(0);
            }
        }
    }

    let mut openai_messages = Vec::new();
    let system_text = content_to_text(system_message.as_ref().unwrap_or(&Value::Null));
    if !system_text.is_empty() {
        openai_messages.push(json!({"role": "system", "content": system_text}));
    }
    for message in messages {
        openai_messages.extend(anthropic_message_to_openai(&message));
    }

    let mut openai_body = Map::new();
    openai_body.insert(
        "model".to_string(),
        body_object
            .get("model")
            .cloned()
            .unwrap_or(Value::String(String::new())),
    );
    openai_body.insert("messages".to_string(), Value::Array(openai_messages));
    openai_body.insert(
        "max_tokens".to_string(),
        body_object
            .get("max_tokens")
            .cloned()
            .unwrap_or_else(|| Value::Number(4096.into())),
    );
    openai_body.insert("stream".to_string(), Value::Bool(stream));
    for field in ["temperature", "top_p"] {
        if let Some(value) = body_object.get(field).filter(|value| !value.is_null()) {
            openai_body.insert(field.to_string(), value.clone());
        }
    }
    if let Some(stop_sequences) = body_object
        .get("stop_sequences")
        .filter(|value| !value_is_empty(value))
    {
        openai_body.insert("stop".to_string(), stop_sequences.clone());
    }
    let tools = anthropic_tools_to_openai(body_object.get("tools").unwrap_or(&Value::Null));
    if !tools.is_empty() {
        openai_body.insert("tools".to_string(), Value::Array(tools));
    }
    if let Some(tool_choice) = body_object.get("tool_choice") {
        openai_body.insert(
            "tool_choice".to_string(),
            anthropic_tool_choice_to_openai(tool_choice),
        );
    }
    Value::Object(openai_body)
}

pub fn openai_chat_to_anthropic(openai_response: &Value, model: &str) -> Value {
    let choice = openai_response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut content_blocks = Vec::new();
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        if !text.is_empty() {
            content_blocks.push(json!({"type": "text", "text": text}));
        }
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for tool_call in tool_calls {
            content_blocks.push(tool_call_to_anthropic_block(tool_call));
        }
    }
    if content_blocks.is_empty() {
        content_blocks.push(json!({"type": "text", "text": ""}));
    }

    json!({
        "id": openai_response.get("id").and_then(Value::as_str).map(ToOwned::to_owned).unwrap_or_else(generated_message_id),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content_blocks,
        "stop_reason": openai_finish_reason_to_anthropic(choice.get("finish_reason").unwrap_or(&Value::Null), message.contains_key("tool_calls")),
        "usage": normalize_usage(openai_response.get("usage").unwrap_or(&Value::Null))
    })
}

pub fn openai_chat_chunk_to_anthropic(chunk: &Value, model: &str) -> Value {
    let Some(choice) = chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
    else {
        return json!({"type": "message_stop"});
    };
    let delta = choice
        .get("delta")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if delta.get("tool_calls").is_some() {
        return json!({
            "type": "error",
            "error": {
                "type": "unsupported_streaming_tool_call",
                "message": "OpenAI Chat experimental adapter does not support streaming tool calls yet."
            }
        });
    }
    let finish_reason = choice.get("finish_reason").filter(|value| !value.is_null());
    let content = delta.get("content").and_then(Value::as_str).unwrap_or("");
    if content.is_empty() {
        if finish_reason.is_some() {
            return json!({"type": "message_stop"});
        }
        if delta.get("role").is_some() {
            return json!({
                "type": "message_start",
                "message": {
                    "id": generated_message_id(),
                    "type": "message",
                    "role": "assistant",
                    "model": model,
                    "content": [],
                    "usage": {"input_tokens": 0, "output_tokens": 0}
                }
            });
        }
        return json!({"type": "ping"});
    }
    json!({
        "type": "content_block_delta",
        "index": 0,
        "delta": {"type": "text_delta", "text": content}
    })
}

pub fn apply_anthropic_request_options(body: &Value, provider: &Provider) -> Value {
    let mut upstream_body = body.as_object().cloned().unwrap_or_default();
    let kind = provider_kind(provider);
    if kind != "deepseek" {
        upstream_body.remove("thinking");
        return Value::Object(upstream_body);
    }
    if let Some(options) = anthropic_request_options(provider) {
        upstream_body = deep_merge(upstream_body, options);
    }
    Value::Object(upstream_body)
}

pub(super) fn active_provider(config: &AppConfig) -> Option<&Provider> {
    let active_id = config.active_provider.as_deref()?;
    config
        .providers
        .iter()
        .find(|provider| provider.id == active_id)
}

pub(super) fn provider_model_ids(provider: Option<&Provider>) -> Vec<String> {
    let Some(provider) = provider else {
        return Vec::new();
    };
    let models = normalize_model_mappings(Some(&provider.models));
    let mut ordered = Vec::new();
    for key in MODEL_ORDER {
        let model_id = models.get(key).map(|value| value.trim()).unwrap_or("");
        if !model_id.is_empty() && !ordered.iter().any(|existing| existing == model_id) {
            ordered.push(model_id.to_string());
        }
    }
    ordered
}

#[derive(Debug, Clone)]
struct ModelEntry {
    name: String,
    display_name: String,
    supports_1m: bool,
}

fn provider_model_entries(provider: &Provider, use_alias: bool) -> Vec<ModelEntry> {
    let provider_name = if provider.name.is_empty() {
        provider.id.as_str()
    } else {
        provider.name.as_str()
    };
    provider_model_ids(Some(provider))
        .into_iter()
        .map(|model_id| {
            let name = if use_alias {
                format!("{}/{}", provider_slug(provider), model_id)
            } else {
                model_id.clone()
            };
            let display_name = if use_alias {
                format!("{provider_name} / {model_id}")
            } else {
                model_id.clone()
            };
            ModelEntry {
                name,
                display_name,
                supports_1m: model_supports_1m(provider, &model_id),
            }
        })
        .collect()
}

fn all_provider_model_entries(providers: &[Provider]) -> Vec<ModelEntry> {
    let mut entries = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for provider in providers {
        for item in provider_model_entries(provider, true) {
            if seen.insert(item.name.clone()) {
                entries.push(item);
            }
        }
    }
    entries
}

pub(super) fn provider_slug(provider: &Provider) -> String {
    let source = if provider.id.is_empty() {
        provider.name.as_str()
    } else {
        provider.id.as_str()
    };
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in source.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
        if slug.len() >= 56 {
            break;
        }
    }
    let trimmed = slug.trim_matches(&['-', '_'][..]);
    if trimmed.is_empty() {
        "provider".to_string()
    } else {
        trimmed.to_string()
    }
}

fn model_supports_1m(provider: &Provider, model_id: &str) -> bool {
    if model_id.to_lowercase().contains("[1m]") {
        return true;
    }
    provider
        .model_capabilities
        .get(model_id)
        .and_then(Value::as_object)
        .and_then(|capability| capability.get("supports1m"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn content_to_text(content: &Value) -> String {
    if content.is_null() {
        return String::new();
    }
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    if let Some(items) = content.as_array() {
        return items
            .iter()
            .filter_map(|item| {
                if let Some(text) = item.as_str() {
                    return Some(text.to_string());
                }
                let object = item.as_object()?;
                if let Some(text) = object.get("text").and_then(Value::as_str) {
                    return Some(text.to_string());
                }
                if let Some(text) = object.get("content").and_then(Value::as_str) {
                    return Some(text.to_string());
                }
                if let Some(content) = object.get("content").filter(|value| value.is_array()) {
                    let text = content_to_text(content);
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
                None
            })
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
    }
    content.to_string()
}

fn tool_result_content(block: &Map<String, Value>) -> String {
    let content = block.get("content").unwrap_or(&Value::Null);
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    if content.is_array() {
        let text = content_to_text(content);
        if text.is_empty() {
            return serde_json::to_string(content).unwrap_or_default();
        }
        return text;
    }
    if content.is_null() {
        return String::new();
    }
    if content.is_object() {
        return serde_json::to_string(content).unwrap_or_default();
    }
    content.to_string()
}

fn anthropic_tools_to_openai(tools: &Value) -> Vec<Value> {
    let Some(tools) = tools.as_array() else {
        return Vec::new();
    };
    tools
        .iter()
        .filter_map(|tool| {
            let object = tool.as_object()?;
            if object.get("type").and_then(Value::as_str) == Some("function")
                && object.get("function").is_some_and(Value::is_object)
            {
                return Some(tool.clone());
            }
            let name = object.get("name").and_then(Value::as_str)?;
            Some(json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": object.get("description").cloned().unwrap_or(Value::String(String::new())),
                    "parameters": object
                        .get("input_schema")
                        .or_else(|| object.get("parameters"))
                        .cloned()
                        .unwrap_or_else(|| json!({"type": "object"}))
                }
            }))
        })
        .collect()
}

fn anthropic_tool_choice_to_openai(tool_choice: &Value) -> Value {
    let Some(object) = tool_choice.as_object() else {
        return tool_choice.clone();
    };
    match object.get("type").and_then(Value::as_str) {
        Some("auto") => Value::String("auto".to_string()),
        Some("any") => Value::String("required".to_string()),
        Some("none") => Value::String("none".to_string()),
        Some("tool") => object
            .get("name")
            .and_then(Value::as_str)
            .map(|name| json!({"type": "function", "function": {"name": name}}))
            .unwrap_or_else(|| tool_choice.clone()),
        _ => tool_choice.clone(),
    }
}

fn anthropic_message_to_openai(message: &Value) -> Vec<Value> {
    let object = message.as_object().cloned().unwrap_or_default();
    let mut role = object
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user")
        .to_string();
    if !matches!(role.as_str(), "system" | "user" | "assistant" | "tool") {
        role = "user".to_string();
    }
    let content = object.get("content").cloned().unwrap_or(Value::Null);
    let Some(blocks) = content.as_array() else {
        return vec![json!({"role": role, "content": content_to_text(&content)})];
    };

    let mut tool_messages = Vec::new();
    let mut text_blocks = Vec::new();
    let mut tool_calls = Vec::new();
    for block in blocks {
        let Some(block_object) = block.as_object() else {
            text_blocks.push(block.to_string());
            continue;
        };
        match block_object.get("type").and_then(Value::as_str) {
            Some("tool_result") => tool_messages.push(json!({
                "role": "tool",
                "tool_call_id": block_object
                    .get("tool_use_id")
                    .or_else(|| block_object.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                "content": tool_result_content(block_object)
            })),
            Some("tool_use") => tool_calls.push(json!({
                "id": block_object
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(generated_tool_call_id),
                "type": "function",
                "function": {
                    "name": block_object.get("name").and_then(Value::as_str).unwrap_or("tool"),
                    "arguments": serde_json::to_string(block_object.get("input").unwrap_or(&json!({}))).unwrap_or_else(|_| "{}".to_string())
                }
            })),
            _ => {
                if let Some(text) = block_object.get("text").and_then(Value::as_str) {
                    text_blocks.push(text.to_string());
                }
            }
        }
    }

    if role == "assistant" && !tool_calls.is_empty() {
        return vec![json!({
            "role": "assistant",
            "content": if text_blocks.is_empty() { Value::Null } else { Value::String(text_blocks.join("\n")) },
            "tool_calls": tool_calls
        })];
    }
    if role == "user" && !tool_messages.is_empty() {
        let mut messages = tool_messages;
        let text = text_blocks.join("\n");
        if !text.is_empty() {
            messages.push(json!({"role": "user", "content": text}));
        }
        return messages;
    }
    vec![json!({"role": role, "content": text_blocks.join("\n")})]
}

fn tool_call_to_anthropic_block(tool_call: &Value) -> Value {
    let function = tool_call
        .get("function")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let raw_args = function
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!("{}"));
    let parsed_args = if let Some(raw_args) = raw_args.as_str() {
        serde_json::from_str::<Value>(raw_args).unwrap_or_else(|_| json!({"arguments": raw_args}))
    } else {
        raw_args
    };
    let input = if parsed_args.is_object() {
        parsed_args
    } else {
        json!({"value": parsed_args})
    };
    json!({
        "type": "tool_use",
        "id": tool_call
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(generated_tool_use_id),
        "name": function.get("name").and_then(Value::as_str).unwrap_or("tool"),
        "input": input
    })
}

fn openai_finish_reason_to_anthropic(reason: &Value, has_tool_calls: bool) -> &'static str {
    if has_tool_calls {
        return "tool_use";
    }
    match reason.as_str() {
        Some("stop") => "end_turn",
        Some("length") => "max_tokens",
        Some("tool_calls" | "function_call") => "tool_use",
        _ => "end_turn",
    }
}

fn normalize_usage(usage: &Value) -> Value {
    let object = usage.as_object().cloned().unwrap_or_default();
    let input = object
        .get("prompt_tokens")
        .or_else(|| object.get("input_tokens"))
        .and_then(value_to_i64)
        .unwrap_or(0);
    let output = object
        .get("completion_tokens")
        .or_else(|| object.get("output_tokens"))
        .and_then(value_to_i64)
        .unwrap_or(0);
    json!({"input_tokens": input, "output_tokens": output})
}

fn value_to_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
}

fn anthropic_request_options(provider: &Provider) -> Option<Map<String, Value>> {
    let options = provider.request_options.as_object()?;
    let anthropic = options
        .get("anthropic")
        .unwrap_or(&provider.request_options);
    anthropic.as_object().cloned()
}

fn provider_kind(provider: &Provider) -> &'static str {
    let probe = format!("{} {}", provider.name, provider.base_url).to_lowercase();
    if probe.contains("deepseek") {
        "deepseek"
    } else if probe.contains("moonshot") || probe.contains("kimi") {
        "kimi"
    } else if probe.contains("bigmodel") || probe.contains("zhipu") || probe.contains("glm") {
        "zhipu"
    } else if probe.contains("dashscope") || probe.contains("bailian") || probe.contains("aliyun") {
        "bailian"
    } else if probe.contains("siliconflow") {
        "siliconflow"
    } else if probe.contains("qnaigc") || probe.contains("qiniu") {
        "qiniu"
    } else {
        "unknown"
    }
}

fn deep_merge(mut target: Map<String, Value>, source: Map<String, Value>) -> Map<String, Value> {
    for (key, value) in source {
        if value.is_object() && target.get(&key).is_some_and(Value::is_object) {
            let current = target
                .remove(&key)
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default();
            target.insert(
                key,
                Value::Object(deep_merge(
                    current,
                    value.as_object().cloned().unwrap_or_default(),
                )),
            );
        } else {
            target.insert(key, value);
        }
    }
    target
}

fn value_is_empty(value: &Value) -> bool {
    value.is_null()
        || value.as_str().is_some_and(str::is_empty)
        || value.as_array().is_some_and(Vec::is_empty)
        || value.as_object().is_some_and(Map::is_empty)
}

fn generated_message_id() -> String {
    format!("msg_{}", now_millis())
}

fn generated_tool_call_id() -> String {
    format!("call_{}", now_millis())
}

fn generated_tool_use_id() -> String {
    format!("toolu_{}", now_millis())
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
