use std::collections::BTreeMap;
use std::time::Instant;

use serde_json::{Map, Value, json};

use crate::models::Provider;
use crate::proxy::{build_upstream_url, get_upstream_headers, normalize_api_format};

const MODEL_EXCLUDE_KEYWORDS: [&str; 8] = [
    "embedding",
    "rerank",
    "moderation",
    "whisper",
    "tts",
    "image",
    "vision",
    "audio",
];

const AUTH_ERROR_KEYWORDS: [&str; 9] = [
    "invalid api key",
    "invalid_api_key",
    "api key",
    "apikey",
    "unauthorized",
    "authentication",
    "authorization",
    "invalid token",
    "access token",
];

const STANDARD_ENDPOINTS: [(&str, &str); 6] = [
    ("/v1/messages", "anthropic"),
    ("/messages", "anthropic"),
    ("/v1/chat/completions", "openai_chat"),
    ("/chat/completions", "openai_chat"),
    ("/v1/responses", "openai_responses"),
    ("/responses", "openai_responses"),
];

pub fn provider_compatibility(provider: &Provider) -> Value {
    let api_format = normalize_api_format(&provider.api_format);
    if api_format == "anthropic" {
        return json!({
            "id": provider.id,
            "name": provider.name,
            "apiFormat": api_format,
            "level": "stable",
            "message": "Anthropic 兼容接口，适合 Claude 桌面版主流程。",
            "checks": {
                "models": true,
                "text": true,
                "stream": true,
                "tools": true,
                "streamingTools": true,
            },
        });
    }
    if api_format == "openai_chat" {
        return json!({
            "id": provider.id,
            "name": provider.name,
            "apiFormat": api_format,
            "level": "experimental",
            "message": "OpenAI Chat 实验适配：文本和非流式工具调用可测试，流式工具调用暂不作为稳定能力。",
            "checks": {
                "models": true,
                "text": true,
                "stream": true,
                "tools": true,
                "streamingTools": false,
            },
        });
    }
    json!({
        "id": provider.id,
        "name": provider.name,
        "apiFormat": api_format,
        "level": "unsupported",
        "message": format!("{api_format} 暂未适配。"),
        "checks": {
            "models": false,
            "text": false,
            "stream": false,
            "tools": false,
            "streamingTools": false,
        },
    })
}

pub fn compatibility_report(providers: &[Provider]) -> Value {
    let providers = providers
        .iter()
        .map(provider_compatibility)
        .collect::<Vec<_>>();
    let experimental_count = providers
        .iter()
        .filter(|item| item.get("level").and_then(Value::as_str) == Some("experimental"))
        .count();
    json!({
        "success": true,
        "providers": providers,
        "experimentalCount": experimental_count,
    })
}

pub fn test_provider_connection(provider: &Provider) -> Value {
    let base_url = clean_base_url(&provider.base_url);
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return json!({"success": false, "message": "API 地址无效"});
    }

    let started = Instant::now();
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            return json!({
                "success": true,
                "ok": false,
                "latencyMs": elapsed_ms(started),
                "message": format!("服务器连接失败，延迟 {} ms", elapsed_ms(started)),
            });
        }
    };

    let result = client
        .head(&base_url)
        .headers(url_ping_headers())
        .send()
        .or_else(|_| client.get(&base_url).headers(url_ping_headers()).send());
    let response = match result {
        Ok(response) => response,
        Err(_) => {
            return json!({
                "success": true,
                "ok": false,
                "latencyMs": elapsed_ms(started),
                "message": format!("服务器连接失败，延迟 {} ms", elapsed_ms(started)),
            });
        }
    };

    let latency_ms = elapsed_ms(started);
    let status_code = response.status().as_u16();
    let (reachable, message) = provider_ping_message(status_code, latency_ms);

    json!({
        "success": true,
        "ok": reachable,
        "latencyMs": latency_ms,
        "statusCode": status_code,
        "message": message,
    })
}

