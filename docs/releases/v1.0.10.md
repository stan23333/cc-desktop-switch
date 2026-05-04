# CC Desktop Switch v1.0.10

本次版本继续收口 Windows 桌面版体验，重点是 CC-Switch 导入、第三方接口兼容说明和文档更新。

## 主要变化

- 新增 CC-Switch 配置导入：只自动导入 Anthropic 兼容项，导入前会自动备份当前配置。
- 导入 CC-Switch 配置时不会覆盖现有 provider；同名同地址会追加 `CC Switch 导入` 后缀。
- 添加/编辑 provider 页面新增“高级：第三方兼容接口”区域，默认展开，并用红框标出。
- 第三方兼容接口提供 `Anthropic 兼容` 和 `OpenAI Chat 实验` 两种选择。
- 设置页新增第三方兼容性检查入口，用于区分稳定主线和实验适配。
- OpenAI Chat 实验适配补充 usage 归一化和常见非流式工具调用转换；流式工具调用暂未标为稳定能力。
- 暂时隐藏“显示全部模型”入口，后续验证成熟后再开放。
- README 增加英文 Quick Start，并说明实验性第三方 API 的边界。

## 平台说明

- 本次发布 Windows 安装版、Windows 便携版、macOS DMG、macOS PKG 和 `latest.json` 更新元数据。
- macOS 的应用内更新会按平台读取 `macos-<arch>` 资产，优先打开 PKG 安装器，DMG 作为拖拽安装兜底。
- macOS 版点击关闭窗口会隐藏应用并保持本机转发服务运行；再次点击 Dock 图标会恢复主窗口，使用 `Cmd+Q` 或退出菜单才会停止应用。

## 隐私说明

- 本版本没有提交本机 `~/.cc-desktop-switch/config.json`、配置备份、`.env`、PFX、私钥或真实 API Key。
- CC-Switch 导入预览只展示 API Key 掩码，导入接口只允许本机请求调用。
- Claude Desktop 只接收本地 gateway 地址、gateway key 和模型列表，不直接接收上游 provider API Key。

## 验证

- `python -m compileall -q backend main.py tests`
- `node --check frontend/js/api.js`
- `node --check frontend/js/app.js`
- `node --check frontend/js/i18n.js`
- `python -m unittest discover -s tests -v`

## 下载建议

- 普通用户优先下载 `CC-Desktop-Switch-v1.0.10-Windows-Setup.exe`。
- 不想安装可以下载 `CC-Desktop-Switch-v1.0.10-Windows-Portable.zip`。
- macOS 用户可以下载 `CC-Desktop-Switch-v1.0.10-macOS-arm64.dmg` 或 `CC-Desktop-Switch-v1.0.10-macOS-arm64.pkg`。
