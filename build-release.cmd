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
set "MAKENSIS="
where makensis >nul 2>&1 && set "MAKENSIS=makensis"
if not defined MAKENSIS if exist "%ProgramFiles(x86)%\NSIS\makensis.exe" set "MAKENSIS=%ProgramFiles(x86)%\NSIS\makensis.exe"

if not defined MAKENSIS (
    echo [SKIP] 未找到 makensis（NSIS 3），跳过安装包生成。
    echo        可直接运行 target\release\eyeflow.exe
    goto done
)

rem installer.nsi 从仓库根调用,输出到仓库根 dist\
if not exist dist mkdir dist
"%MAKENSIS%" installer\installer.nsi
if %ERRORLEVEL% NEQ 0 (
    echo [WARN] NSIS 编译失败（可能是路径问题），安装包未生成
) else (
    echo [OK] Installer generated
)

:done
echo.
echo ======================
echo 产物输出: dist\
echo   - EyeFlow-^<版本^>-Setup.exe（NSIS 安装包，需 NSIS）
echo   - 便携版: 压缩 target\release\eyeflow.exe 即可
echo Done.
pause
