@echo off
chcp 65001 >nul
echo EyeFlow Release Builder
echo ======================
echo.
echo Step 1/2: Building release binary...
cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo [FAIL] cargo build 失败，错误码 %ERRORLEVEL%
    pause
    exit /b %ERRORLEVEL%
)
echo [OK] Binary built: target\release\eyeflow.exe
echo.
echo Step 2/2: Building NSIS installer...
if exist "C:\Program Files (x86)\NSIS\makensis.exe" (
    "C:\Program Files (x86)\NSIS\makensis.exe" installer\installer.nsi
    if %ERRORLEVEL% NEQ 0 (
        echo [WARN] NSIS 编译失败（可能是路径问题），安装包未生成
    ) else (
        echo [OK] Installer generated
    )
) else (
    echo [SKIP] NSIS 未安装，跳过安装包生成
    echo 可直接运行 target\release\eyeflow.exe
)
echo.
echo ======================
echo Done.
pause