fn provider_ping_message(status_code: u16, latency_ms: u128) -> (bool, String) {
    if status_code < 500 {
        (true, format!("服务器连接成功，延迟 {latency_ms} ms"))
    } else {
        (false, format!("服务器连接异常，延迟 {latency_ms} ms"))
    }
}

pub fn fetch_provider_models(provider: &Provider) -> Value {
    let endpoints = model_endpoint_candidates(provider);
    if endpoints.is_empty() {
        return json!({"success": false, "message": "API 地址无效", "models": [], "suggested": {}});
    }
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return json!({"success": false, "message": format!("无法自动获取模型列表: {error}"), "models": [], "suggested": {}});
        }
    };
    let mut errors = Vec::new();
    let mut auth_failure: Option<(u16, String)> = None;
    for endpoint in endpoints {
        let response = client
            .get(&endpoint)
            .headers(headers_for_reqwest(provider, false))
            .send();
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                errors.push(format!("{endpoint}: {}", error_class(&error)));
                continue;
            }
        };
        if !response.status().is_success() {
            let status_code = response.status().as_u16();
            let text = response.text().unwrap_or_default();
            let detail = response_error_message(status_code, &text);
            if let Some(message) = provider_auth_error_message(status_code, &detail) {
                auth_failure = Some((status_code, message.clone()));
                errors.push(format!("{endpoint}: {message}"));
            } else if detail.is_empty() {
                errors.push(format!("{endpoint}: HTTP {status_code}"));
            } else {
                errors.push(format!("{endpoint}: HTTP {status_code}：{detail}"));
            }
            continue;
        }
        let payload = match response.json::<Value>() {
            Ok(payload) => payload,
            Err(_) => {
                errors.push(format!("{endpoint}: 非 JSON 响应"));
                continue;
            }
        };
        let model_ids = extract_model_ids(&payload);
        if !model_ids.is_empty() {
            return json!({
                "success": true,
                "endpoint": endpoint,
                "models": model_ids,
                "suggested": suggest_model_mappings(&model_ids),
            });
        }
        errors.push(format!("{endpoint}: 未发现模型列表"));
    }
    if let Some((status_code, message)) = auth_failure {
        let tail = errors
            .into_iter()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        return json!({
            "success": false,
            "code": "api_key_invalid",
            "statusCode": status_code,
            "message": message,
            "models": [],
            "suggested": {},
            "errors": tail,
        });
    }
    let tail = errors
        .into_iter()
        .rev()
        .take(5)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    json!({
        "success": false,
        "message": "无法自动获取模型列表",
        "models": [],
        "suggested": {},
        "errors": tail,
    })
}

pub fn query_provider_usage(provider: &Provider) -> Value {
    if provider.api_key.is_empty() {
        return json!({"success": false, "message": "请先保存 API Key"});
    }
    let Some((kind, endpoint)) = balance_endpoint(provider) else {
        return json!({
            "success": true,
            "supported": false,
            "items": [],
            "message": "这个提供商暂未适配余额/用量接口",
        });
    };
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return json!({
                "success": true,
                "supported": true,
                "ok": false,
                "message": format!("查询失败：{}", error_class(&error)),
                "items": [],
            });
        }
    };
    let response = match client
        .get(&endpoint)
        .headers(headers_for_reqwest(provider, false))
        .send()
    {
        Ok(response) => response,
        Err(error) => {
            return json!({
                "success": true,
                "supported": true,
                "ok": false,
                "message": format!("查询失败：{}", error_class(&error)),
                "items": [],
            });
        }
    };
    if !response.status().is_success() {
        return json!({
            "success": true,
            "supported": true,
            "ok": false,
            "statusCode": response.status().as_u16(),
            "message": format!("余额接口返回 HTTP {}", response.status().as_u16()),
            "items": [],
        });
    }
    let payload = match response.json::<Value>() {
        Ok(payload) => payload,
        Err(_) => {
            return json!({
                "success": true,
                "supported": true,
                "ok": false,
                "message": "余额接口返回了非 JSON 响应",
                "items": [],
            });
        }
    };
    let items = normalize_balance_payload(&kind, &payload);
    json!({
        "success": true,
        "supported": true,
        "ok": !items.is_empty(),
        "endpoint": endpoint,
        "items": items,
        "message": if items.is_empty() { "余额接口响应中未识别到余额字段" } else { "查询完成" },
    })
}

