# 07 · 快赢功能调研:低成本、高体验收益的下一步(EyeFlow v0.4 之后)

> 调研日期:2026-09。前置文档:03-competitor-ux.md(竞品矩阵)、04-windows-ux-guidelines.md(平台规范)、02-science-evidence.md(行为证据)。
> 本文只回答一个问题:**EyeFlow 还没有、单人 1~2 天能做完、不需要云/账号/ML/摄像头、且体验提升明显的功能与细节**。
> 成本标尺:S = 半天~1 天,M = 1~2 天,L = >2 天(本文只收录 S/M)。收益:H = 明显改变日常手感,M = 锦上添花,L = 感知弱。
> 代码现状核对(2026-09):`core.rs` 纯逻辑调度(时间由外部传入)、`runtime.rs` 事件总线、按需 eframe 会话;**全 src 无 WM_POWERBROADCAST / WTS 会话通知处理**;`ui.rs` 无多显示器处理;`tips.rs` 为静态数组不可自定义;`tray.rs` 已有"立即休息"。

---

## ① 候选清单表

| # | 功能 | 一句话说明 | 来源 URL | 成本 | 收益 | 与"极简/不打扰"冲突? |
|---|------|-----------|----------|------|------|----------------------|
| 1 | 睡眠/恢复事件处理 | 收到 `PBT_APMSUSPEND` 暂停全部计时,`PBT_APMRESUMEAUTOMATIC` 恢复并按"恢复时刻"重置 last_tick,防止挂起时长被算进用屏时间、醒来秒弹休息 | https://learn.microsoft.com/en-us/windows/win32/power/wm-powerbroadcast | S | H | 否(正确性修复) |
| 2 | 锁屏/解锁事件处理 | `WTS_SESSION_LOCK` 视为离开暂停、`WTS_SESSION_UNLOCK` 返回桌面;可加选项"锁屏时预支长休息"(Workrave 有此设置);直接回应 Stretchly #1724"短暂锁屏也重置"的伤疤 | https://learn.microsoft.com/en-us/windows/win32/termserv/wm-wtssession-change ; https://workrave.org/docs/settings/timers/ ; https://github.com/hovancik/stretchly/issues/1724 (3👍/18 评) | S | H | 否 |
| 3 | 多显示器覆盖休息画面 | 预告/休息/严格遮罩按 winit 枚举的每个显示器各开一个 viewport,并提供"全部/主屏/跟随鼠标"选项——双屏用户在副屏看不到休息面板是当前最大体验洞 | https://github.com/hovancik/stretchly/issues/1609 ; https://github.com/hovancik/stretchly/issues/639 | M | H | 否 |
| 4 | 休息前屏幕边缘渐暗预告 | 在预告窗口期用透明点击穿透窗口做全屏边缘渐暗(角部→中心),比角标浮窗更早进入余光,收尾工作更自然 | https://lookaway.com/ (heads-up);https://github.com/slgobinath/safeeyes (提前 10s 预告) | M | H | 否(预告本身就是"不打扰"的核心手段) |
| 5 | Toast 通知操作按钮(延后/跳过) | 预告阶段改发带 [延后 5 分钟] [跳过] 按钮的 toast,用户不切窗口即可决策;需 AUMID + 开始菜单快捷方式(NSIS 已建快捷方式,补 AUMID 即可),按钮按官方指南可后台执行不弹 UI | https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance ; https://learn.microsoft.com/en-us/windows/win32/shell/taskbar-extensions (AUMID) | M | M~H | 否(减少打断:免去"找到浮窗再点"的负担) |
| 6 | 休息结束"欢迎回来"反馈 | 休息完成瞬间:轻合成音 + 面板 2~3 秒显示"欢迎回来 · 今日第 N 次休息",替代"倒计时走完直接消失"的冷结束 | https://lookaway.com/ (结束时 soft chime);https://pubmed.ncbi.nlm.nih.gov/35963776/ (停用 1 周获益消失→即时正反馈重要) | S | M~H | 否 |
| 7 | 每次休息随机音变奏 | 合成音加 4~5 组变奏(节奏/音高/纹理)按休息序号轮换,消除"闹钟疲劳" | https://github.com/hovancik/stretchly (多音色可换);https://hovancik.net/stretchly/about/ | S | M | 否 |
| 8 | 自定义贴士 | `tips.rs` 静态数组之外支持用户文件(如 `%APPDATA%\eyeflow\tips.txt`)追加/覆盖,每行一条 | https://www.dejal.com/timeout/ (Time Out 主题/内容自定义);https://github.com/slgobinath/safeeyes (练习内容) | S | M | 否 |
| 9 | 近黑"真休息"画面模式 | 休息画面提供近黑纯色主题(#050505+暗字),鼓励真离开而非"遮罩下继续看屏" | https://github.com/hovancik/stretchly/issues/1826 (Near-black break mode) | S | M | 否(可并入壁纸/主题选项) |
| 10 | 跟随系统深浅色 + 深色标题栏 + 圆角 | 读 `AppsUseLightTheme` 并监听 `WM_SETTINGCHANGE("ImmersiveColorSet")` 切 egui 主题;`DWMWA_USE_IMMERSIVE_DARK_MODE(=20)`、`DWMWA_WINDOW_CORNER_PREFERENCE(=33)` 各一行让窗口跟上 Win11 | https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute | S | M | 否 |
| 11 | 打字/输入中自动顺延 | 休息最后 N 秒若检测到键盘输入则顺延一次(默认关);`Sensors.idle_secs` 每秒已采样,纯 `core.rs` 规则 | https://lookaway.com/ (typing pause,默认关) | S | M | 否(默认关,温和) |
| 12 | 每日跳过上限 + 跳过按钮冷却 | 统计已有"跳过"计数;可配"今日跳过 N 次后隐藏跳过按钮",以及跳过按钮出现前冷却 X 秒 | https://github.com/hovancik/stretchly/issues/1838 (Add Max Skips per Day);https://github.com/hovancik/stretchly/issues/1841 (Delay Skip button) | S | M | 轻微(默认不限,仅可选;属"有限强制",与 03 文档"有限而非无限跳过"结论一致) |
| 13 | 开机延迟启动(登录风暴) | 自启后延迟 N 秒再开始计时/弹首帧;或 NSIS 改注册 Task Scheduler 登录触发器 `Delay` 属性 | https://learn.microsoft.com/en-us/windows/win32/taskschd/logontrigger | S | M | 否 |
| 14 | 托盘图标进度环 | 预告/休息期间把倒计时画成托盘图标进度弧,折叠在溢出区也有进度可看 | https://learn.microsoft.com/en-us/windows/win32/shell/notification-area (图标即状态) | S | M | 否 |
| 15 | 自然休息抵扣 | Away 期间若持续 ≥ 短休息时长,结算为"完成 1 次"而非单纯暂停(Workrave/Stretchly 的 natural break 语义) | https://github.com/hovancik/stretchly (空闲 5 分钟暂停);https://workrave.org/docs/breaks/ | S | M | 否 |
| 16 | 统计时间线/时段热力图 | 按"工作日×小时"展示休息完成/跳过分布(Stretchly 社区多年诉求) | https://github.com/hovancik/stretchly/issues/266 | M | M | 否(放在统计页内,不添打扰) |
| 17 | "跳过时说明去向"文案 | 跳过/延后后托盘 tooltip 或浮窗一行"已顺延到 14:32",让每次决策有确定结果 | https://hovancik.net/stretchly/about/ (延后一次的确定性) | S | M | 否 |
| 18 | Jump List(任务栏跳转列表) | 托盘常驻无窗口 = 无任务栏按钮,**任务栏 Jump List 入口不存在**;仅开始菜单快捷方式入口可用静态 Tasks——价值低 | https://learn.microsoft.com/en-us/windows/win32/shell/taskbar-extensions | S~M | L(低) | 否,但性价比差 |

> 行为设计小技巧已折叠进 #6/#8/#12/#17 与 Top 10 说明:即时正反馈(休息后反馈、计数)、承诺装置(有限跳过)、摩擦最小化(toast 按钮)、文案给"动作+理由"(tips.rs 现状已符合,见 02 文档 §2.4)。

---

## ② Top 10 推荐排序(性价比 = 收益/成本)

**1. 电源 + 锁屏事件处理(1、2 合并做一个"系统事件桥") — S/H**
在 runtime 侧建一个 message-only 隐藏窗口(`windows` crate,~150 行):`WTSRegisterSessionNotification` 收 `WTS_SESSION_LOCK/UNLOCK`,`WM_POWERBROADCAST` 收 `PBT_APMSUSPEND/PBT_APMRESUMEAUTOMATIC`,翻译成 `Event::SystemSuspended/Locked/Unlocked` 发上事件总线。`core.rs` 新增规则:挂起→全部计时冻结;恢复→以"恢复时刻"为 last_tick,累计离开 ≥ `away_secs` 按 Away 结算(≥ 长休息阈值可提示"刚回来就是休息")。这是竞品公开伤疤(Stretchly #1724:短暂锁屏也重置,18 条评论),且 EyeFlow 现状 grep 证实完全未处理——属于"修的是错误而非加的是功能"。

**2. 休息结束"欢迎回来"反馈(#6) — S/M~H**
倒计时归零结算 Completed 后,休息面板不立即关闭:换一帧文案("欢迎回来 · 今日第 4 次完成")+ 播放一段与提示音不同、更上扬的结束音(audio.rs 合成一组上行三音即可)。按需 eframe 会话因此多存活 2~3 秒,与空闲 16 MB 目标不冲突。RCT 证据(02 文档 E3)显示停用 1 周获益即消失——结束瞬间的正反馈是"让用户下次愿意停下"的最便宜杠杆。

**3. 深浅色跟随 + 深色标题栏 + Win11 圆角(#10) — S/M**
同一个隐藏窗口顺带监听 `WM_SETTINGCHANGE("ImmersiveColorSet")`,读 `AppsUseLightTheme` 后发 `Event::ThemeChanged`,运行中会话即时切 `options.dark_mode`;未运行会话在下次创建时读取即可。窗口创建后对 hwnd 各调一次 `DwmSetWindowAttribute`:`DWMWA_USE_IMMERSIVE_DARK_MODE(20)`、`DWMWA_WINDOW_CORNER_PREFERENCE(33)`。纯外观,但托盘工具"与系统格格不入"是最常见的差评来源。

**4. 多显示器覆盖(#3) — M/H**
eframe 支持多 viewport:预告浮窗与严格遮罩按 `available_monitors()` 每屏开一个,新增配置 `break_window_target: all|primary|cursor`。注意 #639 的教训(副屏遮罩 z 序低于全屏窗口):遮罩窗要保持 always-on-top 并在显示时逐屏置顶。双屏以上用户占比在Stretchly 生态里足够大到让"副屏没有休息提示"成为卸载理由。

**5. 自定义贴士 + 随机音变奏(#8、#7 合并) — S/M**
`config.rs` 加 `tips_override_file: Option<PathBuf>`,加载时逐行读入,与内置 TIPS 合并(tips.rs 结构不变,只是从静态改静态+Vec);audio.rs 按 `stats.today_completed % variants.len()` 从 4~5 组合成参数变奏里选。两者都是"第 10 次休息时的新鲜感"工程,防止内容疲劳——Stretchly 的 break idea 随机与 LookAway 的多音色都验证过这一点。

**6. 边缘渐暗预告(#4) — M/H**
预告期内每屏开一个透明、点击穿透(`WS_EX_LAYERED|WS_EX_TRANSPARENT` 或 winit `with_transparent` + win32 ex-style 补丁)的窗口,egui 画四边向中心的黑渐变,透明度随预告剩余时间从 0 → 0.5 缓升;休息开始时由遮罩接管,避免闪烁。LookAway 把"heads-up"写进产品口号、SafeEyes 默认提前 10 秒通知——预告的形态从"角标"升级为"余光可感",是预告有效性的关键一步。

**7. 打字自动顺延(#11) — S/M**
`core.rs` 休息状态机加一条纯逻辑规则:短休息最后 `typing_grace_secs`(默认 0=关;建议 10s)内若 `Sensors` 检测到新键盘输入且用户未点击过按钮,把休息结束顺延一次(复用 ADR-0002"顺延而非丢弃")。必须默认关且只认键盘不认鼠标——LookAway 之所以默认关,就是因为"伸手点跳过也会被判为打字"。

**8. Toast 操作按钮(#5) — M/M~H**
安装器已建开始菜单快捷方式,NSIS 补一行写 `System.AppUserModel.ID`;用 `windows` crate 的 WinRT `AppNotificationManager` 在预告期发 toast:[开始][延后][跳过] 三按钮,后台执行不走 UI(官方指南明确允许)。价值在于把"预告 → 决策"的摩擦降到零:用户不需要把手移到浮窗上。注意保留气球路径作降级(Win10 老版本/通知被策略关闭时)。

**9. 近黑"真休息"模式 + 每日跳过上限(#9、#12 合并) — S/M**
休息画面主题枚举加 `NearBlack`(近黑背景、暗灰文字、无壁纸),与现有壁纸互斥;统计已有跳过计数,core 在 `skip_today >= max_skips`(0=不限)后让预告只剩[开始][延后]。两条都是 Stretchly 仓库近期的新功能请求(#1826、#1838/#1841),说明"真休息的沉浸感"和"防止无限逃逸"是真实需求;默认值保持温和(不限),符合 ADR-0003。

**10. 开机延迟启动 + 托盘进度环(#13、#14 合并) — S/M**
`autostart.rs` 不必改注册表方案:config 加 `startup_grace_secs`(默认 30),`main.rs` 启动后延迟 grace 时长才让 core 开始计时(期间托盘 tooltip 标注"待命");登录风暴由系统侧解决的话可改 NSIS 注册 Task Scheduler 登录触发器并用其 `Delay` 属性。托盘进度环复用 icon.rs 的运行时合成:预告/休息期把剩余秒数画成弧。两者合起来消掉"开机即弹提醒"与"托盘藏在溢出区时毫无存在感"两个小痛点。

---

## ③ 明确不建议做的

| 不建议 | 理由 |
|--------|------|
| **任务栏 Jump List** | 托盘常驻无窗口 = 没有任务栏按钮,Jump List 主入口不存在;开始菜单入口虽有但触达率低,托盘右键菜单已覆盖同样操作(#18)。 |
| **蓝光过滤 / 亮度调节 / 聚焦压暗** | 与 Windows 夜间模式、CareUEyes/Iris 正面红海,需显示驱动权限,且把产品拖离"提醒工具"定位(03 文档 §③)。 |
| **休息时强制锁键盘**(SafeEyes 式 disable keyboard) | "失控感"是卸载诱因;严格模式已有全屏遮罩,锁键盘属于第二层独裁,收益小、风险大。 |
| **插件系统** | SafeEyes 的插件体系抬高复杂度却非留存动因;EyeFlow 单文件心智不该破。 |
| **完整时间线统计/报告页** | 16 号候选的"热力图"版可在 1~2 天内做;但"完整报告(趋势、对比、导出)"是 L 级,且与按需 UI 会话的省内存设计有张力,先不做。 |
| **手机联动 / Live Activities / 云同步** | 需要·跨设备·账号·云,直接违背"无云、无账号"红线(LookAway 的 Lock Screen 活动是 macOS/iOS 生态专属)。 |
| **摄像头姿态/眨眼检测** | 隐私重负 + ML/驱动成本,L 级,且 02 文档证明提醒类干预已有效,不必上传感。 |
| **每日总结 Toast / 主动营销式通知** | 违反微软通知规范"非关键、可忽略、每天至多一次";托盘 tooltip 已承载统计展示。 |

---

## ④ 来源列表

**竞品与用户呼声**
1. Stretchly 官方说明(预告、延后一次、空闲 5 分钟暂停、break ideas):https://hovancik.net/stretchly/about/
2. Stretchly 仓库(音色、DnD 监听、空闲机制):https://github.com/hovancik/stretchly
3. Stretchly #355 全屏应用被打断(12👍/32 评,EyeFlow 已解决,佐证定位):https://github.com/hovancik/stretchly/issues/355
4. Stretchly #1724 睡眠/锁屏后计时重置(18 评):https://github.com/hovancik/stretchly/issues/1724
5. Stretchly #1609 休息画面显示器选择(all/primary/cursor):https://github.com/hovancik/stretchly/issues/1609
6. Stretchly #639 副屏全屏遮罩 z 序 bug:https://github.com/hovancik/stretchly/issues/639
7. Stretchly #1826 近黑休息模式请求:https://github.com/hovancik/stretchly/issues/1826
8. Stretchly #1838 每日跳过上限:https://github.com/hovancik/stretchly/issues/1838
9. Stretchly #1841 跳过按钮延迟:https://github.com/hovancik/stretchly/issues/1841
10. Stretchly #266 统计诉求:https://github.com/hovancik/stretchly/issues/266
11. Workrave 计时器设置(锁屏即开始长休息):https://workrave.org/docs/settings/timers/
12. Workrave 休息窗口(锁屏/挂起/关机按钮):https://workrave.org/docs/breaks/breaks/
13. Workrave #722 手动触发 mini-pause(9 评;EyeFlow 已有"立即休息",佐证该需求真实):https://github.com/rcaelers/workrave/issues/722
14. SafeEyes(提前 10s 预告、练习内容、smart pause、多屏):https://github.com/slgobinath/safeeyes
15. LookAway(heads-up 预告、结束 soft chime、typing pause、smart pause):https://lookaway.com/
16. Time Out(休息主题/内容自定义、延后灵活):https://www.dejal.com/timeout/
17. Reddit r/productivity"我用 Stretchly/Workrave 但总在跳过"(行为设计动机):https://www.reddit.com/r/productivity/comments/1lp6tjo/is_break_timer_really_a_productivity_tool/

**Windows 平台(Microsoft Learn)**
18. WM_POWERBROADCAST / PBT_APMSUSPEND / PBT_APMRESUMEAUTOMATIC:https://learn.microsoft.com/en-us/windows/win32/power/wm-powerbroadcast
19. WM_WTSSESSION_CHANGE / WTS_SESSION_LOCK/UNLOCK:https://learn.microsoft.com/en-us/windows/win32/termserv/wm-wtssession-change
20. DWMWINDOWATTRIBUTE(DWMWA_USE_IMMERSIVE_DARK_MODE=20、CORNER_PREFERENCE=33,Win11 22000+):https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute
21. Taskbar Extensions(Jump List 归属任务栏按钮与开始菜单入口、AUMID、Tasks 应静态):https://learn.microsoft.com/en-us/windows/win32/shell/taskbar-extensions
22. Notifications design basics(按钮让用户"在通知内完成任务"、后台动作不弹 UI、通知中心整洁):https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance
23. Task Scheduler LogonTrigger 的 Delay 属性(登录延迟启动):https://learn.microsoft.com/en-us/windows/win32/taskschd/logontrigger
24. 通知区域规范(图标即状态、溢出区):https://learn.microsoft.com/en-us/windows/win32/shell/notification-area

**行为/证据(复用 02 文档)**
25. 20-20-20 直接 RCT(停用 1 周获益消失 → 即时反馈与连续性设计依据):https://pubmed.ncbi.nlm.nih.gov/35963776/
26. AOA 计算机视综合征页(20-20-20 与 2h/15min 原文,贴士文案基准):https://www.aoa.org/healthy-eyes/eye-and-vision-conditions/computer-vision-syndrome
