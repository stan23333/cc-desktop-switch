use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::{Map, Value, json};

use crate::config::ConfigStore;
use crate::models::{BackupInfo, Provider};

const SUPPORTED_API_FORMATS: [&str; 2] = ["anthropic", ""];

#[derive(Debug, Clone)]
struct CcSwitchPaths {
    dir: PathBuf,
    db: PathBuf,
    legacy: PathBuf,
}

#[derive(Debug, Clone)]
struct RawRow {
    id: String,
    name: String,
    settings_config: Value,
    meta: Value,
    is_current: bool,
}

pub fn default_root() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| "Cannot determine home directory".to_string())?;
    Ok(PathBuf::from(home).join(".cc-switch"))
}

pub fn status() -> Result<Value, String> {
    status_for_root(&default_root()?)
}

pub fn read_providers_public() -> Result<Value, String> {
    let providers = read_providers(&default_root()?, false)?;
    let supported_count = providers
        .iter()
        .filter(|provider| provider.get("supported").and_then(Value::as_bool) == Some(true))
        .count();
    Ok(json!({
        "success": true,
        "providers": providers,
        "supportedCount": supported_count,
        "unsupportedCount": providers.len().saturating_sub(supported_count),
    }))
}

pub fn import_providers(ids: Option<Vec<String>>, set_default: bool) -> Result<Value, String> {
    import_providers_for_root(&default_root()?, ConfigStore::default()?, ids, set_default)
}

fn paths_for_root(root: &Path) -> CcSwitchPaths {
    CcSwitchPaths {
        dir: root.to_path_buf(),
        db: root.join("cc-switch.db"),
        legacy: root.join("config.json"),
    }
}

fn status_for_root(root: &Path) -> Result<Value, String> {
    let paths = paths_for_root(root);
    let db_exists = paths.db.exists();
    let legacy_exists = paths.legacy.exists();
    let mut provider_count = 0_usize;
    let mut supported_count = 0_usize;
    let mut unsupported_count = 0_usize;
    if db_exists || legacy_exists {
        if let Ok(providers) = read_providers(root, false) {
            provider_count = providers.len();
            supported_count = providers
                .iter()
                .filter(|provider| provider.get("supported").and_then(Value::as_bool) == Some(true))
                .count();
            unsupported_count = provider_count.saturating_sub(supported_count);
        }
    }
    Ok(json!({
        "found": db_exists || legacy_exists,
        "dir": paths.dir.display().to_string(),
        "dbExists": db_exists,
        "legacyConfigExists": legacy_exists,
        "providerCount": provider_count,
        "supportedCount": supported_count,
        "unsupportedCount": unsupported_count,
    }))
}

fn read_providers(root: &Path, include_secret: bool) -> Result<Vec<Value>, String> {
    let rows = raw_rows(root)?;
    Ok(rows
        .iter()
        .map(|row| candidate_from_row(row, include_secret))
        .collect())
}

fn raw_rows(root: &Path) -> Result<Vec<RawRow>, String> {
    let paths = paths_for_root(root);
    if paths.db.exists() {
        return read_sqlite_rows(&paths.db);
    }
    if paths.legacy.exists() {
        return read_legacy_rows(&paths.legacy);
    }
    Ok(Vec::new())
}

fn read_sqlite_rows(db_path: &Path) -> Result<Vec<RawRow>, String> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("读取 CC-Switch 数据库失败: {error}"))?;
    let mut statement = conn
        .prepare(
            r#"
            SELECT id, name, settings_config, meta, is_current, sort_index, created_at
            FROM providers
            WHERE app_type = 'claude'
            ORDER BY COALESCE(sort_index, 999999), COALESCE(created_at, 0), id
            "#,
        )
        .map_err(|error| format!("读取 CC-Switch 数据库失败: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            let settings_config: String = row.get(2)?;
            let meta: String = row.get(3)?;
            Ok(RawRow {
                id: row.get::<_, String>(0).unwrap_or_default(),
                name: row.get::<_, String>(1).unwrap_or_default(),
                settings_config: load_json_value(&settings_config, json!({})),
                meta: load_json_value(&meta, json!({})),
                is_current: row.get::<_, i64>(4).unwrap_or_default() != 0,
            })
        })
        .map_err(|error| format!("读取 CC-Switch 数据库失败: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取 CC-Switch 数据库失败: {error}"))
}

