use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::config::ConfigStore;
use crate::models::UpdateProgress;

const APP_VERSION: &str = "1.1.0";

#[derive(Clone, Default)]
pub struct UpdateRuntime {
    progress: Arc<Mutex<UpdateProgress>>,
}

impl UpdateRuntime {
    pub fn progress(&self) -> UpdateProgress {
        self.progress
            .lock()
            .map(|progress| progress.clone())
            .unwrap_or_default()
    }

    fn set_progress(&self, active: bool, percent: u8, message: impl Into<String>) {
        if let Ok(mut progress) = self.progress.lock() {
            *progress = UpdateProgress {
                active,
                percent,
                message: message.into(),
            };
        }
    }
}

pub fn current_platform() -> String {
    current_platform_from(std::env::consts::OS, std::env::consts::ARCH)
}

pub fn current_platform_from(os: &str, arch: &str) -> String {
    let platform_arch = match arch.to_lowercase().as_str() {
        "amd64" | "x86_64" | "x86-64" => "x64".to_string(),
        "arm64" | "aarch64" => "arm64".to_string(),
        value if !value.is_empty() => value.to_string(),
        _ => "unknown".to_string(),
    };
    let platform_os = match os {
        "windows" | "win32" => "windows",
        "macos" | "darwin" => "macos",
        "linux" => "linux",
        value => value,
    };
    format!("{platform_os}-{platform_arch}")
}

pub fn check_update(
    url: Option<String>,
    current: Option<String>,
    platform: Option<String>,
) -> Result<Value, String> {
    let update_url = update_url_or_default(url)?;
    let platform = platform.unwrap_or_else(current_platform);
    check_update_with_url(
        &update_url,
        current.as_deref().unwrap_or(APP_VERSION),
        &platform,
    )
}

pub fn download_update(
    runtime: &UpdateRuntime,
    url: Option<String>,
    current: Option<String>,
    platform: Option<String>,
    target_dir: Option<PathBuf>,
) -> Result<Value, String> {
    let update_url = update_url_or_default(url)?;
    let platform = platform.unwrap_or_else(current_platform);
    download_update_with_url(
        runtime,
        &update_url,
        current.as_deref().unwrap_or(APP_VERSION),
        &platform,
        target_dir,
    )
}

pub fn launch_installer(
    installer_path: &str,
    platform: &str,
    wait_for_pid: Option<u32>,
) -> Result<bool, String> {
    let command = if platform.starts_with("macos-") {
        if let Some(pid) = wait_for_pid.filter(|pid| *pid > 0) {
            install_after_quit_command(installer_path, platform, pid)?
        } else {
            install_command(installer_path, platform)?
        }
    } else {
        install_command(installer_path, platform)?
    };
    let mut command_iter = command.iter();
    let Some(program) = command_iter.next() else {
        return Err("启动安装器失败: empty command".to_string());
    };
    Command::new(program)
        .args(command_iter)
        .spawn()
        .map_err(|error| format!("启动安装器失败: {error}"))?;
    Ok(platform.starts_with("macos-") && wait_for_pid.is_some())
}

fn update_url_or_default(url: Option<String>) -> Result<String, String> {
    let settings = ConfigStore::default()?.get_settings()?;
    let update_url = url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or(settings.update_url);
    validate_update_url(&update_url)
}

fn validate_update_url(url: &str) -> Result<String, String> {
    let value = url.trim();
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        return Err("更新地址必须是 http 或 https URL".to_string());
    }
    let without_scheme = value
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or_default();
    if without_scheme.split('/').next().unwrap_or("").is_empty() {
        return Err("更新地址必须是 http 或 https URL".to_string());
    }
    Ok(value.to_string())
}

fn check_update_with_url(
    url: &str,
    current_version: &str,
    platform: &str,
) -> Result<Value, String> {
    let latest = fetch_latest_json(url)?;
    let latest_version = latest
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "latest.json 缺少 version 字段".to_string())?;
    let platform_data = pick_platform(&latest, platform)?;
    let assets = platform_data
        .get("assets")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "latest.json assets 字段格式错误".to_string())?;

    Ok(json!({
        "success": true,
        "updateAvailable": is_newer_version(&latest_version, current_version),
        "currentVersion": current_version,
        "latestVersion": latest_version,
        "platform": platform,
        "pubDate": latest.get("pub_date").cloned().unwrap_or(Value::Null),
        "notes": latest.get("notes").cloned().unwrap_or_else(|| Value::String(String::new())),
        "assets": assets,
        "minimumSupportedVersion": latest.get("minimum_supported_version").cloned().unwrap_or(Value::Null),
        "updateProtocol": latest.get("update_protocol").cloned().unwrap_or_else(|| Value::Number(1.into())),
    }))
}

