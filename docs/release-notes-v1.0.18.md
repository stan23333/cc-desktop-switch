# CC Desktop Switch v1.0.18

## 中文

本次版本修复 Claude Desktop 第三方 provider 的应用链路，默认改为本机转发模式，并补齐前端和文档中的运行边界说明。

### 主要变化

- **修复 Claude Desktop 1.6259.x 模型名校验问题**：
  - Desktop 写入和 `/v1/models` 只暴露 `claude-*` 安全路由名。
  - DeepSeek、Kimi、GLM、Qwen 等真实上游模型名只保留在本机代理内部映射。
  - 健康检查会识别旧配置里残留的真实上游模型名，并提示重新一键应用。
- **Desktop 配置默认走本机转发**：
  - `desktop_config_target_for_provider` 默认返回 `local_proxy`。
  - Claude Desktop 会连接 `127.0.0.1` 上的 CC Desktop Switch 转发服务，由后台统一处理模型映射、额外请求头和协议兼容。
  - 使用第三方 provider 时需要保持本工具在后台运行；完全退出本工具会中断 Claude Desktop 到第三方 provider 的访问。
- **修复一键应用返回值**：
  - `/api/desktop/configure` 现在返回 `success`、`message`、`mode`、`requiresProxy`、`proxyStarted` 和 `proxyPort`。
  - 如果 Desktop 配置写入成功但转发服务启动失败，接口会返回 `success=false`，避免前端误报成功。
  - 前端不再丢弃 POST 返回值，会按 `success` 判断是否提示应用成功，并在需要转发时启动或确认代理。
- **文档边界说明**：
  - README 和使用说明已明确 v1.0.18 第三方 provider 需要 CC Desktop Switch 后台运行。
  - `CLAUDE_CODE_ATTRIBUTION_HEADER=0` 只用于 Claude Code 的 prompt cache 兼容，不是 Claude Desktop 第三方推理配置项。

### 下载建议

- Windows 用户可以下载 `CC-Desktop-Switch-v1.0.18-Windows-Setup.exe` 或 `CC-Desktop-Switch-v1.0.18-Windows-Portable.zip`。
- 本轮本地测试包只生成 Windows 资产；macOS 资产由 macOS 维护者单独同步。

## English

This release fixes the Claude Desktop apply flow for third-party providers, switches the default Desktop target to the local proxy, and clarifies the runtime requirement in documentation.

### Highlights

- **Fixed Claude Desktop 1.6259.x model-name validation**:
  - Desktop policy and `/v1/models` expose only `claude-*` safe route names.
  - DeepSeek, Kimi, GLM, Qwen, and other upstream model IDs stay inside the local proxy mapping.
  - Health checks now detect old configs that still contain raw upstream model names and ask users to apply again.
- **Desktop configuration now defaults to the local proxy**:
  - `desktop_config_target_for_provider` returns `local_proxy` by default.
  - Claude Desktop connects to the CC Desktop Switch forwarding service on `127.0.0.1`, where model mapping, extra headers, and protocol compatibility are handled.
  - Keep CC Desktop Switch running in the background when using third-party providers.
- **Fixed apply response handling**:
  - `/api/desktop/configure` now returns `success`, `message`, `mode`, `requiresProxy`, `proxyStarted`, and `proxyPort`.
  - If Desktop config is written but proxy startup fails, the API returns `success=false` so the UI does not report a false success.
  - The frontend keeps the POST response, checks `success`, and starts or confirms the proxy when required.
- **Documentation boundary updates**:
  - README and usage docs now state that v1.0.18 third-party providers require the app to keep running in the background.
  - `CLAUDE_CODE_ATTRIBUTION_HEADER=0` is only for Claude Code prompt-cache compatibility. It is not a Claude Desktop third-party inference setting.

### Downloads

- Windows users can choose `CC-Desktop-Switch-v1.0.18-Windows-Setup.exe` or `CC-Desktop-Switch-v1.0.18-Windows-Portable.zip`.
- This local test build only includes Windows assets. macOS assets are synchronized separately by the macOS maintainer.
