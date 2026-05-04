use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::blocking::multipart::{Form, Part};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::ConfigStore;
use crate::proxy::ProxyRuntime;

const FEEDBACK_WORKER_URL: &str = "https://codex-app-transfer-feedback.alysechencn.workers.dev";
const SUCCESS_COOLDOWN_S: u64 = 60;
const FAILURE_WINDOW_S: u64 = 300;
const FAILURE_LIMIT: usize = 5;
const FAILURE_COOLDOWN_S: u64 = 60;
const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;

#[derive(Default)]
struct FeedbackThrottle {
    last_success_ts: u64,
    failure_ts: Vec<u64>,
    failure_cooldown_until: u64,
}

#[derive(Debug, Deserialize)]
struct FeedbackPayload {
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default = "default_include_diagnostics")]
    include_diagnostics: bool,
    #[serde(default)]
    attachments: Vec<FeedbackAttachment>,
}

#[derive(Debug, Deserialize)]
struct FeedbackAttachment {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    content_b64: String,
}

fn default_include_diagnostics() -> bool {
    true
}

fn throttle() -> &'static Mutex<FeedbackThrottle> {
    static THROTTLE: OnceLock<Mutex<FeedbackThrottle>> = OnceLock::new();
    THROTTLE.get_or_init(|| Mutex::new(FeedbackThrottle::default()))
}

impl FeedbackThrottle {
    fn acquire(&mut self) -> Result<(), String> {
        let now = current_unix_secs();
        if self.last_success_ts > 0 {
            let since_success = now.saturating_sub(self.last_success_ts);
            if since_success < SUCCESS_COOLDOWN_S {
                return Err(format!(
                    "刚提交成功,请等 {} 秒后再发新反馈",
                    SUCCESS_COOLDOWN_S - since_success
                ));
            }
        }

        if now < self.failure_cooldown_until {
            return Err(format!(
                "连续提交失败次数过多,请等 {} 秒后再试",
                self.failure_cooldown_until - now
            ));
        }

        self.failure_ts
            .retain(|ts| now.saturating_sub(*ts) < FAILURE_WINDOW_S);
        Ok(())
    }

    fn record_success(&mut self) {
        self.last_success_ts = current_unix_secs();
        self.failure_ts.clear();
        self.failure_cooldown_until = 0;
    }

    fn record_failure(&mut self) {
        let now = current_unix_secs();
        self.failure_ts
            .retain(|ts| now.saturating_sub(*ts) < FAILURE_WINDOW_S);
        self.failure_ts.push(now);
        if self.failure_ts.len() >= FAILURE_LIMIT {
            self.failure_cooldown_until = now + FAILURE_COOLDOWN_S;
        }
    }
}

