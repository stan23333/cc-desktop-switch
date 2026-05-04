use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value};

use crate::models::{DesktopApplyResult, DesktopConfigSources, DesktopConfigStatus};

use super::{
    CCDS_MARKER, DEFAULT_INFERENCE_MODELS, DESKTOP_CONFIG_NAMES, default_status, failure,
    safe_config_value, success,
};

const MAC_BUNDLE: &str = "com.anthropic.claudefordesktop";
const CONFIG_LIBRARY: &str = "configLibrary";

struct MacPaths {
    config_json: PathBuf,
}

impl MacPaths {
    fn default() -> Result<Self, String> {
        let home =
            env::var_os("HOME").ok_or_else(|| "Cannot determine home directory".to_string())?;
        Ok(Self {
            config_json: PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Claude-3p")
                .join("claude_desktop_config.json"),
        })
    }

    fn library_dir(&self) -> PathBuf {
        self.config_json
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(CONFIG_LIBRARY)
    }

    fn library_meta_path(&self) -> PathBuf {
        self.library_dir().join("_meta.json")
    }

    fn library_entry_path(&self, entry_id: &str) -> PathBuf {
        self.library_dir().join(format!("{entry_id}.json"))
    }
}

pub fn get_config_status() -> DesktopConfigStatus {
    let plist_status = get_plist_config_status();
    let Ok(paths) = MacPaths::default() else {
        return default_status("Cannot determine home directory");
    };
    let json_status = get_json_config_status(&paths);
    let library_status = get_library_config_status(&paths);
    merge_config_statuses(plist_status, json_status, library_status)
}

pub fn apply_config(
    base_url: &str,
    gateway_api_key: &str,
    inference_models: &str,
    auth_scheme: &str,
    gateway_headers: &str,
) -> DesktopApplyResult {
    let plist_result = apply_plist_config(
        base_url,
        gateway_api_key,
        inference_models,
        auth_scheme,
        gateway_headers,
    );
    let Ok(paths) = MacPaths::default() else {
        return failure("Cannot determine home directory");
    };
    let json_result = apply_json_config(
        &paths,
        base_url,
        gateway_api_key,
        inference_models,
        auth_scheme,
        gateway_headers,
    );
    let library_result = apply_library_config(
        &paths,
        base_url,
        gateway_api_key,
        inference_models,
        auth_scheme,
        gateway_headers,
    );

    if plist_result.success && json_result.success && library_result.success {
        return success("macOS Desktop 3P 配置已应用");
    }

    let mut failures = Vec::new();
    if !plist_result.success {
        failures.push(format!("plist: {}", plist_result.message));
    }
    if !json_result.success {
        failures.push(format!("json: {}", json_result.message));
    }
    if !library_result.success {
        failures.push(format!("configLibrary: {}", library_result.message));
    }
    failure(format!("macOS 配置部分写入失败: {}", failures.join("; ")))
}

pub fn clear_config() -> DesktopApplyResult {
    let plist_result = clear_plist_config();
    let Ok(paths) = MacPaths::default() else {
        return failure("Cannot determine home directory");
    };
    let json_result = clear_json_config(&paths);
    let library_result = clear_library_config(&paths);
    if plist_result.success && json_result.success && library_result.success {
        return success("macOS Desktop 3P 配置已清除");
    }

    let mut failures = Vec::new();
    if !plist_result.success {
        failures.push(format!("plist: {}", plist_result.message));
    }
    if !json_result.success {
        failures.push(format!("json: {}", json_result.message));
    }
    if !library_result.success {
        failures.push(format!("configLibrary: {}", library_result.message));
    }
    failure(format!("macOS 配置部分清除失败: {}", failures.join("; ")))
}

pub fn restart_claude_desktop() -> DesktopApplyResult {
    let script = format!(
        r#"if /usr/bin/osascript -e 'application id "{MAC_BUNDLE}" is running' | /usr/bin/grep -qi true; then /usr/bin/osascript -e 'tell application id "{MAC_BUNDLE}" to quit' >/dev/null 2>&1 || true; for i in 1 2 3 4 5 6 7 8 9 10; do if ! /usr/bin/osascript -e 'application id "{MAC_BUNDLE}" is running' | /usr/bin/grep -qi true; then break; fi; sleep 0.2; done; fi; /usr/bin/open -b {MAC_BUNDLE}"#
    );
    match Command::new("/bin/sh").args(["-c", &script]).spawn() {
        Ok(_) => success("已请求打开或重启 Claude Desktop"),
        Err(error) => failure(format!("重启 Claude Desktop 失败: {error}")),
    }
}

