# 04 Windows 桌面 UX 规范审视(EyeFlow)

> 调研日期:2026-09。来源以 Microsoft Learn(learn.microsoft.com)官方文档为主,每条结论附 URL。
> 对象:EyeFlow——Rust 托盘常驻护眼提醒工具(tray-icon、Shell_NotifyIcon NIF_INFO 气球、egui/eframe、WH_KEYBOARD_LL、前台窗口 rect 比对、HKCU\Run、rodio 合成音、无 .ico)。

---

## ① 规范清单(现状做法 → 违反/风险 → 正确做法)

### A. 通知:气球通知(NIF_INFO)的现状与推荐路径

**A1. 气球通知现状**

- 现状:EyeFlow 用 `Shell_NotifyIcon` + `NIF_INFO` 发气球通知。
- 事实:`NIF_INFO` 仍是受支持的 Win32 API(NOTIFYICONDATA 文档持续维护,NIIF_* 标志含 `NIIF_RESPECT_QUIET_TIME`、`NIIF_NOSOUND`);但微软现行文档把"桌面应用发通知"的推荐路径改为 Toast(现名 App Notification):
  - 《Windows notifications overview》API 对比表: unpackaged Win32 → 推荐 `AppNotificationManager`(Windows App SDK);`ToastNotificationManager`(WinRT)仅 UWP,状态"Active development: Yes vs Maintenance only"。
    <https://learn.microsoft.com/en-us/windows/apps/develop/notifications/>
  - 经典 Win32 发 toast 的官方前提:应用须有一个带 `System.AppUserModel.ID`(AUMID)的"开始"菜单快捷方式,且每次 `CreateToastNotifier(appID)` 都要传该 AUMID,否则 toast 不显示。
    <https://learn.microsoft.com/en-us/windows/win32/shell/quickstart-sending-desktop-toast>
- Rust 生态:native 侧只有两条路——继续 `Shell_NotifyIcon`(tray-icon crate 自带),或走 WinRT `ToastNotificationManager`。`notify-rust` 在 Windows 后端就是 `tauri-winrt-notification`(WinRT toast 包装);它默认借用 **PowerShell 的 AUMID**(`notify-rust/src/windows.rs`:`app_id.unwrap_or(Toast::POWERSHELL_APP_ID)`),意味着通知会显示成"Windows PowerShell"发的。
  <https://docs.rs/notify-rust/latest/notify_rust/>、<https://docs.rs/tauri-winrt-notification/latest/tauri_winrt_notification/>、<https://github.com/hoodie/notify-rust/blob/main/src/windows.rs>
- 结论(该不该换):
  - **不必为"换而换"。** `NIF_INFO` 气球在 Win10/11 由 Shell 统一渲染(与 toast 同一表现层、受勿扰/通知中心管理),EyeFlow 的提醒是"非关键、可忽略"信息,与气球定位完全匹配;API 无弃用标记。
  - **但若需要可点击按钮、深度链接、置顶停留(reminder 场景)**,应迁移 WinRT toast:自建 AUMID+开始菜单快捷方式(安装器写),用 `windows` crate 调 `ToastNotificationManager`;**避免** notify-rust 默认的 PowerShell 借壳 AUMID。
  - 若保留气球:务必设 `NIIF_RESPECT_QUIET_TIME`(官方建议"任何尊重安静时间的应用都应始终设置");`uTimeout` 自 Vista 起已废弃,勿再依赖。

  URL:<https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw>

**A2. 通知内容与频率规范(UX 指南,仍被引用)**

- 通知"有用且相关,但**绝不关键**;无需立即行动、可自由忽略";点击通知应**打开一个窗口**让用户执行动作,"不要点击直接执行动作";可选任务类通知"每天最多一次、累计最多三次";声音被视为打断心流的手段之一("You can break users flow by ... Using sound when displaying a notification")。
  <https://learn.microsoft.com/en-us/windows/win32/uxguide/mess-notif>
- 文本上限:标题 ≤48 字符、正文 ≤200 字符(英文,留本地化余量);`szTip` ≤128 字符。 <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw>
- 现代版 UX 指南(Windows App SDK《Notifications design basics》):通知要"informative and valuable / not noisy",可被抑制进通知中心;点击通知"app should launch in the notification's context"(深度链接到通知对应内容,而不是只开主窗口);用户看过对应内容后应清理通知中心旧通知。
  <https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance>

