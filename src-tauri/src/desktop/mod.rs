#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::config::ConfigStore;
use crate::model_alias::{MODEL_ORDER, normalize_model_mappings};
use crate::models::{
    AppConfig, DesktopApplyResult, DesktopConfigSources, DesktopConfigStatus, DesktopHealth,
    DesktopHealthIssue, Provider,
};

pub const CCDS_MARKER: &str = "ccds_managed";
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub const REGISTRY_PATH: &str = r"SOFTWARE\Policies\Claude";

const DESKTOP_CONFIG_NAMES: [&str; 7] = [
    "inferenceProvider",
    "inferenceGatewayApiKey",
    "inferenceGatewayAuthScheme",
    "inferenceGatewayHeaders",
    "inferenceModels",
    "inferenceGatewayBaseUrl",
    "isClaudeCodeForDesktopEnabled",
];

const DEFAULT_INFERENCE_MODELS: [&str; 3] = ["sonnet", "haiku", "opus"];

#[derive(Debug, Clone, PartialEq)]
pub struct DesktopConfigTarget {
    pub base_url: String,
    pub api_key: String,
    pub auth_scheme: String,
    pub gateway_headers: String,
    pub provider: Option<Provider>,
    pub providers: Vec<Provider>,
    pub expose_all: bool,
    pub requires_proxy: bool,
    pub mode: String,
}

pub fn get_config_status() -> DesktopConfigStatus {
    #[cfg(target_os = "macos")]
    {
        return macos::get_config_status();
    }

    #[cfg(target_os = "windows")]
    {
        return windows::get_config_status();
    }

    #[allow(unreachable_code)]
    DesktopConfigStatus {
        configured: false,
        keys: BTreeMap::new(),
        message: "Claude Desktop has no Linux GUI version, no configuration needed".to_string(),
        sources: DesktopConfigSources::default(),
    }
}

pub fn configure_active_provider() -> Result<DesktopApplyResult, String> {
    let store = ConfigStore::default()?;
    let mut config = store.load_config()?;
    let needs_proxy = active_provider(&config)
        .map(|provider| provider.api_format.as_str() != "anthropic")
        .unwrap_or(true);
    let gateway_key = if needs_proxy {
        Some(store.get_or_create_gateway_api_key()?)
    } else {
        None
    };
    if let Some(gateway_key) = gateway_key {
        config.gateway_api_key = Some(gateway_key);
    }
    let target = desktop_config_target_for_config(&config);
    let inference_models = serialize_inference_models(
        target.provider.as_ref(),
        if target.expose_all {
            Some(&target.providers)
        } else {
            None
        },
        target.expose_all,
    )?;

    let mut result = apply_platform_config(
        &target.base_url,
        &target.api_key,
        &inference_models,
        &target.auth_scheme,
        &target.gateway_headers,
    );
    result.mode = Some(target.mode);
    result.requires_proxy = Some(target.requires_proxy);
    Ok(result)
}

pub fn clear_config() -> DesktopApplyResult {
    #[cfg(target_os = "macos")]
    {
        return macos::clear_config();
    }

    #[cfg(target_os = "windows")]
    {
        return windows::clear_config();
    }

    #[allow(unreachable_code)]
    not_supported()
}

pub fn restart_claude_desktop() -> DesktopApplyResult {
    #[cfg(target_os = "macos")]
    {
        return macos::restart_claude_desktop();
    }

    #[cfg(target_os = "windows")]
    {
        return windows::restart_claude_desktop();
    }

    #[allow(unreachable_code)]
    not_supported()
}

pub fn get_desktop_health() -> Result<DesktopHealth, String> {
    let store = ConfigStore::default()?;
    let config = store.load_config()?;
    let status = get_config_status();
    Ok(desktop_health(&status, &config))
}

