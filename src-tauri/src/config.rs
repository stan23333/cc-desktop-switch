use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

use crate::model_alias::{model_mappings_with_legacy_aliases, normalize_model_mappings};
use crate::models::{
    AppConfig, BackupInfo, ExportedConfig, ImportResult, MigrationStatus, Provider, ProviderPreset,
    Settings,
};

const APP_NAME: &str = "CC Desktop Switch";
const APP_VERSION: &str = "1.1.0";
const CONFIG_DIR_NAME: &str = ".cc-desktop-switch";
const CONFIG_FILE_NAME: &str = "config.json";
const BACKUP_DIR_NAME: &str = "backups";
const EXPORT_FORMAT: &str = "cc-desktop-switch.config";
const SETTINGS_FIELDS: [&str; 9] = [
    "theme",
    "language",
    "proxyPort",
    "adminPort",
    "autoStart",
    "exposeAllProviderModels",
    "updateUrl",
    "upstreamProxy",
    "upstreamProxyEnabled",
];
static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct ConfigStore {
    config_dir: PathBuf,
    config_file: PathBuf,
    backup_dir: PathBuf,
}

impl ConfigStore {
    pub fn default() -> Result<Self, String> {
        let home = env::var_os("HOME")
            .or_else(|| env::var_os("USERPROFILE"))
            .ok_or_else(|| "Cannot determine home directory".to_string())?;
        Ok(Self::for_dir(PathBuf::from(home).join(CONFIG_DIR_NAME)))
    }

    pub fn for_dir(config_dir: PathBuf) -> Self {
        let config_file = config_dir.join(CONFIG_FILE_NAME);
        let backup_dir = config_dir.join(BACKUP_DIR_NAME);
        Self {
            config_dir,
            config_file,
            backup_dir,
        }
    }

    pub fn config_file(&self) -> &Path {
        &self.config_file
    }

    pub fn load_config(&self) -> Result<AppConfig, String> {
        ensure_dir(&self.config_dir)?;
        if !self.config_file.exists() {
            return Ok(default_config());
        }
        let text = fs::read_to_string(&self.config_file)
            .map_err(|error| format!("Failed to read config file: {error}"))?;
        let value = serde_json::from_str::<Value>(&text)
            .map_err(|error| format!("Failed to parse config file: {error}"))?;
        normalize_config_value(value)
    }

    pub fn public_config_snapshot(&self) -> Result<Value, String> {
        let mut value = serde_json::to_value(self.load_config()?)
            .map_err(|error| format!("Failed to encode public config: {error}"))?;
        if let Some(providers) = value.get_mut("providers").and_then(Value::as_array_mut) {
            for provider in providers {
                if let Some(object) = provider.as_object_mut() {
                    let has_api_key = object
                        .get("apiKey")
                        .and_then(Value::as_str)
                        .is_some_and(|api_key| !api_key.is_empty());
                    object.remove("apiKey");
                    object.insert("hasApiKey".to_string(), Value::Bool(has_api_key));
                    object.remove("extraHeaders");
                }
            }
        }
        Ok(value)
    }

    pub fn get_settings(&self) -> Result<Settings, String> {
        Ok(self.load_config()?.settings)
    }