### B. 托盘图标(notification area)

官方依据:《Notifications and the Notification Area》(Win32)+ UX 指南《Notification Area》。
<https://learn.microsoft.com/en-us/windows/win32/shell/notification-area>、<https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification>

| 条目 | 官方规范 | EyeFlow 现状评估 |
|---|---|---|
| 何时用托盘 | 托盘用于"**没有桌面呈现**的功能/后台任务的通知与状态";**不是**程序快速启动或命令入口。EyeFlow 无主窗口常驻,属合规的"Background task status and access"模式 | ✅ 合规 |
| 图标资源 | 高 DPI 感知;资源文件中**同时提供 16x16 与 32x32**,用 `LoadIconMetric` 载入;推荐用注册的 **GUID(NIF_GUID)** 标识图标、`NOTIFYICON_VERSION_4` 回调 | ⚠️ 无 .ico → 只能运行时合成单一尺寸,高 DPI 下缩放发糊;应补 16/32 双份图标资源 |
| Tooltip | 推荐 tooltip:格式"(公司)程序名 - 状态";勿写版本号、勿写操作说明 | ⚠️ 检查 szTip 是否含多余信息 |
| 左键单击 | "Display whatever users most likely want to see"(flyout/对话框/程序主窗口);**什么都不显示会让图标显得无响应** | ⚠️ 约定:左键 = 打开/收起设置窗口 |
| 左键双击 | 执行上下文菜单的**默认命令**(通常即打开主 UI);无默认命令则同单击 | 两者合并为"打开设置窗口"即可 |
| 右键 | 显示上下文菜单,默认项加粗;推荐顺序:Open(默认)→ 常用开关(带勾选标记)→ Options → Exit;**不要 About 项**;不适用项应移除而非禁用 | tray-icon 菜单按此整理 |
| 溢出区 | Win7 起新图标**默认进 overflow,只有用户能提升**(程序无法也不得自动提升);安装非临时图标前应征得用户同意;Win11 延续 | 引导用户:设置 > 个性化 > 任务栏 > 其他系统托盘图标,或直接开"在任务栏显示所有托盘图标";首次运行时在向导里说明 |
| 用户控制 | 提供"显示/隐藏图标"选项;Exit 必须真正退出进程 | ✅ 保持 Exit 语义;勿在重启后偷偷恢复图标 |

### C. 窗口与焦点

- **前台抢夺限制**:`SetForegroundWindow` 只有在"调用进程是前台进程 / 由前台进程启动 / 收到最后输入事件 / 当前无前台窗口"等条件之一才成功;否则"Windows flashes the taskbar button"(任务栏闪烁)——后台托盘程序**根本无法合法抢焦点**。
  <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow>
- **提醒浮窗合规做法**:这是"notification UI",按 Win32 官方要求,自绘通知界面"应始终显式调用 `SHQueryUserNotificationState` 决定此刻是否显示"(见 D 节)。窗口样式:
  - 置顶显示但**不激活、不抢键盘焦点**:`WS_EX_NOACTIVATE`(点击不成为前台窗、默认不进任务栏)+ `ShowWindow(SW_SHOWNOACTIVATE)` / `SetWindowPos(SWP_NOACTIVATE|SWP_SHOWWINDOW)`;`WS_EX_TOPMOST` 置顶。
  - **点击穿透**(纯提示层,可选):`WS_EX_LAYERED | WS_EX_TRANSPARENT`——官方原文:"if the layered window has the WS_EX_TRANSPARENT extended window style ... the mouse events will be passed to other windows underneath"。
  - 不进任务栏/Alt+Tab:`WS_EX_TOOLWINDOW`。
    <https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles>、<https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features>
- **深色/浅色主题**:"By default, your Windows app's theme is the user's theme preference from Windows Settings"。Windows 11 应用应默认跟随系统主题。
  <https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color>
  EyeFlow(egui/eframe)应在启动时读 `HKCU\...\Themes\Personalize\AppsUseLightTheme`(或 dark-light crate)选择主题,并提供"跟随系统/浅色/深色"选项。
