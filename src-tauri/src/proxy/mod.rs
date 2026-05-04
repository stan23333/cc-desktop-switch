#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

use crate::config::ConfigStore;
use crate::model_alias::{
    MODEL_ORDER, model_mappings_with_legacy_aliases, normalize_model_mappings,
    resolve_requested_model_slot,
};
use crate::models::{AppConfig, Provider, ProxyLogEntry, ProxyStats, ProxyStatus, Settings};

const MAX_PROXY_LOGS: usize = 200;

#[derive(Clone, Default)]
pub struct ProxyRuntime {
    server: Arc<Mutex<Option<ProxyServerHandle>>>,
    telemetry: Arc<ProxyTelemetry>,
}

#[derive(Default)]
struct ProxyTelemetry {
    stats: Mutex<ProxyStatsState>,
    logs: Mutex<Vec<ProxyLogEntry>>,
}

#[derive(Debug, Clone)]
struct ProxyStatsState {
    total: u64,
    success: u64,
    failed: u64,
    today: u64,
    day: u64,
}

impl Default for ProxyStatsState {
    fn default() -> Self {
        Self {
            total: 0,
            success: 0,
            failed: 0,
            today: 0,
            day: current_day(),
        }
    }
}

impl ProxyTelemetry {
    fn record(&self, success: bool) {
        let Ok(mut stats) = self.stats.lock() else {
            return;
        };
        let day = current_day();
        if stats.day != day {
            stats.day = day;
            stats.today = 0;
        }
        stats.total += 1;
        stats.today += 1;
        if success {
            stats.success += 1;
        } else {
            stats.failed += 1;
        }
    }

    fn log(&self, level: &str, message: impl Into<String>) {
        let Ok(mut logs) = self.logs.lock() else {
            return;
        };
        logs.push(ProxyLogEntry {
            time: current_time_label(),
            level: level.to_string(),
            message: message.into(),
        });
        if logs.len() > MAX_PROXY_LOGS {
            let excess = logs.len() - MAX_PROXY_LOGS;
            logs.drain(0..excess);
        }
    }

    fn stats(&self) -> ProxyStats {
        let Ok(stats) = self.stats.lock() else {
            return ProxyStats::default();
        };
        ProxyStats {
            total: stats.total,
            success: stats.success,
            failed: stats.failed,
            today: if stats.day == current_day() {
                stats.today
            } else {
                0
            },
        }
    }

    fn logs(&self) -> Vec<ProxyLogEntry> {
        self.logs
            .lock()
            .map(|logs| logs.clone())
            .unwrap_or_default()
    }

    fn clear_logs(&self) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.clear();
        }
    }
}

fn current_day() -> u64 {
    current_unix_secs() / 86_400
}

