# DeepSeek + Claude Desktop 问题排查报告

> 基于文档《DeepSeek功能有问题.md》及本项目代码（CC Desktop Switch v1.0.15）的交叉排查

---

## 一、文档中的问题归类

原文档记录的是 **Claude Code**（CLI / VSCode 扩展）使用 DeepSeek 时的踩坑记录，核心问题三类：

| 问题 | 配置位置 | 根因 |
|------|----------|------|
| 权限静默 | `~/.claude/settings.json` + VSCode settings | Claude Code 默认每次工具调用都弹窗确认 |
| WebFetch 预检 | `~/.claude/settings.json` | 抓网页前向 `claude.ai` 发预检，大陆网络被拦截 |
| DeepSeek 功能受限 | DeepSeek API 本身 | Anthropic 兼容层不支持部分 Claude 原生功能 |

**关键区别**：文档中的配置项（`permissions.defaultMode`、`skipWebFetchPreflight` 等）属于 **Claude Code** 的配置体系；而本项目（CC Desktop Switch）配置的是 **Claude Desktop** 的第三方推理（3P）体系。两者配置位置、作用范围完全不同。

---

## 二、Claude Desktop 与 Claude Code 的配置体系对比

| 维度 | Claude Desktop（本项目配置） | Claude Code（文档涉及） |
|------|------------------------------|------------------------|
| 配置文件 | Windows: `HKCU\SOFTWARE\Policies\Claude`<br>macOS: `~/Library/Preferences/com.anthropic.claudefordesktop.plist`<br>+ `claude_desktop_config.json` | `~/.claude/settings.json`<br>VSCode: `settings.json` |
| 配置项前缀 | `inferenceGateway*`, `inferenceModels` | `permissions.*`, `env.*`, `skipWebFetchPreflight` |
| 功能范围 | 第三方 API 地址、Key、模型映射、认证方案 | 工具权限、网页抓取、环境变量、IDE 集成 |
| 本工具支持 | ✅ 完整支持 | ❌ 未支持（也不应混为一谈） |

**结论**：文档中的解决方案（改 `settings.json`、改 VSCode 配置）无法通过本项目自动完成，需要用户手动配置 Claude Code。

---

## 三、存在问题 ==> 解决方案

### 问题 1：本项目未向用户说明 Claude Desktop vs Claude Code 的边界

**现象**：用户容易混淆 Claude Desktop 和 Claude Code，以为配置好 Desktop 的第三方推理后，Code 也能自动使用 DeepSeek。

**根因**：
- Claude Desktop 和 Claude Code 是两个独立产品
- Desktop 的 3P 配置不会同步到 Code
- Code 有自己的 `settings.json` 和权限体系

**解决方案**：
- 在 README / USAGE 中增加明确说明：
  > "本工具配置的是 Claude Desktop 官方客户端的第三方推理。Claude Code（CLI / VSCode 扩展）需要单独配置 `~/.claude/settings.json`，两者互不干扰。"
- 在 DeepSeek 预设说明中补充 Claude Code 配置指引的链接

---

### 问题 2：DeepSeek Anthropic 兼容接口可能不支持 Tools（工具调用）

**现象**：Claude Desktop 开启 MCP / Tools 后，使用 DeepSeek 作为上游时，工具调用可能失败或行为异常。

**根因**：
- 本项目代理层对 Anthropic 格式的请求是**透传**的（`proxy.py:347-353`）
- `tools` 字段原样发给 DeepSeek 的 Anthropic 兼容端点
- DeepSeek 的 Anthropic 兼容层目前**未公开支持** `tools` / `tool_choice` / `tool_use` 等字段
- 对比：OpenAI 格式转换时，`api_adapters.py:175-179` 会显式转换 tools，说明开发者意识到 tools 需要特殊处理

**代码佐证**：
```python
# proxy.py:347-353 — Anthropic 格式直接透传，没有 tools 处理
upstream_body = dict(body)
upstream_body.pop("stream", None)
upstream_body = apply_anthropic_request_options(upstream_body, provider)

# api_adapters.py:175-179 — OpenAI 格式才有 tools 转换
tools = _anthropic_tools_to_openai(body.get("tools"))
if tools:
    openai_body["tools"] = tools
```

**解决方案**：
- **短期**：在文档中明确标注 "DeepSeek Anthropic 兼容接口当前不支持 Tools / MCP，如需工具调用请使用官方 Claude API 或其他支持 tools 的提供商"
- **中期**：在代理层增加检测，当上游是 DeepSeek 且请求含 `tools` 时，返回友好的错误提示：
  > "DeepSeek 当前不支持工具调用，请在 Claude Desktop 设置中关闭相关 MCP / Tools"
- **长期**：等待 DeepSeek 更新 Anthropic 兼容层支持 tools

---

### 问题 3：Claude Desktop 的 WebFetch / WebSearch 仍可能受网络环境影响

**现象**：即使配置了 DeepSeek，Claude Desktop 内置的 WebFetch、WebSearch 功能仍可能提示 "Unable to verify domain"。

