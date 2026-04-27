# CC Desktop Switch v1.0.13

本次版本以 `v1.0.12` 稳定版本为基线，收口 Claude Desktop 官方客户端的 Windows 体验。

## 主要变化

- Anthropic 兼容供应商默认写入 Claude Desktop 直连配置，应用并重启后，关闭本工具仍可继续使用当前供应商。
- 切换 provider 后会稳定同步 Claude Desktop 配置，避免界面显示已启用但桌面端仍使用旧配置。
- 打包版启动时申请管理员权限，减少切换 provider 时反复弹出终端窗口的问题。
- 修复托盘右键菜单里 provider 勾选状态显示异常的问题。
- 自动更新安装包启动改为隐藏窗口方式，减少安装过程中的黑色终端弹窗。
- 继续隐藏不稳定实验入口：配置库直连、显示全部模型、Claude Code settings-only 同步。
- README 增加 badges、Troubleshooting、项目定位说明和用户反馈入口。

## 下载建议

- 普通用户优先下载 `CC-Desktop-Switch-v1.0.13-Windows-Setup.exe`。
- 不想安装可以下载 `CC-Desktop-Switch-v1.0.13-Windows-Portable.zip`。

## 验证

- `python -m compileall -q backend main.py tests`
- `node --check frontend/js/api.js`
- `node --check frontend/js/app.js`
- `node --check frontend/js/i18n.js`
- `python -m unittest discover -s tests -v`
- `powershell -ExecutionPolicy Bypass -File scripts\Test-ReleaseSignature.ps1 -File release\CC-Desktop-Switch-v1.0.13-Windows-Setup.exe`
