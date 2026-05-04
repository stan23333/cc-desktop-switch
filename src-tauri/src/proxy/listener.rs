use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use crate::config::ConfigStore;
use crate::models::{AppConfig, Provider, Settings};

use super::conversion::{
    active_provider, gateway_auth_failed, gateway_models_response, map_model, provider_model_ids,
    provider_slug,
};
use super::forwarding::{forward_non_streaming_request, forward_streaming_request};
use super::runtime::ProxyTelemetry;

pub(super) fn run_listener(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    telemetry: Arc<ProxyTelemetry>,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let request_telemetry = Arc::clone(&telemetry);
                thread::spawn(move || {
                    let mut stream = stream;
                    let _ = handle_stream(&mut stream, &request_telemetry);
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(30));
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn handle_stream(stream: &mut TcpStream, telemetry: &Arc<ProxyTelemetry>) -> std::io::Result<()> {
    let request = match read_http_request(stream) {
        Ok(request) => request,
        Err(message) => {
            telemetry.record(false);
            telemetry.log("ERROR", format!("请求解析失败: {message}"));
            let response = json_response(
                400,
                json!({"error": {"type": "bad_request", "message": message}}),
            );
            return stream.write_all(&response.to_bytes());
        }
    };
    if is_messages_post(&request) {
        return handle_messages_stream(request, stream, telemetry);
    }
    let response = route_http_request(request, telemetry);
    stream.write_all(&response.to_bytes())
}

fn is_messages_post(request: &HttpRequest) -> bool {
    request.method == "POST"
        && matches!(
            request.path.as_str(),
            "/v1/messages" | "/claude/v1/messages"
        )
}

fn handle_messages_stream(
    request: HttpRequest,
    stream: &mut TcpStream,
    telemetry: &Arc<ProxyTelemetry>,
) -> std::io::Result<()> {
    match prepare_messages_request(&request, telemetry) {
        Err(response) => stream.write_all(&response.to_bytes()),
        Ok(context) if context.stream => forward_streaming_request(&context, stream, telemetry),
        Ok(context) => {
            let response = match forward_non_streaming_request(
                &context.body,
                &context.provider,
                &context.settings,
                telemetry,
            ) {
                Ok(result) => json_response(result.status, result.body),
                Err(error) => json_response(
                    502,
                    json!({"error": {"type": "connection_error", "message": error}}),
                ),
            };
            stream.write_all(&response.to_bytes())
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct MessageRequestContext {
    pub(super) body: Value,
    pub(super) provider: Provider,
    pub(super) settings: Settings,
    pub(super) stream: bool,
}

fn prepare_messages_request(
    request: &HttpRequest,
    telemetry: &Arc<ProxyTelemetry>,
) -> Result<MessageRequestContext, HttpResponse> {
    let config = ConfigStore::default()
        .and_then(|store| store.load_config())
        .map_err(|message| {
            json_response(
                500,
                json!({"error": {"type": "config_error", "message": message}}),
            )
        })?;
    if gateway_auth_failed(
        config.gateway_api_key.as_deref(),
        request.headers.get("authorization").map(String::as_str),
        request.headers.get("x-api-key").map(String::as_str),
    ) {
        telemetry.log("ERROR", "本地 gateway 认证失败");
        return Err(gateway_auth_error());
    }
    let mut body = serde_json::from_str::<Value>(&request.body).map_err(|error| {
        telemetry.record(false);
        telemetry.log("ERROR", format!("请求 JSON 解析失败: {error}"));
        json_response(
            400,
            json!({"error": {"type": "invalid_request", "message": format!("Invalid JSON body: {error}")}}),
        )
    })?;
    let original_model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let (provider, mapped_model) =
        resolve_request_provider_and_model(&config, original_model.as_str());
    let Some(provider) = provider else {
        telemetry.log("ERROR", "没有配置有效的提供商");
        return Err(json_response(
            400,
            json!({"error": {"message": "No active provider configured"}}),
        ));
    };
    if provider.api_key.is_empty() {
        telemetry.log("ERROR", "没有配置有效的提供商");
        return Err(json_response(
            400,
            json!({"error": {"message": "No active provider configured"}}),
        ));
    }
    if let Some(object) = body.as_object_mut() {
        object.insert("model".to_string(), Value::String(mapped_model.clone()));
    }
    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    telemetry.log("INFO", "请求: POST /v1/messages");
    telemetry.log(
        "INFO",
        format!("模型映射: {original_model} → {mapped_model}"),
    );
    Ok(MessageRequestContext {
        body,
        provider: provider.clone(),
        settings: config.settings,
        stream,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HttpRequest {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) headers: BTreeMap<String, String>,
    pub(super) body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HttpResponse {
    pub(super) status: u16,
    pub(super) reason: &'static str,
    pub(super) content_type: &'static str,
    pub(super) body: String,
    pub(super) extra_headers: Vec<(&'static str, &'static str)>,
}

impl HttpResponse {
    fn to_bytes(&self) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n",
            self.status,
            self.reason,
            self.content_type,
            self.body.as_bytes().len()
        );
        for (name, value) in &self.extra_headers {
            response.push_str(name);
            response.push_str(": ");
            response.push_str(value);
            response.push_str("\r\n");
        }
        response.push_str("\r\n");
        response.push_str(&self.body);
        response.into_bytes()
    }
}

pub(super) fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let count = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("connection closed before headers".to_string());
        }
        buffer.extend_from_slice(&chunk[..count]);
        if buffer.len() > 1024 * 1024 {
            return Err("request is too large".to_string());
        }
        if let Some(index) = find_header_end(&buffer) {
            break index;
        }
    };

    let header_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = header_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| "missing method".to_string())?
        .to_string();
    let path = request_parts
        .next()
        .ok_or_else(|| "missing path".to_string())?
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let mut headers = BTreeMap::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_lowercase(), value.trim().to_string());
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    while buffer.len().saturating_sub(body_start) < content_length {
        let count = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..count]);
    }
    let available = buffer.len().saturating_sub(body_start).min(content_length);
    let body = String::from_utf8_lossy(&buffer[body_start..body_start + available]).to_string();

    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

