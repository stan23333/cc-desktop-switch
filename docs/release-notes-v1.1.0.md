# CC Desktop Switch v1.1.0

## 中文

v1.1.0 是一次桌面运行时升级版本：应用主体迁移到 Tauri + Rust，同时保留原有前端界面、提供商配置逻辑和 Python 兼容路径。目标是提升启动速度、窗口恢复、托盘行为和本机代理稳定性，同时避免重新设计用户已经熟悉的界面。

### 主要变化

- **迁移到 Tauri + Rust 桌面运行时**：
  - 新增 Tauri v2 应用壳和 Rust 命令层，替代原先较重的桌面承载方式。
  - 保留原有 HTML/CSS/JavaScript 界面，不改变 Provider 管理、设置页和一键应用的主流程。
  - 原 Python 路径仍保留，便于兼容旧的 Windows 打包和回退验证。
- **提升桌面体验**：
  - 优化启动加载链路，减少启动时长时间空白。
  - 修复关闭窗口后隐藏到托盘，再次点击托盘无法唤回窗口的问题。
  - Provider 卡片启用成功后会询问是否重启 Claude Desktop；如果 Claude 未运行，会直接打开。
- **完善 Provider 配置行为**：
  - 恢复 Tauri 路径中的 Xiaomi MiMo、Xiaomi MiMo Token Plan、阿里云百炼 Token Plan 和第三方模型入口。
  - 连接测试改为只判断服务器地址可达；API Key 是否可用由获取模型或模型检测给出明确提示。
  - 获取模型列表时只自动填充 Default 映射，其余模型槽位由用户手动填写或回退到 Default。
  - 一键应用流程改为应用内确认弹窗，避免原生确认框在 Tauri WebView 中无反馈。
- **新增反馈入口**：
  - Dashboard 和设置页新增反馈入口。
  - 支持提交文字、附件和脱敏诊断信息，便于定位问题。

### 下载建议

- Windows 用户可以下载 `CC-Desktop-Switch-v1.1.0-Windows-Setup.exe` 或 `CC-Desktop-Switch-v1.1.0-Windows-Portable.zip`。
- macOS 用户可以下载 `CC-Desktop-Switch-v1.1.0-macOS-arm64.pkg` 或 `CC-Desktop-Switch-v1.1.0-macOS-arm64.dmg`。

## English

v1.1.0 upgrades the desktop runtime to Tauri + Rust while keeping the existing provider-management UI, configuration flow, and Python compatibility path available. The release focuses on startup behavior, window restoration, tray behavior, local proxy stability, and provider-management correctness without redesigning the app.

### Highlights

- **Tauri + Rust desktop runtime**:
  - Added a Tauri v2 shell and Rust command layer for the desktop app.
  - Preserved the existing HTML/CSS/JavaScript interface and the established Provider, Settings, and Apply workflows.
  - Kept the Python runtime path for compatibility, existing Windows packaging, and rollback validation.
- **Desktop experience improvements**:
  - Optimized the startup loading chain to reduce blank-screen delay.
  - Fixed tray restoration after closing the window into the background.
  - Provider enable now asks whether to restart Claude Desktop; if Claude is not running, the app opens it directly.
- **Provider workflow fixes**:
  - Restored Xiaomi MiMo, Xiaomi MiMo Token Plan, Alibaba Cloud Bailian Token Plan, and the generic third-party preset in the Tauri path.
  - Connection test now checks URL reachability only; model discovery and model checks show explicit API key failures.
  - Model discovery only fills the Default mapping and leaves other slots manual or fallback-to-Default.
  - One-click apply now uses an in-app confirmation modal instead of native browser confirmation.
- **Feedback flow**:
  - Added feedback entry points in Dashboard and Settings.
  - Supports text, attachments, and sanitized diagnostics for issue triage.

### Downloads

- Windows users can choose `CC-Desktop-Switch-v1.1.0-Windows-Setup.exe` or `CC-Desktop-Switch-v1.1.0-Windows-Portable.zip`.
- macOS users can choose `CC-Desktop-Switch-v1.1.0-macOS-arm64.pkg` or `CC-Desktop-Switch-v1.1.0-macOS-arm64.dmg`.