pub fn check_model_available(provider: &Provider, model: &str) -> Value {
    let api_format = normalize_api_format(&provider.api_format);
    let url = build_upstream_url(&provider.base_url, &api_format);
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1,
        "stream": false,
    });
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return json!({
                "success": true,
                "available": false,
                "message": format!("{}: {}", error_class(&error), error.to_string().chars().take(200).collect::<String>()),
            });
        }
    };
    let response = match client
        .post(url)
        .headers(headers_for_reqwest(provider, true))
        .json(&body)
        .send()
    {
        Ok(response) => response,
        Err(error) => {
            return json!({
                "success": true,
                "available": false,
                "message": provider_request_error_message(&error),
            });
        }
    };
    let status = response.status();
    if status.is_success() {
        return json!({
            "success": true,
            "available": true,
            "message": "模型响应正常",
        });
    }
    let status_code = status.as_u16();
    let text = response.text().unwrap_or_default();
    let detail = response_error_message(status_code, &text);
    let auth_message = provider_auth_error_message(status_code, &detail);
    let message = auth_message.clone().unwrap_or(detail);
    json!({
        "success": true,
        "available": false,
        "code": if auth_message.is_some() { "api_key_invalid" } else { "model_unavailable" },
        "statusCode": status_code,
        "message": message,
    })
}

pub fn detect_api_format(base_url: &str, api_key: &str) -> Value {
    let clean = clean_base_url(base_url);
    if clean.is_empty() {
        return json!({"success": false, "message": "请填写 Base URL"});
    }

    for (path, api_format) in STANDARD_ENDPOINTS {
        let url = format!("{clean}{path}");
        let result = probe_single_endpoint(&url, api_format, api_key);
        if result.get("detected").and_then(Value::as_bool) == Some(true) {
            return json!({
                "success": true,
                "apiFormat": api_format,
                "endpoint": url,
                "confidence": result.get("confidence").and_then(Value::as_str).unwrap_or("medium"),
            });
        }
    }

    json!({"success": false, "message": "未能识别协议类型，请手动选择"})
}

fn probe_single_endpoint(url: &str, expected_format: &str, api_key: &str) -> Value {
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(_) => return json!({"detected": false, "exists": false}),
    };
    let response = match client
        .post(url)
        .headers(probe_headers(expected_format, api_key))
        .json(&probe_body(expected_format))
        .send()
    {
        Ok(response) => response,
        Err(_) => return json!({"detected": false, "exists": false}),
    };
    let status = response.status().as_u16();
    let text = response.text().unwrap_or_default();
    let data = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({}));
    let (detected, confidence) = detect_format_from_response(&data, status, expected_format);
    if detected {
        return json!({"detected": true, "confidence": confidence});
    }
    if matches!(status, 401 | 403) {
        return json!({"detected": false, "exists": true});
    }
    json!({"detected": false, "exists": false})
}

fn probe_headers(expected_format: &str, api_key: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    if expected_format == "anthropic" {
        headers.insert(
            "anthropic-version",
            reqwest::header::HeaderValue::from_static("2023-06-01"),
        );
    }
    if !api_key.is_empty() {
        if let Ok(value) = reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}")) {
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }
    }
    headers
}