fn get_plist_config_status() -> DesktopConfigStatus {
    let mut keys = BTreeMap::new();
    for name in DESKTOP_CONFIG_NAMES {
        let args = vec!["read".to_string(), MAC_BUNDLE.to_string(), name.to_string()];
        let (ok, output) = run_defaults(&args);
        if ok {
            keys.insert(
                name.to_string(),
                safe_config_value(name, &Value::String(output)),
            );
        }
    }
    let marker_args = vec![
        "read".to_string(),
        MAC_BUNDLE.to_string(),
        CCDS_MARKER.to_string(),
    ];
    let (marker_ok, marker) = run_defaults(&marker_args);
    let marked = marker_ok && marker == "true";
    DesktopConfigStatus {
        configured: keys
            .get("inferenceProvider")
            .is_some_and(|value| value == "gateway")
            && marked,
        keys,
        message: String::new(),
        sources: DesktopConfigSources::default(),
    }
}

fn apply_plist_config(
    base_url: &str,
    gateway_api_key: &str,
    inference_models: &str,
    auth_scheme: &str,
    gateway_headers: &str,
) -> DesktopApplyResult {
    let mut expected = BTreeMap::new();
    let mut failures = Vec::new();
    for name in DESKTOP_CONFIG_NAMES {
        let (value, is_int) = desktop_value(
            name,
            base_url,
            gateway_api_key,
            inference_models,
            auth_scheme,
            gateway_headers,
        );
        expected.insert(name.to_string(), value.clone());
        let mut args = vec![
            "write".to_string(),
            MAC_BUNDLE.to_string(),
            name.to_string(),
            if is_int {
                "-int".to_string()
            } else {
                "-string".to_string()
            },
            value,
        ];
        let (ok, output) = run_defaults(&args);
        args.clear();
        if !ok {
            let detail = if name.to_lowercase().contains("key") {
                "defaults write failed".to_string()
            } else if output.is_empty() {
                "defaults write failed".to_string()
            } else {
                output
            };
            failures.push(format!("{name}: {detail}"));
        }
    }

    let marker_args = vec![
        "write".to_string(),
        MAC_BUNDLE.to_string(),
        CCDS_MARKER.to_string(),
        "-string".to_string(),
        "true".to_string(),
    ];
    let (ok, output) = run_defaults(&marker_args);
    if !ok {
        failures.push(format!(
            "{CCDS_MARKER}: {}",
            if output.is_empty() {
                "defaults write failed"
            } else {
                output.as_str()
            }
        ));
    }
    expected.insert(CCDS_MARKER.to_string(), "true".to_string());

    if !failures.is_empty() {
        return failure(format!("macOS 配置写入失败: {}", failures.join("; ")));
    }

    for (name, value) in expected {
        let args = vec!["read".to_string(), MAC_BUNDLE.to_string(), name.clone()];
        let (ok, output) = run_defaults(&args);
        if !ok {
            failures.push(format!("{name}: readback failed"));
            continue;
        }
        if output != value {
            failures.push(format!("{name}: readback mismatch"));
        }
    }

    if failures.is_empty() {
        success("macOS Desktop 3P 配置已应用")
    } else {
        failure(format!("macOS 配置写入校验失败: {}", failures.join("; ")))
    }
}

fn clear_plist_config() -> DesktopApplyResult {
    let mut count = 0;
    for name in DESKTOP_CONFIG_NAMES
        .into_iter()
        .chain(std::iter::once(CCDS_MARKER))
    {
        let args = vec![
            "delete".to_string(),
            MAC_BUNDLE.to_string(),
            name.to_string(),
        ];
        let (ok, _) = run_defaults(&args);
        if ok {
            count += 1;
        }
    }
    if count > 0 {
        success(format!("已清除 {count} 项配置"))
    } else {
        success("没有需要清除的配置")
    }
}

