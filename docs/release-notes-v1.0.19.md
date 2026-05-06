# CC Desktop Switch v1.0.19

<p align="center">
  <a href="#english">English</a> |
  <a href="#simplified-chinese">简体中文</a>
</p>

<a id="english"></a>

## English

This release narrows the Claude Desktop model menu to only the Claude model slots you explicitly map in CC Desktop Switch.

### Highlights

- **Only explicit model mappings are shown**
  - `Default` is kept only as an internal fallback.
  - `Default` is not written to Claude Desktop as a menu item.
  - Unmapped Opus / Sonnet / Haiku slots are not exposed through `inferenceModels` or `/v1/models`.
  - Explicitly mapped 1M-capable routes still keep their `supports1m` metadata.

- **Unmapped routes now fail clearly**
  - If an old conversation still sends an unmapped Claude route, the local gateway returns a 400 error with a clear message.
  - The gateway no longer silently falls back to `Default`.

- **Better health checks**
  - CC Desktop Switch now detects raw upstream model names such as `deepseek`, `kimi`, `glm`, `qwen`, and `dashscope` in Claude Desktop configuration.
  - It also detects stale Claude-safe routes written by v1.0.18 and asks you to apply again.

- **Single-instance startup on Windows**
  - Launching the desktop shortcut again brings the existing window forward instead of starting a second CC Desktop Switch instance.

### Upgrade Notes

After upgrading, open CC Desktop Switch, apply the current provider to Claude Desktop again, and fully restart Claude Desktop. This clears old raw upstream model names and stale routes from the Claude Desktop policy.

### Downloads

- Windows: `CC-Desktop-Switch-v1.0.19-Windows-Setup.exe` or `CC-Desktop-Switch-v1.0.19-Windows-Portable.zip`
- macOS arm64: `CC-Desktop-Switch-v1.0.19-macOS-arm64.pkg` or `CC-Desktop-Switch-v1.0.19-macOS-arm64.dmg`

The macOS package was built on a GitHub Actions macOS runner and passed a headless startup smoke test, PKG expansion check, and DMG verification.

<a id="simplified-chinese"></a>

## 简体中文

本次版本收敛 Claude Desktop 模型菜单，只显示你在 CC Desktop Switch 中明确映射的 Claude 模型槽位。

### 主要变化

- **模型菜单只显示显式映射项**
  - `Default` 只作为本工具内部保存的后备模型。
  - `Default` 不再写入 Claude Desktop 菜单。
  - 未配置的 Opus / Sonnet / Haiku 槽位不会出现在 `inferenceModels` 或 `/v1/models`。
  - 已映射且支持 1M 的 route 仍会保留 `supports1m` 元数据。

- **未映射模型明确拒绝**
  - 如果旧会话仍请求未映射的 Claude route，本机 gateway 会返回 400，并给出明确提示。
  - 不再把未映射 route 静默回退到 `Default`。

- **健康检查更准确**
  - 可以识别 Claude Desktop 配置中残留的 `deepseek`、`kimi`、`glm`、`qwen`、`dashscope` 等真实上游模型名。
  - 也会识别 v1.0.18 写入过、但当前没有映射的多余 Claude-safe route，并提示重新应用。

- **Windows 防重复启动**
  - Windows 下再次点击桌面快捷方式会唤起已有窗口，不会再启动第二个 CC Desktop Switch 实例。

### 升级说明

升级后，请打开 CC Desktop Switch，重新对当前 provider 执行“一键应用到 Claude 桌面版”，然后完整重启 Claude Desktop。这样可以清掉旧版本写入的真实上游模型名和多余 route。

### 下载

- Windows：`CC-Desktop-Switch-v1.0.19-Windows-Setup.exe` 或 `CC-Desktop-Switch-v1.0.19-Windows-Portable.zip`
- macOS arm64：`CC-Desktop-Switch-v1.0.19-macOS-arm64.pkg` 或 `CC-Desktop-Switch-v1.0.19-macOS-arm64.dmg`

macOS 包已在 GitHub Actions 的 macOS runner 上完成构建、无界面启动 smoke test、PKG 展开校验和 DMG verify。