fn probe_body(expected_format: &str) -> Value {
    if expected_format == "openai_responses" {
        return json!({
            "model": "___probe_test___",
            "input": "test",
        });
    }
    json!({
        "model": "___probe_test___",
        "messages": [{"role": "user", "content": "test"}],
        "max_tokens": 1,
    })
}

fn detect_format_from_response(
    data: &Value,
    status_code: u16,
    expected_format: &str,
) -> (bool, &'static str) {
    if !data.is_object() {
        return (false, "");
    }
    if status_code == 200 {
        if expected_format == "anthropic"
            && data.get("type").and_then(Value::as_str) == Some("message")
            && data.get("content").and_then(Value::as_array).is_some()
        {
            return (true, "high");
        }
        if expected_format == "openai_chat"
            && data.get("choices").and_then(Value::as_array).is_some()
        {
            return (true, "high");
        }
        if expected_format == "openai_responses"
            && data.get("output").and_then(Value::as_array).is_some()
        {
            return (true, "high");
        }
    }
    if matches!(status_code, 400 | 422) {
        if expected_format == "anthropic"
            && data.get("type").and_then(Value::as_str) == Some("error")
            && data.get("error").and_then(Value::as_object).is_some()
        {
            return (true, "high");
        }
        if matches!(expected_format, "openai_chat" | "openai_responses")
            && data
                .get("error")
                .and_then(Value::as_object)
                .and_then(|error| error.get("message"))
                .is_some()
        {
            return (true, "high");
        }
    }
    (false, "")
}

fn provider_error_message(text: &str) -> Option<String> {
    let data = serde_json::from_str::<Value>(text).ok()?;
    data.get("error")
        .and_then(Value::as_object)
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| data.get("message").and_then(Value::as_str))
        .map(|message| message.chars().take(200).collect())
}

fn response_error_message(status_code: u16, text: &str) -> String {
    provider_error_message(text).unwrap_or_else(|| {
        if text.is_empty() {
            format!("HTTP {status_code}")
        } else {
            text.chars().take(200).collect()
        }
    })
}

fn provider_auth_error_message(status_code: u16, detail: &str) -> Option<String> {
    let normalized = detail.trim();
    let lower = normalized.to_lowercase();
    let is_auth_status = matches!(status_code, 401 | 403);
    let is_auth_text = AUTH_ERROR_KEYWORDS
        .iter()
        .any(|keyword| lower.contains(keyword));
    if !(is_auth_status || is_auth_text) {
        return None;
    }
    let prefix = if normalized.is_empty() {
        format!("API Key 不可用：上游返回 HTTP {status_code}")
    } else {
        format!("API Key 不可用：{normalized}")
    };
    Some(format!(
        "{prefix}。请检查 API Key 是否正确、是否属于当前 Base URL/地区，以及认证方式是否匹配。"
    ))
}

fn provider_request_error_message(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "请求超时".to_string()
    } else if error.is_connect() {
        "连接失败，请检查网络".to_string()
    } else {
        format!(
            "{}: {}",
            error_class(error),
            error.to_string().chars().take(200).collect::<String>()
        )
    }
}

pub fn model_endpoint_candidates(provider: &Provider) -> Vec<String> {
    let base_url = clean_base_url(&provider.base_url);
    if base_url.is_empty() {
        return Vec::new();
    }
    let api_format = normalize_api_format(&provider.api_format);
    let upstream = build_upstream_url(&base_url, &api_format);
    let mut candidates = Vec::new();
    if api_format == "openai_chat" {
        candidates.push(replace_path_suffix(
            &upstream,
            &["/chat/completions", "/completions"],
            "/models",
        ));
        candidates.push(format!("{base_url}/models"));
    } else {
        candidates.push(replace_path_suffix(
            &upstream,
            &["/v1/messages", "/messages"],
            "/v1/models",
        ));
        if base_url.to_lowercase().ends_with("/v1") {
            candidates.push(format!("{base_url}/models"));
        }
        candidates.push(format!("{base_url}/models"));
        if let Some(root) = base_url.strip_suffix("/anthropic") {
            let root = root.trim_end_matches('/');
            candidates.push(format!("{root}/models"));
            candidates.push(format!("{root}/v1/models"));
        }
    }
    let mut result = Vec::new();
    for candidate in candidates {
        if !candidate.is_empty() && !result.contains(&candidate) {
            result.push(candidate);
        }
    }
    result
}

