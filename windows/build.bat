@echo off
title CC Desktop Switch - Tauri Build Tool
chcp 65001 >nul
cd /d "%~dp0\.."

:MENU
cls
echo ========================================
echo    CC Desktop Switch v1.1.0 - Tauri Build
echo ========================================
echo.
echo  请选择打包方式：
echo.
echo    [1] Tauri 可执行文件       —— 仅编译 release exe，不生成安装器
echo    [2] Tauri NSIS 安装包      —— 当前用户安装，创建快捷方式
echo    [3] Release 产物           —— 生成 Setup、Portable ZIP、x64 EXE、latest.json
echo.
echo    [Q] 退出
echo.

choice /c 123Q /n /m "请输入选项 (1/2/3/Q): "
set choice=%errorlevel%

if %choice%==4 exit /b 0
if %choice%==1 set MODE=exe
if %choice%==2 set MODE=nsis
if %choice%==3 set MODE=release
if not defined MODE goto MENU

where pnpm >nul 2>&1
if %errorlevel% neq 0 (
    echo [错误] 未找到 pnpm。请先安装 Node.js 并启用 corepack。
    pause
    exit /b 1
)

where cargo >nul 2>&1
if %errorlevel% neq 0 (
    echo [错误] 未找到 cargo。请先安装 Rust 工具链。
    pause
    exit /b 1
)

if "%MODE%"=="exe" (
    pnpm install --frozen-lockfile
    if %errorlevel% neq 0 goto FAIL
    pnpm tauri build --no-bundle --no-sign --ci
    if %errorlevel% neq 0 goto FAIL
    echo.
    echo  输出文件: src-tauri\target\release\cc-desktop-switch.exe
    goto DONE
)

if "%MODE%"=="nsis" (
    pnpm install --frozen-lockfile
    if %errorlevel% neq 0 goto FAIL
    pnpm tauri build --bundles nsis --no-sign --ci
    if %errorlevel% neq 0 goto FAIL
    echo.
    echo  输出目录: src-tauri\target\release\bundle\nsis
    goto DONE
)

if "%MODE%"=="release" (
    for /f "usebackq delims=" %%v in (`node -e "const fs=require('fs'); console.log(JSON.parse(fs.readFileSync('package.json','utf8')).version)"`) do set VERSION=%%v
    if "%VERSION%"=="" (
        echo [错误] 无法读取 package.json 版本号
        pause
        exit /b 1
    )
    pnpm install --frozen-lockfile
    if %errorlevel% neq 0 goto FAIL
    cargo run -p xtask -- release windows --version %VERSION% --build --try-installer
    if %errorlevel% neq 0 goto FAIL
    echo.
    echo  输出目录: release\
    goto DONE
)

:FAIL
echo.
echo [错误] 构建失败
pause
exit /b 1

:DONE
echo.
echo 构建完成。
pause
