use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::models::{DesktopApplyResult, DesktopConfigSources, DesktopConfigStatus};

use super::{
    CCDS_MARKER, DESKTOP_CONFIG_NAMES, REGISTRY_PATH, failure, managed_policy_names,
    safe_config_value, success,
};

pub fn get_config_status() -> DesktopConfigStatus {
    let command = format!(
        "$path = 'HKCU:\\{REGISTRY_PATH}'; \
         if (-not (Test-Path -LiteralPath $path)) {{ '{{}}'; exit 0 }}; \
         $item = Get-ItemProperty -LiteralPath $path; \
         $out = @{{}}; \
         @({names}) | ForEach-Object {{ if ($null -ne $item.$_) {{ $out[$_] = $item.$_ }} }}; \
         $out | ConvertTo-Json -Compress",
        names = policy_names_for_powershell()
    );
    let (ok, output) = run_powershell(&command);
    if !ok {
        return DesktopConfigStatus {
            configured: false,
            keys: BTreeMap::new(),
            message: output,
            sources: DesktopConfigSources::default(),
        };
    }
    let value = serde_json::from_str::<Value>(&output).unwrap_or_else(|_| json!({}));
    let Some(object) = value.as_object() else {
        return DesktopConfigStatus {
            configured: false,
            keys: BTreeMap::new(),
            message: String::new(),
            sources: DesktopConfigSources::default(),
        };
    };
    let mut keys = BTreeMap::new();
    for (name, value) in object {
        keys.insert(name.clone(), safe_config_value(name, value));
    }
    DesktopConfigStatus {
        configured: keys
            .get("inferenceProvider")
            .is_some_and(|value| value == "gateway")
            && keys.get(CCDS_MARKER).is_some_and(|value| value == "true"),
        keys,
        message: String::new(),
        sources: DesktopConfigSources::default(),
    }
}

pub fn apply_config(
    base_url: &str,
    gateway_api_key: &str,
    inference_models: &str,
    auth_scheme: &str,
    gateway_headers: &str,
) -> DesktopApplyResult {
    let payload = json!({
        "baseUrl": base_url,
        "gatewayApiKey": gateway_api_key,
        "inferenceModels": if inference_models.is_empty() { "[\"sonnet\",\"haiku\",\"opus\"]" } else { inference_models },
        "authScheme": if auth_scheme.is_empty() { "bearer" } else { auth_scheme },
        "gatewayHeaders": if gateway_headers.is_empty() { "[]" } else { gateway_headers },
    });
    let command = format!(
        "$ErrorActionPreference = 'Stop'; \
         $payload = $input | Out-String | ConvertFrom-Json; \
         $path = 'HKCU:\\{REGISTRY_PATH}'; \
         if (-not (Test-Path -LiteralPath $path)) {{ New-Item -Path $path -Force | Out-Null }}; \
         New-ItemProperty -LiteralPath $path -Name 'inferenceProvider' -Value 'gateway' -PropertyType String -Force | Out-Null; \
         New-ItemProperty -LiteralPath $path -Name 'inferenceGatewayBaseUrl' -Value $payload.baseUrl -PropertyType String -Force | Out-Null; \
         New-ItemProperty -LiteralPath $path -Name 'inferenceGatewayApiKey' -Value $payload.gatewayApiKey -PropertyType String -Force | Out-Null; \
         New-ItemProperty -LiteralPath $path -Name 'inferenceGatewayAuthScheme' -Value $payload.authScheme -PropertyType String -Force | Out-Null; \
         New-ItemProperty -LiteralPath $path -Name 'inferenceGatewayHeaders' -Value $payload.gatewayHeaders -PropertyType String -Force | Out-Null; \
         New-ItemProperty -LiteralPath $path -Name 'inferenceModels' -Value $payload.inferenceModels -PropertyType String -Force | Out-Null; \
         New-ItemProperty -LiteralPath $path -Name 'isClaudeCodeForDesktopEnabled' -Value 1 -PropertyType DWord -Force | Out-Null; \
         New-ItemProperty -LiteralPath $path -Name '{CCDS_MARKER}' -Value 'true' -PropertyType String -Force | Out-Null"
    );
    let (ok, output) = run_powershell_with_stdin(&command, &payload.to_string());
    if ok {
        success("Desktop 3P 配置已应用")
    } else {
        failure(format!("配置失败: {output}"))
    }
}

