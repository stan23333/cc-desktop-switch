# CC Desktop Switch

CC Desktop Switch 是一个轻量桌面工具，用本地桌面界面管理第三方 API 提供商，并把 Claude Desktop 的第三方推理请求转发到 DeepSeek、Kimi、智谱、阿里云百炼等平台。
Windows 安装版和便携版默认会打开独立桌面窗口；浏览器地址只作为调试和备用入口。
Windows 版点击窗口关闭按钮时，应用会缩小到系统托盘继续运行；需要完全退出时，请右键托盘图标选择“退出”。

项目当前支持 Windows 和 macOS。Windows 版写入 Claude Desktop 的本机策略配置；macOS 版会写入 Claude Desktop 的本机 3P 配置，并自动定位当前生效的 `configLibrary` 配置条目。Linux 可以运行管理后台和代理，但 Claude Desktop 没有对应 GUI 版本。

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

最新版本在 GitHub Release：

```text
https://github.com/lonr-6/cc-desktop-switch/releases/latest
```

推荐普通用户下载：

- `CC-Desktop-Switch-v1.0.10-Windows-Setup.exe`：安装版
- `CC-Desktop-Switch-v1.0.10-Windows-Portable.zip`：便携版

macOS 版本由另一位维护者单独构建和发布。本次发布只上传 Windows 安装版、便携版和更新元数据。

Windows 版目前还没有 Authenticode 代码签名证书，系统可能提示未知发布者。Release 页面提供了 `.sha256` 和 `.sig` 文件用于校验下载完整性。

## 能做什么

- 管理 DeepSeek、Kimi、智谱、阿里云百炼等 API 提供商。
- 一键写入 Claude 桌面版第三方推理配置。
- 启动本机转发服务，把 Claude 模型名映射到上游模型。
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

CC Desktop Switch is a lightweight desktop app for connecting Claude Desktop to third-party API providers through a local gateway. It is designed for providers that expose Anthropic-compatible endpoints, including DeepSeek, Kimi, Zhipu GLM, and Alibaba Cloud Bailian.

1. Download the latest installer or portable package from [GitHub Releases](https://github.com/lonr-6/cc-desktop-switch/releases/latest).
2. Open CC Desktop Switch.
3. Pick a provider preset, enter your own API key, and adjust model mapping if needed.
4. Click `Apply to Claude Desktop`.
5. Fully restart Claude Desktop, then use Claude as usual.

Supported stable path:

- Anthropic-compatible APIs are the recommended path.
- DeepSeek 1M context, DeepSeek Max effort, and Qwen 1M context can be enabled from the provider edit page.

Experimental compatibility:

- OpenAI Chat, new-api, CPA reverse proxies, and OpenCode Go style endpoints are experimental.
- Basic text, streaming text, usage normalization, and common tool-call conversion are implemented, but not every third-party endpoint supports Claude Code tool usage correctly.
- CC-Switch import only auto-imports Anthropic-compatible items. OpenAI-format items are shown but skipped by default to avoid breaking a working configuration.

Security and limits:

- Your upstream API key is stored only in the local config file: `~/.cc-desktop-switch/config.json`.
- Claude Desktop receives only the local gateway address and gateway key; it does not receive your upstream provider API key directly.
- Windows builds are not Authenticode-signed yet, so Windows may show an unknown publisher warning.
- This project is not affiliated with Anthropic or CC-Switch.

## 默认端口

- 管理界面：`18081`
- 本机转发服务：`18080`

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

## 技术栈

- 后端：Python, FastAPI, httpx, uvicorn
- 前端：HTML, CSS, Vanilla JavaScript, Bootstrap 5.3 CDN
- 存储：`~/.cc-desktop-switch/config.json`
- 打包：PyInstaller, NSIS

## 安全说明

- API Key 只保存在本机配置文件中，不要上传 `~/.cc-desktop-switch/config.json`。
- “一键应用到 Claude 桌面版”会写入 Claude Desktop 在当前系统上使用的本机配置。Windows 使用本机策略配置；macOS 会同时覆盖 Claude Desktop 3P 根配置和当前选中的 `configLibrary` 条目。
- Claude Desktop 使用本工具生成的本地 gateway key 调用代理；真正的上游 API Key 不直接写进 Claude Desktop。

## 致谢

本项目的方向参考了 CC-Switch 这类社区工具的思路：用更轻的桌面界面降低 Claude Desktop / Claude Code 第三方 API 配置门槛。本项目不是 Anthropic 或 CC-Switch 官方项目，也不复用它们的商标、Logo 或发布身份。

## 许可证

MIT License。完整文本见 [LICENSE.txt](LICENSE.txt)。