fn clean_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn replace_path_suffix(url: &str, suffixes: &[&str], replacement: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let mut path = format!("/{}", path.trim_end_matches('/'));
    let lower = path.to_lowercase();
    for suffix in suffixes {
        if lower.ends_with(suffix) {
            path.truncate(path.len() - suffix.len());
            break;
        }
    }
    format!(
        "{scheme}://{host}{}",
        join_paths(path.trim_end_matches('/'), replacement)
    )
}

fn join_paths(left: &str, right: &str) -> String {
    format!(
        "{}/{}",
        left.trim_end_matches('/'),
        right.trim_start_matches('/')
    )
}

pub fn extract_model_ids(payload: &Value) -> Vec<String> {
    let candidates = if let Some(items) = payload.as_array() {
        items.clone()
    } else if let Some(object) = payload.as_object() {
        model_items_from_object(object)
    } else {
        Vec::new()
    };
    let mut result = Vec::new();
    for item in candidates {
        let Some(model_id) = model_id_from_item(&item) else {
            continue;
        };
        if !result.contains(&model_id) {
            result.push(model_id);
        }
    }
    result
}

fn model_items_from_object(object: &Map<String, Value>) -> Vec<Value> {
    for key in ["data", "models", "items", "result"] {
        if let Some(items) = object.get(key).and_then(Value::as_array) {
            return items.clone();
        }
    }
    if let Some(data) = object.get("data").and_then(Value::as_object) {
        for key in ["models", "items"] {
            if let Some(items) = data.get(key).and_then(Value::as_array) {
                return items.clone();
            }
        }
    }
    Vec::new()
}

fn model_id_from_item(item: &Value) -> Option<String> {
    if let Some(value) = item
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(value.to_string());
    }
    let object = item.as_object()?;
    for key in ["id", "name", "model", "model_id"] {
        if let Some(value) = object
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(value.to_string());
        }
    }
    None
}

pub fn suggest_model_mappings(model_ids: &[String]) -> BTreeMap<String, String> {
    let usable = usable_model_ids(model_ids);
    let default = pick_model(
        &usable,
        &[
            "sonnet",
            "claude",
            "k2",
            "glm-5.1",
            "qwen3-max",
            "max",
            "pro",
            "chat",
        ],
        0,
    );
    let mut models = BTreeMap::new();
    models.insert("default".to_string(), default);
    models
}

fn usable_model_ids(model_ids: &[String]) -> Vec<String> {
    let usable = model_ids
        .iter()
        .filter(|model_id| {
            let lower = model_id.to_lowercase();
            !MODEL_EXCLUDE_KEYWORDS
                .iter()
                .any(|keyword| lower.contains(keyword))
        })
        .cloned()
        .collect::<Vec<_>>();
    if usable.is_empty() {
        model_ids.to_vec()
    } else {
        usable
    }
}

fn pick_model(model_ids: &[String], keywords: &[&str], fallback_index: usize) -> String {
    for keyword in keywords {
        for model_id in model_ids {
            if model_id.to_lowercase().contains(keyword) {
                return model_id.clone();
            }
        }
    }
    model_ids
        .get(fallback_index.min(model_ids.len().saturating_sub(1)))
        .cloned()
        .unwrap_or_default()
}