pub fn clear_config() -> DesktopApplyResult {
    let names = DESKTOP_CONFIG_NAMES
        .iter()
        .copied()
        .chain(std::iter::once(CCDS_MARKER))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let managed = managed_policy_names(&names);
    let payload = json!({ "names": managed });
    let command = format!(
        "$ErrorActionPreference = 'Stop'; \
         $payload = $input | Out-String | ConvertFrom-Json; \
         $path = 'HKCU:\\{REGISTRY_PATH}'; \
         if (-not (Test-Path -LiteralPath $path)) {{ exit 0 }}; \
         foreach ($name in $payload.names) {{ \
           Remove-ItemProperty -LiteralPath $path -Name $name -ErrorAction SilentlyContinue \
         }}"
    );
    let (ok, output) = run_powershell_with_stdin(&command, &payload.to_string());
    if ok {
        success("Desktop 3P 配置已清除")
    } else {
        failure(format!("清除失败: {output}"))
    }
}

pub fn restart_claude_desktop() -> DesktopApplyResult {
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$processes = Get-Process | Where-Object { $_.ProcessName -in @('Claude', 'Claude Desktop') -or $_.MainWindowTitle -like '*Claude*' }
if ($processes) {
    foreach ($process in $processes) {
        if ($process.MainWindowHandle -ne 0) { [void]$process.CloseMainWindow() }
    }
    Start-Sleep -Seconds 2
    $processes = Get-Process | Where-Object { $_.ProcessName -in @('Claude', 'Claude Desktop') -or $_.MainWindowTitle -like '*Claude*' }
    if ($processes) { $processes | Stop-Process -Force }
}
$candidates = @(
    "$env:LOCALAPPDATA\Programs\Claude\Claude.exe",
    "$env:LOCALAPPDATA\AnthropicClaude\Claude.exe",
    "$env:PROGRAMFILES\Claude\Claude.exe",
    "${env:PROGRAMFILES(X86)}\Claude\Claude.exe"
)
foreach ($candidate in $candidates) {
    if ($candidate -and (Test-Path -LiteralPath $candidate)) {
        Start-Process -FilePath $candidate
        exit 0
    }
}
Start-Process -FilePath "Claude"
"#;
    match Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .spawn()
    {
        Ok(_) => success("已请求打开或重启 Claude Desktop"),
        Err(error) => failure(format!("重启 Claude Desktop 失败: {error}")),
    }
}

fn policy_names_for_powershell() -> String {
    DESKTOP_CONFIG_NAMES
        .iter()
        .copied()
        .chain(std::iter::once(CCDS_MARKER))
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(",")
}

fn run_powershell(command: &str) -> (bool, String) {
    match Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            command,
        ])
        .output()
    {
        Ok(output) => output_result(output),
        Err(error) => (false, error.to_string()),
    }
}

fn run_powershell_with_stdin(command: &str, stdin: &str) -> (bool, String) {
    let child_result = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            command,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child_result {
        Ok(child) => child,
        Err(error) => return (false, error.to_string()),
    };
    if let Some(mut input) = child.stdin.take() {
        if let Err(error) = input.write_all(stdin.as_bytes()) {
            return (false, error.to_string());
        }
    }
    match child.wait_with_output() {
        Ok(output) => output_result(output),
        Err(error) => (false, error.to_string()),
    }
}

fn output_result(output: std::process::Output) -> (bool, String) {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let message = [stdout, stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (output.status.success(), message)
}