    pub fn update_settings(&self, data: Value) -> Result<Settings, String> {
        let Some(incoming) = data.as_object() else {
            return Err("Settings payload must be a JSON object".to_string());
        };
        let mut config = self.load_config()?;
        let mut settings = serde_json::to_value(&config.settings)
            .map_err(|error| format!("Failed to serialize settings: {error}"))?
            .as_object()
            .cloned()
            .unwrap_or_default();
        for key in SETTINGS_FIELDS {
            if let Some(value) = incoming.get(key) {
                settings.insert(key.to_string(), value.clone());
            }
        }
        config.settings = normalize_settings(&settings);
        self.save_config(&config)?;
        Ok(config.settings)
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<(), String> {
        ensure_dir(&self.config_dir)?;
        let normalized = normalize_config_value(
            serde_json::to_value(config)
                .map_err(|error| format!("Failed to serialize config: {error}"))?,
        )?;
        let text = serde_json::to_string_pretty(&normalized)
            .map_err(|error| format!("Failed to encode config: {error}"))?;
        let tmp_path = self.config_file.with_extension("json.tmp");
        fs::write(&tmp_path, format!("{text}\n"))
            .map_err(|error| format!("Failed to write temp config: {error}"))?;
        fs::rename(&tmp_path, &self.config_file)
            .map_err(|error| format!("Failed to replace config file: {error}"))
    }

    pub fn add_provider(&self, provider: Value) -> Result<Provider, String> {
        let mut config = self.load_config()?;
        let mut provider = normalize_provider_value(provider);
        let existing_ids = provider_ids(&config.providers);
        let base_id = if provider.id.is_empty() {
            "provider".to_string()
        } else {
            provider.id.clone()
        };
        let mut candidate_id = base_id.clone();
        while existing_ids.contains(&candidate_id) {
            candidate_id = format!("{base_id}-{}", unique_suffix());
        }
        provider.id = candidate_id;
        provider.sort_index = config.providers.len();

        config.providers.push(provider.clone());
        if config.providers.len() == 1 {
            config.active_provider = Some(provider.id.clone());
        }
        self.save_config(&config)?;
        Ok(provider_with_legacy_aliases(provider))
    }

    pub fn update_provider(
        &self,
        provider_id: &str,
        data: Value,
    ) -> Result<Option<Provider>, String> {
        let mut config = self.load_config()?;
        let Some(index) = config
            .providers
            .iter()
            .position(|provider| provider.id == provider_id)
        else {
            return Ok(None);
        };

        let current = config.providers[index].clone();
        let mut updated = merge_provider_value(current.clone(), data);
        updated.id = provider_id.to_string();
        updated.is_builtin = current.is_builtin;
        config.providers[index] = updated.clone();
        self.save_config(&config)?;
        Ok(Some(provider_with_legacy_aliases(updated)))
    }

    pub fn get_provider(&self, provider_id: &str) -> Result<Option<Provider>, String> {
        Ok(self
            .load_config()?
            .providers
            .into_iter()
            .find(|provider| provider.id == provider_id))
    }

    pub fn update_models(
        &self,
        provider_id: &str,
        models: BTreeMap<String, String>,
    ) -> Result<bool, String> {
        let mut config = self.load_config()?;
        let Some(provider) = config
            .providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
        else {
            return Ok(false);
        };
        provider.models = normalize_model_mappings(Some(&models));
        self.save_config(&config)?;
        Ok(true)
    }

    pub fn delete_provider(&self, provider_id: &str) -> Result<bool, String> {
        let mut config = self.load_config()?;
        let before = config.providers.len();
        config
            .providers
            .retain(|provider| provider.id.as_str() != provider_id);
        if config.providers.len() == before {
            return Ok(false);
        }

        if config.active_provider.as_deref() == Some(provider_id) {
            config.active_provider = config.providers.first().map(|provider| provider.id.clone());
        }
        for (index, provider) in config.providers.iter_mut().enumerate() {
            provider.sort_index = index;
        }
        self.save_config(&config)?;
        Ok(true)
    }

    pub fn set_active_provider(&self, provider_id: &str) -> Result<Option<Provider>, String> {
        let mut config = self.load_config()?;
        let Some(provider) = config
            .providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .cloned()
        else {
            return Ok(None);
        };
        config.active_provider = Some(provider_id.to_string());
        self.save_config(&config)?;
        Ok(Some(provider_with_legacy_aliases(provider)))
    }

    pub fn reorder_providers(&self, provider_ids: Vec<String>) -> Result<bool, String> {
        let mut config = self.load_config()?;
        let mut by_id: BTreeMap<String, Provider> = config
            .providers
            .iter()
            .cloned()
            .map(|provider| (provider.id.clone(), provider))
            .collect();
        let mut ordered = Vec::new();
        let mut seen = BTreeSet::new();
        for provider_id in provider_ids {
            if seen.contains(&provider_id) {
                continue;
            }
            if let Some(provider) = by_id.remove(&provider_id) {
                seen.insert(provider_id);
                ordered.push(provider);
            }
        }
        for provider in &config.providers {
            if let Some(provider) = by_id.remove(&provider.id) {
                ordered.push(provider);
            }
        }
        if ordered.len() != config.providers.len() {
            return Ok(false);
        }
        for (index, provider) in ordered.iter_mut().enumerate() {
            provider.sort_index = index;
        }
        config.providers = ordered;
        self.save_config(&config)?;
        Ok(true)
    }

    pub fn create_backup(&self, reason: &str) -> Result<BackupInfo, String> {
        ensure_dir(&self.backup_dir)?;
        if !self.config_file.exists() {
            self.save_config(&self.load_config()?)?;
        }

        let safe_reason = sanitize_reason(reason);
        let now = now_millis();
        let filename = format!(
            "config-{now}-{}-{}.json",
            if safe_reason.is_empty() {
                "manual"
            } else {
                &safe_reason
            },
            unique_suffix()
        );
        let target = self.backup_dir.join(filename);
        fs::copy(&self.config_file, &target)
            .map_err(|error| format!("Failed to copy backup: {error}"))?;
        backup_info(&target)
    }

    pub fn list_backups(&self) -> Result<Vec<BackupInfo>, String> {
        ensure_dir(&self.backup_dir)?;
        let mut backups = Vec::new();
        for entry in fs::read_dir(&self.backup_dir)
            .map_err(|error| format!("Failed to list backups: {error}"))?
        {
            let entry = entry.map_err(|error| format!("Failed to read backup entry: {error}"))?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("json") {
                backups.push(backup_info(&path)?);
            }
        }
        backups.sort_by(|left, right| right.name.cmp(&left.name));
        Ok(backups)
    }

    pub fn export_config(&self) -> Result<ExportedConfig, String> {
        Ok(ExportedConfig {
            format: EXPORT_FORMAT.to_string(),
            exported_at: now_millis().to_string(),
            config: self.load_config()?,
        })
    }

    pub fn import_config(&self, value: Value) -> Result<ImportResult, String> {
        let backup = self.create_backup("before-import")?;
        let config = normalize_config_value(value)?;
        self.save_config(&config)?;
        Ok(ImportResult { config, backup })
    }

    pub fn get_or_create_gateway_api_key(&self) -> Result<String, String> {
        let mut config = self.load_config()?;
        if let Some(api_key) = config
            .gateway_api_key
            .as_deref()
            .filter(|api_key| !api_key.is_empty())
        {
            return Ok(api_key.to_string());
        }

        let api_key = format!("ccds_{}", random_hex(32)?);
        config.gateway_api_key = Some(api_key.clone());
        self.save_config(&config)?;
        Ok(api_key)
    }
}

pub fn migration_status(started_at_ms: u128) -> Result<MigrationStatus, String> {
    let store = ConfigStore::default()?;
    let config_path = store.config_file().to_path_buf();
    let config_exists = config_path.exists();
    let data = store.load_config()?;
    let active_provider_name = data.active_provider.as_deref().and_then(|active_id| {
        data.providers
            .iter()
            .find(|provider| provider.id == active_id)
            .map(|provider| provider.name.clone())
    });

    Ok(MigrationStatus {
        app_name: APP_NAME.to_string(),
        version: APP_VERSION.to_string(),
        runtime: "tauri-rust".to_string(),
        config_path: config_path.display().to_string(),
        config_exists,
        active_provider_id: data.active_provider,
        active_provider_name,
        provider_count: data.providers.len(),
        admin_port: data.settings.admin_port,
        proxy_port: data.settings.proxy_port,
        started_at_ms,
    })
}

pub fn builtin_presets() -> Vec<ProviderPreset> {
    builtin_preset_values()
        .into_iter()
        .map(normalize_provider_value)
        .map(|provider| {
            let mut extra = provider.extra;
            if !provider.extra_headers.is_empty() {
                extra.insert("extraHeaders".to_string(), json!(provider.extra_headers));
            }
            if !provider.model_capabilities.is_null() {
                extra.insert(
                    "modelCapabilities".to_string(),
                    provider.model_capabilities.clone(),
                );
            }
            if !provider.request_options.is_null() {
                extra.insert(
                    "requestOptions".to_string(),
                    provider.request_options.clone(),
                );
            }
            ProviderPreset {
                id: provider.id,
                name: provider.name,
                base_url: provider.base_url,
                auth_scheme: provider.auth_scheme,
                api_format: provider.api_format,
                models: model_mappings_with_legacy_aliases(&provider.models),
                extra,
            }
        })
        .collect()
}

pub fn normalize_provider_payload(value: Value) -> Provider {
    normalize_provider_value(value)
}

fn default_config() -> AppConfig {
    AppConfig {
        version: APP_VERSION.to_string(),
        active_provider: None,
        gateway_api_key: None,
        providers: Vec::new(),
        settings: Settings::default(),
    }
}

fn normalize_config_value(value: Value) -> Result<AppConfig, String> {
    let source = value
        .get("config")
        .filter(|item| item.is_object())
        .cloned()
        .unwrap_or(value);
    let source_object = source
        .as_object()
        .ok_or_else(|| "Config file must be a JSON object".to_string())?;

    let mut config = default_config();
    if let Some(version) = string_field(source_object, "version") {
        config.version = version;
    }
    if let Some(key) = string_field(source_object, "gatewayApiKey") {
        if !key.is_empty() {
            config.gateway_api_key = Some(key);
        }
    }

    if let Some(settings) = source_object.get("settings").and_then(Value::as_object) {
        config.settings = normalize_settings(settings);
    }

    let providers_value = match source_object.get("providers") {
        Some(value) => value
            .as_array()
            .cloned()
            .ok_or_else(|| "providers must be an array".to_string())?,
        None => Vec::new(),
    };
    let mut providers = Vec::new();
    let mut seen = BTreeSet::new();
    for provider_value in providers_value {
        if !provider_value.is_object() {
            continue;
        }
        let mut provider = normalize_provider_value(provider_value);
        while seen.contains(&provider.id) {
            provider.id = format!("{}-{}", provider.id, unique_suffix());
        }
        seen.insert(provider.id.clone());
        providers.push(provider);
    }
    config.providers = providers;

    let provider_ids = provider_ids(&config.providers);
    let active_provider = string_field(source_object, "activeProvider");
    config.active_provider = if let Some(active_provider) = active_provider {
        if provider_ids.contains(&active_provider) {
            Some(active_provider)
        } else {
            config.providers.first().map(|provider| provider.id.clone())
        }
    } else {
        config.providers.first().map(|provider| provider.id.clone())
    };

    Ok(config)
}

fn normalize_settings(settings: &Map<String, Value>) -> Settings {
    let mut normalized = Settings::default();
    if let Some(value) = string_field(settings, "theme") {
        normalized.theme = value;
    }
    if let Some(value) = string_field(settings, "language") {
        normalized.language = value;
    }
    if let Some(value) = u16_field(settings, "proxyPort") {
        normalized.proxy_port = value;
    }
    if let Some(value) = u16_field(settings, "adminPort") {
        normalized.admin_port = value;
    }
    if let Some(value) = bool_field(settings, "autoStart") {
        normalized.auto_start = value;
    }
    normalized.expose_all_provider_models = false;
    if let Some(value) = string_field(settings, "updateUrl") {
        if !value.is_empty() {
            normalized.update_url = value;
        }
    }
    if let Some(value) = string_field(settings, "upstreamProxy") {
        normalized.upstream_proxy = value;
    }
    if let Some(value) = bool_field(settings, "upstreamProxyEnabled") {
        normalized.upstream_proxy_enabled = value;
    }
    normalized
}

fn normalize_provider_value(value: Value) -> Provider {
    let object = value.as_object().cloned().unwrap_or_default();
    let raw_id = string_field(&object, "id").unwrap_or_default();
    let id = sanitize_provider_id(&raw_id);
    let raw_models = object
        .get("models")
        .and_then(Value::as_object)
        .map(string_map_from_object)
        .unwrap_or_default();
    let extra_headers = object
        .get("extraHeaders")
        .and_then(Value::as_object)
        .map(string_map_from_object)
        .unwrap_or_default();
    let mut extra = BTreeMap::new();
    for (key, value) in object.iter() {
        if !provider_known_field(&key) {
            extra.insert(key.clone(), value.clone());
        }
    }

    Provider {
        id: if id.is_empty() { unique_suffix() } else { id },
        name: string_field(&object, "name").unwrap_or_else(|| "Unnamed Provider".to_string()),
        base_url: string_field(&object, "baseUrl").unwrap_or_default(),
        auth_scheme: string_field(&object, "authScheme").unwrap_or_else(|| "bearer".to_string()),
        api_format: string_field(&object, "apiFormat").unwrap_or_else(|| "anthropic".to_string()),
        api_key: string_field(&object, "apiKey").unwrap_or_default(),
        models: normalize_model_mappings(Some(&raw_models)),
        extra_headers,
        model_capabilities: object
            .get("modelCapabilities")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new())),
        request_options: object
            .get("requestOptions")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new())),
        is_builtin: object
            .get("isBuiltin")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        sort_index: object
            .get("sortIndex")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(0),
        extra,
    }
}