fn download_update_with_url(
    runtime: &UpdateRuntime,
    url: &str,
    current_version: &str,
    platform: &str,
    target_dir: Option<PathBuf>,
) -> Result<Value, String> {
    let result = check_update_with_url(url, current_version, platform)?;
    if result.get("updateAvailable").and_then(Value::as_bool) != Some(true) {
        let mut result = result;
        let object = result.as_object_mut().expect("check result object");
        object.insert("downloaded".to_string(), Value::Bool(false));
        object.insert(
            "message".to_string(),
            Value::String("当前已是最新版本".to_string()),
        );
        return Ok(result);
    }

    let assets = result
        .get("assets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let installer_asset = pick_platform_installer(&assets, platform)?;
    let downloaded = download_asset(runtime, &installer_asset, target_dir, platform)?;
    let mut result = result;
    let object = result.as_object_mut().expect("check result object");
    object.insert("downloaded".to_string(), Value::Bool(true));
    object.insert("installerAsset".to_string(), installer_asset);
    object.insert(
        "installerPath".to_string(),
        Value::String(downloaded.path.display().to_string()),
    );
    object.insert(
        "installerSha256".to_string(),
        Value::String(downloaded.sha256),
    );
    object.insert(
        "installerSize".to_string(),
        Value::Number(downloaded.size.into()),
    );
    Ok(result)
}

fn fetch_latest_json(url: &str) -> Result<Value, String> {
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| format!("更新地址请求失败: {error}"))?
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("更新地址请求失败: {error}"))?;
    let bytes = response
        .bytes()
        .map_err(|error| format!("更新地址请求失败: {error}"))?;
    parse_latest_json_bytes(&bytes)
}

fn parse_latest_json_bytes(bytes: &[u8]) -> Result<Value, String> {
    let text = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        String::from_utf8_lossy(&bytes[3..]).to_string()
    } else {
        String::from_utf8_lossy(bytes).to_string()
    };
    let value: Value =
        serde_json::from_str(&text).map_err(|_| "更新地址返回的不是有效 JSON".to_string())?;
    if value.is_object() {
        Ok(value)
    } else {
        Err("latest.json 格式错误".to_string())
    }
}

fn pick_platform<'a>(latest: &'a Value, platform: &str) -> Result<&'a Value, String> {
    latest
        .get("platforms")
        .and_then(Value::as_object)
        .and_then(|platforms| platforms.get(platform))
        .filter(|value| value.is_object())
        .ok_or_else(|| format!("latest.json 中没有 {platform} 平台资产"))
}

fn pick_platform_installer(assets: &[Value], platform: &str) -> Result<Value, String> {
    if platform.starts_with("windows-") {
        return assets
            .iter()
            .find(|asset| {
                asset_name(asset)
                    .to_lowercase()
                    .ends_with("windows-setup.exe")
            })
            .cloned()
            .ok_or_else(|| "当前版本没有 Windows 安装包资产".to_string());
    }
    if platform.starts_with("macos-") {
        if let Some(pkg) = assets
            .iter()
            .find(|asset| asset_name(asset).to_lowercase().ends_with(".pkg"))
        {
            return Ok(pkg.clone());
        }
        return assets
            .iter()
            .find(|asset| asset_name(asset).to_lowercase().ends_with(".dmg"))
            .cloned()
            .ok_or_else(|| "当前版本没有 macOS 安装资产".to_string());
    }
    Err(format!("当前平台暂不支持应用内安装: {platform}"))
}

fn allowed_install_extensions(platform: &str) -> &'static [&'static str] {
    if platform.starts_with("windows-") {
        &[".exe"]
    } else if platform.starts_with("macos-") {
        &[".pkg", ".dmg"]
    } else {
        &[]
    }
}

fn install_command(path: &str, platform: &str) -> Result<Vec<String>, String> {
    if platform.starts_with("windows-") {
        return Ok(vec![path.to_string()]);
    }
    if platform.starts_with("macos-") {
        return Ok(vec!["open".to_string(), path.to_string()]);
    }
    Err(format!("当前平台暂不支持应用内安装: {platform}"))
}

fn install_after_quit_command(
    path: &str,
    platform: &str,
    wait_for_pid: u32,
) -> Result<Vec<String>, String> {
    if wait_for_pid == 0 {
        return Err("等待退出的进程 ID 无效".to_string());
    }
    if platform.starts_with("macos-") {
        return Ok(vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "pid=\"$1\"; installer=\"$2\"; while kill -0 \"$pid\" 2>/dev/null; do sleep 0.2; done; exec open \"$installer\"".to_string(),
            "ccds-update-installer".to_string(),
            wait_for_pid.to_string(),
            path.to_string(),
        ]);
    }
    install_command(path, platform)
}

