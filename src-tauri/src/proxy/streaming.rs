use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use serde_json::{Map, Value, json};

use super::conversion::{normalize_anthropic_sse_event, openai_chat_chunk_to_anthropic};

pub(super) fn stream_openai_sse_response(
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

pub(super) fn stream_anthropic_sse_response(
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

pub(super) fn write_sse_headers(stream: &mut TcpStream) -> std::io::Result<()> {
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

pub(super) fn write_sse_error_event(
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

pub(super) fn write_max_unsupported_hint_sse(
    stream: &mut TcpStream,
    model: &str,
) -> std::io::Result<()> {
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