- **DPI**:"recommended that desktop applications be updated to use per-monitor DPI awareness (V2)";DPI unaware 会被系统位图拉伸(模糊);托盘图标须 16/32 双份。eframe 默认声明 per-monitor aware,保持即可;托盘图标补足资源。
  <https://learn.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows>

### D. 全屏 / 勿扰检测

**官方 API 就是为此准备的**:`SHQueryUserNotificationState`——"Checks the state of the computer for the current user to determine whether sending a notification is appropriate";且官方明确:**使用自定义通知方式(不经 Shell_NotifyIcon/IUserNotification)的应用,必须显式调用它**决定是否展示通知 UI;只有 `QUNS_ACCEPTS_NOTIFICATIONS` 时才可发;`QUNS_QUIET_TIME` 仅 critical 例外。
<https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shqueryusernotificationstate>

枚举值(Vista/7 起支持,共 7 个;**不存在 QUNS_RUNNING_GPU_ACCELERATED**,此前假设有误):
<https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state>

| 值 | 含义 |
|---|---|
| QUNS_NOT_PRESENT | 屏保/锁屏/非活动快速用户切换 |
| QUNS_BUSY | **全屏应用(F11)或演示设置生效** |
| QUNS_RUNNING_D3D_FULL_SCREEN | **独占模式 Direct3D 全屏(游戏)** |
| QUNS_PRESENTATION_MODE | 投影/演示模式 |
| QUNS_ACCEPTS_NOTIFICATIONS | 正常可打扰 |
| QUNS_QUIET_TIME | 系统升级/新装后第一小时 |
| QUNS_APP | Windows Store 应用全屏(Win8+) |

**游戏/全屏时的系统级行为**:Windows 11"勿扰"自动规则官方列表——"During certain time of the day / When duplicating the display / **When playing a game** / **When using an app in full-screen mode** / For the first hour after a Windows feature update";勿扰开启时横幅被静默、进通知中心。走系统通知(气球/toast)即可自动被这些规则管理;**自绘浮窗必须自己查 `SHQueryUserNotificationState`**。
<https://support.microsoft.com/en-us/windows/experience/notifications-and-do-not-disturb-in-windows>

边界:`SHQueryUserNotificationState` 对**无边框窗口化(borderless windowed)游戏**可能不返回全屏态(它是 Win7 时代 API,也不感知 Win11 勿扰开关)。因此保留现有"前台窗口 rect == 显示器 rect"比对作为补强,并加 `WS_CAPTION`/`WS_EX_TOOLWINDOW` 样式位检查(全屏窗口通常无边框样式)与 DWM cloaked 过滤。WH_KEYBOARD_LL 心流计数只作"用户活跃"信号,不作勿扰依据。

### E. 开机自启

- 被官方承认的桌面自启机制就三类,且全部受任务管理器"启动应用"管控:**Run/RunOnce 注册表键(HKLM、HKCU、含 Wow6432Node)、开始菜单"启动"文件夹(每用户/公共)**。
  <https://learn.microsoft.com/en-us/windows/win32/w8cookbook/startup-apps>、<https://learn.microsoft.com/en-us/windows/compatibility/startup-apps>、<https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys>
- 任务计划(登录触发)**不在**任务管理器启动应用列表中,用户更难发现/禁用,与"end users are always in control"的原则相悖——不建议用于常驻工具。
- 任务管理器条目的友好名/图标:官方文档只说明"显示启动应用列表与影响评级(CPU/磁盘)",未写明字段来源;工程通行认知是取自 exe 版本资源 `FileDescription` 与图标资源——**EyeFlow 无 .ico、若也无版本信息,启动项会显示为裸 exe 名**,应补版本资源(FileDescription="EyeFlow 护眼提醒")+ .ico。
- 打包(MSIX)应用可用 `StartupTask` API 声明式请求自启,但非本项目路径。
  <https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.startuptask>
- 决策:保持 HKCU\Run(合规、用户可禁用),但**默认不写、首次运行询问(opt-in)**;卸载时清理该值。