fn read_legacy_rows(config_path: &Path) -> Result<Vec<RawRow>, String> {
    let text = fs::read_to_string(config_path)
        .map_err(|error| format!("读取 CC-Switch 旧配置失败: {error}"))?;
    let data: Value = serde_json::from_str(&text)
        .map_err(|error| format!("读取 CC-Switch 旧配置失败: {error}"))?;
    let providers = data
        .get("providers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let current = data.get("current").and_then(Value::as_str).unwrap_or("");
    let mut rows = Vec::new();
    for (provider_id, provider) in providers {
        let Some(provider) = provider.as_object() else {
            continue;
        };
        rows.push(RawRow {
            id: provider_id.clone(),
            name: provider
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(provider_id.as_str())
                .to_string(),
            settings_config: provider
                .get("settingsConfig")
                .or_else(|| provider.get("settings_config"))
                .cloned()
                .unwrap_or_else(|| Value::Object(provider.clone())),
            meta: provider.get("meta").cloned().unwrap_or_else(|| json!({})),
            is_current: provider_id == current,
        });
    }
    Ok(rows)
}

fn candidate_from_row(row: &RawRow, include_secret: bool) -> Value {
    let settings_config = normalized_object(&row.settings_config);
    let meta = normalized_object(&row.meta);
    let env = settings_config
        .get("env")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let api_format = api_format(&meta, &settings_config);
    let base_url = normalize_base_url(string_from_map(&env, "ANTHROPIC_BASE_URL"));
    let api_key = string_from_map(&env, "ANTHROPIC_AUTH_TOKEN")
        .or_else(|| string_from_map(&env, "ANTHROPIC_API_KEY"))
        .unwrap_or_default();
    let mut supported = SUPPORTED_API_FORMATS.contains(&api_format.as_str());
    let mut reason = String::new();
    if !SUPPORTED_API_FORMATS.contains(&api_format.as_str()) {
        reason = unsupported_format_message(&api_format);
    } else if base_url.is_empty() {
        supported = false;
        reason = "没有发现 API 地址，可能是官方登录或空配置。".to_string();
    } else if is_local_proxy_url(&base_url) {
        supported = false;
        reason = "这是 CC-Switch 本机代理地址，不能作为上游 API 导入。".to_string();
    } else if api_key.is_empty() {
        supported = false;
        reason = "没有发现 API Key。".to_string();
    }
    let defaults = builtin_defaults(&row.name, &base_url);
    let mut provider = Map::new();
    provider.insert("id".to_string(), Value::String(row.id.clone()));
    provider.insert("name".to_string(), Value::String(row.name.clone()));
    provider.insert("current".to_string(), Value::Bool(row.is_current));
    provider.insert("apiFormat".to_string(), Value::String(api_format));
    provider.insert("baseUrl".to_string(), Value::String(base_url));
    provider.insert("hasApiKey".to_string(), Value::Bool(!api_key.is_empty()));
    provider.insert(
        "apiKeyPreview".to_string(),
        Value::String(mask_secret(&api_key)),
    );
    provider.insert("models".to_string(), models_from_env(&env));
    provider.insert(
        "authScheme".to_string(),
        Value::String(defaults.auth_scheme),
    );
    provider.insert("extraHeaders".to_string(), defaults.extra_headers);
    provider.insert("supported".to_string(), Value::Bool(supported));
    provider.insert("reason".to_string(), Value::String(reason));
    if include_secret {
        provider.insert("apiKey".to_string(), Value::String(api_key));
    }
    Value::Object(provider)
}

fn import_providers_for_root(
    root: &Path,
    store: ConfigStore,
    ids: Option<Vec<String>>,
    set_default: bool,
) -> Result<Value, String> {
    let candidates = read_providers(root, true)?;
    let selected_ids = ids.unwrap_or_else(|| {
        candidates
            .iter()
            .filter(|item| item.get("supported").and_then(Value::as_bool) == Some(true))
            .filter_map(|item| {
                item.get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect()
    });
    let selected_ids = selected_ids.into_iter().collect::<BTreeSet<_>>();
    let config = store.load_config()?;
    let mut existing = existing_keys(&config.providers);
    let mut supported_to_import = Vec::new();
    let mut skipped = Vec::new();
    let mut unsupported = Vec::new();

    for candidate in candidates {
        let candidate_id = candidate
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if !selected_ids.contains(&candidate_id) {
            continue;
        }
        let name = candidate
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if candidate.get("supported").and_then(Value::as_bool) != Some(true) {
            unsupported.push(json!({
                "id": candidate_id,
                "name": name,
                "reason": candidate.get("reason").cloned().unwrap_or_else(|| Value::String(String::new())),
            }));
            continue;
        }
        if existing.source.contains(&candidate_id) {
            skipped.push(json!({
                "id": candidate_id,
                "name": name,
                "reason": "已导入过这个 CC-Switch 配置",
            }));
            continue;
        }
        let base_url = normalize_base_url(
            candidate
                .get("baseUrl")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        );
        let key = (name.trim().to_lowercase(), base_url.to_lowercase());
        let mut candidate = candidate;
        if existing.provider.contains(&key) {
            let import_name = dedupe_import_name(&name, &mut existing.names);
            if let Some(object) = candidate.as_object_mut() {
                object.insert("importName".to_string(), Value::String(import_name));
            }
        }
        supported_to_import.push(candidate);
    }

    let mut imported = Vec::new();
    let mut backup: Option<BackupInfo> = None;
    if !supported_to_import.is_empty() {
        backup = Some(store.create_backup("before-ccswitch-import")?);
        for candidate in supported_to_import {
            let provider_payload = to_ccds_provider(&candidate);
            let provider = store.add_provider(provider_payload)?;
            imported.push(json!({
                "id": provider.id,
                "name": provider.name,
                "baseUrl": provider.base_url,
            }));
            existing.provider.insert((
                provider.name.trim().to_lowercase(),
                normalize_base_url(Some(provider.base_url.clone())).to_lowercase(),
            ));
            existing.names.insert(provider.name.trim().to_lowercase());
            if let Some(source_id) = provider
                .extra
                .get("source")
                .and_then(Value::as_object)
                .and_then(|source| source.get("id"))
                .and_then(Value::as_str)
            {
                existing.source.insert(source_id.to_string());
            }
        }
        if set_default {
            if let Some(first_id) = imported
                .first()
                .and_then(|item| item.get("id"))
                .and_then(Value::as_str)
            {
                let _ = store.set_active_provider(first_id)?;
            }
        }
    }

    Ok(json!({
        "success": true,
        "message": format!("已导入 {} 个 CC-Switch 配置", imported.len()),
        "imported": imported,
        "skipped": skipped,
        "unsupported": unsupported,
        "backup": backup,
    }))
}

fn to_ccds_provider(candidate: &Value) -> Value {
    let id = candidate.get("id").and_then(Value::as_str).unwrap_or("");
    json!({
        "id": format!("ccswitch-{}", safe_id(id)),
        "name": candidate
            .get("importName")
            .or_else(|| candidate.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("CC-Switch Provider"),
        "baseUrl": candidate.get("baseUrl").cloned().unwrap_or_else(|| Value::String(String::new())),
        "apiKey": candidate.get("apiKey").cloned().unwrap_or_else(|| Value::String(String::new())),
        "authScheme": candidate.get("authScheme").cloned().unwrap_or_else(|| Value::String("bearer".to_string())),
        "apiFormat": "anthropic",
        "models": candidate.get("models").cloned().unwrap_or_else(|| json!({})),
        "extraHeaders": candidate.get("extraHeaders").cloned().unwrap_or_else(|| json!({})),
        "isBuiltin": false,
        "source": {
            "type": "cc-switch",
            "id": id,
        },
    })
}

#[derive(Default)]
struct ExistingKeys {
    provider: BTreeSet<(String, String)>,
    source: BTreeSet<String>,
    names: BTreeSet<String>,
}

fn existing_keys(providers: &[Provider]) -> ExistingKeys {
    let mut keys = ExistingKeys::default();
    for provider in providers {
        let name = provider.name.trim().to_lowercase();
        keys.provider.insert((
            name.clone(),
            normalize_base_url(Some(provider.base_url.clone())).to_lowercase(),
        ));
        if !name.is_empty() {
            keys.names.insert(name);
        }
        if let Some(source_id) = provider
            .extra
            .get("source")
            .and_then(Value::as_object)
            .filter(|source| source.get("type").and_then(Value::as_str) == Some("cc-switch"))
            .and_then(|source| source.get("id"))
            .and_then(Value::as_str)
        {
            keys.source.insert(source_id.to_string());
        }
    }
    keys
}

fn dedupe_import_name(name: &str, existing_names: &mut BTreeSet<String>) -> String {
    let base = format!("{name} CC Switch 导入");
    let mut candidate = base.clone();
    let mut index = 2;
    while existing_names.contains(&candidate.to_lowercase()) {
        candidate = format!("{base} {index}");
        index += 1;
    }
    existing_names.insert(candidate.to_lowercase());
    candidate
}

struct BuiltinDefaults {
    auth_scheme: String,
    extra_headers: Value,
}

fn builtin_defaults(name: &str, base_url: &str) -> BuiltinDefaults {
    let probe = format!("{name} {base_url}").to_lowercase();
    if probe.contains("deepseek") {
        return BuiltinDefaults {
            auth_scheme: "bearer".to_string(),
            extra_headers: json!({"x-api-key": "{apiKey}"}),
        };
    }
    if probe.contains("bigmodel")
        || probe.contains("zhipu")
        || probe.contains("glm")
        || probe.contains("dashscope")
        || probe.contains("bailian")
        || probe.contains("aliyun")
    {
        return BuiltinDefaults {
            auth_scheme: "x-api-key".to_string(),
            extra_headers: json!({}),
        };
    }
    BuiltinDefaults {
        auth_scheme: "bearer".to_string(),
        extra_headers: json!({}),
    }
}

fn models_from_env(env: &Map<String, Value>) -> Value {
    let default_model = string_from_map(env, "ANTHROPIC_MODEL").unwrap_or_default();
    let sonnet = string_from_map(env, "ANTHROPIC_DEFAULT_SONNET_MODEL")
        .unwrap_or_else(|| default_model.clone());
    let haiku = string_from_map(env, "ANTHROPIC_DEFAULT_HAIKU_MODEL")
        .unwrap_or_else(|| default_model.clone());
    let opus = string_from_map(env, "ANTHROPIC_DEFAULT_OPUS_MODEL")
        .unwrap_or_else(|| default_model.clone());
    let default = first_non_empty(&[&default_model, &sonnet, &opus, &haiku]);
    json!({
        "sonnet": if sonnet.is_empty() { default.clone() } else { sonnet },
        "haiku": if haiku.is_empty() { default.clone() } else { haiku },
        "opus": if opus.is_empty() { default.clone() } else { opus },
        "default": default,
    })
}

fn first_non_empty(values: &[&String]) -> String {
    values
        .iter()
        .find(|value| !value.is_empty())
        .map(|value| (*value).clone())
        .unwrap_or_default()
}

fn api_format(meta: &Map<String, Value>, settings_config: &Map<String, Value>) -> String {
    for key in ["apiFormat", "api_format"] {
        if let Some(value) = string_from_map(meta, key).filter(|value| !value.is_empty()) {
            return value.to_lowercase();
        }
    }
    if settings_config
        .get("env")
        .and_then(Value::as_object)
        .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
        .is_some()
    {
        return "anthropic".to_string();
    }
    "anthropic".to_string()
}

fn unsupported_format_message(api_format: &str) -> String {
    match api_format {
        "openai_chat" => "OpenAI Chat 格式本轮不自动导入，避免转换兼容风险。".to_string(),
        "openai_responses" => "OpenAI Responses 格式暂未适配，暂不自动导入。".to_string(),
        value => format!("{value} 格式暂不支持自动导入。"),
    }
}

fn is_local_proxy_url(url: &str) -> bool {
    let lower = normalize_base_url(Some(url.to_string())).to_lowercase();
    let local_hosts = ["http://127.0.0.1:", "http://localhost:", "http://[::1]:"];
    if !local_hosts.iter().any(|prefix| lower.starts_with(prefix)) {
        return false;
    }
    lower.ends_with(":15721")
        || lower.contains(":15721/")
        || lower.ends_with(":18080")
        || lower.contains(":18080/")
}

fn normalize_base_url(url: Option<String>) -> String {
    url.unwrap_or_default()
        .trim()
        .trim_end_matches('/')
        .to_string()
}

fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else if value.len() <= 8 {
        "******".to_string()
    } else {
        format!("{}...{}", &value[..4], &value[value.len() - 4..])
    }
}

fn safe_id(value: &str) -> String {
    let result = value
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric() || matches!(ch, '-' | '_'))
        .take(56)
        .collect::<String>();
    if result.is_empty() {
        "provider".to_string()
    } else {
        result
    }
}

fn load_json_value(value: &str, default: Value) -> Value {
    if value.trim().is_empty() {
        return default;
    }
    serde_json::from_str(value).unwrap_or(default)
}

fn normalized_object(value: &Value) -> Map<String, Value> {
    match value {
        Value::Object(object) => object.clone(),
        Value::String(text) => load_json_value(text, json!({}))
            .as_object()
            .cloned()
            .unwrap_or_default(),
        _ => Map::new(),
    }
}

fn string_from_map(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use rusqlite::params;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ccds-tauri-ccswitch-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).expect("temp root");
        root
    }

    fn init_db(root: &Path) {
        let conn = Connection::open(root.join("cc-switch.db")).expect("db");
        conn.execute_batch(
            r#"
            CREATE TABLE providers (
                id TEXT PRIMARY KEY,
                app_type TEXT,
                name TEXT,
                settings_config TEXT,
                meta TEXT,
                is_current INTEGER,
                sort_index INTEGER,
                created_at INTEGER
            )
            "#,
        )
        .expect("schema");
    }

    fn insert_provider(root: &Path, id: &str, name: &str, env: Value, meta: Value) {
        let conn = Connection::open(root.join("cc-switch.db")).expect("db");
        conn.execute(
            r#"
            INSERT INTO providers
                (id, app_type, name, settings_config, meta, is_current, sort_index, created_at)
            VALUES (?1, 'claude', ?2, ?3, ?4, 0, 0, 1)
            "#,
            params![id, name, json!({"env": env}).to_string(), meta.to_string()],
        )
        .expect("insert");
    }

    #[test]
    fn preview_masks_secret_and_skips_local_proxy() {
        let root = temp_root("preview");
        init_db(&root);
        insert_provider(
            &root,
            "deepseek",
            "DeepSeek",
            json!({
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "sk-test-secret",
                "ANTHROPIC_MODEL": "deepseek-v4-pro",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "deepseek-v4-flash"
            }),
            json!({"apiFormat": "anthropic"}),
        );
        insert_provider(
            &root,
            "local",
            "Local Proxy",
            json!({
                "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721",
                "ANTHROPIC_AUTH_TOKEN": "sk-test-secret",
                "ANTHROPIC_MODEL": "local-model"
            }),
            json!({"apiFormat": "anthropic"}),
        );

        let providers = read_providers(&root, false).expect("providers");
        let by_id = providers
            .iter()
            .map(|provider| (provider["id"].as_str().unwrap(), provider))
            .collect::<BTreeMap<_, _>>();

        assert_eq!(by_id["deepseek"]["apiKeyPreview"], "sk-t...cret");
        assert!(by_id["deepseek"]["apiKey"].is_null());
        assert_eq!(by_id["deepseek"]["models"]["default"], "deepseek-v4-pro");
        assert_eq!(by_id["deepseek"]["models"]["haiku"], "deepseek-v4-flash");
        assert_eq!(by_id["deepseek"]["supported"], true);
        assert_eq!(by_id["local"]["supported"], false);
        assert!(
            by_id["local"]["reason"]
                .as_str()
                .unwrap()
                .contains("本机代理地址")
        );
    }

    #[test]
    fn import_adds_supported_only_and_skips_duplicate_source() {
        let root = temp_root("import");
        init_db(&root);
        insert_provider(
            &root,
            "deepseek",
            "DeepSeek",
            json!({
                "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "sk-test-secret",
                "ANTHROPIC_MODEL": "deepseek-v4-pro"
            }),
            json!({"apiFormat": "anthropic"}),
        );
        insert_provider(
            &root,
            "openai",
            "OpenAI Like",
            json!({
                "ANTHROPIC_BASE_URL": "https://api.example.com/v1",
                "ANTHROPIC_AUTH_TOKEN": "sk-openai-secret",
                "ANTHROPIC_MODEL": "example-model"
            }),
            json!({"apiFormat": "openai_responses"}),
        );
        let store = ConfigStore::for_dir(temp_root("store"));

        let first = import_providers_for_root(&root, store.clone(), None, false).expect("import");
        let second = import_providers_for_root(&root, store.clone(), None, false).expect("import");
        let config = store.load_config().expect("config");

        assert_eq!(first["imported"].as_array().unwrap().len(), 1);
        assert_eq!(first["unsupported"].as_array().unwrap().len(), 0);
        assert_eq!(second["imported"].as_array().unwrap().len(), 0);
        assert_eq!(
            second["skipped"][0]["reason"],
            "已导入过这个 CC-Switch 配置"
        );
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].name, "DeepSeek");
        assert_eq!(config.providers[0].api_format, "anthropic");
        assert_eq!(config.providers[0].extra_headers["x-api-key"], "{apiKey}");
        assert_eq!(
            config.providers[0].extra["source"],
            json!({"type": "cc-switch", "id": "deepseek"})
        );
    }
}
