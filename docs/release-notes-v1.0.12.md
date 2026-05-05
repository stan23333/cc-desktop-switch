# CC Desktop Switch v1.0.12

本次版本修正 `v1.0.11` 中“清除桌面版配置”按钮位置不明显的问题。

## 主要变化

- 将“清除桌面版配置”按钮移动到首页右上角动作区，位于“导入 CC Switch 配置”和添加按钮之间。
- 保留点击前确认弹窗：只清除 Claude 桌面版连接配置，不删除本工具保存的提供商和 API Key。
- 补充静态测试，确保按钮保持在首页第一屏动作区。

## 验证

- `node --check frontend/js/app.js`
- `node --check frontend/js/i18n.js`
- `python -m unittest tests.test_provider_config_and_proxy.StaticFrontendTests.test_dashboard_presets_selection_update_and_desktop_health_ui_exist -v`
- `git diff --check`
- `python -m unittest discover -s tests -v`

## 下载建议

- 普通用户优先下载 `CC-Desktop-Switch-v1.0.12-Windows-Setup.exe`。
- 不想安装可以下载 `CC-Desktop-Switch-v1.0.12-Windows-Portable.zip`。