fn merge_provider_value(current: Provider, data: Value) -> Provider {
    let mut incoming = normalize_provider_value(data.clone());
    incoming.id = current.id;
    incoming.sort_index = current.sort_index;
    incoming.is_builtin = current.is_builtin;

    if data
        .get("apiKey")
        .and_then(Value::as_str)
        .unwrap_or("")
        .is_empty()
    {
        incoming.api_key = current.api_key;
    }
    if !data.get("extraHeaders").is_some_and(|value| {
        value
            .as_object()
            .map(|object| !object.is_empty())
            .unwrap_or(false)
    }) {
        incoming.extra_headers = current.extra_headers;
    }
    if data.get("modelCapabilities").is_none() {
        incoming.model_capabilities = current.model_capabilities;
    }
    if data.get("requestOptions").is_none() {
        incoming.request_options = current.request_options;
    }
    if let Some(models) = data.get("models").and_then(Value::as_object) {
        let mut merged = current.models;
        for (key, value) in string_map_from_object(models) {
            merged.insert(key, value);
        }
        incoming.models = normalize_model_mappings(Some(&merged));
    }
    incoming
}

fn provider_with_legacy_aliases(mut provider: Provider) -> Provider {
    provider.models = model_mappings_with_legacy_aliases(&provider.models);
    provider
}