fn desktop_value(
    name: &str,
    base_url: &str,
    gateway_api_key: &str,
    inference_models: &str,
    auth_scheme: &str,
    gateway_headers: &str,
) -> (String, bool) {
    match name {
        "inferenceProvider" => ("gateway".to_string(), false),
        "inferenceGatewayBaseUrl" => (base_url.to_string(), false),
        "inferenceGatewayApiKey" => (gateway_api_key.to_string(), false),
        "inferenceGatewayAuthScheme" => (
            if auth_scheme.is_empty() {
                "bearer"
            } else {
                auth_scheme
            }
            .to_string(),
            false,
        ),
        "inferenceGatewayHeaders" => (
            if gateway_headers.is_empty() {
                "[]"
            } else {
                gateway_headers
            }
            .to_string(),
            false,
        ),
        "inferenceModels" => (
            if inference_models.is_empty() {
                default_inference_models_json()
            } else {
                inference_models.to_string()
            },
            false,
        ),
        "isClaudeCodeForDesktopEnabled" => ("1".to_string(), true),
        _ => (String::new(), false),
    }
}

fn get_json_config_status(paths: &MacPaths) -> DesktopConfigStatus {
    let (ok, data, message) = read_json_file(&paths.config_json);
    if !ok {
        return default_status(message);
    }
    let Some(enterprise_config) = data.get("enterpriseConfig").and_then(Value::as_object) else {
        return default_status("");
    };
    let keys = json_status_keys(enterprise_config);
    DesktopConfigStatus {
        configured: data.get("deploymentMode").and_then(Value::as_str) == Some("3p")
            && keys
                .get("inferenceProvider")
                .is_some_and(|value| value == "gateway"),
        keys,
        message: String::new(),
        sources: DesktopConfigSources::default(),
    }
}

