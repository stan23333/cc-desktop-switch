use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationStatus {
    pub app_name: String,
    pub version: String,
    pub runtime: String,
    pub config_path: String,
    pub config_exists: bool,
    pub active_provider_id: Option<String>,
    pub active_provider_name: Option<String>,
    pub provider_count: usize,
    pub admin_port: u16,
    pub proxy_port: u16,
    pub started_at_ms: u128,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub auth_scheme: String,
    pub api_format: String,
    pub models: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub version: String,
    pub active_provider: Option<String>,
    pub gateway_api_key: Option<String>,
    pub providers: Vec<Provider>,
    pub settings: Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: String,
    pub language: String,
    pub proxy_port: u16,
    pub admin_port: u16,
    pub auto_start: bool,
    pub expose_all_provider_models: bool,
    pub update_url: String,
    pub upstream_proxy: String,
    pub upstream_proxy_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            language: "zh".to_string(),
            proxy_port: 18080,
            admin_port: 18081,
            auto_start: false,
            expose_all_provider_models: false,
            update_url:
                "https://github.com/lonr-6/cc-desktop-switch/releases/latest/download/latest.json"
                    .to_string(),
            upstream_proxy: String::new(),
            upstream_proxy_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub auth_scheme: String,
    pub api_format: String,
    pub api_key: String,
    pub models: BTreeMap<String, String>,
    pub extra_headers: BTreeMap<String, String>,
    pub model_capabilities: Value,
    pub request_options: Value,
    pub is_builtin: bool,
    pub sort_index: usize,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub name: String,
    pub size: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportedConfig {
    pub format: String,
    pub exported_at: String,
    pub config: AppConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportResult {
    pub config: AppConfig,
    pub backup: BackupInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DesktopConfigSources {
    pub plist: bool,
    pub json: bool,
    pub config_library: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopConfigStatus {
    pub configured: bool,
    pub keys: BTreeMap<String, String>,
    pub message: String,
    pub sources: DesktopConfigSources,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopHealthIssue {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopHealth {
    pub needs_apply: bool,
    pub one_million_ready: bool,
    pub expected_base_url: String,
    pub actual_base_url: String,
    pub mode: String,
    pub requires_proxy: bool,
    pub issues: Vec<DesktopHealthIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopApplyResult {
    pub success: bool,
    pub message: String,
    pub mode: Option<String>,
    pub requires_proxy: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
    pub running: bool,
    pub port: u16,
    pub active_provider_id: Option<String>,
    pub has_gateway_key: bool,
    pub implemented: bool,
    pub stats: ProxyStats,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStats {
    pub total: u64,
    pub success: u64,
    pub failed: u64,
    pub today: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyLogEntry {
    pub time: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub active: bool,
    pub percent: u8,
    pub message: String,
}

impl Default for UpdateProgress {
    fn default() -> Self {
        Self {
            active: false,
            percent: 0,
            message: String::new(),
        }
    }
}
