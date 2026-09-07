---
status: accepted
date: 2026-09-08
---

# 可打扰性判断以 SHQueryUserNotificationState 为主、前台窗口矩形比对为兜底；视觉提醒由自绘浮窗承担，系统气泡通知作为辅助通道

v0.1 只用“前台窗口尺寸 == 主显示器尺寸”判断全屏，误判面大（自动隐藏任务栏时最大化窗口会被判为游戏，多显示器副屏全屏检测不到），且“系统通知”只是日志占位。Microsoft Learn 明确：`SHQueryUserNotificationState` 就是为“此刻是否适合向用户发通知”准备的官方 API，**使用自定义通知界面的应用必须调用它**；只有 `QUNS_ACCEPTS_NOTIFICATIONS` 时才可展示，`QUNS_BUSY`（F11 全屏 / 演示）、`QUNS_RUNNING_D3D_FULL_SCREEN`（独占游戏）、`QUNS_NOT_PRESENT`（锁屏）、`QUNS_PRESENTATION_MODE`、`QUNS_QUIET_TIME` 都视为不可打扰。该 API 对无边框窗口化游戏可能失效，因此保留“前台窗口矩形 == 其所在显示器矩形 且 无 WS_CAPTION 样式”作为补强。视觉提醒采用自绘置顶浮窗（`WS_EX_NOACTIVATE` 语义，不抢焦点），气泡通知仅作为“退出全屏后补发轻提示”等低强度场景的辅助通道，并设置 `NIIF_RESPECT_QUIET_TIME` / `NIIF_NOSOUND`。不采用 WinRT Toast：经典 Win32 应用需要注册 AUMID + 开始菜单快捷方式，便携运行时不成立，且 notify-rust 默认借用 PowerShell 的 AUMID 会让通知显示为“Windows PowerShell”。

## Sources

- SHQueryUserNotificationState: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shqueryusernotificationstate
- QUERY_USER_NOTIFICATION_STATE 枚举（仅 7 个值）: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state
- Extended Window Styles（WS_EX_NOACTIVATE / TOOLWINDOW）: https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles
- NOTIFYICONDATA（NIIF_RESPECT_QUIET_TIME）: https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw
- 桌面应用发 Toast 的 AUMID 前提: https://learn.microsoft.com/en-us/windows/win32/shell/quickstart-sending-desktop-toast
