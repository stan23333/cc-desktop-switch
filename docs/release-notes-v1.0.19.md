# CC Desktop Switch v1.0.19

## 中文

本次版本收敛 Claude Desktop 模型菜单，只显示用户明确映射的 Claude 模型槽位。

### 主要变化

- **模型菜单只显示显式映射项**：
  - `Default` 只作为本工具内部保存的后备模型，不写入 Claude Desktop 菜单。
  - 未配置的 Opus / Sonnet / Haiku 槽位不会出现在 `inferenceModels` 或 `/v1/models`。
  - 已映射的 1M 模型仍会保留 `supports1m`，Claude Desktop 可以继续显示对应 1M 入口。
- **未映射模型明确拒绝**：
  - 如果旧会话仍请求未映射的 Claude route，本机 gateway 会返回 400，并提示重新选择已映射模型或重新一键应用。
  - 不再把未映射 route 静默回退到 `Default`。
- **健康检查更准确**：
  - 检测到 v1.0.18 写入过的多余安全 route 时，会提示重新一键应用并重启 Claude Desktop。
- **避免重复启动**：
  - Windows 下再次点击桌面快捷方式会唤起已有窗口，不再启动第二个 CC Desktop Switch 实例。

### 下载建议

- Windows 用户可以下载 `CC-Desktop-Switch-v1.0.19-Windows-Setup.exe` 或 `CC-Desktop-Switch-v1.0.19-Windows-Portable.zip`。
- 本轮本地测试包只生成 Windows 资产；macOS 资产由 macOS 维护者单独同步。

## English

This release narrows the Claude Desktop model menu to only explicitly mapped Claude model slots.

### Highlights

- **Model menu only shows explicit mappings**:
  - `Default` is kept only as an internal fallback in this app and is not written as a Claude Desktop menu item.
  - Unmapped Opus / Sonnet / Haiku slots are not exposed through `inferenceModels` or `/v1/models`.
  - Explicitly mapped 1M models still keep `supports1m`, so Claude Desktop can continue to show the matching 1M entry.
- **Unmapped routes are rejected clearly**:
  - If an old conversation still requests an unmapped Claude route, the local gateway returns 400 and asks the user to choose a mapped model or apply again.
  - Unmapped routes no longer silently fall back to `Default`.
- **More accurate health checks**:
  - Extra safe routes written by v1.0.18 are detected and reported as requiring apply + Claude Desktop restart.
- **Single-instance startup**:
  - On Windows, launching the desktop shortcut again brings the existing window forward instead of starting a second CC Desktop Switch instance.

### Downloads

- Windows users can choose `CC-Desktop-Switch-v1.0.19-Windows-Setup.exe` or `CC-Desktop-Switch-v1.0.19-Windows-Portable.zip`.
- This local test build only includes Windows assets. macOS assets are synchronized separately by the macOS maintainer.
