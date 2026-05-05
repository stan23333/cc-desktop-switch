# CC Desktop Switch v1.0.14

## 中文

本次版本继续收口桌面端配置体验，并同步修复 Windows 和 macOS 应用内更新链路。

### 主要变化

- 重做模型映射区域，改成更直接的双列映射表单，支持 `Default / Opus 4.7 / Opus 4.6 / Opus 3 / Sonnet 4.6 / Sonnet 4.5 / Haiku 4.5` 多槽位映射；未单独配置的 Claude 模型会回退到 `Default`。
- 提供商编辑页支持获取模型列表后直接选择，同时保留手动输入模型名称，便于接口不返回模型列表时继续使用。
- 新增 `Xiaomi MiMo (Pay for Token)` 和 `Xiaomi MiMo (Token Plan)` 两个预设，默认模型映射到 `mimo-v2.5-pro`，并补充 Token Plan 活动专属 Base URL 提示。
- 主题系统扩展为 6 套主题，链接、高亮背景、默认按钮和相关浅色态会随当前主题特征色一起变化。
- 收口首页、设置页和映射卡片的多处布局细节，包括按钮样式、悬停反馈、GitHub 行高度、状态区横向布局和主题色块展示。
- 修复 Windows 和 macOS 应用内更新链路：下载更新时现在会显示明确反馈，安装流程会更稳定地启动对应平台的安装包，解决“下载无反馈”和“应用内安装失败”的问题。

### 下载建议

- Windows 用户可以下载 `CC-Desktop-Switch-v1.0.14-Windows-Setup.exe` 或 `CC-Desktop-Switch-v1.0.14-Windows-Portable.zip`。
- macOS 用户可以下载 `CC-Desktop-Switch-v1.0.14-macOS-arm64.pkg` 或 `CC-Desktop-Switch-v1.0.14-macOS-arm64.dmg`。

## English

This release continues to tighten the desktop configuration flow and fixes the in-app update flow on both Windows and macOS.

### Highlights

- Reworked model mapping into a clearer two-column layout with dedicated slots for `Default / Opus 4.7 / Opus 4.6 / Opus 3 / Sonnet 4.6 / Sonnet 4.5 / Haiku 4.5`. Unmapped Claude models now fall back to `Default`.
- Provider editing now supports choosing from fetched model lists while still allowing manual model input when upstream model discovery is unavailable.
- Added `Xiaomi MiMo (Pay for Token)` and `Xiaomi MiMo (Token Plan)` built-in presets, with `mimo-v2.5-pro` as the default mapping and a dedicated Token Plan Base URL hint for promotional memberships.
- Expanded the theme system to six themes and made links, highlight backgrounds, default buttons, and related light states follow the active theme color.
- Refined multiple dashboard, settings, and mapping-card layout details, including button styling, hover feedback, GitHub row height, horizontal status layout, and theme swatch presentation.
- Fixed the Windows and macOS in-app update flow: the UI now shows clear download feedback, and the app more reliably starts the matching platform installer, which resolves missing feedback and failed in-app install issues.

### Downloads

- Windows users can choose `CC-Desktop-Switch-v1.0.14-Windows-Setup.exe` or `CC-Desktop-Switch-v1.0.14-Windows-Portable.zip`.
- macOS users can choose `CC-Desktop-Switch-v1.0.14-macOS-arm64.pkg` or `CC-Desktop-Switch-v1.0.14-macOS-arm64.dmg`.