fn builtin_preset_values() -> Vec<Value> {
    vec![
        json!({
            "id": "deepseek",
            "name": "DeepSeek",
            "baseUrl": "https://api.deepseek.com/anthropic",
            "authScheme": "bearer",
            "apiFormat": "anthropic",
            "models": {
                "sonnet": "deepseek-v4-pro",
                "haiku": "deepseek-v4-flash",
                "opus": "deepseek-v4-pro",
                "default": "deepseek-v4-pro"
            },
            "modelOptions": {
                "deepseek_1m": {
                    "label": "解锁 1M 上下文",
                    "description": "用于 Claude Code/长上下文场景。开启后 Sonnet、Opus 和默认模型使用 deepseek-v4-pro[1m]，Haiku/Flash 也会标记 1M 能力。",
                    "models": {
                        "sonnet": "deepseek-v4-pro[1m]",
                        "haiku": "deepseek-v4-flash",
                        "opus": "deepseek-v4-pro[1m]",
                        "default": "deepseek-v4-pro[1m]"
                    },
                    "modelCapabilities": {
                        "deepseek-v4-pro[1m]": {"supports1m": true},
                        "deepseek-v4-flash": {"supports1m": true}
                    }
                }
            },
            "requestOptions": {},
            "requestOptionPresets": {
                "deepseek_max_effort": {
                    "label": "DeepSeek Max 思维",
                    "description": "Low：更快更省，适合简单任务。\nMedium：速度和效果平衡，适合日常使用。\nHigh：更认真思考，适合复杂代码和排错。\n勾选后：本工具会按 DeepSeek Max 转发；未勾选则使用 Claude 当前默认配置。",
                    "requestOptions": {
                        "anthropic": {
                            "thinking": {"type": "enabled"},
                            "output_config": {"effort": "max"}
                        }
                    }
                }
            },
            "extraHeaders": {"x-api-key": "{apiKey}"},
            "isBuiltin": true
        }),
        json!({
            "id": "third-party",
            "name": "第三方模型",
            "baseUrl": "",
            "authScheme": "bearer",
            "apiFormat": "anthropic",
            "models": {
                "sonnet": "",
                "haiku": "",
                "opus": "",
                "default": ""
            },
            "modelOptions": {
                "third_party_1m": {
                    "label": "解锁 1M 上下文",
                    "description": "用于 Claude Code/长上下文场景。开启后 Sonnet、Opus 和默认模型使用支持 1M 上下文的模型。",
                    "models": {
                        "sonnet": "",
                        "haiku": "",
                        "opus": "",
                        "default": ""
                    },
                    "modelCapabilities": {}
                }
            },
            "requestOptions": {},
            "requestOptionPresets": {
                "third_party_max_effort": {
                    "label": "Max 思维",
                    "description": "Low：更快更省，适合简单任务。\nMedium：速度和效果平衡，适合日常使用。\nHigh：更认真思考，适合复杂代码和排错。\n勾选后：本工具会按 Max 思维转发；未勾选则使用 Claude 当前默认配置。",
                    "requestOptions": {
                        "anthropic": {
                            "thinking": {"type": "enabled"},
                            "output_config": {"effort": "max"}
                        }
                    }
                }
            },
            "isBuiltin": true
        }),
        json!({
            "id": "kimi",
            "name": "Kimi (月之暗面)",
            "baseUrl": "https://api.moonshot.cn/anthropic",
            "authScheme": "bearer",
            "apiFormat": "anthropic",
            "models": {
                "sonnet": "kimi-k2.6",
                "haiku": "kimi-k2.6",
                "opus": "kimi-k2.6",
                "default": "kimi-k2.6"
            },
            "isBuiltin": true
        }),
        json!({
            "id": "kimi-code",
            "name": "Kimi Code",
            "baseUrl": "https://api.kimi.com/coding",
            "authScheme": "bearer",
            "apiFormat": "anthropic",
            "models": {
                "sonnet": "kimi-for-coding",
                "haiku": "kimi-for-coding",
                "opus": "kimi-for-coding",
                "default": "kimi-for-coding"
            },
            "isBuiltin": true
        }),
        json!({
            "id": "xiaomi-mimo-payg",
            "name": "Xiaomi MiMo (Pay for Token)",
            "baseUrl": "https://api.xiaomimimo.com/anthropic",
            "authScheme": "bearer",
            "apiFormat": "anthropic",
            "models": {
                "sonnet": "",
                "haiku": "",
                "opus": "",
                "default": "mimo-v2.5-pro"
            },
            "isBuiltin": true
        }),
        json!({
            "id": "xiaomi-mimo-token-plan",
            "name": "Xiaomi MiMo (Token Plan)",
            "baseUrl": "https://token-plan-cn.xiaomimimo.com/anthropic",
            "authScheme": "bearer",
            "apiFormat": "anthropic",
            "baseUrlOptions": [
                {
                    "label": "中国集群",
                    "value": "https://token-plan-cn.xiaomimimo.com/anthropic"
                },
                {
                    "label": "新加坡集群",
                    "value": "https://token-plan-sgp.xiaomimimo.com/anthropic"
                },
                {
                    "label": "欧洲集群",
                    "value": "https://token-plan-ams.xiaomimimo.com/anthropic"
                }
            ],
            "baseUrlHint": "请使用账号所属地区的 Base URL，若不清楚请访问 https://platform.xiaomimimo.com/console/plan-manage 获取专属Base URL。",
            "models": {
                "sonnet": "",
                "haiku": "",
                "opus": "",
                "default": "mimo-v2.5-pro"
            },
            "isBuiltin": true
        }),
        json!({
            "id": "zhipu",
            "name": "智谱 GLM",
            "baseUrl": "https://open.bigmodel.cn/api/anthropic",
            "authScheme": "x-api-key",
            "apiFormat": "anthropic",
            "models": {
                "sonnet": "glm-5.1",
                "haiku": "glm-4.7",
                "opus": "glm-5.1",
                "default": "glm-5.1"
            },
            "isBuiltin": true
        }),
        json!({
            "id": "bailian",
            "name": "阿里云百炼",
            "baseUrl": "https://dashscope.aliyuncs.com/apps/anthropic",
            "authScheme": "x-api-key",
            "apiFormat": "anthropic",
            "models": {
                "sonnet": "qwen3.6-plus",
                "haiku": "qwen3.6-flash",
                "opus": "qwen3.6-max-preview",
                "default": "qwen3.6-plus"
            },
            "modelOptions": {
                "qwen_1m": {
                    "label": "开启千问 1M 上下文",
                    "description": "阿里云文档确认 qwen3.6-plus / qwen3.6-flash 支持 1M。勾选后会把 1M 能力写入 Claude 桌面版；不勾选则按普通上下文显示。",
                    "modelCapabilities": {
                        "qwen3.6-plus": {"supports1m": true},
                        "qwen3.6-flash": {"supports1m": true}
                    }
                }
            },
            "modelCapabilities": {},
            "requestOptions": {},
            "isBuiltin": true
        }),
        json!({
            "id": "bailian-token-plan",
            "name": "阿里云百炼 (Token Plan)",
            "baseUrl": "https://token-plan.cn-beijing.maas.aliyuncs.com/apps/anthropic",
            "authScheme": "bearer",
            "apiFormat": "anthropic",
            "models": {
                "sonnet": "",
                "haiku": "",
                "opus": "",
                "default": "qwen3.6-plus"
            },
            "isBuiltin": true
        }),
    ]
}