fn balance_endpoint(provider: &Provider) -> Option<(String, String)> {
    let kind = provider_kind(provider);
    let base = clean_base_url(&provider.base_url).to_lowercase();
    if kind == "deepseek" {
        return Some((kind, "https://api.deepseek.com/user/balance".to_string()));
    }
    if kind == "siliconflow" {
        let host = if base.contains(".com") {
            "https://api.siliconflow.com"
        } else {
            "https://api.siliconflow.cn"
        };
        return Some((kind, format!("{host}/v1/user/info")));
    }
    if kind == "openrouter" {
        return Some((kind, "https://openrouter.ai/api/v1/credits".to_string()));
    }
    if kind == "novita" {
        return Some((kind, "https://api.novita.ai/v3/user/balance".to_string()));
    }
    if kind == "stepfun" {
        return Some((kind, "https://api.stepfun.com/v1/accounts".to_string()));
    }
    None
}

fn provider_kind(provider: &Provider) -> String {
    let probe = format!("{} {}", provider.name, provider.base_url).to_lowercase();
    if probe.contains("deepseek") {
        "deepseek".to_string()
    } else if probe.contains("siliconflow") {
        "siliconflow".to_string()
    } else if probe.contains("openrouter") {
        "openrouter".to_string()
    } else if probe.contains("novita") {
        "novita".to_string()
    } else if probe.contains("stepfun") || probe.contains("step") {
        "stepfun".to_string()
    } else {
        "unknown".to_string()
    }
}

fn normalize_balance_payload(kind: &str, payload: &Value) -> Vec<Value> {
    if kind == "deepseek" {
        return payload
            .get("balance_infos")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .map(|item| {
                let currency = item
                    .get("currency")
                    .and_then(Value::as_str)
                    .unwrap_or("CNY");
                money_item(
                    currency,
                    item.get("total_balance"),
                    item.get("granted_balance"),
                    item.get("topped_up_balance"),
                    currency,
                )
            })
            .collect();
    }
    if kind == "openrouter" {
        let data = payload.get("data").unwrap_or(payload);
        let total = float_or_none(data.get("total_credits"));
        let used = float_or_none(data.get("total_usage"));
        let remaining = total.zip(used).map(|(total, used)| total - used);
        return vec![money_item_from_numbers(
            "credits", remaining, total, used, "USD",
        )];
    }
    let data = payload.get("data").unwrap_or(payload);
    if let Some(object) = data.as_object() {
        for key in [
            "balance",
            "remaining",
            "available_balance",
            "availableBalance",
            "credit",
        ] {
            if object.contains_key(key) {
                return vec![money_item(
                    "balance",
                    object.get(key),
                    object
                        .get("total")
                        .or_else(|| object.get("totalBalance"))
                        .or_else(|| object.get("total_credits")),
                    object
                        .get("used")
                        .or_else(|| object.get("usage"))
                        .or_else(|| object.get("usedBalance")),
                    object
                        .get("currency")
                        .or_else(|| object.get("unit"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )];
            }
        }
    }
    Vec::new()
}

fn money_item(
    label: &str,
    remaining: Option<&Value>,
    total: Option<&Value>,
    used: Option<&Value>,
    unit: &str,
) -> Value {
    money_item_from_numbers(
        label,
        remaining.and_then(|value| float_or_none(Some(value))),
        total.and_then(|value| float_or_none(Some(value))),
        used.and_then(|value| float_or_none(Some(value))),
        unit,
    )
}

fn money_item_from_numbers(
    label: &str,
    remaining: Option<f64>,
    total: Option<f64>,
    used: Option<f64>,
    unit: &str,
) -> Value {
    json!({
        "label": label,
        "remaining": remaining,
        "total": total,
        "used": used,
        "unit": unit,
    })
}

fn float_or_none(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    }
}

fn url_ping_headers() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    headers
}

fn headers_for_reqwest(
    provider: &Provider,
    include_content_type: bool,
) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in get_upstream_headers(provider) {
        if !include_content_type && name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        let Ok(name) = reqwest::header::HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Ok(value) = reqwest::header::HeaderValue::from_str(&value) else {
            continue;
        };
        headers.insert(name, value);
    }
    headers
}