fn current_time_label() -> String {
    let seconds = current_unix_secs() % 86_400;
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

struct ProxyServerHandle {
    port: u16,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Drop for ProxyServerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl ProxyRuntime {
    pub fn start(&self, requested_port: u16) -> Result<u16, String> {
        let mut guard = self
            .server
            .lock()
            .map_err(|_| "Proxy runtime lock is poisoned".to_string())?;
        if let Some(handle) = guard.as_ref() {
            return Ok(handle.port);
        }

        let listener = TcpListener::bind(("127.0.0.1", requested_port))
            .map_err(|error| format!("Failed to bind Rust proxy listener: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("Failed to configure Rust proxy listener: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("Failed to read Rust proxy port: {error}"))?
            .port();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let telemetry = Arc::clone(&self.telemetry);
        let join = thread::spawn(move || run_listener(listener, thread_stop, telemetry));
        *guard = Some(ProxyServerHandle {
            port,
            stop,
            join: Some(join),
        });
        Ok(port)
    }

    pub fn stop(&self) -> Result<bool, String> {
        let mut guard = self
            .server
            .lock()
            .map_err(|_| "Proxy runtime lock is poisoned".to_string())?;
        Ok(guard.take().is_some())
    }

    pub fn running_port(&self) -> Option<u16> {
        self.server
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|handle| handle.port))
    }

    pub fn stats(&self) -> ProxyStats {
        self.telemetry.stats()
    }

    pub fn logs(&self) -> Vec<ProxyLogEntry> {
        self.telemetry.logs()
    }

    pub fn clear_logs(&self) {
        self.telemetry.clear_logs();
    }
}

pub fn start_proxy_listener(runtime: &ProxyRuntime) -> Result<ProxyStatus, String> {
    let config = ConfigStore::default()?.load_config()?;
    runtime.start(config.settings.proxy_port)?;
    proxy_status(runtime)
}

pub fn stop_proxy_listener(runtime: &ProxyRuntime) -> Result<ProxyStatus, String> {
    runtime.stop()?;
    proxy_status(runtime)
}

pub fn proxy_status(runtime: &ProxyRuntime) -> Result<ProxyStatus, String> {
    let config = ConfigStore::default()?.load_config()?;
    let running_port = runtime.running_port();
    Ok(ProxyStatus {
        running: running_port.is_some(),
        port: running_port.unwrap_or(config.settings.proxy_port),
        active_provider_id: config.active_provider,
        has_gateway_key: config
            .gateway_api_key
            .as_deref()
            .is_some_and(|key| !key.is_empty()),
        implemented: true,
        stats: runtime.stats(),
        message: if running_port.is_some() {
            "Rust proxy listener is running; non-streaming and streaming forwarding are available."
                .to_string()
        } else {
            "Rust proxy forwarding is implemented; HTTP listener is stopped.".to_string()
        },
    })
}

pub fn gateway_models_for_active_provider() -> Result<Value, String> {
    let config = ConfigStore::default()?.load_config()?;
    let provider = active_provider(&config);
    let providers = if config.settings.expose_all_provider_models {
        Some(config.providers.as_slice())
    } else {
        None
    };
    Ok(gateway_models_response(
        provider,
        providers,
        config.settings.expose_all_provider_models,
    ))
}

pub fn proxy_logs(runtime: &ProxyRuntime) -> Vec<ProxyLogEntry> {
    runtime.logs()
}

pub fn clear_proxy_logs(runtime: &ProxyRuntime) -> bool {
    runtime.clear_logs();
    true
}

fn run_listener(listener: TcpListener, stop: Arc<AtomicBool>, telemetry: Arc<ProxyTelemetry>) {
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
struct MessageRequestContext {
    body: Value,
    provider: Provider,
    settings: Settings,
    stream: bool,
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
struct HttpRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpResponse {
    status: u16,
    reason: &'static str,
    content_type: &'static str,
    body: String,
    extra_headers: Vec<(&'static str, &'static str)>,
}

#[derive(Debug, Clone, PartialEq)]
struct UpstreamHttpResult {
    status: u16,
    body: Value,
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

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
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

fn route_http_request(request: HttpRequest, telemetry: &Arc<ProxyTelemetry>) -> HttpResponse {
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

fn status_reason(status: u16) -> &'static str {
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

fn resolve_request_provider_and_model<'a>(
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

fn forward_non_streaming_request(
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

fn forward_streaming_request(
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

fn stream_openai_sse_response(
    response: reqwest::blocking::Response,
    stream: &mut TcpStream,
    model: &str,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(response);
    let mut line = String::new();
    while reader.read_line(&mut line)? > 0 {
        let raw = line.trim_end_matches(&['\r', '\n'][..]).trim();
        if let Some(data) = raw.strip_prefix("data:") {
            let data = data.trim();
            if data == "[DONE]" {
                write_sse_named_event(stream, "done", &json!({}))?;
            } else if let Ok(chunk) = serde_json::from_str::<Value>(data) {
                let event = openai_chat_chunk_to_anthropic(&chunk, model);
                write_sse_data_event(stream, &event)?;
            }
        }
        line.clear();
    }
    Ok(())
}

fn stream_anthropic_sse_response(
    response: reqwest::blocking::Response,
    stream: &mut TcpStream,
    model: &str,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(response);
    let mut line = String::new();
    while reader.read_line(&mut line)? > 0 {
        let raw = line.trim_end_matches(&['\r', '\n'][..]);
        if let Some(data) = raw.strip_prefix("data:") {
            let data = data.trim();
            if !data.is_empty() && data != "[DONE]" {
                if let Ok(event) = serde_json::from_str::<Value>(data) {
                    let normalized = normalize_anthropic_sse_event(&event, model);
                    let serialized =
                        serde_json::to_string(&normalized).unwrap_or_else(|_| "{}".to_string());
                    stream.write_all(format!("data: {serialized}\n").as_bytes())?;
                    stream.flush()?;
                    line.clear();
                    continue;
                }
            }
        }
        stream.write_all(line.as_bytes())?;
        stream.flush()?;
        line.clear();
    }
    Ok(())
}

fn write_sse_headers(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
    )
}

fn write_sse_data_event(stream: &mut TcpStream, data: &Value) -> std::io::Result<()> {
    let serialized = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    stream.write_all(format!("data: {serialized}\n\n").as_bytes())?;
    stream.flush()
}

fn write_sse_named_event(stream: &mut TcpStream, event: &str, data: &Value) -> std::io::Result<()> {
    let serialized = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    stream.write_all(format!("event: {event}\ndata: {serialized}\n\n").as_bytes())?;
    stream.flush()
}

fn write_sse_error_event(
    stream: &mut TcpStream,
    error_type: &str,
    message: &str,
    status: Option<u16>,
) -> std::io::Result<()> {
    let mut error = Map::new();
    error.insert("type".to_string(), Value::String(error_type.to_string()));
    error.insert("message".to_string(), Value::String(message.to_string()));
    if let Some(status) = status {
        error.insert("status".to_string(), Value::Number(status.into()));
    }
    write_sse_named_event(
        stream,
        "error",
        &json!({"type": "error", "error": Value::Object(error)}),
    )
}

fn write_max_unsupported_hint_sse(stream: &mut TcpStream, model: &str) -> std::io::Result<()> {
    write_sse_named_event(
        stream,
        "message_start",
        &json!({
            "type": "message_start",
            "message": {
                "id": "msg_hint",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": model,
                "stop_reason": Value::Null,
                "stop_sequence": Value::Null,
                "usage": {"input_tokens": 0, "output_tokens": 0}
            }
        }),
    )?;
    write_sse_named_event(
        stream,
        "content_block_start",
        &json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
    )?;
    write_sse_named_event(
        stream,
        "content_block_delta",
        &json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "该模型不支持 max，请取消勾选。"}}),
    )?;
    write_sse_named_event(
        stream,
        "content_block_stop",
        &json!({"type": "content_block_stop", "index": 0}),
    )?;
    write_sse_named_event(
        stream,
        "message_delta",
        &json!({"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": Value::Null}, "usage": {"output_tokens": 0}}),
    )?;
    write_sse_named_event(stream, "message_stop", &json!({"type": "message_stop"}))
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

fn normalize_anthropic_response(upstream_data: &Value, model: &str) -> Value {
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

fn normalize_anthropic_sse_event(event: &Value, model: &str) -> Value {
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

fn max_unsupported_hint_message(model: &str) -> Value {
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

fn is_max_unsupported_error(status_code: u16, error_text: &str) -> bool {
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

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn error_name(error: &reqwest::Error) -> &'static str {
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

fn active_provider(config: &AppConfig) -> Option<&Provider> {
    let active_id = config.active_provider.as_deref()?;
    config
        .providers
        .iter()
        .find(|provider| provider.id == active_id)
}

fn provider_model_ids(provider: Option<&Provider>) -> Vec<String> {
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

fn provider_slug(provider: &Provider) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(value: Value) -> Provider {
        serde_json::from_value(value).expect("provider")
    }

    fn deepseek_provider() -> Provider {
        provider(json!({
            "id": "deepseek",
            "name": "DeepSeek",
            "baseUrl": "https://api.deepseek.com/anthropic",
            "authScheme": "bearer",
            "apiFormat": "anthropic",
            "apiKey": "secret-key",
            "models": {
                "sonnet": "deepseek-v4-pro[1m]",
                "haiku": "deepseek-v4-flash",
                "default": "deepseek-v4-pro[1m]"
            },
            "extraHeaders": {},
            "modelCapabilities": {
                "deepseek-v4-pro[1m]": {"supports1m": true}
            },
            "requestOptions": {},
            "isBuiltin": false,
            "sortIndex": 0
        }))
    }

    fn telemetry() -> Arc<ProxyTelemetry> {
        Arc::new(ProxyTelemetry::default())
    }

    fn spawn_upstream_response(status: u16, body: &'static str) -> (u16, JoinHandle<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        let port = listener.local_addr().expect("local addr").port();
        let join = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept upstream");
            let request = read_http_request(&mut stream).expect("read upstream request");
            let reason = status_reason(status);
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.as_bytes().len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write upstream response");
            request.body
        });
        (port, join)
    }

    fn read_response_lossy(stream: &mut TcpStream) -> String {
        let mut buffer = [0_u8; 4096];
        let mut response = Vec::new();
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => response.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => break,
                Err(error) => panic!("read response: {error}"),
            }
        }
        String::from_utf8_lossy(&response).to_string()
    }

    fn capture_streaming_response(context: MessageRequestContext) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind client sink");
        let port = listener.local_addr().expect("sink addr").port();
        let join = thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect sink");
            read_response_lossy(&mut stream)
        });
        let (mut server_stream, _) = listener.accept().expect("accept sink");
        forward_streaming_request(&context, &mut server_stream, &telemetry())
            .expect("stream forward");
        drop(server_stream);
        join.join().expect("join sink")
    }

    #[test]
    fn upstream_url_accepts_base_url_or_full_endpoint() {
        assert_eq!(
            build_upstream_url("https://api.deepseek.com/anthropic", "anthropic"),
            "https://api.deepseek.com/anthropic/v1/messages"
        );
        assert_eq!(
            build_upstream_url(
                "https://api.deepseek.com/anthropic/v1/messages",
                "anthropic"
            ),
            "https://api.deepseek.com/anthropic/v1/messages"
        );
        assert_eq!(
            build_upstream_url("https://api.anthropic-compatible.test/v1", "anthropic"),
            "https://api.anthropic-compatible.test/v1/messages"
        );
        assert_eq!(
            build_upstream_url("https://api.moonshot.ai/v1", "openai"),
            "https://api.moonshot.ai/v1/chat/completions"
        );
    }

    #[test]
    fn gateway_auth_accepts_bearer_or_x_api_key() {
        assert!(gateway_auth_failed(Some("local-key"), None, None));
        assert!(!gateway_auth_failed(
            Some("local-key"),
            Some("Bearer local-key"),
            None
        ));
        assert!(!gateway_auth_failed(
            Some("local-key"),
            None,
            Some("local-key")
        ));
    }

    #[test]
    fn options_route_returns_cors_headers() {
        let response = route_http_request(
            HttpRequest {
                method: "OPTIONS".to_string(),
                path: "/v1/messages".to_string(),
                headers: BTreeMap::new(),
                body: String::new(),
            },
            &telemetry(),
        );

        assert_eq!(response.status, 200);
        assert!(
            response
                .extra_headers
                .iter()
                .any(|(name, _)| { *name == "Access-Control-Allow-Methods" })
        );
    }

    #[test]
    fn proxy_listener_serves_health_endpoint() {
        let mut last_response = String::new();
        for _ in 0..5 {
            let runtime = ProxyRuntime::default();
            let port = runtime.start(0).expect("start listener");
            thread::sleep(Duration::from_millis(20));
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect listener");
            stream
                .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
                .expect("write request");
            let response = read_response_lossy(&mut stream);
            runtime.stop().expect("stop listener");
            if response.starts_with("HTTP/1.1 200 OK") {
                assert!(response.contains(r#""forwarding":"available""#));
                return;
            }
            last_response = response;
        }
        panic!("unexpected listener response: {last_response}");
    }

    #[test]
    fn proxy_runtime_returns_and_clears_log_buffer() {
        let runtime = ProxyRuntime::default();
        runtime.telemetry.log("INFO", "请求: POST /v1/messages");

        assert_eq!(proxy_logs(&runtime).len(), 1);
        assert!(clear_proxy_logs(&runtime));
        assert!(proxy_logs(&runtime).is_empty());
    }

    #[test]
    fn non_streaming_anthropic_forwarding_posts_and_normalizes_response() {
        let (port, join) = spawn_upstream_response(
            200,
            r#"{"type":"message","role":"assistant","content":"ok"}"#,
        );
        let mut provider = deepseek_provider();
        provider.base_url = format!("http://127.0.0.1:{port}");
        let body = json!({
            "model": "deepseek-v4-pro[1m]",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": false,
            "thinking": {"type": "enabled"}
        });

        let telemetry = telemetry();
        let result =
            forward_non_streaming_request(&body, &provider, &Settings::default(), &telemetry)
                .expect("forward");
        let upstream_body = serde_json::from_str::<Value>(&join.join().expect("join upstream"))
            .expect("upstream json body");

        assert_eq!(result.status, 200);
        assert_eq!(upstream_body["model"], "deepseek-v4-pro[1m]");
        assert_eq!(upstream_body["thinking"], json!({"type": "enabled"}));
        assert!(upstream_body.get("stream").is_none());
        assert_eq!(
            result.body["content"],
            json!([{"type": "text", "text": "ok"}])
        );
        assert_eq!(
            result.body["usage"],
            json!({"input_tokens": 0, "output_tokens": 0})
        );
        assert_eq!(telemetry.stats().success, 1);
        assert_eq!(telemetry.stats().failed, 0);
        assert!(
            telemetry
                .logs()
                .iter()
                .any(|entry| { entry.level == "SUCCESS" && entry.message.contains("响应 200") })
        );
    }

    #[test]
    fn max_unsupported_upstream_error_returns_user_hint_message() {
        let (port, join) = spawn_upstream_response(
            400,
            r#"{"error":{"message":"output_config effort max is unsupported"}}"#,
        );
        let mut provider = deepseek_provider();
        provider.base_url = format!("http://127.0.0.1:{port}");
        let body = json!({
            "model": "deepseek-v4-pro[1m]",
            "messages": [{"role": "user", "content": "hello"}]
        });

        let result =
            forward_non_streaming_request(&body, &provider, &Settings::default(), &telemetry())
                .expect("forward");
        let _ = join.join().expect("join upstream");

        assert_eq!(result.status, 200);
        assert_eq!(result.body["id"], "msg_hint");
        assert_eq!(
            result.body["content"][0]["text"],
            "该模型不支持 max，请取消勾选。"
        );
    }

    #[test]
    fn streaming_anthropic_forwarding_normalizes_sse_events() {
        let (port, join) = spawn_upstream_response(
            200,
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[]}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":2}}\n\n",
        );
        let mut provider = deepseek_provider();
        provider.base_url = format!("http://127.0.0.1:{port}");
        let context = MessageRequestContext {
            body: json!({
                "model": "deepseek-v4-pro[1m]",
                "messages": [{"role": "user", "content": "hello"}],
                "stream": true,
                "thinking": {"type": "enabled"}
            }),
            provider,
            settings: Settings::default(),
            stream: true,
        };

        let response = capture_streaming_response(context);
        let upstream_body = serde_json::from_str::<Value>(&join.join().expect("join upstream"))
            .expect("upstream json body");

        assert_eq!(upstream_body["stream"], true);
        assert_eq!(upstream_body["thinking"], json!({"type": "enabled"}));
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Content-Type: text/event-stream"));
        assert!(response.contains(r#""input_tokens":0"#));
        assert!(response.contains(r#""output_tokens":0"#));
    }

    #[test]
    fn streaming_openai_forwarding_converts_chunks_to_anthropic_events() {
        let (port, join) = spawn_upstream_response(
            200,
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n",
        );
        let provider = provider(json!({
            "id": "kimi",
            "name": "Kimi",
            "baseUrl": format!("http://127.0.0.1:{port}/v1"),
            "authScheme": "bearer",
            "apiFormat": "openai_chat",
            "apiKey": "secret-key",
            "models": {"default": "kimi-k2.6"},
            "extraHeaders": {},
            "modelCapabilities": {},
            "requestOptions": {},
            "isBuiltin": false,
            "sortIndex": 0
        }));
        let context = MessageRequestContext {
            body: json!({
                "model": "kimi-k2.6",
                "messages": [{"role": "user", "content": "hello"}],
                "stream": true
            }),
            provider,
            settings: Settings::default(),
            stream: true,
        };

        let response = capture_streaming_response(context);
        let upstream_body = serde_json::from_str::<Value>(&join.join().expect("join upstream"))
            .expect("upstream json body");

        assert_eq!(upstream_body["stream"], true);
        assert!(response.contains(r#""type":"message_start""#));
        assert!(response.contains(r#""type":"content_block_delta""#));
        assert!(response.contains(r#""text":"hi""#));
        assert!(response.contains("event: done"));
    }

    #[test]
    fn all_provider_model_alias_selects_matching_provider() {
        let deepseek = deepseek_provider();
        let kimi = provider(json!({
            "id": "kimi-provider",
            "name": "Kimi Provider",
            "baseUrl": "https://api.moonshot.cn/v1",
            "authScheme": "bearer",
            "apiFormat": "openai_chat",
            "apiKey": "secret-key",
            "models": {"default": "kimi-k2.6"},
            "extraHeaders": {},
            "modelCapabilities": {},
            "requestOptions": {},
            "isBuiltin": false,
            "sortIndex": 1
        }));
        let config = AppConfig {
            version: "1".to_string(),
            active_provider: Some(deepseek.id.clone()),
            gateway_api_key: Some("local-key".to_string()),
            providers: vec![deepseek, kimi],
            settings: Settings {
                expose_all_provider_models: true,
                ..Settings::default()
            },
        };

        let (provider, model) =
            resolve_request_provider_and_model(&config, "kimi-provider/kimi-k2.6");

        assert_eq!(provider.expect("provider").id, "kimi-provider");
        assert_eq!(model, "kimi-k2.6");
    }

    #[test]
    fn model_mapping_preserves_exact_gateway_model_ids() {
        let provider = deepseek_provider();

        assert_eq!(
            map_model("deepseek-v4-pro[1m]", Some(&provider)),
            "deepseek-v4-pro[1m]"
        );
        assert_eq!(
            map_model("claude-sonnet-4-6", Some(&provider)),
            "deepseek-v4-pro[1m]"
        );
    }

    #[test]
    fn gateway_models_response_exposes_exact_ids_and_1m_flags() {
        let provider = deepseek_provider();
        let response = gateway_models_response(Some(&provider), None, false);
        let data = response["data"].as_array().expect("data");

        assert_eq!(data[0]["id"], "deepseek-v4-pro[1m]");
        assert_eq!(data[1]["id"], "deepseek-v4-flash");
    }

    #[test]
    fn anthropic_to_openai_body_flattens_text_without_mutating_source() {
        let body = json!({
            "model": "kimi-k2.6",
            "system": [{"type": "text", "text": "Be brief."}],
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "Hello"},
                    {"type": "text", "text": "World"}
                ]
            }],
            "max_tokens": 32
        });

        let converted = anthropic_to_openai_chat_body(&body, false);

        assert_eq!(
            converted["messages"][0],
            json!({"role": "system", "content": "Be brief."})
        );
        assert_eq!(
            converted["messages"][1],
            json!({"role": "user", "content": "Hello\nWorld"})
        );
        assert!(body["messages"][0]["content"].is_array());
    }

    #[test]
    fn anthropic_to_openai_body_converts_tools_and_tool_results() {
        let body = json!({
            "model": "custom-model",
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": "I will call a tool."},
                        {"type": "tool_use", "id": "toolu_1", "name": "read_file", "input": {"path": "README.md"}}
                    ]
                },
                {
                    "role": "user",
                    "content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "ok"}]
                }
            ],
            "tools": [{"name": "read_file", "description": "Read a file", "input_schema": {"type": "object"}}],
            "tool_choice": {"type": "any"}
        });

        let converted = anthropic_to_openai_chat_body(&body, false);

        assert_eq!(converted["tools"][0]["function"]["name"], "read_file");
        assert_eq!(converted["tool_choice"], "required");
        assert_eq!(converted["messages"][0]["tool_calls"][0]["id"], "toolu_1");
        assert_eq!(
            serde_json::from_str::<Value>(
                converted["messages"][0]["tool_calls"][0]["function"]["arguments"]
                    .as_str()
                    .expect("arguments")
            )
            .expect("json"),
            json!({"path": "README.md"})
        );
        assert_eq!(
            converted["messages"][1],
            json!({"role": "tool", "tool_call_id": "toolu_1", "content": "ok"})
        );
    }

    #[test]
    fn openai_response_converts_tool_calls_and_usage() {
        let response = openai_chat_to_anthropic(
            &json!({
                "id": "chatcmpl_1",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {"name": "search", "arguments": "{\"q\":\"hello\"}"}
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {"prompt_tokens": 11, "completion_tokens": 3}
            }),
            "custom-model",
        );

        assert_eq!(response["stop_reason"], "tool_use");
        assert_eq!(
            response["usage"],
            json!({"input_tokens": 11, "output_tokens": 3})
        );
        assert_eq!(response["content"][0]["type"], "tool_use");
        assert_eq!(response["content"][0]["input"], json!({"q": "hello"}));
    }

    #[test]
    fn openai_streaming_tool_calls_return_clear_error() {
        let event = openai_chat_chunk_to_anthropic(
            &json!({"choices": [{"delta": {"tool_calls": [{"id": "call_1"}]}, "finish_reason": null}]}),
            "custom-model",
        );

        assert_eq!(event["type"], "error");
        assert_eq!(event["error"]["type"], "unsupported_streaming_tool_call");
    }

    #[test]
    fn request_options_keep_deepseek_thinking_and_strip_others() {
        let deepseek = provider(json!({
            "id": "deepseek",
            "name": "DeepSeek",
            "baseUrl": "https://api.deepseek.com/anthropic",
            "authScheme": "bearer",
            "apiFormat": "anthropic",
            "apiKey": "secret-key",
            "models": {"default": "deepseek-v4-pro"},
            "extraHeaders": {},
            "modelCapabilities": {},
            "requestOptions": {"anthropic": {"output_config": {"effort": "max"}}},
            "isBuiltin": false,
            "sortIndex": 0
        }));
        let kimi = provider(json!({
            "id": "kimi",
            "name": "Kimi",
            "baseUrl": "https://api.moonshot.cn/anthropic",
            "authScheme": "bearer",
            "apiFormat": "anthropic",
            "apiKey": "secret-key",
            "models": {"default": "kimi-k2.6"},
            "extraHeaders": {},
            "modelCapabilities": {},
            "requestOptions": {},
            "isBuiltin": false,
            "sortIndex": 0
        }));
        let body = json!({
            "model": "model",
            "thinking": {"type": "enabled"},
            "output_config": {"effort": "low"}
        });

        let deepseek_body = apply_anthropic_request_options(&body, &deepseek);
        let kimi_body = apply_anthropic_request_options(&body, &kimi);

        assert_eq!(deepseek_body["thinking"], json!({"type": "enabled"}));
        assert_eq!(deepseek_body["output_config"]["effort"], "max");
        assert!(kimi_body.get("thinking").is_none());
        assert_eq!(kimi_body["output_config"]["effort"], "low");
    }
}