### F. 音频

- 规范原文(WASAPI Audio Sessions):"Typical audio applications should avoid modifying the volume and mute settings for sessions. Instead, users control these settings through ... Sndvol.exe(音量合成器)";rodio 走默认输出设备的本进程 session,用户可单独调/静音 EyeFlow——天然合规。
  <https://learn.microsoft.com/en-us/windows/win32/coreaudio/audio-sessions>
- UX 层:声音被点名为打断心流的手段;提醒音应是可选的(默认轻或无),且天然尊重系统静音/会话音量(不要代码里绕过)。
  <https://learn.microsoft.com/en-us/windows/win32/uxguide/mess-notif>
- 更"系统原生"的替代:提示音用 `PlaySound`(官方注明它走系统通知声 session,"play a sound through the system session for notification sounds by calling PlaySound"),受系统声音方案统一控制。
  <https://learn.microsoft.com/en-us/windows/win32/coreaudio/audio-sessions>
  建议:保留 rodio 自合成音作为"自定义音效"选项,但设置里提供"无/轻/自定义",并遵守静音。

### G. 安装 / 卸载

- **winget 是当前微软对"开源免费小工具"分发的主推零门槛渠道**:向 `microsoft/winget-pkgs` 提 manifest PR 即可;要求:清单合规、URL 直连发布者站点(GitHub Releases 直链可)、安装器支持**非交互(静默)安装/卸载**、无毒(会过多个杀软扫描)。NSIS 支持静默参数(/S),仍是社区主流、被 winget 完整支持;也可用 `portable` 类型清单免安装器。
  <https://learn.microsoft.com/en-us/windows/package-manager/package/repository>
- **MSIX**:官方定位是"现代打包格式:可靠安装/卸载(保证干净卸载、无残留)、差量自动更新、包身份(推送通知等平台能力所需)"。代价:必须签名、对注册 HKCU\Run 等传统机制有虚拟化影响。对 EyeFlow 属"可选未来项",不是必需。
  <https://learn.microsoft.com/en-us/windows/msix/overview>
- 建议:GitHub Releases(NSIS 静默安装包 + portable zip)→ 提交 winget-pkgs 清单;**注意**:键盘钩子(WH_KEYBOARD_LL)+ 注册表写入组合易被杀软判 PUA,提交前自查 Defender 误报(官方提供申诉通道)。

---

## ② 全屏/勿扰检测的推荐 API 决策

1. **首选 `SHQueryUserNotificationState`**(windows-rs `SHQueryUserNotificationState`,Shell32.dll,Vista+):每次触发提醒前查询,**仅 `QUNS_ACCEPTS_NOTIFICATIONS` 展示浮窗/声音**;`QUNS_NOT_PRESENT/BUSY/RUNNING_D3D_FULL_SCREEN/APP/PRESENTATION_MODE/QUIET_TIME` 全部静默跳过(或仅静默记录待补发)。这是官方指定给"能否发通知"的 API,自绘浮窗属"custom notification method",必须调用。
2. **补强检测**(官方 API 盲区):前台窗口 rect == 其所在显示器 rect,且 `(GetWindowLong(GWL_STYLE) & WS_CAPTION)==0`,过滤 shell/桌面窗口与 DWM cloaked 窗口 → 覆盖无边框窗口化游戏。
3. **纠正**:枚举只有 7 个值,**没有** `QUNS_RUNNING_GPU_ACCELERATED`。
4. 通知渠道交给系统(气球/未来 toast),让 Windows 11 勿扰自动规则(玩游戏/全屏/复制屏幕/定时)统一管理;自绘浮窗的抑制逻辑用 1+2 自己兜底。
5. WH_KEYBOARD_LL 仅用于心流(活跃)判定,勿作勿扰依据;钩子回调须快,避免拖慢系统输入。

## ③ 自启方式决策

