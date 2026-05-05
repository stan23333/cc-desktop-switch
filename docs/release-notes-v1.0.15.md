# CC Desktop Switch v1.0.15

## 中文

本次版本优化了桌面版配置状态的提示体验，新增模型可用性检测、上游代理开关，并改进了部分内置预设的交互细节。

### 主要变化

- **新增顶部全局健康横幅**：将原来位于页面底部的桌面版配置警告移到顶部，避免用户忽略。
  - 横幅内集成"应用"按钮，点击即可一键重新应用到 Claude 桌面版，无需翻找底部操作区。
  - 完全未配置（如清除配置后）也会触发警告提示，引导用户重新添加提供商并应用。

- **新增模型可用性检测**：在提供商编辑页的模型映射区域，每个映射行旁新增"检测"按钮。
  - 通过发送最小对话请求（而非仅查询模型列表）来验证模型是否真实可用。
  - 检测结果在模型映射卡片上方以 Alert 弹窗展示，5 秒后自动消失，也可手动关闭。

- **新增上游代理开关**：设置页「上游代理」旁新增启用/禁用开关。
  - 默认关闭；开启后可配置 HTTP 代理用于访问上游 API。
  - 关闭后完全禁用代理（包括不再读取系统环境变量 `HTTP_PROXY` / `HTTPS_PROXY`）。
  - 支持自动检测本机常见代理端口。

- **改进小米 MiMo (Token Plan) 预设**：
  - Base URL 下拉框改为三个地区选项：中国集群 / 新加坡集群 / 欧洲集群。
  - 说明文字更新为引导用户按账号所属地区选择。

- **UI 细节优化**：
  - "移除"映射按钮改为红色危险风格，以提示高危操作。
  - 模型映射行按钮宽度收窄并增加间距。

- **错误提示增强**：代理层超时和连接错误提示增加网络排查建议，方便中国大陆用户定位问题。

### 下载建议

- Windows 用户可以下载 `CC-Desktop-Switch-v1.0.15-Windows-Setup.exe` 或 `CC-Desktop-Switch-v1.0.15-Windows-Portable.zip`。
- macOS 用户可以下载 `CC-Desktop-Switch-v1.0.15-macOS-arm64.pkg` 或 `CC-Desktop-Switch-v1.0.15-macOS-arm64.dmg`。

## English

This release improves the desktop configuration status alerting experience, adds model availability checks, an upstream proxy toggle, and refines some built-in preset interactions.

### Highlights

- **Top global health banner**: moved the desktop configuration warning from the bottom to the top so users won't miss it.
  - An "Apply" button is integrated inside the banner for one-click reapply to Claude Desktop.
  - A fully unconfigured state (e.g. after clearing config) now also triggers a warning.

- **Model availability check**: a "Check" button is added next to each mapping row on the provider edit page.
  - Verifies whether a model actually works by sending a minimal chat request (not just listing models).
  - Results are shown as an Alert banner above the mapping card, auto-dismisses after 5 seconds, and can be manually closed.

- **Upstream proxy toggle**: an on/off switch is added next to "Upstream Proxy" in Settings.
  - Defaults to off; when turned on, you can configure an HTTP proxy for upstream API access.
  - When turned off, the proxy is completely disabled (including system environment variables like `HTTP_PROXY` / `HTTPS_PROXY`).
  - Supports auto-detection of common local proxy ports.

- **Improved Xiaomi MiMo (Token Plan) preset**:
  - Base URL dropdown now offers three region options: China / Singapore / Europe.
  - Hint text updated to guide users to select the region matching their account.

- **UI refinements**:
  - "Remove" mapping button now uses a red danger style to indicate a destructive action.
  - Mapping row buttons are narrower with added spacing.

- **Enhanced error messages**: timeout and connection error hints now include network troubleshooting guidance, helpful for users in mainland China.

### Downloads

- Windows users can choose `CC-Desktop-Switch-v1.0.15-Windows-Setup.exe` or `CC-Desktop-Switch-v1.0.15-Windows-Portable.zip`.
- macOS users can choose `CC-Desktop-Switch-v1.0.15-macOS-arm64.pkg` or `CC-Desktop-Switch-v1.0.15-macOS-arm64.dmg`.