pub fn desktop_health(status: &DesktopConfigStatus, config: &AppConfig) -> DesktopHealth {
    let target = desktop_config_target_for_config(config);
    let expected_base_url = trim_gateway_base_url(&target.base_url);
    let actual_base_url = status
        .keys
        .get("inferenceGatewayBaseUrl")
        .map(|value| trim_gateway_base_url(value))
        .unwrap_or_default();
    let mut issues = Vec::new();

    if !actual_base_url.is_empty() && actual_base_url != expected_base_url {
        issues.push(desktop_health_issue(
            "gateway_base_url_mismatch",
            "Claude 桌面版仍指向旧地址，请重新一键应用到 Claude 桌面版。",
        ));
    }

    if !status.configured {
        if status.keys.is_empty() {
            issues.push(desktop_health_issue(
                "desktop_not_configured",
                "桌面版尚未配置，请添加提供商并一键应用到 Claude 桌面版。",
            ));
        } else {
            issues.push(desktop_health_issue(
                "not_managed_by_ccds",
                "当前桌面版配置不是由本工具最新版本写入。",
            ));
        }
    }

    let inference_models = parse_inference_models(
        status
            .keys
            .get("inferenceModels")
            .map(String::as_str)
            .unwrap_or(""),
    );
    let target_models = provider_inference_models(target.provider.as_ref());
    let one_million_models: Vec<String> = target_models
        .iter()
        .filter_map(|item| one_million_model_name(item))
        .collect();

    let mut one_million_ready = true;
    if !one_million_models.is_empty() {
        let written_one_million: BTreeSet<String> = inference_models
            .iter()
            .filter_map(|item| one_million_model_name(item))
            .collect();
        one_million_ready = one_million_models
            .iter()
            .all(|model| written_one_million.contains(model));
        if !one_million_ready {
            issues.push(desktop_health_issue(
                "one_million_not_written",
                "1M 上下文模型尚未写入桌面版配置，请重新一键应用并重启 Claude 桌面版。",
            ));
        }
    }

    DesktopHealth {
        needs_apply: !issues.is_empty(),
        one_million_ready,
        expected_base_url,
        actual_base_url,
        mode: target.mode,
        requires_proxy: target.requires_proxy,
        issues,
    }
}

fn apply_platform_config(
    base_url: &str,
    gateway_api_key: &str,
    inference_models: &str,
    auth_scheme: &str,
    gateway_headers: &str,
) -> DesktopApplyResult {
    #[cfg(target_os = "macos")]
    {
        return macos::apply_config(
            base_url,
            gateway_api_key,
            inference_models,
            auth_scheme,
            gateway_headers,
        );
    }

    #[cfg(target_os = "windows")]
    {
        return windows::apply_config(
            base_url,
            gateway_api_key,
            inference_models,
            auth_scheme,
            gateway_headers,
        );
    }

    #[allow(unreachable_code)]
    not_supported()
}

pub fn desktop_config_target_for_config(config: &AppConfig) -> DesktopConfigTarget {
    let provider = active_provider(config).cloned();
    let requires_proxy = provider
        .as_ref()
        .map(|provider| provider.api_format.as_str() != "anthropic")
        .unwrap_or(true);

    if requires_proxy {
        return DesktopConfigTarget {
            base_url: format!("http://127.0.0.1:{}", config.settings.proxy_port),
            api_key: config.gateway_api_key.clone().unwrap_or_default(),
            auth_scheme: "bearer".to_string(),
            gateway_headers: String::new(),
            provider,
            providers: Vec::new(),
            expose_all: false,
            requires_proxy: true,
            mode: "local_proxy".to_string(),
        };
    }

    let provider = provider.expect("direct provider requires an active provider");
    let api_key = provider.api_key.clone();
    DesktopConfigTarget {
        base_url: provider.base_url.trim_end_matches('/').to_string(),
        api_key: api_key.clone(),
        auth_scheme: if provider.auth_scheme.is_empty() {
            "bearer".to_string()
        } else {
            provider.auth_scheme.clone()
        },
        gateway_headers: serialize_gateway_headers(&provider.extra_headers, &api_key),
        provider: Some(provider),
        providers: Vec::new(),
        expose_all: false,
        requires_proxy: false,
        mode: "direct_provider".to_string(),
    }
}

fn parse_inference_models(raw_value: &str) -> Vec<Value> {
    let Ok(parsed) = serde_json::from_str::<Value>(raw_value) else {
        return Vec::new();
    };
    parsed.as_array().cloned().unwrap_or_default()
}