- **维持 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`**(单用户工具,无需管理员;受任务管理器"启动应用"管控,用户可一键禁用——符合"用户始终可控"原则)。
- 不用任务计划(脱离用户管控视图);不用 HKLM(需要管理员且影响所有用户)。
- 配套:exe 补**版本资源(FileDescription)与 .ico**,让任务管理器条目显示友好名与图标;卸载/取消勾选时删除该值;默认 opt-in。
- 便携版可改为在"启动"文件夹(shell:startup)放快捷方式,同样受管控、用户肉眼可见。

## ④ 安装分发建议

- 主线:**GitHub Releases(NSIS 静默 /S 安装包 + portable zip)+ winget-pkgs 提交**。门槛低(一次 PR)、免费开源工具收益高(`winget install EyeFlow`);NSIS 仍主流且被 winget 支持,满足"非交互安装/卸载"政策即可。
- 提醒:InstallerUrl 必须是发布站点直链(GitHub Releases 资产直链合规);防杀软 PUA 误报(钩子)提前自测并准备申诉。
- MSIX 暂缓:需签名与包身份改造,当前通知/自启方案不依赖它;未来若要推送通知/商店分发再评估。

## ⑤ 来源列表

**Microsoft Learn — 通知/托盘**
1. Notifications and the Notification Area: <https://learn.microsoft.com/en-us/windows/win32/shell/notification-area>
2. Notifications (Design basics, UX 指南): <https://learn.microsoft.com/en-us/windows/win32/uxguide/mess-notif>
3. Notification Area (UX 指南): <https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification>
4. NOTIFYICONDATA 结构(NIIF_*、szTip/szInfo 上限): <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw>
5. Shell_NotifyIcon(VERSION_4 回调语义): <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyicona>
6. Windows notifications overview(AppNotificationManager vs ToastNotificationManager): <https://learn.microsoft.com/en-us/windows/apps/develop/notifications/>
7. App notifications overview(Windows App SDK): <https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/>
8. Notifications design basics(现代通知 UX): <https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance>
9. App notification content(内容/声音/激活): <https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content>
10. Sending a toast notification from the desktop(AUMID 前提、点击激活应前台打开相关视图): <https://learn.microsoft.com/en-us/windows/win32/shell/quickstart-sending-desktop-toast>

**Microsoft Learn — 勿扰/全屏/焦点/窗口/DPI**
11. SHQueryUserNotificationState: <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shqueryusernotificationstate>
12. QUERY_USER_NOTIFICATION_STATE 枚举: <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state>
13. Notifications and Do Not Disturb in Windows(Win11 勿扰自动规则): <https://support.microsoft.com/en-us/windows/experience/notifications-and-do-not-disturb-in-windows>
14. SetForegroundWindow(前台锁定规则): <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow>
15. Extended Window Styles(WS_EX_NOACTIVATE/TRANSPARENT/LAYERED/TOOLWINDOW): <https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles>
16. Window Features(分层窗口命中测试/点击穿透): <https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features>
17. High DPI Desktop Application Development(PMv2 推荐): <https://learn.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows>
18. Color in Windows(默认跟随系统 light/dark 主题): <https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color>

**Microsoft Learn — 自启/音频/打包**
19. Run and RunOnce Registry Keys: <https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys>
20. Startup apps(启动应用分类与任务管理器管控/影响评级): <https://learn.microsoft.com/en-us/windows/win32/w8cookbook/startup-apps>
21. Desktop Startup apps - Compatibility Cookbook: <https://learn.microsoft.com/en-us/windows/compatibility/startup-apps>
22. StartupTask(打包应用自启): <https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.startuptask>
23. Audio Sessions(应用 session 音量/静音归用户;PlaySound 走系统通知 session): <https://learn.microsoft.com/en-us/windows/win32/coreaudio/audio-sessions>
24. What is MSIX?: <https://learn.microsoft.com/en-us/windows/msix/overview>
25. Submit your manifest to the repository(winget-pkgs 提交与政策): <https://learn.microsoft.com/en-us/windows/package-manager/package/repository>

**Rust 生态**
26. notify-rust(Windows 后端=tauri-winrt-notification;默认 PowerShell AUMID): <https://docs.rs/notify-rust/latest/notify_rust/>、<https://github.com/hoodie/notify-rust/blob/main/src/windows.rs>
27. tauri-winrt-notification(WinRT toast 包装;Win8.1+): <https://docs.rs/tauri-winrt-notification/latest/tauri_winrt_notification/>
