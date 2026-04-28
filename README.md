# CC Desktop Switch

[![GitHub stars](https://img.shields.io/github/stars/lonr-6/cc-desktop-switch?style=social)](https://github.com/lonr-6/cc-desktop-switch/stargazers)
[![License](https://img.shields.io/github/license/lonr-6/cc-desktop-switch)](LICENSE.txt)
[![Python](https://img.shields.io/badge/Python-3.11%2B-blue?logo=python)](https://www.python.org/)
[![Downloads](https://img.shields.io/github/downloads/lonr-6/cc-desktop-switch/total?label=downloads)](https://github.com/lonr-6/cc-desktop-switch/releases)

CC Desktop Switch 是一个面向 **Claude Desktop 官方桌面客户端** 的轻量配置工具。它用桌面界面管理 DeepSeek、Kimi、智谱 GLM、阿里云百炼等 API 提供商，并一键写入 Claude Desktop 的第三方推理配置。

和 `farion1231/cc-switch` 这类偏 Claude Code / CLI 的工具不同，本项目的定位是：让普通用户在 Windows 和 macOS 上更方便地配置 Claude Desktop 官方客户端。

Windows 安装版和便携版默认会打开独立桌面窗口；浏览器地址只作为调试和备用入口。点击窗口关闭按钮时，应用会缩小到系统托盘继续运行；需要完全退出时，请右键托盘图标选择“退出”。

Windows 和 macOS 稳定路径默认使用直连配置：完成“一键应用”并重启 Claude Desktop 后，即使关闭本工具，Claude Desktop 仍可以继续使用当前供应商。OpenAI / new-api / 反代类接口属于实验兼容路径，必要时才使用本机转发服务。

macOS 版本由 macOS 维护者单独同步；Linux 可以运行管理后台和代理，但 Claude Desktop 没有对应 GUI 版本。

## 界面预览

<table>
  <tr>
    <td width="50%">
      <img src="docs/promo/screenshots/readme-provider-list.png" alt="Provider 管理页面">
    </td>
    <td width="50%">
      <img src="docs/promo/screenshots/readme-deepseek-config.png" alt="添加 DeepSeek Provider">
    </td>
  </tr>
  <tr>
    <td align="center">Provider 管理和快速切换</td>
    <td align="center">选择预设后自动填入 API 地址和推荐模型</td>
  </tr>
  <tr>
    <td width="50%">
      <img src="docs/promo/screenshots/readme-deepseek-options.png" alt="DeepSeek 1M 和 Max 思维配置">
    </td>
    <td width="50%">
      <img src="docs/promo/screenshots/readme-qwen-1m-menu.png" alt="通义千问 1M 模型菜单">
    </td>
  </tr>
  <tr>
    <td align="center">DeepSeek 1M 上下文和 Max 思维开关</td>
    <td align="center">通义千问 1M 上下文模型写入 Claude 桌面版</td>
  </tr>
</table>

## 使用效果

<table>
  <tr>
    <td width="50%">
      <img src="docs/promo/screenshots/readme-deepseek-1m-context.png" alt="DeepSeek 1M 上下文窗口">
    </td>
    <td width="50%">
      <img src="docs/promo/screenshots/readme-qwen-1m-context.png" alt="通义千问 1M 上下文窗口">
    </td>
  </tr>
  <tr>
    <td align="center">DeepSeek 1M 上下文在 Claude 桌面版中生效</td>
    <td align="center">通义千问 1M 上下文在 Claude 桌面版中生效</td>
  </tr>
  <tr>
    <td width="50%">
      <img src="docs/promo/screenshots/readme-kimi-menu.png" alt="Kimi 模型菜单">
    </td>
    <td width="50%">
      <img src="docs/promo/screenshots/readme-glm-menu.png" alt="智谱 GLM 模型菜单">
    </td>
  </tr>
  <tr>
    <td align="center">Kimi 模型显示和切换</td>
    <td align="center">智谱 GLM 模型显示和切换</td>
  </tr>
</table>

<details>
  <summary>更多模型菜单展示</summary>

  <table>
    <tr>
      <td width="50%">
        <img src="docs/promo/screenshots/readme-deepseek-menu.png" alt="DeepSeek 模型菜单">
      </td>
      <td width="50%">
        <img src="docs/promo/screenshots/readme-qwen-menu.png" alt="通义千问模型菜单">
      </td>
    </tr>
    <tr>
      <td align="center">DeepSeek 模型菜单和思维深度选项</td>
      <td align="center">通义千问模型菜单和思维深度选项</td>
    </tr>
  </table>
</details>

## 下载

最新已发布版本在 GitHub Release：

```text
https://github.com/lonr-6/cc-desktop-switch/releases/latest
```

推荐普通用户下载：

- `CC-Desktop-Switch-v<版本>-Windows-Setup.exe`：Windows 安装版
- `CC-Desktop-Switch-v<版本>-Windows-Portable.zip`：Windows 便携版

macOS 版本由 macOS 维护者单独同步；如果 Release 页面出现 `macOS-arm64.dmg` 或 `macOS-arm64.pkg`，DMG 为拖拽安装版，PKG 会安装到 `/Applications/CC Desktop Switch.app`。

Windows 版目前还没有 Authenticode 代码签名证书，系统可能提示未知发布者。Release 页面提供了 `.sha256` 和 `.sig` 文件用于校验下载完整性。

如果这个工具对你有帮助，欢迎 Star 一下不迷路。遇到问题、想支持新的 API 厂商，或者有更顺手的交互建议，都可以直接发到 [Issues](https://github.com/lonr-6/cc-desktop-switch/issues)。真实用户反馈会优先排期，也能帮助更多人发现这个项目。

## 能做什么

- 管理 DeepSeek、Kimi、智谱、阿里云百炼等 API 提供商。
- 一键写入 Claude 桌面版第三方推理配置。
- Anthropic 兼容接口默认直连，关闭本工具后 Claude Desktop 仍可继续使用。
- OpenAI / new-api / 反代类接口保留实验兼容模式，需要时走本机转发。
- 对提供商 API 地址做基础连通测速。
- 主流程使用 Anthropic 兼容接口；后端保留 OpenAI 转换兼容，用于旧配置或自定义接口。
- 支持 SSE 流式转发。
- 提供中文/英文界面和浅色/深色模式。

## 基本用法

1. 启动 CC Desktop Switch。
2. 在弹出的桌面窗口里操作。
3. 选择快捷预设，填写自己的 API Key，必要时调整模型映射。
4. 点击“一键应用到 Claude 桌面版”。
5. 重启 Claude Desktop 后测试。

更详细的步骤见 [使用说明](docs/USAGE.md) 和 [图文快速教程](docs/QUICK_START.md)。

如果桌面窗口无法打开，可以手动访问备用地址：

```text
http://127.0.0.1:18081
```

## English Quick Start

CC Desktop Switch is a lightweight desktop app for the official Claude Desktop client. It helps Windows and macOS users configure third-party Anthropic-compatible API providers such as DeepSeek, Kimi, Zhipu GLM, and Alibaba Cloud Bailian.

1. Download the latest installer or portable package from [GitHub Releases](https://github.com/lonr-6/cc-desktop-switch/releases/latest).
2. Open CC Desktop Switch.
3. Pick a provider preset, enter your own API key, and adjust model mapping if needed.
4. Click `Apply to Claude Desktop`.
5. Fully restart Claude Desktop, then use Claude as usual.

Supported stable path:

- Anthropic-compatible APIs are the recommended path.
- The default Windows and macOS flow writes the selected provider directly into Claude Desktop's local configuration. After applying and fully restarting Claude Desktop, the selected provider can keep working even if CC Desktop Switch is closed.
- DeepSeek 1M context, DeepSeek Max effort, and Qwen 1M context can be enabled from the provider edit page.

Experimental compatibility:

- OpenAI Chat, new-api, CPA reverse proxies, and OpenCode Go style endpoints are experimental.
- Basic text, streaming text, usage normalization, and common tool-call conversion are implemented, but not every third-party endpoint supports Claude Code tool usage correctly.
- CC-Switch import only auto-imports Anthropic-compatible items. OpenAI-format items are shown but skipped by default to avoid breaking a working configuration.

Security and limits:

- Your upstream API key is stored only in the local config file: `~/.cc-desktop-switch/config.json`.
- For stable Anthropic-compatible providers, the app writes the selected provider URL and API key into Claude Desktop's local machine/user configuration so Claude Desktop can work without the app running.
- Experimental OpenAI/new-api/reverse-proxy providers may still require the local forwarding service.
- Windows builds are not Authenticode-signed yet, so Windows may show an unknown publisher warning.
- This project is not affiliated with Anthropic or CC-Switch.

## 默认端口

- 管理界面：`18081`
- 本机转发服务：`18080`，仅实验兼容接口或调试时需要。

## 本地开发

```powershell
git clone https://github.com/lonr-6/cc-desktop-switch.git
cd cc-desktop-switch
pip install -r requirements.txt
python main.py
```

默认会打开桌面窗口。调试时也可以用浏览器模式：

```powershell
python main.py --browser
```

## 验证

```powershell
python -m compileall -q backend main.py
python -m unittest discover -s tests -v
node --check frontend/js/api.js
node --check frontend/js/app.js
node --check frontend/js/i18n.js
```

## Troubleshooting

### Claude 重启后没有生效

Claude Desktop 会在启动时读取第三方推理配置。切换供应商后，请完整退出 Claude Desktop，再重新打开。只关闭聊天窗口通常不够，可以在任务栏托盘里退出 Claude，或在任务管理器里确认没有残留的 Claude 进程。

如果仍然没有变化，请在 CC Desktop Switch 里重新点击当前供应商的“启用”或“一键应用到 Claude 桌面版”，看到成功提示后再重启 Claude Desktop。

### 端口冲突

普通 Anthropic 兼容供应商默认直连，不依赖本机转发端口。只有实验兼容接口、调试后台或浏览器备用入口会用到端口：

- 管理界面：`18081`
- 本机转发：`18080`

如果页面打不开或实验接口无法连接，可以检查端口占用：

```powershell
netstat -ano | findstr :18081
netstat -ano | findstr :18080
```

看到占用后，可以关闭对应程序，或在设置里换一个端口后重新应用配置。

### 防火墙或安全软件拦截

如果使用直连模式，请确认当前电脑可以访问对应供应商的 API 域名。比如 DeepSeek、Kimi、智谱、阿里云百炼的 API 地址需要能正常出站访问。

如果使用实验本机转发模式，请允许本工具监听 `127.0.0.1`。它不是系统代理，也不会接管全局网络，只是给 Claude Desktop 提供一个本机 API 入口。

### Windows 提示未知发布者

当前 Windows 构建还没有 Authenticode 代码签名证书，所以 Windows 可能提示未知发布者。Release 页面提供 `.sha256` 和 `.sig`，可以用于校验安装包没有被替换。

### 切换供应商后模型名看起来没变

先确认你已经完整重启 Claude Desktop。部分模型会在回答里自称 Claude 或 Sonnet，这不一定代表实际没有切换。更可靠的验证方式是看 Claude 模型菜单、供应商后台调用记录，或在本工具里检查当前启用的供应商。

## 技术栈

- 后端：Python, FastAPI, httpx, uvicorn
- 前端：HTML, CSS, Vanilla JavaScript, Bootstrap 5.3 CDN
- 存储：`~/.cc-desktop-switch/config.json`
- 打包：PyInstaller, NSIS

## 安全说明

- API Key 只保存在本机配置文件中，不要上传 `~/.cc-desktop-switch/config.json`。
- “一键应用到 Claude 桌面版”会写入 Claude Desktop 在当前系统上使用的本机配置。Windows 和 macOS 稳定路径会写入当前供应商的 API 地址、API Key、认证方案、额外请求头和模型列表，这样关闭本工具后 Claude Desktop 也能继续使用。
- OpenAI / new-api / 反代类实验接口可能需要本机转发服务；这类模式下请保持本工具运行。
- 不要把 `~/.cc-desktop-switch/config.json`、截图里的完整 API Key、或 Claude Desktop 的本机配置文件上传到公开仓库。

## 致谢

本项目的方向参考了 CC-Switch 这类社区工具的思路：用更轻的桌面界面降低第三方 API 配置门槛。本项目专注 Claude Desktop 官方客户端，不是 Anthropic、CC-Switch 或 `farion1231/cc-switch` 官方项目，也不复用它们的商标、Logo 或发布身份。

## 许可证

MIT License。完整文本见 [LICENSE.txt](LICENSE.txt)。
