mod ccswitch_import;
mod config;
mod desktop;
mod feedback;
mod generated;
mod model_alias;
mod models;
mod provider_tools;
mod proxy;
mod state;
mod static_frontend;
mod system_proxy;
mod update;

use models::{
    BackupInfo, DesktopApplyResult, DesktopConfigStatus, DesktopHealth, ExportedConfig,
    ImportResult, MigrationStatus, Provider, ProviderPreset, ProxyLogEntry, ProxyStatus, Settings,
    UpdateProgress,
};
use serde_json::Value;
use state::AppState;
use tauri::{
    Manager, WindowEvent,
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

#[tauri::command]
fn get_migration_status(state: tauri::State<'_, AppState>) -> Result<MigrationStatus, String> {
    config::migration_status(state.started_at_ms())
}

#[tauri::command]
fn list_builtin_presets() -> Vec<ProviderPreset> {
    config::builtin_presets()
}

#[tauri::command(async)]
fn get_config_snapshot() -> Result<Value, String> {
    config::ConfigStore::default()?.public_config_snapshot()
}

#[tauri::command(async)]
fn get_settings() -> Result<Settings, String> {
    config::ConfigStore::default()?.get_settings()
}

#[tauri::command]
fn update_settings(settings: Value) -> Result<Settings, String> {
    config::ConfigStore::default()?.update_settings(settings)
}

#[tauri::command]
fn add_provider(provider: Value) -> Result<Provider, String> {
    config::ConfigStore::default()?.add_provider(provider)
}

#[tauri::command]
fn update_provider(provider_id: String, provider: Value) -> Result<Option<Provider>, String> {
    config::ConfigStore::default()?.update_provider(&provider_id, provider)
}

#[tauri::command]
fn get_provider_secret(provider_id: String) -> Result<Value, String> {
    let provider = config::ConfigStore::default()?
        .get_provider(&provider_id)?
        .ok_or_else(|| "提供商不存在".to_string())?;
    Ok(serde_json::json!({
        "success": true,
        "apiKey": provider.api_key,
    }))
}

#[tauri::command]
fn update_provider_models(provider_id: String, models: Value) -> Result<bool, String> {
    let mappings = models
        .as_object()
        .ok_or_else(|| "模型映射必须是 JSON 对象".to_string())?
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
        .collect();
    config::ConfigStore::default()?.update_models(&provider_id, mappings)
}

#[tauri::command]
fn detect_api_format(base_url: String, api_key: Option<String>) -> Value {
    provider_tools::detect_api_format(&base_url, api_key.as_deref().unwrap_or(""))
}

#[tauri::command]
fn test_provider(provider: Value) -> Result<Value, String> {
    let provider = config::normalize_provider_payload(provider);
    Ok(provider_tools::test_provider_connection(&provider))
}

#[tauri::command]
fn test_saved_provider(provider_id: String) -> Result<Value, String> {
    let provider = config::ConfigStore::default()?
        .get_provider(&provider_id)?
        .ok_or_else(|| "提供商不存在".to_string())?;
    Ok(provider_tools::test_provider_connection(&provider))
}

#[tauri::command]
fn query_provider_usage(provider_id: String) -> Result<Value, String> {
    let provider = config::ConfigStore::default()?
        .get_provider(&provider_id)?
        .ok_or_else(|| "提供商不存在".to_string())?;
    Ok(provider_tools::query_provider_usage(&provider))
}

#[tauri::command]
fn check_model_available(provider_id: String, model: String) -> Result<Value, String> {
    let provider = config::ConfigStore::default()?
        .get_provider(&provider_id)?
        .ok_or_else(|| "提供商不存在".to_string())?;
    Ok(provider_tools::check_model_available(&provider, &model))
}

#[tauri::command]
fn provider_compatibility_report() -> Result<Value, String> {
    let config = config::ConfigStore::default()?.load_config()?;
    Ok(provider_tools::compatibility_report(&config.providers))
}

#[tauri::command]
fn fetch_provider_models(provider: Value) -> Result<Value, String> {
    let provider = config::normalize_provider_payload(provider);
    Ok(provider_tools::fetch_provider_models(&provider))
}

#[tauri::command]
fn fetch_saved_provider_models(provider_id: String) -> Result<Value, String> {
    let provider = config::ConfigStore::default()?
        .get_provider(&provider_id)?
        .ok_or_else(|| "提供商不存在".to_string())?;
    Ok(provider_tools::fetch_provider_models(&provider))
}

#[tauri::command]
fn autofill_provider_models(provider_id: String) -> Result<Value, String> {
    let store = config::ConfigStore::default()?;
    let provider = store
        .get_provider(&provider_id)?
        .ok_or_else(|| "提供商不存在".to_string())?;
    let result = provider_tools::fetch_provider_models(&provider);
    if result.get("success").and_then(Value::as_bool) != Some(true) {
        return Ok(result);
    }
    let suggested_object = result
        .get("suggested")
        .and_then(Value::as_object)
        .ok_or_else(|| "模型映射推荐结果格式错误".to_string())?;
    let default_model = suggested_object
        .get("default")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string();
    let mut merged = provider.models.clone();
    if !default_model.is_empty() {
        merged.insert("default".to_string(), default_model.clone());
    }
    if !store.update_models(&provider_id, merged)? {
        return Err("提供商不存在".to_string());
    }
    Ok(serde_json::json!({
        "success": true,
        "models": result.get("models").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
        "suggested": if default_model.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::json!({ "default": default_model })
        },
        "endpoint": result.get("endpoint").cloned().unwrap_or(Value::Null),
        "message": "模型映射已自动填充",
    }))
}