**根因**：
- 文档中已指出：Claude Code 的 WebFetch 会先向 `claude.ai` 发预检请求
- **Claude Desktop 同理**：Desktop 内置的网页抓取、搜索等功能，在触发前也可能需要与 Anthropic 服务通信（验证域名、获取搜索 token 等）
- 这些请求**不经过本工具的代理**，是 Claude Desktop 自身的行为
- 中国大陆网络对 `claude.ai` 有拦截，导致预检失败

**解决方案**：
- 在文档 Troubleshooting 中补充说明：
  > "Claude Desktop 的 WebFetch / WebSearch 等功能依赖 Anthropic 在线服务（claude.ai），与第三方 API 配置无关。如果这些功能不可用，属于 Claude Desktop 自身的网络限制，需要配合合适的网络环境使用。"
- **注意**：`skipWebFetchPreflight` 是 Claude Code 的配置项，Claude Desktop 目前没有公开的等价配置

---

### 问题 4：`isClaudeCodeForDesktopEnabled` 可能引发 organization-managed 提示

**现象**：部分用户反馈配置后出现 "Your organization manages this application" 或类似提示，影响使用。

**根因**：
- 本项目在写入 Claude Desktop 配置时，会同时写入 `isClaudeCodeForDesktopEnabled = 1`（`registry.py:24, 230, 283, 519`）
- 该配置项启用 Claude Desktop 内的 Claude Code 功能
- 在某些企业环境或旧版本 Claude Desktop 中，这个标记可能被误识别为 organization-managed 配置
- v1.0.11 已修复清除逻辑（清除时会删除此字段），但写入时仍保留

**代码佐证**：
```python
# registry.py:24
DESKTOP_CONFIG = {
    ...
    "isClaudeCodeForDesktopEnabled": (1, int),
}
```

**解决方案**：
- 评估是否需要继续写入 `isClaudeCodeForDesktopEnabled`
- 如果该字段对核心功能（第三方推理）没有实质作用，建议移除，避免副作用
- 或者改为可配置项，让用户自行决定是否启用 Claude Code for Desktop

---

### 问题 5：DeepSeek "Max 思维" 与 Claude Desktop 的 effort UI 不匹配

**现象**：用户勾选 "DeepSeek Max 思维" 后，Claude Desktop 界面仍显示 `Low` / `Medium` / `High`，但实际生效的是 DeepSeek 的 max effort。

**根因**：
- 本工具在 `config.py:68-71` 中配置：`thinking: {type: "enabled"}` + `output_config: {effort: "max"}`
- 代理层对 DeepSeek 透传这些字段（`proxy.py:255-256`）
- 但 Claude Desktop 的 effort UI 是客户端本地的，它不知道上游实际是 DeepSeek，仍按 Anthropic 的 effort 级别显示
- 这是**预期行为**，文档中已有说明（"Claude 界面可能仍显示 High"）

**解决方案**：
- 当前处理已正确，无需修改代码
- 建议在 UI 提示中更明确地说明：
  > "Claude Desktop 的 effort 滑块仅影响界面显示，实际推理深度由 DeepSeek 的 Max 思维选项控制"

---

### 问题 6：1M 上下文模型在 gateway 模式下可能被错误映射

**现象**：用户选择 `deepseek-v4-pro[1m]` 后，某些场景下模型名被映射回普通版本。

**根因**：
- `proxy.py:133-136` 有保护逻辑：如果模型名已在 provider 暴露的模型列表中，直接透传
- 但如果 Claude Desktop 发送的是别名（如 `claude-sonnet-4-6`），会走 `map_model` 逻辑
- `map_model` 通过 `resolve_requested_model_slot` 解析槽位，再从模型映射中查找
- 如果模型映射配置不正确，1M 模型可能被映射回非 1M 版本

**代码佐证**：
```python
# proxy.py:133-136
if original_model in provider_model_ids(provider):
    return original_model
```

**解决方案**：
- 当前代码已有保护措施，一般场景下没问题
- 建议在前端增加更明显的提示：当用户开启 1M 上下文选项时，显示当前映射的完整模型名

---

## 四、总结

| 优先级 | 问题 | 建议措施 |
|--------|------|----------|
| 🔴 高 | 用户混淆 Claude Desktop 与 Claude Code | 文档增加明确边界说明 |
| 🔴 高 | DeepSeek 不支持 Tools / MCP | 文档标注限制；代理层增加友好提示 |
| 🟡 中 | `isClaudeCodeForDesktopEnabled` 副作用 | 评估是否移除该字段 |
| 🟡 中 | WebFetch 网络限制 | Troubleshooting 补充说明 |
| 🟢 低 | effort UI 显示不一致 | 优化 UI 提示文案 |
| 🟢 低 | 1M 模型映射 | 前端增加映射确认提示 |

**核心结论**：文档中的问题本质是 **Claude Code** 的配置问题，本工具（配置 Claude Desktop 第三方推理）无法直接解决。但本项目应在文档中明确区分两者边界，并标注 DeepSeek 在 Claude Desktop 下的已知限制（尤其是 Tools / MCP 不支持）。
