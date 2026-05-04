# CC Desktop Switch v1.0.17

## 中文

本次版本继续完善第三方模型接入、协议识别、更新下载反馈、DeepSeek V4 适配，并同步回传新版桌面界面美化方案。

### 主要变化

- **新增通用第三方模型入口**：
  - 新增「第三方模型」预设，用于手动接入未内置的 Anthropic / OpenAI 兼容接口。
  - 支持通用 1M 上下文和 Max 思维选项，保留用户手动填写模型映射的灵活性。
  - 新增协议类型探测按钮，可根据实际端点响应识别 Anthropic 兼容、OpenAI Chat 或 OpenAI Responses 风格接口。
- **界面美化和布局优化**：
  - 将 CodeX APP Transfer 改造后的视觉方案回传到本项目，同时保留 CC Desktop Switch 的现有功能边界。
  - 顶部继续保留「导入 CC Switch 配置」和「清除桌面版配置」快捷按钮，避免误删本项目需要的 CC-Switch 导入入口。
  - Claude 桌面版配置警告现在移动到顶部按钮行下方，保持原有警告样式不变，但更容易被用户看到。
  - 设置页左右列调整为约 1:2，左侧标签更紧凑，右侧按钮和输入控件获得更宽操作空间。
- **优化更新安装体验**：
  - 更新包下载时增加进度状态，前端会轮询下载进度并显示当前百分比。
  - 安装包启动失败或下载失败时给出更明确的界面反馈。
- **优化 Max 思维错误提示**：
  - 当上游模型不支持 `thinking` / `output_config.effort=max` 时，本地代理会返回更友好的提示，提醒用户取消 Max 选项。
- **更新 DeepSeek V4 适配**：
  - 保持默认模型映射为 `deepseek-v4-pro` / `deepseek-v4-flash`。
  - DeepSeek 1M 选项现在会同时把 `deepseek-v4-pro[1m]` 和 `deepseek-v4-flash` 写入 1M 能力声明。
  - 桌面版健康检查会确认所有声明为 1M 的模型都已写入 `supports1m`，避免部分模型漏写时误判正常。
  - 文档同步更新 DeepSeek Anthropic API 的工具调用兼容范围，移除“DeepSeek 不支持 Tools / MCP”的过时笼统结论。

### 下载建议

- Windows 用户可以下载 `CC-Desktop-Switch-v1.0.17-Windows-Setup.exe` 或 `CC-Desktop-Switch-v1.0.17-Windows-Portable.zip`。
- macOS 用户可以下载 `CC-Desktop-Switch-v1.0.17-macOS-arm64.pkg` 或 `CC-Desktop-Switch-v1.0.17-macOS-arm64.dmg`。

## English

This release continues improving third-party model setup, protocol detection, update download feedback, DeepSeek V4 compatibility, and brings the refreshed desktop UI back into this project.

### Highlights

- **New generic third-party model entry**:
  - Adds a "Third-party Model" preset for manually connecting non-builtin Anthropic / OpenAI compatible endpoints.
  - Supports generic 1M context and Max effort options while keeping manual model mapping flexible.
  - Adds a protocol detection button that identifies Anthropic-compatible, OpenAI Chat, or OpenAI Responses-style endpoints from live responses.
- **Refreshed UI and layout**:
  - Ports the visual refresh from the CodeX APP Transfer work back into this project while preserving CC Desktop Switch behavior.
  - Keeps the header shortcuts for "Import CC Switch Config" and "Clear Desktop Config", including the CC-Switch import entry this project still needs.
  - Moves the Claude Desktop configuration warning below the top action row without changing the warning style, making it visible earlier.
  - Adjusts Settings rows to an approximately 1:2 label/control layout so controls have more usable horizontal space.
- **Improved update install experience**:
  - Shows download progress while fetching update packages.
  - Provides clearer UI feedback when download or installer launch fails.
- **Better Max effort error hint**:
  - When an upstream model does not support `thinking` / `output_config.effort=max`, the local proxy now returns a friendlier hint asking the user to disable the Max option.
- **Updated DeepSeek V4 compatibility**:
  - Keeps the default model mapping on `deepseek-v4-pro` / `deepseek-v4-flash`.
  - The DeepSeek 1M option now marks both `deepseek-v4-pro[1m]` and `deepseek-v4-flash` as 1M-capable.
  - Desktop health checks now require every declared 1M model to be written with `supports1m`, avoiding false positives when only part of the model list is updated.
  - Documentation now reflects the current DeepSeek Anthropic API tool compatibility range and removes the outdated blanket statement that DeepSeek does not support Tools / MCP.

### Downloads

- Windows users can choose `CC-Desktop-Switch-v1.0.17-Windows-Setup.exe` or `CC-Desktop-Switch-v1.0.17-Windows-Portable.zip`.
- macOS users can choose `CC-Desktop-Switch-v1.0.17-macOS-arm64.pkg` or `CC-Desktop-Switch-v1.0.17-macOS-arm64.dmg`.