#[tauri::command]
fn delete_provider(provider_id: String) -> Result<bool, String> {
    config::ConfigStore::default()?.delete_provider(&provider_id)
}

#[tauri::command]
fn set_active_provider(provider_id: String) -> Result<Option<Provider>, String> {
    config::ConfigStore::default()?.set_active_provider(&provider_id)
}

#[tauri::command]
fn reorder_providers(provider_ids: Vec<String>) -> Result<bool, String> {
    config::ConfigStore::default()?.reorder_providers(provider_ids)
}

#[tauri::command]
fn create_config_backup(reason: Option<String>) -> Result<BackupInfo, String> {
    config::ConfigStore::default()?.create_backup(reason.as_deref().unwrap_or("manual"))
}

#[tauri::command]
fn list_config_backups() -> Result<Vec<BackupInfo>, String> {
    config::ConfigStore::default()?.list_backups()
}

#[tauri::command]
fn export_config() -> Result<ExportedConfig, String> {
    config::ConfigStore::default()?.export_config()
}

#[tauri::command]
fn import_config(config: Value) -> Result<ImportResult, String> {
    config::ConfigStore::default()?.import_config(config)
}

#[tauri::command]
fn submit_feedback(payload: Value, state: tauri::State<'_, AppState>) -> Result<Value, String> {
    feedback::submit_feedback(payload, state.proxy_runtime())
}

#[tauri::command(async)]
fn get_desktop_status() -> DesktopConfigStatus {
    desktop::get_config_status()
}

#[tauri::command(async)]
fn get_desktop_health() -> Result<DesktopHealth, String> {
    desktop::get_desktop_health()
}

#[tauri::command]
fn configure_desktop() -> Result<DesktopApplyResult, String> {
    desktop::configure_active_provider()
}

#[tauri::command]
fn clear_desktop_config() -> DesktopApplyResult {
    desktop::clear_config()
}

#[tauri::command]
fn restart_claude_desktop() -> DesktopApplyResult {
    desktop::restart_claude_desktop()
}

#[tauri::command]
fn get_proxy_status(state: tauri::State<'_, AppState>) -> Result<ProxyStatus, String> {
    proxy::proxy_status(state.proxy_runtime())
}

#[tauri::command]
fn get_proxy_logs(state: tauri::State<'_, AppState>) -> Vec<ProxyLogEntry> {
    proxy::proxy_logs(state.proxy_runtime())
}

#[tauri::command]
fn clear_proxy_logs(state: tauri::State<'_, AppState>) -> bool {
    proxy::clear_proxy_logs(state.proxy_runtime())
}

#[tauri::command]
fn start_proxy_listener(state: tauri::State<'_, AppState>) -> Result<ProxyStatus, String> {
    proxy::start_proxy_listener(state.proxy_runtime())
}

#[tauri::command]
fn stop_proxy_listener(state: tauri::State<'_, AppState>) -> Result<ProxyStatus, String> {
    proxy::stop_proxy_listener(state.proxy_runtime())
}

#[tauri::command]
fn get_gateway_models_preview() -> Result<Value, String> {
    proxy::gateway_models_for_active_provider()
}

#[tauri::command]
fn get_ccswitch_status() -> Result<Value, String> {
    ccswitch_import::status()
}

#[tauri::command]
fn get_ccswitch_providers() -> Result<Value, String> {
    ccswitch_import::read_providers_public()
}