fn error_class(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "TimeoutException"
    } else if error.is_connect() {
        "ConnectError"
    } else {
        "RequestError"
    }
}

fn elapsed_ms(started: Instant) -> u128 {
    started.elapsed().as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider(value: Value) -> Provider {
        serde_json::from_value(value).expect("provider")
    }

    #[test]
    fn model_endpoint_candidates_handle_common_url_shapes() {
        let openai = provider(json!({
            "id": "openai",
            "name": "OpenAI",
            "baseUrl": "https://api.example.com/v1",
            "apiFormat": "openai_chat",
            "authScheme": "bearer",
            "apiKey": "",
            "models": {},
            "extraHeaders": {},
            "modelCapabilities": {},
            "requestOptions": {},
            "isBuiltin": false,
            "sortIndex": 0
        }));
        let anthropic = provider(json!({
            "id": "anthropic",
            "name": "Anthropic",
            "baseUrl": "https://api.example.com/anthropic",
            "apiFormat": "anthropic",
            "authScheme": "bearer",
            "apiKey": "",
            "models": {},
            "extraHeaders": {},
            "modelCapabilities": {},
            "requestOptions": {},
            "isBuiltin": false,
            "sortIndex": 0
        }));

        assert!(
            model_endpoint_candidates(&openai)
                .contains(&"https://api.example.com/v1/models".to_string())
        );
        assert!(
            model_endpoint_candidates(&anthropic)
                .contains(&"https://api.example.com/v1/models".to_string())
        );
    }

    #[test]
    fn extract_model_ids_and_suggest_mappings() {
        let models = extract_model_ids(&json!({
            "data": [
                {"id": "qwen3-max"},
                {"name": "qwen3-flash"},
                {"model": "embedding-model"}
            ]
        }));
        let suggested = suggest_model_mappings(&models);

        assert_eq!(models, vec!["qwen3-max", "qwen3-flash", "embedding-model"]);
        assert_eq!(suggested["default"], "qwen3-max");
        assert_eq!(suggested.len(), 1);
        assert!(!suggested.contains_key("haiku"));
        assert!(!suggested.contains_key("sonnet"));
        assert!(!suggested.contains_key("opus"));
    }

    #[test]
    fn provider_auth_error_message_is_actionable() {
        let message = provider_auth_error_message(401, "Invalid API Key").expect("auth message");

        assert!(message.contains("API Key 不可用"));
        assert!(message.contains("Invalid API Key"));
        assert!(message.contains("Base URL/地区"));
    }

    #[test]
    fn provider_ping_message_hides_http_status_from_user_copy() {
        let (ok, message) = provider_ping_message(404, 1291);

        assert!(ok);
        assert_eq!(message, "服务器连接成功，延迟 1291 ms");
        assert!(!message.contains("HTTP"));
        assert!(!message.contains("获取模型"));
    }

    #[test]
    fn normalize_openrouter_usage() {
        let items = normalize_balance_payload(
            "openrouter",
            &json!({"data": {"total_credits": 12.5, "total_usage": 2.0}}),
        );

        assert_eq!(items[0]["remaining"], 10.5);
        assert_eq!(items[0]["unit"], "USD");
    }

    #[test]
    fn detect_format_from_success_and_error_shapes() {
        assert_eq!(
            detect_format_from_response(
                &json!({"type": "message", "content": []}),
                200,
                "anthropic"
            ),
            (true, "high")
        );
        assert_eq!(
            detect_format_from_response(&json!({"choices": []}), 200, "openai_chat"),
            (true, "high")
        );
        assert_eq!(
            detect_format_from_response(
                &json!({"error": {"message": "missing model"}}),
                400,
                "openai_chat"
            ),
            (true, "high")
        );
        assert_eq!(
            detect_format_from_response(&json!({"unexpected": true}), 200, "anthropic"),
            (false, "")
        );
    }
}