struct DownloadedAsset {
    path: PathBuf,
    sha256: String,
    size: u64,
}

fn download_asset(
    runtime: &UpdateRuntime,
    asset: &Value,
    target_dir: Option<PathBuf>,
    platform: &str,
) -> Result<DownloadedAsset, String> {
    let url = validate_update_url(&asset.get("url").and_then(Value::as_str).unwrap_or(""))?;
    let filename = safe_asset_name(
        asset
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_else(|| url.rsplit('/').next().unwrap_or("")),
    )?;
    let allowed = allowed_install_extensions(platform);
    if allowed.is_empty() {
        return Err(format!("当前平台暂不支持应用内安装: {platform}"));
    }
    if !allowed
        .iter()
        .any(|extension| filename.to_lowercase().ends_with(extension))
    {
        return Err(format!("当前平台只能下载安装资产: {}", allowed.join(" / ")));
    }

    let updates_dir = target_dir.unwrap_or_else(|| {
        std::env::temp_dir()
            .join("CC-Desktop-Switch")
            .join("updates")
    });
    fs::create_dir_all(&updates_dir).map_err(|error| format!("写入安装包失败: {error}"))?;
    let target = updates_dir.join(filename);
    let partial = target.with_file_name(format!(
        "{}.download",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("installer")
    ));

    let download_result = (|| -> Result<(), String> {
        let mut response = reqwest::blocking::Client::builder()
            .timeout(None)
            .build()
            .map_err(|error| format!("下载安装包失败: {error}"))?
            .get(&url)
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(|error| format!("下载安装包失败: {error}"))?;
        let total_size = response.content_length().unwrap_or(0);
        let mut downloaded = 0_u64;
        let mut file =
            fs::File::create(&partial).map_err(|error| format!("写入安装包失败: {error}"))?;
        let mut buffer = [0_u8; 64 * 1024];
        runtime.set_progress(true, 0, "开始下载...");
        loop {
            let count = response
                .read(&mut buffer)
                .map_err(|error| format!("下载安装包失败: {error}"))?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])
                .map_err(|error| format!("写入安装包失败: {error}"))?;
            downloaded += count as u64;
            if total_size > 0 {
                let percent = ((downloaded * 100) / total_size).min(100) as u8;
                runtime.set_progress(true, percent, format!("下载中 {percent}%"));
            }
        }
        runtime.set_progress(true, 100, "下载完成，正在校验...");
        Ok(())
    })();
    runtime.set_progress(false, 0, "");
    if let Err(error) = download_result {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }

    let actual_sha = file_sha256(&partial)?;
    let expected_sha = asset
        .get("sha256")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_lowercase();
    if !expected_sha.is_empty() && actual_sha.to_lowercase() != expected_sha {
        let _ = fs::remove_file(&partial);
        return Err("安装包校验失败，已取消安装".to_string());
    }

    fs::rename(&partial, &target).map_err(|error| format!("写入安装包失败: {error}"))?;
    let size = fs::metadata(&target)
        .map_err(|error| format!("写入安装包失败: {error}"))?
        .len();
    Ok(DownloadedAsset {
        path: target,
        sha256: actual_sha,
        size,
    })
}