#[tauri::command]
fn import_ccswitch_providers(
    ids: Option<Vec<String>>,
    set_default: Option<bool>,
) -> Result<Value, String> {
    ccswitch_import::import_providers(ids, set_default.unwrap_or(false))
}

#[tauri::command]
fn detect_local_proxy() -> String {
    system_proxy::detect_local_proxy()
}

#[tauri::command(async)]
fn check_update(
    url: Option<String>,
    current: Option<String>,
    platform: Option<String>,
) -> Result<Value, String> {
    update::check_update(url, current, platform)
}

#[tauri::command]
fn install_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    url: Option<String>,
    current: Option<String>,
    platform: Option<String>,
) -> Result<Value, String> {
    let mut result = update::download_update(state.update_runtime(), url, current, platform, None)?;
    if result.get("updateAvailable").and_then(Value::as_bool) != Some(true) {
        return Ok(result);
    }
    let installer_path = result
        .get("installerPath")
        .and_then(Value::as_str)
        .ok_or_else(|| "下载安装包失败".to_string())?
        .to_string();
    let resolved_platform = result
        .get("platform")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let wait_for_pid = if resolved_platform.starts_with("macos-") {
        Some(std::process::id())
    } else {
        None
    };
    let quit_requested =
        update::launch_installer(&installer_path, &resolved_platform, wait_for_pid)?;
    if quit_requested {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(800));
            app.exit(0);
        });
    }
    let message = if resolved_platform.starts_with("macos-") {
        if quit_requested {
            "更新包已下载，应用即将退出并启动安装器。"
        } else {
            "更新包已下载并打开。请先退出当前应用，再按 macOS 提示完成安装。"
        }
    } else {
        "安装包已下载并启动。安装器会沿用旧安装目录，并在安装前关闭正在运行的 CC Desktop Switch。"
    };
    let object = result.as_object_mut().expect("install result object");
    object.insert("success".to_string(), Value::Bool(true));
    object.insert("installerStarted".to_string(), Value::Bool(true));
    object.insert("quitRequested".to_string(), Value::Bool(quit_requested));
    object.insert("message".to_string(), Value::String(message.to_string()));
    Ok(result)
}

#[tauri::command]
fn get_update_progress(state: tauri::State<'_, AppState>) -> UpdateProgress {
    state.update_runtime().progress()
}

fn show_main_window(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.show();

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }

    #[cfg(target_os = "macos")]
    activate_macos_app();
}

#[cfg(target_os = "macos")]
fn activate_macos_app() {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    let app = NSRunningApplication::currentApplication();
    app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app.handle())
        .text("show-main-window", "显示 CC Desktop Switch")
        .separator()
        .text("quit-app", "退出 CC Desktop Switch")
        .build()?;

    let mut tray = TrayIconBuilder::with_id("cc-desktop-switch")
        .menu(&menu)
        .tooltip("CC Desktop Switch")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show-main-window" => show_main_window(app),
            "quit-app" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => show_main_window(tray.app_handle()),
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => show_main_window(tray.app_handle()),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .manage(AppState::new())
        .register_asynchronous_uri_scheme_protocol("ccds", |_app, request, responder| {
            responder.respond(static_frontend::serve(request.uri().path()));
        })
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                #[cfg(target_os = "macos")]
                {
                    let app_handle = window.app_handle().clone();
                    let _ = window.run_on_main_thread(move || {
                        let _ = app_handle.hide();
                    });
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_migration_status,
            list_builtin_presets,
            get_config_snapshot,
            get_settings,
            update_settings,
            add_provider,
            update_provider,
            get_provider_secret,
            update_provider_models,
            detect_api_format,
            test_provider,
            test_saved_provider,
            query_provider_usage,
            check_model_available,
            provider_compatibility_report,
            fetch_provider_models,
            fetch_saved_provider_models,
            autofill_provider_models,
            delete_provider,
            set_active_provider,
            reorder_providers,
            create_config_backup,
            list_config_backups,
            export_config,
            import_config,
            submit_feedback,
            get_desktop_status,
            get_desktop_health,
            configure_desktop,
            clear_desktop_config,
            restart_claude_desktop,
            get_proxy_status,
            get_proxy_logs,
            clear_proxy_logs,
            start_proxy_listener,
            stop_proxy_listener,
            get_gateway_models_preview,
            get_ccswitch_status,
            get_ccswitch_providers,
            import_ccswitch_providers,
            detect_local_proxy,
            check_update,
            install_update,
            get_update_progress,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run CC Desktop Switch Tauri application");
}
