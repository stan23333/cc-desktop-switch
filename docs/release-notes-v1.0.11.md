# CC Desktop Switch v1.0.11

本次版本是一个小范围收口版本，重点解决用户找不到“清除桌面版配置”的问题，并修正清除后的策略残留。

## 主要变化

- 首页快捷操作区新增“清除桌面版配置”按钮。
- 点击清除前会弹出通俗提醒：只清除 Claude 桌面版连接配置，不删除本工具保存的提供商和 API Key。
- 从首页清除后会自动刷新桌面版状态。
- 清除 Windows Claude policy 时补充删除 `isClaudeCodeForDesktopEnabled`，避免旧版本残留导致 Claude 仍显示 organization-managed。
- README 和使用说明更新为 Windows 自动发布口径；macOS 版本由 macOS 维护者单独同步。

## 隐私说明

- 本版本没有提交本机 `~/.cc-desktop-switch/config.json`、配置备份、`.env`、PFX、私钥或真实 API Key。
- 清除桌面版配置只删除 Claude Desktop 读取的本机连接策略，不会删除本工具里的 provider 配置。

## 验证

- `python -m compileall -q backend main.py tests`
- `node --check frontend/js/api.js`
- `node --check frontend/js/app.js`
- `node --check frontend/js/i18n.js`
- `git diff --check`
- `python -m unittest discover -s tests -v`

## 下载建议

- 普通用户优先下载 `CC-Desktop-Switch-v1.0.11-Windows-Setup.exe`。
- 不想安装可以下载 `CC-Desktop-Switch-v1.0.11-Windows-Portable.zip`。
- macOS 版本如需同步，请等待 macOS 维护者上传对应资产。