pub fn submit_feedback(payload: Value, proxy_runtime: &ProxyRuntime) -> Result<Value, String> {
    {
        let mut guard = throttle()
            .lock()
            .map_err(|_| "反馈节流状态不可用".to_string())?;
        guard.acquire()?;
    }

    let input: FeedbackPayload =
        serde_json::from_value(payload).map_err(|_| "请求体非 JSON".to_string())?;
    let title = input.title.trim().to_string();
    let body = input.body.trim().to_string();
    if body.is_empty() {
        return Err("请填写描述".to_string());
    }

    let mut meta = json!({
        "app": "cc-desktop-switch",
        "app_version": env!("CARGO_PKG_VERSION"),
    });
    if input.include_diagnostics {
        let active_provider_name = active_provider_name().unwrap_or_default();
        meta["os"] = json!(std::env::consts::OS);
        meta["arch"] = json!(std::env::consts::ARCH);
        meta["active_provider_name"] = json!(active_provider_name);
        meta["include_diagnostics"] = json!(true);
    }

    let mut form = Form::new()
        .part("meta", text_part(meta.to_string(), "application/json")?)
        .part("title", text_part(title, "text/plain")?)
        .part("body", text_part(body, "text/plain")?);

    let mut screenshot_index = 0usize;
    let mut log_index = 0usize;
    for attachment in input.attachments {
        let Ok(bytes) = STANDARD.decode(attachment.content_b64.as_bytes()) else {
            continue;
        };
        if bytes.is_empty() || bytes.len() > MAX_FILE_BYTES {
            continue;
        }
        let is_screenshot = attachment.kind == "screenshot";
        let field_name = if is_screenshot {
            let field = format!("screenshot{screenshot_index}");
            screenshot_index += 1;
            field
        } else {
            let field = format!("log{log_index}");
            log_index += 1;
            field
        };
        let file_name = safe_feedback_file_name(&attachment.name, &attachment.kind);
        let content_type = if attachment.content_type.trim().is_empty() {
            "application/octet-stream"
        } else {
            attachment.content_type.trim()
        };
        let part = Part::bytes(bytes)
            .file_name(file_name)
            .mime_str(content_type)
            .map_err(|error| format!("附件类型无效: {error}"))?;
        form = form.part(field_name, part);
    }

    if input.include_diagnostics {
        let tail = proxy_runtime
            .logs()
            .into_iter()
            .rev()
            .take(200)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|entry| {
                sanitize_feedback_log_text(&format!(
                    "{} {} {}",
                    entry.time, entry.level, entry.message
                ))
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !tail.trim().is_empty() {
            form = form.part(
                "log_proxy_tail",
                Part::bytes(tail.into_bytes())
                    .file_name("proxy-tail.log")
                    .mime_str("text/plain")
                    .map_err(|error| format!("诊断日志类型无效: {error}"))?,
            );
        }
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("反馈服务暂不可用:{error}"))?;
    let response = match client.post(FEEDBACK_WORKER_URL).multipart(form).send() {
        Ok(response) => response,
        Err(error) => {
            record_feedback_failure();
            return Err(format!("反馈服务暂不可用:{error}"));
        }
    };
    let status = response.status();
    let response_text = response.text().unwrap_or_default();
    let response_data: Value = serde_json::from_str(&response_text).unwrap_or_else(|_| json!({}));
    if !status.is_success() || response_data.get("ok").and_then(Value::as_bool) != Some(true) {
        record_feedback_failure();
        let message = response_data
            .get("error")
            .or_else(|| response_data.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("上游错误");
        return Err(message.to_string());
    }

    if let Ok(mut guard) = throttle().lock() {
        guard.record_success();
    }
    let feedback_id = response_data
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Ok(json!({
        "success": true,
        "id": feedback_id,
        "message": format!("反馈已收到 (ID: {feedback_id})"),
        "email_sent": response_data
            .get("email_sent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }))
}

fn text_part(text: String, mime: &str) -> Result<Part, String> {
    Part::text(text)
        .mime_str(mime)
        .map_err(|error| format!("反馈请求构造失败: {error}"))
}

fn active_provider_name() -> Result<String, String> {
    let config = ConfigStore::default()?.load_config()?;
    let active_id = config.active_provider.as_deref();
    let chosen = active_id
        .and_then(|id| config.providers.iter().find(|provider| provider.id == id))
        .or_else(|| config.providers.first());
    Ok(chosen
        .map(|provider| provider.name.clone())
        .unwrap_or_default())
}

fn safe_feedback_file_name(name: &str, kind: &str) -> String {
    let fallback = if kind == "screenshot" {
        "screenshot.bin"
    } else {
        "attachment.bin"
    };
    let source = if name.trim().is_empty() {
        fallback
    } else {
        name.trim()
    };
    let sanitized = source
        .chars()
        .map(|ch| {
            if ch.is_control() || ch == '/' || ch == '\\' {
                '_'
            } else {
                ch
            }
        })
        .take(200)
        .collect::<String>();
    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}

fn sanitize_feedback_log_text(input: &str) -> String {
    let no_urls = redact_url_like_tokens(input);
    no_urls
        .split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            if lower.starts_with("sk-") || lower.starts_with("sk_ant") {
                "[API_KEY]"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_url_like_tokens(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for token in input.split_inclusive(char::is_whitespace) {
        let trimmed = token.trim_end();
        let suffix = &token[trimmed.len()..];
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            output.push_str("[URL]");
            output.push_str(suffix);
        } else {
            output.push_str(token);
        }
    }
    output
}

fn record_feedback_failure() {
    if let Ok(mut guard) = throttle().lock() {
        guard.record_failure();
    }
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_log_sanitizer_removes_urls_and_keys() {
        let text = "12:00 INFO 转发请求 -> https://api.example.com/anthropic sk-testsecret";
        let sanitized = sanitize_feedback_log_text(text);
        assert!(sanitized.contains("[URL]"));
        assert!(sanitized.contains("[API_KEY]"));
        assert!(!sanitized.contains("api.example.com"));
        assert!(!sanitized.contains("sk-testsecret"));
    }
}