fn safe_asset_name(name: &str) -> Result<String, String> {
    let filename = Path::new(name.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_default();
    if filename.is_empty() {
        Err("更新资产缺少文件名".to_string())
    } else {
        Ok(filename)
    }
}

fn asset_name(asset: &Value) -> String {
    asset
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| format!("读取安装包失败: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("读取安装包失败: {error}"))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    let mut latest_parts = version_parts(latest);
    let mut current_parts = version_parts(current);
    let width = latest_parts.len().max(current_parts.len());
    latest_parts.resize(width, 0);
    current_parts.resize(width, 0);
    latest_parts > current_parts
}

fn version_parts(version: &str) -> Vec<u32> {
    let mut result = Vec::new();
    let mut current = String::new();
    for ch in version.trim().trim_start_matches(['v', 'V']).chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            result.push(current.parse::<u32>().unwrap_or(0));
            current.clear();
        }
    }
    if !current.is_empty() {
        result.push(current.parse::<u32>().unwrap_or(0));
    }
    if result.is_empty() { vec![0] } else { result }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn version_compare_matches_existing_update_semantics() {
        assert!(!is_newer_version("1.0.4", "1.0.4"));
        assert!(!is_newer_version("v1.0.4", "1.0.4"));
        assert!(is_newer_version("1.0.10", "1.0.9"));
    }

    #[test]
    fn platform_helpers_match_latest_json_keys() {
        assert_eq!(current_platform_from("macos", "aarch64"), "macos-arm64");
        assert_eq!(current_platform_from("darwin", "arm64"), "macos-arm64");
        assert_eq!(current_platform_from("windows", "x86_64"), "windows-x64");
    }

    #[test]
    fn parse_latest_json_accepts_utf8_bom() {
        let data = parse_latest_json_bytes(
            b"\xef\xbb\xbf{\"version\":\"1.0.9\",\"platforms\":{\"windows-x64\":{\"assets\":[]}}}",
        )
        .expect("latest json");

        assert_eq!(data["version"], "1.0.9");
    }

    #[test]
    fn installer_pick_prefers_setup_exe_and_macos_pkg() {
        let windows = pick_platform_installer(
            &[
                json!({"name": "CC-Desktop-Switch-v1.0.5-Windows-Portable.zip"}),
                json!({"name": "CC-Desktop-Switch-v1.0.5-Windows-x64.exe"}),
                json!({"name": "CC-Desktop-Switch-v1.0.5-Windows-Setup.exe"}),
            ],
            "windows-x64",
        )
        .expect("windows setup");
        let macos = pick_platform_installer(
            &[
                json!({"name": "CC-Desktop-Switch-v1.0.10-macOS-arm64.dmg"}),
                json!({"name": "CC-Desktop-Switch-v1.0.10-macOS-arm64.pkg"}),
            ],
            "macos-arm64",
        )
        .expect("macos pkg");

        assert_eq!(
            windows["name"],
            "CC-Desktop-Switch-v1.0.5-Windows-Setup.exe"
        );
        assert_eq!(macos["name"], "CC-Desktop-Switch-v1.0.10-macOS-arm64.pkg");
    }

    #[test]
    fn install_after_quit_command_waits_for_pid() {
        let command =
            install_after_quit_command("/tmp/app.pkg", "macos-arm64", 4321).expect("command");

        assert_eq!(command[0], "/bin/sh");
        assert!(command[2].contains("kill -0 \"$pid\""));
        assert_eq!(command[4], "4321");
        assert_eq!(command[5], "/tmp/app.pkg");
    }

    #[test]
    fn safe_asset_name_strips_path_segments() {
        assert_eq!(
            safe_asset_name("../nested/CC-Desktop-Switch-Windows-Setup.exe").expect("name"),
            "CC-Desktop-Switch-Windows-Setup.exe"
        );
    }

    #[test]
    fn download_update_downloads_and_verifies_installer_without_launching() {
        let installer = b"fake macos installer package";
        let mut hasher = Sha256::new();
        hasher.update(installer);
        let sha256 = format!("{:x}", hasher.finalize());
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind update fixture");
        let port = listener.local_addr().expect("fixture addr").port();
        let latest = format!(
            r#"{{
                "version": "9.9.9",
                "platforms": {{
                    "macos-arm64": {{
                        "assets": [
                            {{
                                "name": "CC-Desktop-Switch-v9.9.9-macOS-arm64.pkg",
                                "url": "http://127.0.0.1:{port}/installer.pkg",
                                "sha256": "{sha256}"
                            }}
                        ]
                    }}
                }}
            }}"#
        );
        let latest_bytes = latest.into_bytes();
        let installer_bytes = installer.to_vec();
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept fixture request");
                let mut request = [0_u8; 1024];
                let count = stream.read(&mut request).expect("read request");
                let request = String::from_utf8_lossy(&request[..count]);
                let (content_type, body) = if request.starts_with("GET /latest.json ") {
                    ("application/json", latest_bytes.as_slice())
                } else if request.starts_with("GET /installer.pkg ") {
                    ("application/octet-stream", installer_bytes.as_slice())
                } else {
                    ("text/plain", b"not found".as_slice())
                };
                let status = if content_type == "text/plain" {
                    "404 Not Found"
                } else {
                    "200 OK"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write response head");
                stream.write_all(body).expect("write response body");
            }
        });
        let target_dir = std::env::temp_dir().join(format!(
            "ccds-tauri-update-download-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        let runtime = UpdateRuntime::default();

        let result = download_update_with_url(
            &runtime,
            &format!("http://127.0.0.1:{port}/latest.json"),
            "1.0.0",
            "macos-arm64",
            Some(target_dir.clone()),
        )
        .expect("download update");

        server.join().expect("fixture server");
        assert_eq!(result["downloaded"], true);
        assert_eq!(result["installerSha256"], sha256);
        assert_eq!(result["installerSize"], installer.len() as u64);
        let installer_path = result["installerPath"].as_str().expect("installer path");
        assert_eq!(
            fs::read(installer_path).expect("downloaded installer"),
            installer
        );
        assert!(
            !Path::new(installer_path)
                .with_extension("pkg.download")
                .exists()
        );
        assert!(!runtime.progress().active);
        assert_eq!(runtime.progress().percent, 0);
    }
}
