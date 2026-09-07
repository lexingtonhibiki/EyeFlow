; EyeFlow Installer
; NSIS 3.09+
; Build: makensis installer.nsi

Unicode true
ManifestDPIAware true

; Product metadata

!define PRODUCT_NAME "EyeFlow"
!define PRODUCT_VERSION "0.1.0"
!define PRODUCT_PUBLISHER "EyeFlow Team"
!define PRODUCT_DIR "$LOCALAPPDATA\${PRODUCT_NAME}"
!define CONFIG_DIR "$APPDATA\eyeflow"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define STARTUP_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "EyeFlow-${PRODUCT_VERSION}-Setup.exe"
InstallDir "${PRODUCT_DIR}"
RequestExecutionLevel user

; Pages

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

; ------- Install -------

Section "Install" SEC_MAIN
    SetOutPath "$INSTDIR"

    ; Copy main binary
    File "..\target\release\eyeflow.exe"

    ; Generate uninstaller (NSIS auto-builds this)
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; Register in Add/Remove Programs
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\eyeflow.exe,0"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" "$INSTDIR\uninstall.exe"
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" 4600

    ; Add to startup (boot with Windows)
    WriteRegStr HKCU "${STARTUP_KEY}" "${PRODUCT_NAME}" "$INSTDIR\eyeflow.exe"

    ; Start menu shortcuts
    CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
    CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\eyeflow.exe"
    CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk" "$INSTDIR\uninstall.exe"

    DetailPrint "EyeFlow installed successfully"
SectionEnd

; ------- Uninstall -------

Section "Uninstall"
    ; 1. Kill running EyeFlow process before deleting files
    ;    taskkill /f is forceful, no confirmation
    ;    This prevents "file in use" errors during file deletion
    nsExec::ExecToLog 'taskkill /f /im eyeflow.exe'
    Sleep 500

    ; 2. Remove installed files
    Delete "$INSTDIR\eyeflow.exe"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"

    ; 3. Remove user config (APPDATA)
    Delete "${CONFIG_DIR}\config.toml"
    RMDir "${CONFIG_DIR}"

    ; 4. Remove start menu entries
    Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
    Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk"
    RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

    ; 5. Remove registry keys
    DeleteRegKey HKCU "${UNINSTALL_KEY}"
    DeleteRegValue HKCU "${STARTUP_KEY}" "${PRODUCT_NAME}"

    ; 6. Self-delete the uninstaller
    ;    SetAutoClose true closes the uninstall window immediately
    ;    cmd schedules deletion of uninstall.exe and the empty dir after 1 second
    SetAutoClose true
    Exec '"cmd.exe" /c ping 127.0.0.1 -n 2 > nul & del /f "$INSTDIR\uninstall.exe" & rmdir "$INSTDIR"'

    DetailPrint "EyeFlow fully uninstalled (zero residual)"
SectionEnd