pub(super) fn route_http_request(
    request: HttpRequest,
    telemetry: &Arc<ProxyTelemetry>,
) -> HttpResponse {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") | ("GET", "/status") => json_response(
            200,
            json!({
                "status": "ok",
                "runtime": "tauri-rust",
                "proxy": "listener",
                "forwarding": "available",
                "stats": telemetry.stats()
            }),
        ),
        ("OPTIONS", "/v1/models")
        | ("OPTIONS", "/claude/v1/models")
        | ("OPTIONS", "/v1/messages")
        | ("OPTIONS", "/claude/v1/messages") => options_response(),
        ("GET", "/v1/models") | ("GET", "/claude/v1/models") => {
            handle_models_request(&request, telemetry)
        }
        ("POST", "/v1/messages") | ("POST", "/claude/v1/messages") => {
            handle_messages_request(&request, telemetry)
        }
        _ => json_response(
            404,
            json!({"error": {"type": "not_found", "message": "Not found"}}),
        ),
    }
}

fn handle_models_request(request: &HttpRequest, telemetry: &Arc<ProxyTelemetry>) -> HttpResponse {
    let config = match ConfigStore::default().and_then(|store| store.load_config()) {
        Ok(config) => config,
        Err(message) => {
            return json_response(
                500,
                json!({"error": {"type": "config_error", "message": message}}),
            );
        }
    };
    if gateway_auth_failed(
        config.gateway_api_key.as_deref(),
        request.headers.get("authorization").map(String::as_str),
        request.headers.get("x-api-key").map(String::as_str),
    ) {
        telemetry.log("ERROR", "本地 gateway 认证失败");
        return gateway_auth_error();
    }
    let provider = active_provider(&config);
    let providers = if config.settings.expose_all_provider_models {
        Some(config.providers.as_slice())
    } else {
        None
    };
    json_response(
        200,
        gateway_models_response(
            provider,
            providers,
            config.settings.expose_all_provider_models,
        ),
    )
}

fn handle_messages_request(request: &HttpRequest, telemetry: &Arc<ProxyTelemetry>) -> HttpResponse {
    let context = match prepare_messages_request(request, telemetry) {
        Ok(context) => context,
        Err(response) => return response,
    };
    if context.stream {
        let pending_body = json!({
            "error": {
                "type": "proxy_forwarding_pending",
                "message": "Streaming responses are only available through the live listener path."
            },
            "mappedModel": context.body.get("model").and_then(Value::as_str).unwrap_or("")
        });
        return sse_response(
            200,
            format!(
                "event: error\ndata: {}\n\n",
                serde_json::to_string(&pending_body).unwrap_or_else(|_| "{}".to_string())
            ),
        );
    }
    match forward_non_streaming_request(
        &context.body,
        &context.provider,
        &context.settings,
        telemetry,
    ) {
        Ok(result) => json_response(result.status, result.body),
        Err(error) => json_response(
            502,
            json!({"error": {"type": "connection_error", "message": error}}),
        ),
    }
}

fn gateway_auth_error() -> HttpResponse {
    json_response(
        401,
        json!({"error": {"message": "Invalid gateway API key"}}),
    )
}

fn options_response() -> HttpResponse {
    HttpResponse {
        status: 200,
        reason: "OK",
        content_type: "application/json",
        body: "{}".to_string(),
        extra_headers: vec![
            ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
            ("Access-Control-Allow-Headers", "*"),
        ],
    }
}

fn json_response(status: u16, body: Value) -> HttpResponse {
    HttpResponse {
        status,
        reason: status_reason(status),
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string()),
        extra_headers: Vec::new(),
    }
}

fn sse_response(status: u16, body: String) -> HttpResponse {
    HttpResponse {
        status,
        reason: status_reason(status),
        content_type: "text/event-stream; charset=utf-8",
        body,
        extra_headers: vec![("Cache-Control", "no-cache")],
    }
}

pub(super) fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ if (200..300).contains(&status) => "OK",
        _ if (400..500).contains(&status) => "Bad Request",
        _ if status >= 500 => "Internal Server Error",
        _ => "OK",
    }
}

pub(super) fn resolve_request_provider_and_model<'a>(
    config: &'a AppConfig,
    original_model: &str,
) -> (Option<&'a Provider>, String) {
    if config.settings.expose_all_provider_models {
        if let Some((provider, model_id)) = resolve_model_alias(&config.providers, original_model) {
            return (Some(provider), model_id);
        }
    }
    let provider = active_provider(config);
    let mapped_model = map_model(original_model, provider);
    (provider, mapped_model)
}

fn resolve_model_alias<'a>(
    providers: &'a [Provider],
    requested_model: &str,
) -> Option<(&'a Provider, String)> {
    if requested_model.is_empty() {
        return None;
    }
    for provider in providers {
        let slug = provider_slug(provider);
        for model_id in provider_model_ids(Some(provider)) {
            if format!("{slug}/{model_id}") == requested_model {
                return Some((provider, model_id));
            }
        }
    }
    None
}
