use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{Value, json};

use crate::models::{AppConfig, Provider, Settings};

use super::forwarding::{forward_non_streaming_request, forward_streaming_request};
use super::listener::{
    HttpRequest, MessageRequestContext, read_http_request, resolve_request_provider_and_model,
    route_http_request, status_reason,
};
use super::runtime::ProxyTelemetry;
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
    forward_streaming_request(&context, &mut server_stream, &telemetry()).expect("stream forward");
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
    let result = forward_non_streaming_request(&body, &provider, &Settings::default(), &telemetry)
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

    let (provider, model) = resolve_request_provider_and_model(&config, "kimi-provider/kimi-k2.6");

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