fn apply_json_config(
    paths: &MacPaths,
    base_url: &str,
    gateway_api_key: &str,
    inference_models: &str,
    auth_scheme: &str,
    gateway_headers: &str,
) -> DesktopApplyResult {
    let (ok, mut data, message) = read_json_file(&paths.config_json);
    if !ok {
        return failure(format!("JSON 配置读取失败: {message}"));
    }

    let expected = enterprise_config(
        base_url,
        gateway_api_key,
        inference_models,
        auth_scheme,
        gateway_headers,
    );
    let enterprise_config = data
        .entry("enterpriseConfig".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !enterprise_config.is_object() {
        *enterprise_config = Value::Object(Map::new());
    }
    let enterprise_object = enterprise_config.as_object_mut().expect("object");
    for (key, value) in expected.clone() {
        enterprise_object.insert(key, value);
    }
    data.insert(
        "deploymentMode".to_string(),
        Value::String("3p".to_string()),
    );

    if let Err(message) = write_json_file(&paths.config_json, &data) {
        return failure(format!("JSON 配置写入失败: {message}"));
    }

    let (ok, saved, message) = read_json_file(&paths.config_json);
    if !ok {
        return failure(format!("JSON 配置读回失败: {message}"));
    }
    if saved.get("deploymentMode").and_then(Value::as_str) != Some("3p") {
        return failure("JSON 配置写入校验失败: deploymentMode 或 enterpriseConfig 不正确");
    }
    let Some(saved_enterprise) = saved.get("enterpriseConfig").and_then(Value::as_object) else {
        return failure("JSON 配置写入校验失败: deploymentMode 或 enterpriseConfig 不正确");
    };
    let mut failures = Vec::new();
    for (name, expected_value) in expected {
        if saved_enterprise.get(&name) != Some(&expected_value) {
            failures.push(format!("{name}: readback mismatch"));
        }
    }
    if failures.is_empty() {
        success("macOS JSON 3P 配置已应用")
    } else {
        failure(format!("JSON 配置写入校验失败: {}", failures.join("; ")))
    }
}

fn clear_json_config(paths: &MacPaths) -> DesktopApplyResult {
    let (ok, mut data, message) = read_json_file(&paths.config_json);
    if !ok {
        return failure(format!("JSON 配置读取失败: {message}"));
    }
    if data.is_empty() {
        return success("JSON 配置不存在，无需清除");
    }

    let mut changed = false;
    if data.remove("enterpriseConfig").is_some() {
        changed = true;
    }
    if data.get("deploymentMode").and_then(Value::as_str) != Some("clear") {
        data.insert(
            "deploymentMode".to_string(),
            Value::String("clear".to_string()),
        );
        changed = true;
    }
    if !changed {
        return success("JSON 配置无需清除");
    }
    match write_json_file(&paths.config_json, &data) {
        Ok(()) => success("JSON 3P 配置已清除"),
        Err(message) => failure(format!("JSON 配置写入失败: {message}")),
    }
}

fn get_library_config_status(paths: &MacPaths) -> DesktopConfigStatus {
    let (ok, paths, message) = config_library_entry_paths(paths, false);
    if !ok {
        return default_status(message);
    }
    if paths.is_empty() {
        return default_status("");
    }

    for path in paths {
        let (ok, data, message) = read_json_file(&path);
        if !ok {
            return default_status(message);
        }
        let keys = flat_config_status_keys(&data);
        if !keys.is_empty() {
            return DesktopConfigStatus {
                configured: keys
                    .get("inferenceProvider")
                    .is_some_and(|value| value == "gateway"),
                keys,
                message: String::new(),
                sources: DesktopConfigSources::default(),
            };
        }
    }
    default_status("")
}

fn apply_library_config(
    paths: &MacPaths,
    base_url: &str,
    gateway_api_key: &str,
    inference_models: &str,
    auth_scheme: &str,
    gateway_headers: &str,
) -> DesktopApplyResult {
    let (ok, entry_paths, message) = config_library_entry_paths(paths, true);
    if !ok {
        return failure(format!("configLibrary 元数据读取失败: {message}"));
    }
    if entry_paths.is_empty() {
        return success("configLibrary 不存在，无需写入");
    }

    let expected = enterprise_config(
        base_url,
        gateway_api_key,
        inference_models,
        auth_scheme,
        gateway_headers,
    );
    let mut failures = Vec::new();
    for path in entry_paths {
        let display_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("entry.json")
            .to_string();
        let (ok, mut data, message) = read_json_file(&path);
        if !ok {
            failures.push(format!("{display_name}: read failed: {message}"));
            continue;
        }
        for (key, value) in expected.clone() {
            data.insert(key, value);
        }
        if let Err(message) = write_json_file(&path, &data) {
            failures.push(format!("{display_name}: write failed: {message}"));
            continue;
        }

        let (ok, saved, message) = read_json_file(&path);
        if !ok {
            failures.push(format!("{display_name}: readback failed: {message}"));
            continue;
        }
        for (name, expected_value) in &expected {
            if saved.get(name) != Some(expected_value) {
                failures.push(format!("{display_name}: {name}: readback mismatch"));
            }
        }
    }

    if failures.is_empty() {
        success("macOS configLibrary 3P 配置已应用")
    } else {
        failure(format!(
            "configLibrary 写入校验失败: {}",
            failures.join("; ")
        ))
    }
}

fn clear_library_config(paths: &MacPaths) -> DesktopApplyResult {
    let (ok, entry_paths, message) = config_library_entry_paths(paths, false);
    if !ok {
        return failure(format!("configLibrary 元数据读取失败: {message}"));
    }
    if entry_paths.is_empty() {
        return success("configLibrary 不存在，无需清除");
    }

    let managed = [
        "inferenceProvider",
        "inferenceGatewayBaseUrl",
        "inferenceGatewayApiKey",
        "inferenceGatewayAuthScheme",
        "inferenceGatewayHeaders",
        "inferenceModels",
        "isClaudeCodeForDesktopEnabled",
        "provider",
        "apiKey",
        "authScheme",
        "baseUrl",
        "models",
    ];
    let mut failures = Vec::new();
    for path in entry_paths {
        let display_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("entry.json")
            .to_string();
        let (ok, mut data, message) = read_json_file(&path);
        if !ok {
            failures.push(format!("{display_name}: read failed: {message}"));
            continue;
        }
        let mut changed = false;
        for name in managed {
            if data.remove(name).is_some() {
                changed = true;
            }
        }
        if !changed {
            continue;
        }
        if let Err(message) = write_json_file(&path, &data) {
            failures.push(format!("{display_name}: write failed: {message}"));
        }
    }

    if failures.is_empty() {
        success("configLibrary 3P 配置已清除")
    } else {
        failure(format!("configLibrary 清除失败: {}", failures.join("; ")))
    }
}

fn merge_config_statuses(
    plist_status: DesktopConfigStatus,
    json_status: DesktopConfigStatus,
    library_status: DesktopConfigStatus,
) -> DesktopConfigStatus {
    let library_has_runtime_config = !library_status.keys.is_empty();
    let json_has_runtime_config = !json_status.keys.is_empty();

    let (keys, configured) = if library_has_runtime_config {
        (library_status.keys.clone(), library_status.configured)
    } else {
        let mut keys = plist_status.keys.clone();
        for (name, value) in &json_status.keys {
            if name == "inferenceModels" && keys.contains_key("inferenceModels") {
                continue;
            }
            keys.insert(name.clone(), value.clone());
        }
        let configured = if json_has_runtime_config {
            json_status.configured
        } else {
            plist_status.configured
        };
        (keys, configured)
    };

    DesktopConfigStatus {
        configured,
        keys,
        message: first_non_empty(&[
            library_status.message,
            json_status.message,
            plist_status.message,
        ]),
        sources: DesktopConfigSources {
            plist: plist_status.configured,
            json: json_status.configured,
            config_library: library_status.configured,
        },
    }
}

fn first_non_empty(values: &[String]) -> String {
    values
        .iter()
        .find(|value| !value.is_empty())
        .cloned()
        .unwrap_or_default()
}

fn json_status_keys(enterprise_config: &Map<String, Value>) -> BTreeMap<String, String> {
    let mut keys = BTreeMap::new();
    for name in DESKTOP_CONFIG_NAMES {
        let Some(value) = enterprise_config.get(name) else {
            continue;
        };
        keys.insert(name.to_string(), safe_config_value(name, value));
    }
    keys
}

fn flat_config_status_keys(config: &Map<String, Value>) -> BTreeMap<String, String> {
    let mut keys = json_status_keys(config);
    let aliases = [
        ("provider", "inferenceProvider"),
        ("apiKey", "inferenceGatewayApiKey"),
        ("authScheme", "inferenceGatewayAuthScheme"),
        ("baseUrl", "inferenceGatewayBaseUrl"),
        ("models", "inferenceModels"),
    ];
    for (source, target) in aliases {
        if keys.contains_key(target) {
            continue;
        }
        if let Some(value) = config.get(source) {
            keys.insert(target.to_string(), safe_config_value(target, value));
        }
    }
    keys
}

fn enterprise_config(
    base_url: &str,
    gateway_api_key: &str,
    inference_models: &str,
    auth_scheme: &str,
    gateway_headers: &str,
) -> Map<String, Value> {
    let mut object = Map::new();
    object.insert(
        "inferenceProvider".to_string(),
        Value::String("gateway".to_string()),
    );
    object.insert(
        "inferenceGatewayBaseUrl".to_string(),
        Value::String(base_url.to_string()),
    );
    object.insert(
        "inferenceGatewayApiKey".to_string(),
        Value::String(gateway_api_key.to_string()),
    );
    object.insert(
        "inferenceGatewayAuthScheme".to_string(),
        Value::String(if auth_scheme.is_empty() {
            "bearer".to_string()
        } else {
            auth_scheme.to_string()
        }),
    );
    object.insert(
        "inferenceGatewayHeaders".to_string(),
        Value::Array(
            json_gateway_headers(gateway_headers)
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    object.insert(
        "inferenceModels".to_string(),
        Value::Array(
            json_model_names(inference_models)
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    object.insert(
        "isClaudeCodeForDesktopEnabled".to_string(),
        Value::Bool(true),
    );
    object
}

fn json_model_names(inference_models: &str) -> Vec<String> {
    let default_models;
    let raw_models = if inference_models.is_empty() {
        default_models = default_inference_models_json();
        default_models.as_str()
    } else {
        inference_models
    };
    let parsed =
        serde_json::from_str::<Value>(raw_models).unwrap_or_else(|_| Value::Array(Vec::new()));
    let mut result = Vec::new();
    if let Some(items) = parsed.as_array() {
        for item in items {
            let name = if let Some(object) = item.as_object() {
                object.get("name").and_then(Value::as_str).unwrap_or("")
            } else {
                item.as_str().unwrap_or("")
            };
            let name = name.trim();
            if !name.is_empty() && !result.iter().any(|existing| existing == name) {
                result.push(name.to_string());
            }
        }
    }
    if result.is_empty() {
        DEFAULT_INFERENCE_MODELS
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    } else {
        result
    }
}

fn json_gateway_headers(gateway_headers: &str) -> Vec<String> {
    let parsed = serde_json::from_str::<Value>(if gateway_headers.is_empty() {
        "[]"
    } else {
        gateway_headers
    })
    .unwrap_or_else(|_| Value::Array(Vec::new()));
    if let Some(items) = parsed.as_array() {
        return items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect();
    }
    parsed
        .as_str()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| vec![item.to_string()])
        .unwrap_or_default()
}

fn config_library_entry_paths(
    paths: &MacPaths,
    include_missing_active: bool,
) -> (bool, Vec<PathBuf>, String) {
    let meta_path = paths.library_meta_path();
    let (ok, meta, message) = read_json_file(&meta_path);
    if !ok {
        return (false, Vec::new(), message);
    }

    let mut result = Vec::new();
    let applied_id = meta
        .get("appliedId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if !applied_id.is_empty() && !applied_id.contains('/') && !applied_id.contains('\\') {
        let active_path = paths.library_entry_path(applied_id);
        if include_missing_active || active_path.exists() {
            result.push(active_path);
        }
    }

    if result.is_empty() {
        if let Ok(entries) = fs::read_dir(paths.library_dir()) {
            let mut files = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension().and_then(|item| item.to_str()) == Some("json")
                        && path.file_name().and_then(|item| item.to_str()) != Some("_meta.json")
                })
                .collect::<Vec<_>>();
            files.sort();
            result.extend(files);
        }
    }

    (true, result, String::new())
}

fn read_json_file(path: &Path) -> (bool, Map<String, Value>, String) {
    if !path.exists() {
        return (true, Map::new(), String::new());
    }
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => return (false, Map::new(), error.to_string()),
    };
    let value = match serde_json::from_str::<Value>(&text) {
        Ok(value) => value,
        Err(error) => return (false, Map::new(), error.to_string()),
    };
    if let Some(object) = value.as_object() {
        (true, object.clone(), String::new())
    } else {
        (false, Map::new(), "JSON root is not an object".to_string())
    }
}

fn write_json_file(path: &Path, data: &Map<String, Value>) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or_else(|| "Target JSON path has no parent directory".to_string())?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let temp_path = directory.join(format!(
        ".ccds-{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    let text = serde_json::to_string_pretty(data).map_err(|error| error.to_string())?;
    let write_result = (|| -> Result<(), String> {
        let mut file = fs::File::create(&temp_path).map_err(|error| error.to_string())?;
        file.write_all(text.as_bytes())
            .map_err(|error| error.to_string())?;
        file.write_all(b"\n").map_err(|error| error.to_string())?;
        fs::rename(&temp_path, path).map_err(|error| error.to_string())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

fn run_defaults(args: &[String]) -> (bool, String) {
    match Command::new("defaults").args(args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let message = [stdout, stderr]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            (output.status.success(), message)
        }
        Err(error) => (false, error.to_string()),
    }
}

fn default_inference_models_json() -> String {
    serde_json::to_string(&DEFAULT_INFERENCE_MODELS)
        .unwrap_or_else(|_| "[\"sonnet\",\"haiku\",\"opus\"]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_paths(name: &str) -> MacPaths {
        let root = env::temp_dir().join(format!(
            "ccds-tauri-macos-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        MacPaths {
            config_json: root.join("Claude-3p").join("claude_desktop_config.json"),
        }
    }

    #[test]
    fn json_apply_preserves_preferences_and_writes_enterprise_config() {
        let paths = temp_paths("json-apply");
        let mut seed = Map::new();
        seed.insert(
            "deploymentMode".to_string(),
            Value::String("1p".to_string()),
        );
        seed.insert("preferences".to_string(), json!({"sidebarMode": "task"}));
        write_json_file(&paths.config_json, &seed).expect("seed json");

        let result = apply_json_config(
            &paths,
            "http://127.0.0.1:18080",
            "secret-value",
            r#"[{"name":"model-a","displayName":"Model A"},{"name":"model-b","supports1m":true}]"#,
            "x-api-key",
            r#"["x-api-key: secret-value"]"#,
        );

        assert!(result.success);
        let (_, saved, _) = read_json_file(&paths.config_json);
        assert_eq!(saved["deploymentMode"], "3p");
        assert_eq!(saved["preferences"], json!({"sidebarMode": "task"}));
        let enterprise = saved["enterpriseConfig"].as_object().expect("enterprise");
        assert_eq!(
            enterprise["inferenceGatewayBaseUrl"],
            "http://127.0.0.1:18080"
        );
        assert_eq!(enterprise["inferenceGatewayApiKey"], "secret-value");
        assert_eq!(enterprise["inferenceGatewayAuthScheme"], "x-api-key");
        assert_eq!(
            enterprise["inferenceGatewayHeaders"],
            json!(["x-api-key: secret-value"])
        );
        assert_eq!(enterprise["inferenceModels"], json!(["model-a", "model-b"]));
        assert_eq!(enterprise["isClaudeCodeForDesktopEnabled"], true);
    }

    #[test]
    fn library_apply_updates_active_entry_and_keeps_other_fields() {
        let paths = temp_paths("library-apply");
        let entry_id = "1b050dc2-874f-4096-a303-566f42c64bcb";
        let mut meta = Map::new();
        meta.insert("appliedId".to_string(), Value::String(entry_id.to_string()));
        write_json_file(&paths.library_meta_path(), &meta).expect("meta");
        let mut entry = Map::new();
        entry.insert("note".to_string(), Value::String("keep me".to_string()));
        entry.insert(
            "inferenceGatewayBaseUrl".to_string(),
            Value::String("https://old.example".to_string()),
        );
        write_json_file(&paths.library_entry_path(entry_id), &entry).expect("entry");

        let result = apply_library_config(
            &paths,
            "http://127.0.0.1:18080",
            "secret-value",
            r#"[{"name":"model-a"},{"name":"model-b","supports1m":true}]"#,
            "x-api-key",
            r#"["x-api-key: secret-value"]"#,
        );

        assert!(result.success);
        let (_, saved, _) = read_json_file(&paths.library_entry_path(entry_id));
        assert_eq!(saved["note"], "keep me");
        assert_eq!(saved["inferenceGatewayBaseUrl"], "http://127.0.0.1:18080");
        assert_eq!(saved["inferenceGatewayApiKey"], "secret-value");
        assert_eq!(
            saved["inferenceGatewayHeaders"],
            json!(["x-api-key: secret-value"])
        );
        assert_eq!(saved["inferenceModels"], json!(["model-a", "model-b"]));
    }

    #[test]
    fn status_prefers_library_over_root_json() {
        let plist_status = DesktopConfigStatus {
            configured: true,
            keys: BTreeMap::from([
                ("inferenceProvider".to_string(), "gateway".to_string()),
                (
                    "inferenceGatewayBaseUrl".to_string(),
                    "http://127.0.0.1:18080".to_string(),
                ),
                (
                    "inferenceModels".to_string(),
                    r#"[{"name":"plist-model","supports1m":true}]"#.to_string(),
                ),
            ]),
            message: String::new(),
            sources: DesktopConfigSources::default(),
        };
        let json_status = DesktopConfigStatus {
            configured: true,
            keys: BTreeMap::from([
                ("inferenceProvider".to_string(), "gateway".to_string()),
                (
                    "inferenceGatewayBaseUrl".to_string(),
                    "https://root.example".to_string(),
                ),
            ]),
            message: String::new(),
            sources: DesktopConfigSources::default(),
        };
        let library_status = DesktopConfigStatus {
            configured: true,
            keys: BTreeMap::from([
                ("inferenceProvider".to_string(), "gateway".to_string()),
                (
                    "inferenceGatewayBaseUrl".to_string(),
                    "https://library.example".to_string(),
                ),
                ("inferenceGatewayApiKey".to_string(), "******".to_string()),
                (
                    "inferenceModels".to_string(),
                    r#"["library-model"]"#.to_string(),
                ),
            ]),
            message: String::new(),
            sources: DesktopConfigSources::default(),
        };

        let merged = merge_config_statuses(plist_status, json_status, library_status);

        assert!(merged.configured);
        assert_eq!(
            merged.keys["inferenceGatewayBaseUrl"],
            "https://library.example"
        );
        assert_eq!(merged.keys["inferenceGatewayApiKey"], "******");
        assert_eq!(merged.keys["inferenceModels"], r#"["library-model"]"#);
        assert!(merged.sources.plist);
        assert!(merged.sources.json);
        assert!(merged.sources.config_library);
    }

    #[test]
    fn status_merges_json_runtime_values_but_keeps_plist_models() {
        let plist_status = DesktopConfigStatus {
            configured: true,
            keys: BTreeMap::from([
                ("inferenceProvider".to_string(), "gateway".to_string()),
                (
                    "inferenceGatewayBaseUrl".to_string(),
                    "http://127.0.0.1:18080".to_string(),
                ),
                (
                    "inferenceModels".to_string(),
                    r#"[{"name":"model-a","supports1m":true}]"#.to_string(),
                ),
            ]),
            message: String::new(),
            sources: DesktopConfigSources::default(),
        };
        let json_status = DesktopConfigStatus {
            configured: true,
            keys: BTreeMap::from([
                ("inferenceProvider".to_string(), "gateway".to_string()),
                (
                    "inferenceGatewayBaseUrl".to_string(),
                    "https://stale.example".to_string(),
                ),
                ("inferenceGatewayApiKey".to_string(), "******".to_string()),
                ("inferenceModels".to_string(), r#"["model-a"]"#.to_string()),
            ]),
            message: String::new(),
            sources: DesktopConfigSources::default(),
        };
        let library_status = DesktopConfigStatus {
            configured: false,
            keys: BTreeMap::new(),
            message: String::new(),
            sources: DesktopConfigSources::default(),
        };

        let merged = merge_config_statuses(plist_status, json_status, library_status);

        assert!(merged.configured);
        assert_eq!(
            merged.keys["inferenceGatewayBaseUrl"],
            "https://stale.example"
        );
        assert_eq!(merged.keys["inferenceGatewayApiKey"], "******");
        assert_eq!(
            merged.keys["inferenceModels"],
            r#"[{"name":"model-a","supports1m":true}]"#
        );
        assert!(merged.sources.plist);
        assert!(merged.sources.json);
        assert!(!merged.sources.config_library);
    }

    #[test]
    fn clear_removes_json_enterprise_config_without_touching_preferences() {
        let paths = temp_paths("json-clear");
        let mut seed = Map::new();
        seed.insert(
            "deploymentMode".to_string(),
            Value::String("3p".to_string()),
        );
        seed.insert(
            "enterpriseConfig".to_string(),
            json!({"inferenceProvider": "gateway"}),
        );
        seed.insert("preferences".to_string(), json!({"sidebarMode": "task"}));
        write_json_file(&paths.config_json, &seed).expect("seed json");

        let result = clear_json_config(&paths);

        assert!(result.success);
        let (_, saved, _) = read_json_file(&paths.config_json);
        assert_eq!(saved["deploymentMode"], "clear");
        assert!(!saved.contains_key("enterpriseConfig"));
        assert_eq!(saved["preferences"], json!({"sidebarMode": "task"}));
    }

    #[test]
    fn clear_library_removes_managed_keys_only() {
        let paths = temp_paths("library-clear");
        let entry_id = "1b050dc2-874f-4096-a303-566f42c64bcb";
        let mut meta = Map::new();
        meta.insert("appliedId".to_string(), Value::String(entry_id.to_string()));
        write_json_file(&paths.library_meta_path(), &meta).expect("meta");
        write_json_file(
            &paths.library_entry_path(entry_id),
            json!({
                "inferenceProvider": "gateway",
                "inferenceGatewayBaseUrl": "http://127.0.0.1:18080",
                "inferenceGatewayApiKey": "secret-value",
                "inferenceGatewayHeaders": ["x-api-key: secret-value"],
                "inferenceModels": ["model-a"],
                "note": "keep me"
            })
            .as_object()
            .expect("object"),
        )
        .expect("entry");

        let result = clear_library_config(&paths);

        assert!(result.success);
        let (_, saved, _) = read_json_file(&paths.library_entry_path(entry_id));
        assert_eq!(
            saved,
            Map::from_iter([("note".to_string(), Value::String("keep me".to_string()))])
        );
    }
}
