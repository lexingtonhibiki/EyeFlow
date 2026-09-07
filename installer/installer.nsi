; EyeFlow Installer — NSIS 3.09+
;
; 编译约定(重要,两套相对路径语义不同,已实测确认):
;   1) Icon / File / OutFile 的相对路径相对【本脚本所在目录】(installer\)解析,
;      因此指向仓库根的文件要带 "..\" 前缀:
;       ..\target\release\eyeflow.exe     —— cargo 构建产物
;       ..\assets\icon.ico                —— 安装器/卸载器图标(置于仓库根 assets\ 下)
;       ..\dist\EyeFlow-<版本>-Setup.exe  —— 输出(仓库根 dist\)
;      注意:NSIS 在读脚本前就会打开输出文件,且不会自动创建目录——
;      dist\ 必须预先存在,由调用方创建(build-release.cmd 与 CI 已处理)。
;   2) 调用方式:在【仓库根】执行
;       if not exist dist mkdir dist
;       makensis /DPRODUCT_VERSION=0.2.0 installer\installer.nsi
;      (CI(.github/workflows/release.yml)与 build-release.cmd 均按此约定从仓库根调用。
;       不支持在 installer\ 目录内直接 `makensis installer.nsi`,路径会错位。)

Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma

; ---------------- Product metadata ----------------

!define PRODUCT_NAME "EyeFlow"
; CI 通过 /DPRODUCT_VERSION=x.y.z 注入真实版本(不带 v 前缀);本地直接编译时回退到默认值
!ifndef PRODUCT_VERSION
  !define PRODUCT_VERSION "0.2.0"
!endif
!define PRODUCT_PUBLISHER "lexingtonhibiki"
!define PRODUCT_DIR "$LOCALAPPDATA\${PRODUCT_NAME}"
!define CONFIG_DIR "$APPDATA\eyeflow"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define STARTUP_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

Name "${PRODUCT_NAME}"
OutFile "..\dist\EyeFlow-${PRODUCT_VERSION}-Setup.exe"
InstallDir "${PRODUCT_DIR}"
; 覆盖安装时沿用上次的安装目录
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
RequestExecutionLevel user

; 图标(相对本脚本目录;assets\icon.ico 由构建流程生成)
Icon "..\assets\icon.ico"
UninstallIcon "..\assets\icon.ico"

; ---------------- Pages ----------------

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

; ------- Install -------

Section "Install" SEC_MAIN
    ; 0. 结束旧进程,避免"文件被占用"导致覆盖失败
    nsExec::ExecToLog 'taskkill /f /im eyeflow.exe'
    Sleep 500

    SetOutPath "$INSTDIR"

    ; 1. 主程序
    File "..\target\release\eyeflow.exe"

    ; 2. 生成卸载程序
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; 3. Add/Remove Programs 注册(仅 HKCU,per-user,无需管理员)
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\eyeflow.exe,0"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" "$INSTDIR\uninstall.exe"
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" 5000

    ; 4. 开机自启(HKCU\Run;应用内提供开关,卸载时清理 —— ADR-0005)
    WriteRegStr HKCU "${STARTUP_KEY}" "${PRODUCT_NAME}" "$INSTDIR\eyeflow.exe"

    ; 5. 开始菜单快捷方式
    CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
    CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\eyeflow.exe"
    CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk" "$INSTDIR\uninstall.exe"

    ; 6. 可选立即启动(保持无 MUI 的简单脚本,用 MessageBox 询问;
    ;    /S 静默安装时 MessageBox 被抑制,不会自动启动)
    MessageBox MB_YESNO|MB_ICONQUESTION "安装完成。现在启动 ${PRODUCT_NAME} 吗?" IDYES launch_now
    DetailPrint "稍后可从开始菜单或系统托盘启动 ${PRODUCT_NAME}"
    Goto install_done
launch_now:
    Exec '"$INSTDIR\eyeflow.exe"'
install_done:
    DetailPrint "${PRODUCT_NAME} 安装完成"
SectionEnd

; ------- Uninstall -------

Section "Uninstall"
    ; 1. 结束运行中的进程,防止"文件被占用"
    nsExec::ExecToLog 'taskkill /f /im eyeflow.exe'
    Sleep 500

    ; 2. 程序文件
    Delete "$INSTDIR\eyeflow.exe"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"

    ; 3. 用户数据:配置与统计(含解析失败时产生的 .bak 备份)
    Delete "${CONFIG_DIR}\config.toml"
    Delete "${CONFIG_DIR}\config.toml.bak-*"
    Delete "${CONFIG_DIR}\stats.toml"
    RMDir "${CONFIG_DIR}"

    ; 4. 开始菜单
    Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
    Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk"
    RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

    ; 5. 注册表:卸载项 + 自启项(HKCU\Run —— ADR-0005)
    DeleteRegKey HKCU "${UNINSTALL_KEY}"
    DeleteRegValue HKCU "${STARTUP_KEY}" "${PRODUCT_NAME}"

    ; 6. 自删卸载器
    ;    SetAutoClose true 关闭卸载窗口;cmd 延迟 1 秒后删除 uninstall.exe 与空目录
    DetailPrint "${PRODUCT_NAME} 已完全卸载"
    SetAutoClose true
    Exec '"cmd.exe" /c ping 127.0.0.1 -n 2 > nul & del /f "$INSTDIR\uninstall.exe" & rmdir "$INSTDIR"'
SectionEnd