fn provider_known_field(key: &str) -> bool {
    matches!(
        key,
        "id" | "name"
            | "baseUrl"
            | "authScheme"
            | "apiFormat"
            | "apiKey"
            | "models"
            | "extraHeaders"
            | "modelCapabilities"
            | "requestOptions"
            | "isBuiltin"
            | "sortIndex"
    )
}

fn string_map_from_object(object: &Map<String, Value>) -> BTreeMap<String, String> {
    object
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                value
                    .as_str()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            )
        })
        .collect()
}

fn string_field(object: &Map<String, Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
}

fn bool_field(object: &Map<String, Value>, key: &str) -> Option<bool> {
    object.get(key).and_then(Value::as_bool)
}

fn u16_field(object: &Map<String, Value>, key: &str) -> Option<u16> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
}

fn sanitize_provider_id(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric() || matches!(ch, '-' | '_'))
        .take(64)
        .collect()
}

fn sanitize_reason(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric() || matches!(ch, '-' | '_'))
        .take(32)
        .collect()
}

fn provider_ids(providers: &[Provider]) -> BTreeSet<String> {
    providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect()
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| format!("Failed to create directory: {error}"))
}

fn backup_info(path: &Path) -> Result<BackupInfo, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("Failed to stat backup: {error}"))?;
    Ok(BackupInfo {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config-backup.json")
            .to_string(),
        size: metadata.len(),
        created_at: now_millis().to_string(),
    })
}

