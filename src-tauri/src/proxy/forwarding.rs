use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::models::{Provider, Settings};

use super::conversion::{
    anthropic_to_openai_chat_body, apply_anthropic_request_options, build_upstream_url, error_name,
    get_upstream_headers, is_max_unsupported_error, max_unsupported_hint_message,
    normalize_anthropic_response, normalize_api_format, openai_chat_to_anthropic, truncate_chars,
};
use super::listener::MessageRequestContext;
use super::runtime::ProxyTelemetry;
use super::streaming::{
    stream_anthropic_sse_response, stream_openai_sse_response, write_max_unsupported_hint_sse,
    write_sse_error_event, write_sse_headers,
};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct UpstreamHttpResult {
    pub(super) status: u16,
    pub(super) body: Value,
}

pub(super) fn forward_non_streaming_request(
    body: &Value,
    provider: &Provider,
    settings: &Settings,
    telemetry: &Arc<ProxyTelemetry>,
) -> Result<UpstreamHttpResult, String> {
    let (api_format, upstream_url, upstream_body) = upstream_request_parts(body, provider, false);
    let started = Instant::now();
    telemetry.log("INFO", format!("转发请求 → {upstream_url}"));
    telemetry.log(
        "INFO",
        format!(
            "模型: {} → {}",
            body.get("model").and_then(Value::as_str).unwrap_or(""),
            upstream_body
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("")
        ),
    );
    if settings.upstream_proxy_enabled
        && configured_upstream_proxy(&settings.upstream_proxy).is_some()
    {
        telemetry.log("INFO", "使用上游代理");
    }

    let client = match upstream_client(settings, Duration::from_secs(120)) {
        Ok(client) => client,
        Err(error) => {
            telemetry.record(false);
            telemetry.log("ERROR", format!("请求失败: {error}"));
            return Err(error);
        }
    };
    let mut request = client.post(&upstream_url).json(&upstream_body);
    for (name, value) in get_upstream_headers(provider) {
        request = request.header(name.as_str(), value.as_str());
    }

    let response = request.send().map_err(|error| {
        telemetry.record(false);
        if error.is_timeout() {
            telemetry.log("ERROR", "请求超时");
            "上游 API 请求超时。若在中国大陆使用，请检查本地网络是否能稳定访问该 API 地址。"
                .to_string()
        } else {
            let message = format!(
                "连接上游 API 失败: {}: {}。请检查网络连接和 API 地址是否正确。",
                error_name(&error),
                error
            );
            telemetry.log("ERROR", format!("请求失败: {message}"));
            message
        }
    })?;

    let status = response.status().as_u16();
    let success = response.status().is_success();
    let text = response.text().map_err(|error| {
        telemetry.record(false);
        let message = format!("读取上游响应失败: {error}");
        telemetry.log("ERROR", message.clone());
        message
    })?;
    if !success {
        telemetry.record(false);
        telemetry.log(
            "ERROR",
            format!("响应 {status} ({:.2}s)", started.elapsed().as_secs_f64()),
        );
        let error_text = truncate_chars(&text, 500);
        if is_max_unsupported_error(status, &error_text) {
            let model = body.get("model").and_then(Value::as_str).unwrap_or("");
            return Ok(UpstreamHttpResult {
                status: 200,
                body: max_unsupported_hint_message(model),
            });
        }
        return Ok(UpstreamHttpResult {
            status: upstream_error_status(status),
            body: json!({
                "error": {
                    "type": "upstream_error",
                    "status": status,
                    "message": if error_text.is_empty() { "上游 API 返回错误".to_string() } else { error_text }
                }
            }),
        });
    }

    let upstream_data = match serde_json::from_str::<Value>(&text) {
        Ok(value) => value,
        Err(_) => {
            telemetry.record(false);
            telemetry.log("ERROR", "上游 API 返回了非 JSON 响应");
            return Ok(UpstreamHttpResult {
                status: 502,
                body: json!({
                    "error": {
                        "type": "invalid_upstream_response",
                        "message": "上游 API 返回了非 JSON 响应"
                    }
                }),
            });
        }
    };
    let model = body.get("model").and_then(Value::as_str).unwrap_or("");
    let normalized = if api_format == "openai_chat" {
        openai_chat_to_anthropic(&upstream_data, model)
    } else {
        normalize_anthropic_response(&upstream_data, model)
    };
    telemetry.record(true);
    telemetry.log(
        "SUCCESS",
        format!("响应 {status} ({:.2}s)", started.elapsed().as_secs_f64()),
    );
    Ok(UpstreamHttpResult {
        status: 200,
        body: normalized,
    })
}