fn one_million_model_name(item: &Value) -> Option<String> {
    let object = item.as_object()?;
    if object.get("supports1m").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    object
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn trim_gateway_base_url(value: &str) -> String {
    value.trim_end_matches('/').to_string()
}

fn desktop_health_issue(code: &str, message: &str) -> DesktopHealthIssue {
    DesktopHealthIssue {
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn active_provider(config: &AppConfig) -> Option<&Provider> {
    let active_id = config.active_provider.as_deref()?;
    config
        .providers
        .iter()
        .find(|provider| provider.id.as_str() == active_id)
}

pub fn serialize_gateway_headers(
    extra_headers: &BTreeMap<String, String>,
    api_key: &str,
) -> String {
    if extra_headers.is_empty() {
        return String::new();
    }
    let headers: Vec<String> = extra_headers
        .iter()
        .filter_map(|(name, value)| {
            let header_name = name.trim();
            if header_name.is_empty() {
                return None;
            }
            let header_value = value.replace("{apiKey}", api_key);
            Some(format!("{header_name}: {header_value}"))
        })
        .collect();
    if headers.is_empty() {
        String::new()
    } else {
        serde_json::to_string(&headers).unwrap_or_default()
    }
}

pub fn serialize_inference_models(
    provider: Option<&Provider>,
    providers: Option<&[Provider]>,
    expose_all: bool,
) -> Result<String, String> {
    let models = if expose_all {
        all_provider_inference_models(providers.unwrap_or_default())
    } else {
        provider_inference_models(provider)
    };
    serde_json::to_string(&models).map_err(|error| format!("Failed to serialize models: {error}"))
}

pub fn provider_inference_models(provider: Option<&Provider>) -> Vec<Value> {
    let models = provider
        .map(|provider| provider_model_entries(provider, false))
        .unwrap_or_default();
    if models.is_empty() {
        return fallback_inference_models();
    }
    desktop_model_items(models)
}

pub fn all_provider_inference_models(providers: &[Provider]) -> Vec<Value> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for provider in providers {
        for item in provider_model_entries(provider, true) {
            if seen.insert(item.name.clone()) {
                result.push(item);
            }
        }
    }
    if result.is_empty() {
        fallback_inference_models()
    } else {
        desktop_model_items(result)
    }
}

fn fallback_inference_models() -> Vec<Value> {
    DEFAULT_INFERENCE_MODELS
        .iter()
        .map(|name| json!(name))
        .collect()
}

fn desktop_model_items(items: Vec<ModelEntry>) -> Vec<Value> {
    items
        .into_iter()
        .map(|item| {
            let mut object = serde_json::Map::new();
            object.insert("name".to_string(), Value::String(item.name));
            object.insert("displayName".to_string(), Value::String(item.display_name));
            if item.supports_1m {
                object.insert("supports1m".to_string(), Value::Bool(true));
            }
            Value::Object(object)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    provider_model_ids(provider)
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

fn provider_model_ids(provider: &Provider) -> Vec<String> {
    let models = normalize_model_mappings(Some(&provider.models));
    let mut result = Vec::new();
    for key in MODEL_ORDER {
        let model_id = models.get(key).map(|value| value.trim()).unwrap_or("");
        if !model_id.is_empty() && !result.iter().any(|existing| existing == model_id) {
            result.push(model_id.to_string());
        }
    }
    result
}

fn provider_slug(provider: &Provider) -> String {
    let source = if !provider.id.is_empty() {
        provider.id.as_str()
    } else if !provider.name.is_empty() {
        provider.name.as_str()
    } else {
        "provider"
    };
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in source.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
            slug.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
        if slug.len() >= 56 {
            break;
        }
    }
    let slug = slug.trim_matches(&['-', '_'][..]).to_string();
    if slug.is_empty() {
        "provider".to_string()
    } else {
        slug
    }
}

fn model_supports_1m(provider: &Provider, model_id: &str) -> bool {
    if model_id.to_lowercase().contains("[1m]") {
        return true;
    }
    provider
        .model_capabilities
        .as_object()
        .and_then(|capabilities| capabilities.get(model_id))
        .and_then(Value::as_object)
        .and_then(|capability| capability.get("supports1m"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn managed_policy_names(names: &[String]) -> Vec<String> {
    let managed: BTreeSet<&str> = DESKTOP_CONFIG_NAMES
        .iter()
        .copied()
        .chain(std::iter::once(CCDS_MARKER))
        .collect();
    names
        .iter()
        .filter(|name| managed.contains(name.as_str()))
        .cloned()
        .collect()
}

pub fn safe_config_value(name: &str, value: &Value) -> String {
    let lowered = name.to_lowercase();
    if lowered.contains("headers") {
        if value.is_null()
            || value
                .as_str()
                .is_some_and(|item| item.is_empty() || item == "[]")
            || value.as_array().is_some_and(Vec::is_empty)
        {
            return String::new();
        }
    }
    if ["key", "token", "secret", "authorization", "headers"]
        .iter()
        .any(|token| lowered.contains(token))
    {
        return if value_is_empty(value) {
            String::new()
        } else {
            "******".to_string()
        };
    }
    if let Some(value) = value.as_bool() {
        return if value { "1" } else { "0" }.to_string();
    }
    if let Some(value) = value.as_str() {
        return value.to_string();
    }
    if value.is_array() || value.is_object() {
        return serde_json::to_string(value).unwrap_or_default();
    }
    value.to_string()
}

fn value_is_empty(value: &Value) -> bool {
    value.is_null()
        || value.as_str().is_some_and(str::is_empty)
        || value.as_array().is_some_and(Vec::is_empty)
        || value.as_object().is_some_and(serde_json::Map::is_empty)
}

pub fn default_status(message: impl Into<String>) -> DesktopConfigStatus {
    DesktopConfigStatus {
        configured: false,
        keys: BTreeMap::new(),
        message: message.into(),
        sources: DesktopConfigSources::default(),
    }
}

pub fn success(message: impl Into<String>) -> DesktopApplyResult {
    DesktopApplyResult {
        success: true,
        message: message.into(),
        mode: None,
        requires_proxy: None,
    }
}

pub fn failure(message: impl Into<String>) -> DesktopApplyResult {
    DesktopApplyResult {
        success: false,
        message: message.into(),
        mode: None,
        requires_proxy: None,
    }
}

fn not_supported() -> DesktopApplyResult {
    failure("Claude Desktop has no Linux GUI version, no configuration needed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider() -> Provider {
        serde_json::from_value(json!({
            "id": "deepseek",
            "name": "DeepSeek",
            "baseUrl": "https://api.deepseek.com/anthropic/",
            "authScheme": "x-api-key",
            "apiFormat": "anthropic",
            "apiKey": "provider-key",
            "extraHeaders": {"x-api-key": "{apiKey}"},
            "models": {
                "sonnet": "deepseek-v4-pro[1m]",
                "haiku": "deepseek-v4-flash",
                "default": "deepseek-v4-pro[1m]"
            },
            "modelCapabilities": {
                "deepseek-v4-flash": {"supports1m": true}
            },
            "requestOptions": {},
            "isBuiltin": false,
            "sortIndex": 0
        }))
        .expect("provider")
    }

    fn config_with_provider(provider: Provider) -> AppConfig {
        AppConfig {
            version: "1.1.1".to_string(),
            active_provider: Some(provider.id.clone()),
            gateway_api_key: None,
            providers: vec![provider],
            settings: Default::default(),
        }
    }

    fn desktop_status(
        configured: bool,
        base_url: &str,
        inference_models: &str,
    ) -> DesktopConfigStatus {
        DesktopConfigStatus {
            configured,
            keys: BTreeMap::from([
                ("inferenceGatewayBaseUrl".to_string(), base_url.to_string()),
                ("inferenceModels".to_string(), inference_models.to_string()),
            ]),
            message: String::new(),
            sources: DesktopConfigSources::default(),
        }
    }

    fn health_codes(health: &DesktopHealth) -> BTreeSet<String> {
        health
            .issues
            .iter()
            .map(|issue| issue.code.clone())
            .collect()
    }

    #[test]
    fn target_uses_direct_provider_for_anthropic_format() {
        let provider = provider();
        let config = config_with_provider(provider);

        let target = desktop_config_target_for_config(&config);

        assert_eq!(target.mode, "direct_provider");
        assert!(!target.requires_proxy);
        assert_eq!(target.base_url, "https://api.deepseek.com/anthropic");
        assert_eq!(target.api_key, "provider-key");
        assert_eq!(target.auth_scheme, "x-api-key");
        assert_eq!(
            serde_json::from_str::<Vec<String>>(&target.gateway_headers).expect("headers"),
            vec!["x-api-key: provider-key"]
        );
    }

    #[test]
    fn inference_models_keep_capability_flags() {
        let provider = provider();
        let serialized =
            serialize_inference_models(Some(&provider), None, false).expect("serialized models");
        let parsed: Vec<Value> = serde_json::from_str(&serialized).expect("model json");

        assert!(parsed.iter().any(|item| {
            item.get("name").and_then(Value::as_str) == Some("deepseek-v4-pro[1m]")
                && item.get("supports1m").and_then(Value::as_bool) == Some(true)
        }));
        assert!(parsed.iter().any(|item| {
            item.get("name").and_then(Value::as_str) == Some("deepseek-v4-flash")
                && item.get("supports1m").and_then(Value::as_bool) == Some(true)
        }));
    }

    #[test]
    fn desktop_health_detects_stale_gateway_and_missing_1m() {
        let config = config_with_provider(provider());
        let old_status = desktop_status(
            false,
            "http://127.0.0.1:18080",
            r#"["sonnet","haiku","opus"]"#,
        );

        let health = desktop_health(&old_status, &config);
        let codes = health_codes(&health);

        assert!(health.needs_apply);
        assert!(codes.contains("gateway_base_url_mismatch"));
        assert!(codes.contains("one_million_not_written"));
        assert_eq!(
            health.expected_base_url,
            "https://api.deepseek.com/anthropic"
        );
        assert_eq!(health.actual_base_url, "http://127.0.0.1:18080");

        let current_status = desktop_status(
            true,
            "https://api.deepseek.com/anthropic",
            r#"[{"name":"deepseek-v4-pro[1m]","supports1m":true},{"name":"deepseek-v4-flash","supports1m":true}]"#,
        );
        let current_health = desktop_health(&current_status, &config);

        assert!(!current_health.needs_apply);
        assert!(current_health.one_million_ready);
    }

    #[test]
    fn desktop_health_detects_capability_based_1m_models() {
        let provider: Provider = serde_json::from_value(json!({
            "id": "qwen",
            "name": "Qwen",
            "baseUrl": "https://dashscope.aliyuncs.com/apps/anthropic",
            "authScheme": "bearer",
            "apiFormat": "anthropic",
            "apiKey": "provider-key",
            "extraHeaders": {},
            "models": {
                "sonnet": "qwen3.6-plus",
                "haiku": "qwen3.6-flash",
                "opus": "qwen3.6-max-preview",
                "default": "qwen3.6-plus"
            },
            "modelCapabilities": {
                "qwen3.6-plus": {"supports1m": true},
                "qwen3.6-flash": {"supports1m": true}
            },
            "requestOptions": {},
            "isBuiltin": false,
            "sortIndex": 0
        }))
        .expect("qwen provider");
        let config = config_with_provider(provider);

        let missing = desktop_health(
            &desktop_status(
                true,
                "https://dashscope.aliyuncs.com/apps/anthropic",
                r#"[{"name":"qwen3.6-plus"},{"name":"qwen3.6-flash"}]"#,
            ),
            &config,
        );
        let ready = desktop_health(
            &desktop_status(
                true,
                "https://dashscope.aliyuncs.com/apps/anthropic",
                r#"[{"name":"qwen3.6-plus","supports1m":true},{"name":"qwen3.6-flash","supports1m":true}]"#,
            ),
            &config,
        );

        assert!(missing.needs_apply);
        assert!(!missing.one_million_ready);
        assert!(!ready.needs_apply);
        assert!(ready.one_million_ready);
    }

    #[test]
    fn all_provider_models_use_aliases() {
        let provider = provider();
        let models = all_provider_inference_models(&[provider]);

        assert!(models.iter().any(|item| {
            item.get("name").and_then(Value::as_str) == Some("deepseek/deepseek-v4-pro[1m]")
                && item.get("displayName").and_then(Value::as_str)
                    == Some("DeepSeek / deepseek-v4-pro[1m]")
        }));
    }

    #[test]
    fn managed_policy_name_filter_keeps_only_owned_keys() {
        let names = vec![
            "inferenceProvider".to_string(),
            "isClaudeCodeForDesktopEnabled".to_string(),
            "ccds_managed".to_string(),
            "unrelatedPreference".to_string(),
        ];

        assert_eq!(
            managed_policy_names(&names),
            vec![
                "inferenceProvider".to_string(),
                "isClaudeCodeForDesktopEnabled".to_string(),
                "ccds_managed".to_string(),
            ]
        );
    }
}