fn unique_suffix() -> String {
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{counter:04x}")
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn random_hex(byte_count: usize) -> Result<String, String> {
    let mut bytes = vec![0_u8; byte_count];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("Failed to generate random key: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> ConfigStore {
        let dir =
            env::temp_dir().join(format!("ccds-tauri-config-test-{name}-{}", unique_suffix()));
        ConfigStore::for_dir(dir)
    }

    #[test]
    fn load_missing_config_returns_defaults() {
        let store = temp_store("missing");
        let config = store.load_config().expect("default config");

        assert_eq!(config.version, APP_VERSION);
        assert_eq!(config.providers.len(), 0);
        assert_eq!(config.settings.proxy_port, 18080);
    }

    #[test]
    fn add_provider_sets_first_provider_active_and_normalizes_models() {
        let store = temp_store("add");
        let provider = store
            .add_provider(json!({
                "id": "deepseek",
                "name": "DeepSeek",
                "baseUrl": "https://api.deepseek.com/anthropic",
                "apiKey": "secret",
                "models": {"sonnet": "deepseek-v4-pro"}
            }))
            .expect("provider added");
        let config = store.load_config().expect("saved config");

        assert_eq!(provider.models["sonnet"], "deepseek-v4-pro");
        assert_eq!(config.active_provider.as_deref(), Some("deepseek"));
        assert_eq!(config.providers[0].sort_index, 0);
    }

    #[test]
    fn update_settings_merges_known_fields_and_keeps_hidden_model_exposure_off() {
        let store = temp_store("settings");
        let updated = store
            .update_settings(json!({
                "proxyPort": 19080,
                "adminPort": 19081,
                "autoStart": true,
                "upstreamProxy": "http://127.0.0.1:7890",
                "upstreamProxyEnabled": true,
                "exposeAllProviderModels": true,
                "unknown": "ignored"
            }))
            .expect("settings updated");
        let saved = store.get_settings().expect("saved settings");

        assert_eq!(updated.proxy_port, 19080);
        assert_eq!(updated.admin_port, 19081);
        assert!(updated.auto_start);
        assert_eq!(saved.upstream_proxy, "http://127.0.0.1:7890");
        assert!(saved.upstream_proxy_enabled);
        assert!(!updated.expose_all_provider_models);
    }

    #[test]
    fn update_settings_keeps_default_update_url_when_blank() {
        let store = temp_store("settings-update-url");
        let updated = store
            .update_settings(json!({"updateUrl": ""}))
            .expect("settings updated");

        assert_eq!(updated.update_url, Settings::default().update_url);
    }

    #[test]
    fn update_provider_keeps_saved_secret_and_headers_when_blank() {
        let store = temp_store("update");
        let provider = store
            .add_provider(json!({
                "id": "deepseek",
                "name": "DeepSeek",
                "baseUrl": "https://api.deepseek.com/anthropic",
                "apiKey": "secret",
                "extraHeaders": {"x-api-key": "{apiKey}"},
                "models": {"sonnet": "deepseek-v4-pro", "haiku": "deepseek-v4-flash"}
            }))
            .expect("provider added");
        let updated = store
            .update_provider(
                &provider.id,
                json!({
                    "name": "DeepSeek",
                    "baseUrl": "https://api.deepseek.com/anthropic/v1/messages",
                    "apiKey": "",
                    "extraHeaders": {},
                    "models": {"sonnet": "deepseek-v4-pro"}
                }),
            )
            .expect("update result")
            .expect("provider exists");

        assert_eq!(updated.api_key, "secret");
        assert_eq!(updated.extra_headers["x-api-key"], "{apiKey}");
        assert_eq!(updated.models["haiku"], "deepseek-v4-flash");
    }

    #[test]
    fn delete_active_provider_selects_next_provider() {
        let store = temp_store("delete");
        store
            .add_provider(json!({"id": "a", "name": "A"}))
            .expect("first provider");
        store
            .add_provider(json!({"id": "b", "name": "B"}))
            .expect("second provider");

        assert!(store.delete_provider("a").expect("delete"));
        let config = store.load_config().expect("saved config");

        assert_eq!(config.active_provider.as_deref(), Some("b"));
        assert_eq!(config.providers[0].sort_index, 0);
    }

    #[test]
    fn set_active_provider_persists_selected_provider() {
        let store = temp_store("set-active");
        store
            .add_provider(json!({"id": "a", "name": "A"}))
            .expect("first provider");
        store
            .add_provider(json!({"id": "b", "name": "B", "models": {"default": "model-b"}}))
            .expect("second provider");

        let provider = store
            .set_active_provider("b")
            .expect("set active result")
            .expect("provider exists");
        let config = store.load_config().expect("saved config");

        assert_eq!(provider.id, "b");
        assert_eq!(provider.models["default"], "model-b");
        assert_eq!(config.active_provider.as_deref(), Some("b"));
        assert!(
            store
                .set_active_provider("missing")
                .expect("missing")
                .is_none()
        );
    }

    #[test]
    fn reorder_providers_persists_sort_index() {
        let store = temp_store("reorder");
        store
            .add_provider(json!({"id": "a", "name": "A"}))
            .expect("first provider");
        store
            .add_provider(json!({"id": "b", "name": "B"}))
            .expect("second provider");

        assert!(
            store
                .reorder_providers(vec!["b".to_string(), "a".to_string()])
                .expect("reorder")
        );
        let config = store.load_config().expect("saved config");

        assert_eq!(config.providers[0].id, "b");
        assert_eq!(config.providers[0].sort_index, 0);
        assert_eq!(config.providers[1].sort_index, 1);
    }

    #[test]
    fn import_config_creates_backup_and_sanitizes_duplicate_ids() {
        let store = temp_store("import");
        store
            .add_provider(json!({"id": "existing", "name": "Existing"}))
            .expect("seed provider");

        let result = store
            .import_config(json!({
                "providers": [
                    {"id": "bad\"><script>", "name": "A"},
                    {"id": "bad\"><script>", "name": "B"}
                ]
            }))
            .expect("import result");

        assert!(result.backup.name.ends_with(".json"));
        assert_eq!(result.config.providers.len(), 2);
        assert_ne!(result.config.providers[0].id, result.config.providers[1].id);
        assert!(!result.config.providers[0].id.contains('<'));
    }

    #[test]
    fn builtin_presets_include_expected_urls() {
        let presets = builtin_presets();
        let by_id = presets
            .iter()
            .map(|preset| (preset.id.as_str(), preset))
            .collect::<BTreeMap<_, _>>();
        let expected_urls = [
            ("deepseek", "https://api.deepseek.com/anthropic"),
            ("kimi", "https://api.moonshot.cn/anthropic"),
            ("kimi-code", "https://api.kimi.com/coding"),
            ("xiaomi-mimo-payg", "https://api.xiaomimimo.com/anthropic"),
            (
                "xiaomi-mimo-token-plan",
                "https://token-plan-cn.xiaomimimo.com/anthropic",
            ),
            ("zhipu", "https://open.bigmodel.cn/api/anthropic"),
            ("bailian", "https://dashscope.aliyuncs.com/apps/anthropic"),
            (
                "bailian-token-plan",
                "https://token-plan.cn-beijing.maas.aliyuncs.com/apps/anthropic",
            ),
        ];

        assert!(by_id.contains_key("third-party"));
        for (preset_id, base_url) in expected_urls {
            let preset = by_id[preset_id];
            assert_eq!(preset.base_url, base_url);
            assert_eq!(preset.api_format, "anthropic");
            assert!(!preset.models["default"].is_empty());
        }
        assert_eq!(by_id["deepseek"].models["default"], "deepseek-v4-pro");
        assert_eq!(
            by_id["xiaomi-mimo-token-plan"].models["default"],
            "mimo-v2.5-pro"
        );
        assert_eq!(
            by_id["xiaomi-mimo-token-plan"]
                .extra
                .get("baseUrlOptions")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(3)
        );
        let token_plan_json =
            serde_json::to_value(by_id["xiaomi-mimo-token-plan"]).expect("serialize preset");
        assert_eq!(token_plan_json["authScheme"], "bearer");
        assert_eq!(
            token_plan_json["baseUrlOptions"].as_array().map(Vec::len),
            Some(3)
        );
        assert!(by_id["third-party"].base_url.is_empty());
        assert!(
            by_id["third-party"]
                .extra
                .contains_key("requestOptionPresets")
        );
        let third_party_json =
            serde_json::to_value(by_id["third-party"]).expect("serialize third-party preset");
        assert!(third_party_json.get("requestOptionPresets").is_some());
        assert_eq!(by_id["bailian-token-plan"].auth_scheme, "bearer");
    }

    #[test]
    fn gateway_key_is_created_once_and_persisted() {
        let store = temp_store("gateway-key");
        let first = store.get_or_create_gateway_api_key().expect("gateway key");
        let second = store
            .get_or_create_gateway_api_key()
            .expect("same gateway key");
        let saved = store.load_config().expect("saved config");

        assert!(first.starts_with("ccds_"));
        assert_eq!(first, second);
        assert_eq!(saved.gateway_api_key.as_deref(), Some(first.as_str()));
    }
}