pub(super) fn forward_streaming_request(
    context: &MessageRequestContext,
    stream: &mut TcpStream,
    telemetry: &Arc<ProxyTelemetry>,
) -> std::io::Result<()> {
    let (api_format, upstream_url, upstream_body) =
        upstream_request_parts(&context.body, &context.provider, true);
    telemetry.log("INFO", format!("流式请求 → {upstream_url}"));
    if context.settings.upstream_proxy_enabled
        && configured_upstream_proxy(&context.settings.upstream_proxy).is_some()
    {
        telemetry.log("INFO", "使用上游代理");
    }
    let client = match upstream_client(&context.settings, Duration::from_secs(300)) {
        Ok(client) => client,
        Err(error) => {
            telemetry.record(false);
            telemetry.log("ERROR", format!("流式请求失败: {error}"));
            write_sse_headers(stream)?;
            return write_sse_error_event(stream, "connection_error", &error, None);
        }
    };
    let mut request = client.post(&upstream_url).json(&upstream_body);
    for (name, value) in get_upstream_headers(&context.provider) {
        request = request.header(name.as_str(), value.as_str());
    }

    let response = match request.send() {
        Ok(response) => response,
        Err(error) => {
            telemetry.record(false);
            write_sse_headers(stream)?;
            let message = if error.is_timeout() {
                telemetry.log("ERROR", "流式请求超时");
                "上游 API 流式请求超时。若在中国大陆使用，请检查本地网络是否能稳定访问该 API 地址。"
                    .to_string()
            } else {
                let message = format!(
                    "连接上游 API 失败: {}: {}。请检查网络连接和 API 地址是否正确。",
                    error_name(&error),
                    error
                );
                telemetry.log("ERROR", format!("流式请求失败: {message}"));
                message
            };
            return write_sse_error_event(stream, "connection_error", &message, None);
        }
    };

    let status = response.status().as_u16();
    if !response.status().is_success() {
        telemetry.record(false);
        telemetry.log("ERROR", format!("流式响应 {status}"));
        let error_text = response
            .text()
            .unwrap_or_else(|_| "上游 API 返回错误".to_string());
        let error_text = truncate_chars(&error_text, 500);
        let model = context
            .body
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("");
        write_sse_headers(stream)?;
        if is_max_unsupported_error(status, &error_text) {
            return write_max_unsupported_hint_sse(stream, model);
        }
        return write_sse_error_event(
            stream,
            "upstream_error",
            if error_text.is_empty() {
                "上游 API 返回错误"
            } else {
                &error_text
            },
            Some(status),
        );
    }

    write_sse_headers(stream)?;
    let model = context
        .body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("");
    let result = if api_format == "openai_chat" {
        stream_openai_sse_response(response, stream, model)
    } else {
        stream_anthropic_sse_response(response, stream, model)
    };
    if result.is_ok() {
        telemetry.record(true);
        telemetry.log("SUCCESS", "流式完成");
    } else {
        telemetry.record(false);
        telemetry.log("ERROR", "流式响应写入失败");
    }
    result
}

fn upstream_request_parts(
    body: &Value,
    provider: &Provider,
    stream: bool,
) -> (String, String, Value) {
    let api_format = normalize_api_format(&provider.api_format);
    let upstream_url = build_upstream_url(&provider.base_url, &api_format);
    let upstream_body = if api_format == "openai_chat" {
        anthropic_to_openai_chat_body(body, stream)
    } else {
        let mut object = body.as_object().cloned().unwrap_or_default();
        if stream {
            object.insert("stream".to_string(), Value::Bool(true));
        } else {
            object.remove("stream");
        }
        apply_anthropic_request_options(&Value::Object(object), provider)
    };
    (api_format, upstream_url, upstream_body)
}

fn upstream_client(
    settings: &Settings,
    timeout: Duration,
) -> Result<reqwest::blocking::Client, String> {
    let mut builder = reqwest::blocking::Client::builder().timeout(timeout);
    if !settings.upstream_proxy_enabled {
        builder = builder.no_proxy();
    } else if let Some(proxy_url) = configured_upstream_proxy(&settings.upstream_proxy) {
        let proxy = reqwest::Proxy::all(&proxy_url)
            .map_err(|error| format!("上游代理地址无效: {error}"))?;
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|error| format!("创建上游 HTTP 客户端失败: {error}"))
}

fn configured_upstream_proxy(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("://") {
        Some(trimmed.to_string())
    } else {
        Some(format!("http://{trimmed}"))
    }
}

fn upstream_error_status(status: u16) -> u16 {
    if (400..=599).contains(&status) {
        status
    } else {
        502
    }
}
