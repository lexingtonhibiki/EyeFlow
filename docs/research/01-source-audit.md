# EyeFlow 源码审计报告(用户体验视角)

> 审计日期:2026-09-08
> 审计范围:`src/` 全部 9 个 .rs 文件(共 1620 行)、`docs/spec.md`、`tasks.md`、`INSTALL.md`、`build-check.bat`、`build-release.cmd`、`installer/installer.nsi`
> 审计方式:只读静态审计 + `cargo check` 编译验证 + 构建目录取证。未修改任何源码。

---

## ① 编译状态

### cargo check 结果:**通过,0 error,0 warning**

```
$ cargo check
    Checking eyeflow v0.1.0 (E:\Workspaces\Hanako\eyeflow)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 35s   (exit 0)
```

- 无任何编译错误、无任何 warning(代码里已有 4 处 `#[allow(dead_code)]` 压住了死代码告警:`state.rs:54`、`detector.rs:78`、`audio.rs:33`、`event.rs:24/35/42`,以及 `main.rs:125` 分支)。
- `state.rs`、`reminder.rs` 内置 9 个单元测试(本次未运行 `cargo test`,属 check 范围外)。

### 工具链情况

| 项 | 值 |
|---|---|
| `rustup show` Default host | `x86_64-pc-windows-msvc` |
| **实际活动工具链** | `stable-x86_64-pc-windows-gnu`(default) |
| rustc / cargo | 1.95.0 (2026-04-14 / 2026-03-21),LLVM 22.1.2 |
| rustup home | `D:\DevTools\SDK\Rust\rustup`(非默认位置) |
| cargo 位置 | `PATH` 中的 cargo;`build-check.bat:3` 硬编码 `D:\DevTools\SDK\Rust\cargo\bin\cargo.exe` |

注意:host 默认是 msvc,但项目实际用 **GNU 工具链** 编译。GNU 工具链能编过也能跑,但对 Windows GUI 程序来说 msvc 更常规(调试符号、SDK 兼容性更好),这是一个潜在的环境分叉点。

### target* 目录取证(解释了"之前构建出过问题")

四个目录 `target`、`target6`、`target_fresh`、`target_fresh2` 全部存在但**内容已清空**(只剩空壳 debug/release 子目录),创建时间为 2026-05-17 08:22–08:33,彼此间隔仅 1–2 分钟。关键证据:

- `target6\.rustc_info.json` 记录的 toolchain 路径是 `C:\Users\14970\.rustup\toolchains\stable-x86_64-pc-windows-gnu`;
- `target\.rustc_info.json` 记录的是 `D:\DevTools\SDK\Rust\rustup\toolchains\...`(本次 check 刷新)。

即:项目曾把 rustup 从 `C:\Users\14970\.rustup` 迁移到 `D:\DevTools\SDK\Rust\rustup`,导致 cargo 缓存指纹(rustc_fingerprint:2335886405715689262 → 536861375123928602)全部失效、增量缓存作废,当时通过反复换 `CARGO_TARGET_DIR` 新建 target 目录绕过(target6 → target_fresh → target_fresh2)。目前这些目录均为空壳残留,只造成困惑,无实际作用,可安全删除。本次 check 已重新填充 `target\`。

### 产物时效问题(P1)

- `installer\eyeflow.exe`(2026-05-17 06:36)与 `installer\EyeFlow-Setup.exe`(06:37)**早于 src/ 的最后修改时间**(main.rs / audio.rs / tray.rs / ui.rs 均为 08:09)。即当前随附的 exe 和安装包**不包含最新源码**。
- `installer.nsi:36` 引用 `..\target\release\eyeflow.exe`,而 `target\release` 现在是空目录——现在直接跑 makensis 会失败,必须先重新 `cargo build --release`。

---

## ② 功能实现矩阵(F1~F9 + 配置项)

图例:✅ 实现 | 🟡 部分实现/有缺陷 | ❌ 未实现

| ID | 需求 | 状态 | 证据与说明 |
|----|------|:----:|-----------|
| F1 | 定时提醒(加权随机 10~18min,提醒护眼 25s) | 🟡 | 加权随机调度器已实现(三角分布,`reminder.rs:28-39`),默认 600~1080s(`config.rs:76-77`),主循环接入(`main.rs:133-158`)。**但"护眼 25 秒"没有任何落地**:`eye_rest_secs` 唯一用途是拼进一条永远不会显示的通知文案(`main.rs:230`),无倒计时界面、无休息确认,提醒实际只是"哔一声"。 |
| F2 | 上下文感知(按状态切换提醒方式) | 🟡 | 四状态机已实现且可编译(`state.rs:23-138`),主循环按状态分支(`main.rs:137-153`)。但三种状态的"提醒方式"实际是:**Desktop=仅声音**(通知是 stub,见 F3)、**Gaming=彻底无声**(见 F5)、**Flow=提醒被吞**(见 F4)。上下文感知的实际效果是"让提醒变少/消失",而不是 spec 1.1 期望的"换一种不打扰的方式"。 |
| F3 | 多模式提醒(系统通知 + 提示音) | ❌ | **系统通知是日志占位**:`tray.rs:112-116` `show_notification()` 仅 `log::info!`,没有 Shell_NotifyIcon NIF_INFO(tasks.md 任务 7 标注"待开始",属实)。因此 `notification_enabled` 配置和 UI 复选框(`ui.rs:95-97`)完全无效。多模式实际只剩"提示音"一种模式。 |
| F4 | 心流延迟(高频输入延后,停歇触发) | ❌(有致命 bug) | 检测链路齐全:键盘钩子 → `Event::KeyboardActivity`(`detector.rs:154-170`)→ 双份 30s 滑窗(`state.rs:66-83` 与 `detector.rs:65-76`)。停歇 8s 的判断在 `main.rs:143-151`。**但 `Event::ReminderTriggered` 处理器调用的 `trigger_reminder()` 中 `ContextState::Flow => {}` 为空臂(`main.rs:224-239`),随后 `reminder.reset()` 把计时器重置**——心流用户停下来 8 秒后触发的"提醒"被静默吞掉,一个声音/通知都不会有。详见 ③-P0-1。 |
| F5 | 游戏免打扰(检测全屏,仅音效) | ❌(偏离 spec) | 全屏检测已实现(`detector.rs:10-45`,GetForegroundWindow + GetWindowRect + SM_CXSCREEN 对比)。**但 `main.rs:152` `ContextState::Gaming | ContextState::Away => {}`——游戏状态下 `should_remind()` 根本不被评估,任何提醒(连声音)都不发**。`trigger_reminder()` 里的 Gaming 分支"仅播放声音"(`main.rs:234-238`)实际不可达(仅在事件排队间隙状态恰好切换的竞态下才可能触到)。spec 4.2.1 要求 GAMING="仅提示音,不弹窗",实现成了"完全静默"。全屏检测本身还有误判面,见 ③-P1-9。 |
| F6 | 离开检测(无输入超时不提醒,重置计时) | ✅ | `GetLastInputInfo` 手写 extern 实现(`detector.rs:104-137`,wrapping_sub 处理了 49.7 天回绕);检测线程 idle≥3min 每秒发 `IdleTimeout`(`main.rs:61-65`);主循环重置状态机和计时器(`main.rs:92-96`);回来后键盘活动 → Desktop(`state.rs:69-74`),计时从零开始,符合 spec。小瑕疵:离开期间每秒重复 reset(冗余),返回后 ≤1s 内旧事件可能把状态又打回 Away 一次(自愈)。 |
| F7 | 配置界面(可视化配置所有参数) | 🟡 | egui 窗口覆盖全部配置项(`ui.rs:27-142`):开关/间隔/护眼时长/提示音+预设+试听/通知开关/免打扰时段/灵敏度/快捷键。缺陷:无"恢复默认";免打扰时间是裸文本框无校验;最小/最大间隔滑条可互相矛盾;全局快捷键输入框是装饰品(见配置项表);保存后窗口不关闭。详见 ③-P1/P2。 |
| F8 | 系统托盘(常驻图标,快捷开关) | 🟡 | 托盘图标(程序化绘制的 32×32 眼睛,`tray.rs:136-187`)+ 右键菜单 4 项(提醒开关/静音/设置/退出,`tray.rs:69-81`)+ MenuEvent 监听线程(`tray.rs:84-97`)。**无双击事件**:`TrayIconEvent` 全仓 grep 为 0 命中,tasks.md 任务 1 声称"双击托盘图标发送 OpenConfig"与代码不符;打开设置只能右键菜单。菜单无"立即休息/延后",无状态/下次提醒时间展示。 |
| F9 | 开机自启 | ✅(仅安装器) | `installer.nsi:16,52`:`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 写入 `"EyeFlow" = "$INSTDIR\eyeflow.exe"`(即 `%LOCALAPPDATA%\EyeFlow\eyeflow.exe`),路径正确、绝对路径、per-user 无需提权(`RequestExecutionLevel user`,nsi:21);卸载时删除该值(nsi:87)并 taskkill 进程(nsi:68)。**但**:①应用内无自启开关,用户必须重装/改注册表;②当前安装包里封的是 5-17 06:36 的旧 exe(见 ①)。 |

### 配置项清单(spec 3.3)逐项核对

| 配置项 | spec 默认 | 实现 | 问题 |
|--------|-----------|------|------|
| 提醒开关 | on | ✅ `config.rs:75` | 托盘/UI 均可切换并持久化(`main.rs:104-109`) |
| 最小间隔 10min | ✅ `config.rs:76` | UI 滑条 5..=60 分钟,截断秒 |
| 最大间隔 18min | ✅ `config.rs:77` | UI 滑条 5..=120,**可小于最小值**,静默 clamp(`reminder.rs:56-60`) |
| 护眼时长 25s | 🟡 `config.rs:78` | 可配置但无任何作用(无通知显示、无倒计时) |
| 提示音开关 | on | ✅ `config.rs:79` | 生效 |
| 系统通知开关 | on | ❌ 无效(stub,F3) |
| 免打扰时段 00:00-08:00 | 🟡 `config.rs:82-83,140-152` | 逻辑正确(含跨零点);但 UI 裸文本无校验,解析失败静默按 00:00(`config.rs:155-163`) |
| 心流灵敏度(低6/中10/高15) | 🟡 `config.rs:14-22` | 阈值正确;但"中=10 次/30s"即 0.33 键/s,**几乎所有打字都判定为心流**(见 P1-11);且监控全部按键,非 spec 限定的"26 字母+方向+数字键" |
| 全局静音快捷键 Ctrl+Shift+E | ❌ | `event.rs:22-25` `GlobalHotkey` 事件**从未被发送**;Cargo.toml 无 `global-hotkey` 依赖;`main.rs:125-130` 的处理分支是死代码;`ui.rs:139-141` 的输入框改了也没用 |

---

## ③ UX 问题清单(按严重度排序)

### P0 —— 直接导致"效果不佳、不实用"的问题

**P0-1 心流状态下提醒被静默吞掉:键盘重度用户几乎永远收不到提醒**
- 证据:`main.rs:143-151`(Flow 停歇 ≥8s 时发送 `ReminderTriggered`)→ `main.rs:155-158`(处理器调用 `trigger_reminder` 后立刻 `reminder.reset()`)→ `main.rs:239`(`ContextState::Flow | ContextState::Away => {}` 空臂,什么都不做)。
- 链路推演:用户持续打字 → `flow_key_count ≥ 阈值`(`state.rs:127`)保持 Flow;到点后停歇 8s → 发送 ReminderTriggered → **无声音、无通知,且计时器被重置为 10~18 分钟后**;要等 30 秒滑窗过期(`state.rs:121-124`)状态才回到 Desktop,但那时计时器早已被重置。只要用户以 >阈值/30s 的频率打字(中等灵敏度=10 次/30s≈0.33 键/s,轻量打字即满足),这个循环会无限重复:**每次该提醒时都被吞掉并顺延 10~18 分钟**。
- 影响:这是"效果不佳"的第一根因。spec F4 的承诺"等停歇再触发"在交付层面是假的——触发的是一个空操作。

**P0-2 游戏/全屏状态下彻底静默,连 spec 承诺的"仅提示音"都没有**
- 证据:`main.rs:152` `ContextState::Gaming | ContextState::Away => {}`(Gaming 时不评估 `should_remind`);`main.rs:234-238` 的 Gaming"仅播声音"分支不可达。
- 影响:用户打 3 小时游戏一次提醒都没有,护眼工具在最伤眼的场景完全失效。且与 P1-9 的误判叠加后,普通窗口最大化也可能被当成"游戏"而静音。

**P0-3 系统通知是日志占位:提醒没有视觉通道,"完成一次护眼"闭环不存在**
- 证据:`tray.rs:112-116`(`show_notification` 仅写日志);`main.rs:229-232`(Desktop 提醒的文案只在 stub 中使用);UI 的"启用弹窗通知"复选框(`ui.rs:96`)无效;全程序无倒计时 UI、无休息浮窗、无"完成/跳过/延后 10 分钟"任何交互(`eye_rest_secs` 只出现在上述 stub 文案)。
- 影响:到点后发生的全部事情 = 一声 0.5 秒合成音。用户在看文档/开会时极易错过或忽略;错过了也没有任何痕迹可追。spec 2.1 的核心目标"提醒后用户需要看远 25 秒"没有任何机制承载——用户无法"完成"一次护眼,也无法跳过/延后(这两个概念在代码里根本不存在)。这是"不实用"的第二根因。

**P0-4 全局静音快捷键(Ctrl+Shift+E)未实现,配置项是空壳**
- 证据:`event.rs:22-25`(事件定义,标 `#[allow(dead_code)]`,无发送点);`Cargo.toml`(无 `global-hotkey` 依赖,spec 4.1 技术栈表中承诺使用);`main.rs:124-130`(死分支);`ui.rs:139-141`(可编辑文本框,改了无效果,误导用户)。
- 影响:spec 3.3 明确要求的配置项缺失;游戏场景下用户唯一能期待的"一键静音"手段不存在。

**P0-5 音频初始化失败直接 panic 崩溃(无音频设备机器上开机即死)**
- 证据:`audio.rs:16-25`:`DeviceSinkBuilder::open_default_sink()` 失败后仅记日志,然后**再次调用同一函数并 `expect("不可恢复")`** → panic;Cargo.toml:44 `panic = "abort"` + `main.rs:1` `windows_subsystem = "windows"` → 无弹窗、无日志文件,进程无声消失。
- 影响:远程桌面(无音频重定向)、禁用了音频端点的机器上,程序启动即崩溃;配合 F9 开机自启 = **每次开机静默崩溃一次**,托盘图标从不出现,用户视角就是"装了没反应"。崩溃前配置可能已被 `Config::load` 重写过(见 P1-7)。

### P1 —— 显著损害体验

**P1-6 配置解析失败即"毁尸灭迹":用户配置被删除重建**
- 证据:`config.rs:100-104`:toml 解析失败 → `remove_file` → 用默认值覆盖重建。
- 影响:用户手改 config.toml 打错一个引号,全部自定义(间隔、时段、灵敏度)丢失且无备份、无提示(log 仅 debug 级,默认不可见)。对"零配置"理念的工具,正确做法是重命名备份 + 告警。

**P1-7 随包安装器/ exe 是旧版本,且当前无法直接重打包**
- 证据:`installer\eyeflow.exe`(05-17 06:36)< src 最新修改(05-17 08:09,main/audio/tray/ui);`target\release\` 为空;`installer.nsi:36` `File "..\target\release\eyeflow.exe"`。
- 影响:现在装出来的程序不含最新代码;重新打安装包必须先 build,脚本无此强约束(build-release.cmd 有 build 步骤但 NSIS 失败仅 WARN)。

**P1-8 全屏检测误判面大:最大化窗口可被误判为"游戏"(→ 彻底静音)**
- 证据:`detector.rs:31-37`:仅对比**主显示器**(SM_CXSCREEN/SM_CYSCREEN)+ 2px 容差。任务栏设为自动隐藏时,最大化窗口的 rect 恰好等于全屏 → 误判 GAMING → 叠加 P0-2,提醒全静;多显示器场景下副屏全屏检测不到;无边框窗口化游戏与全屏独占不区分。且 `on_fullscreen_change` 返回的 changed 值无人使用(`state.rs:85-89`),每秒重复发事件(`main.rs:58-59`)。
- 影响:状态误判直接放大 P0-2 的静默范围。

**P1-9 心流阈值语义过松:中等灵敏度=10 键/30s,任何打字都算"心流"**
- 证据:`config.rs:18`(Medium=10)+ `state.rs:127`(`>=` 判定)。10 次/30 秒 = 0.33 键/秒,正常打字 5-10 键/秒,超标 15~30 倍。
- 影响:绝大多数桌面工作者常年处于 Flow 状态;再叠加 P0-1 的吞提醒 bug,"提醒从不出现"成为默认体验。灵敏度低档=6 更松。

**P1-10 托盘双击无响应,菜单缺关键动作,状态不可见**
- 证据:`TrayIconEvent` 全仓 0 命中(tasks.md 任务 1 声称已实现双击 → 与代码不符);菜单仅 4 项(`tray.rs:69-74`),无"立即休息"、无"延后 10 分钟"、无关于/版本;开关状态用 emoji 文字表达(`tray.rs:104-109`),无勾选态;无"下次提醒还有 X 分钟"或当前状态(Desktop/Flow/Gaming/Away)的展示。
- 影响:用户打开设置的路径过深(右键→设置);无法主动触发或推迟提醒;不双击图标就完全感知不到程序存在——"常驻但零存在感"走到极端。

### P2 —— 细节与打磨

**P2-11 设置窗口同步阻塞主事件循环**
- 证据:`main.rs:193-196` 在主 loop 内直接调 `run_config_window` → `eframe::run_native`(`main.rs:255`)阻塞返回前,提醒调度、托盘菜单事件全部排队暂停。
- 影响:开着设置挂机一小时 = 一小时不提醒(这也许可以接受,但托盘点"退出"也要等窗口关闭才生效,容易让用户困惑)。

**P2-12 配置窗口控件易用性欠缺**
- 证据:`ui.rs:38,44` 最小/最大滑条范围独立(5..=60 / 5..=120),可配出 min>max,靠 `reminder.rs:56-60` 静默纠正;`ui.rs:108-112` 免打扰时间裸文本无格式校验/无错误提示;`ui.rs:146-150` 保存后不关窗、无"保存并关闭";无"恢复默认"按钮;`ui.rs:59,63,101,117` 用双 `ui.separator()` 占位造成孤行。
- 影响:配置出错无反馈;想回到默认状态的用户只能删文件或重装。

**P2-13 键盘钩子细节:不计 autorepeat、监控全键、无钩子失效自愈**
- 证据:`detector.rs:159-167`:回调仅对 WM_KEYDOWN/WM_SYSKEYDOWN 做 `tx.send`,无 KF_REPEAT 位过滤(长按键会被反复计数,虚增心流计数);未按 spec 过滤键位(26 字母+方向+数字);若 Windows 因超时静默摘除 LL 钩子(Win11 行为),无检测/重挂逻辑;`stop_keyboard_hook`(`detector.rs:205-213`)从主线程 Unhook + 投递 WM_QUIT,而钩子线程退出时又 Unhook 一次(双重 Unhook,第二次无害失败)。
- 正面结论:**回调本身足够轻**(仅一次 mpsc send,无重活),不会拖慢系统,也不丢键(总是 CallNextHookEx,`detector.rs:169`);GetMessageW 消息泵写法正确(`detector.rs:196`)。事件通道无界,主循环被阻塞(见 P2-11)时按键事件会堆积,但量级可忽略。

**P2-14 音频实现细节:采样率硬编码 48k、部分预设频段不符 spec N4、生成在主线程**
- 证据:`audio.rs:9`(SAMPLE_RATE=48000)+ `audio.rs:29-31`(源直接 `mixer().add()`,未显式重采样;44.1kHz 设备上会变调约 +8.8%,需运行时验证);`SoftTap=500Hz`(audio.rs:139)、`TripleBeep=1.2kHz`(audio.rs:193)、`WaterDrop` 下扫至 800Hz(audio.rs:159)均低于 spec N4 的 1.5~2.5kHz 穿透频段(默认 GentleChime 1.5→2.5k 合规,audio.rs:122-135);`play_preset` 在主循环线程现场合成 24000 个样本(audio.rs:27-31,耗时微小);Chime 的相位公式 `sin(2π·f(t)·t)` 而非积分相位,扫频有轻微频率失真(audio.rs:129-130)。
- 线程安全无问题:`preview_sound` 每次开临时 sink + 后台线程(audio.rs:44-62)。

**P2-15 空闲→返回的 1 秒竞态**
- 证据:`main.rs:61-65` 检测线程在 idle≥3min 期间**每秒**重发 `IdleTimeout`;`main.rs:92-96` 每次都 `reminder.reset()`。用户返回后 ≤1s 内,最后一条陈旧 `IdleTimeout` 可能再把状态压回 Away、把计时器再重置一次(下一 tick 自愈)。
- 影响:极小,但说明 IdleTimeout 应是边沿触发(仅状态切换时发一次)而非电平触发。

**P2-16 exe 无图标、无版本信息资源**
- 证据:全仓无 `build.rs`/`.rc`/`.ico`(grep 0 命中);`installer.nsi:45` `DisplayIcon "$INSTDIR\eyeflow.exe,0"` 指向不存在的图标资源 → 控制面板"程序和功能"显示通用图标;任务栏/托盘用的图标是运行时程序化绘制的(tray.rs:136-187,视觉上是一个纯色圆+高光,辨识度低)。tasks.md 任务 8"添加应用图标"未完成,属实。

**P2-17 死代码与死依赖**
- 证据:`Cargo.toml:31` `directories` 依赖在 src 中 0 引用(config.rs:130 实际用 `std::env::var("APPDATA")`,APPDATA 缺失时回退到当前目录写配置——边缘情况);`event.rs:35,42` `OpenSettings`/`Quit` 从未发送;`state.rs:54-57` `last_activity()` 仅测试用;`state.rs` 与 `detector.rs` 各自维护一套 30s 滑窗(state.rs:76-82 整窗过期重置 vs detector.rs:68-74 逐元素出队),判定语义微妙不一致,属重复实现。
- 影响:check 零 warning 是靠 `#[allow(dead_code)]` 压制,掩盖了"计划中的功能(热键、通知)其实没做完"这一事实。

**P2-18 构建环境残留**
- 证据:四个空 target* 目录(见 ①);`build-check.bat:3` 硬编码 `D:\DevTools\SDK\Rust\cargo\bin\cargo.exe`;GNU/MSVC 工具链并存且默认是 GNU。
- 影响:新环境/CI 复现构建容易踩坑;目录残留误导排查。

### 内存/性能总体评估

- 常驻路径(无设置窗口):1 个事件循环线程(recv_timeout 100ms)+ 1 Hz 检测线程 + 键盘钩子线程 + 托盘监听线程 + rodio 混音线程。空闲 CPU ≈ 0,符合 N2。
- 内存:常驻部分为纯 Rust 无 GUI,egui/eframe 仅在设置窗口打开时初始化,关闭即释放(run_native 返回)。预计常驻 <15MB,满足 N1(<30MB)。
- 无发现明显内存泄漏;唯一无界结构是 mpsc 事件通道(P2-13,量级可忽略)。

---

## ④ 技术债清单

| # | 债项 | 位置 | 说明 |
|---|------|------|------|
| T1 | 通知层缺失但上层已按"会有通知"设计 | tray.rs:112-116, ui.rs:96, main.rs:229 | spec/任务表承诺 Shell_NotifyIcon,实际 log stub,上层 UI/config 全部空转 |
| T2 | 热键层缺失但 UI/config 已暴露配置 | event.rs:22-25, ui.rs:139-141, config.rs:85 | 依赖未加、事件未发,配置项成为陷阱 |
| T3 | trigger_reminder 与状态机的职责错位 | main.rs:219-241 | "该不该提醒"的判定分散在 TimerTick 分支(main.rs:137-153)和 trigger_reminder 两处,Flow/Gaming 空臂导致提醒被吞/缺失 |
| T4 | 双份心流滑窗实现 | state.rs:25-28,66-83 vs detector.rs:50-98 | 语义可能漂移,应合一 |
| T5 | IdleTimeout 电平触发重复发 | main.rs:61-65 | 应边沿触发,顺带消除 P2-15 竞态 |
| T6 | 配置破坏式自愈 | config.rs:100-104 | 解析失败应备份而非删除 |
| T7 | 音频 panic 路径 | audio.rs:16-25 | 外设缺失属预期故障,应降级(如 MessageBeep)而非崩溃 |
| T8 | 全屏检测单显示器 + 2px 容差 | detector.rs:31-37 | 需按窗口所在显示器匹配;自动隐藏任务栏误判 |
| T9 | 无版本资源/图标/build.rs | 全仓 | 安装器 DisplayIcon 落空,任务栏无标识 |
| T10 | 工具链/目录历史包袱 | target*/, build-check.bat:3 | rustup 迁移残留,GNU/MSVC 并存,硬编码路径 |
| T11 | 随附产物过期且无 CI 校验 | installer/*.exe | exe 落后 src 1.5 小时的改动,重打包链条脆弱 |
| T12 | tasks.md 与代码实况不符 | tasks.md 任务 1 | "双击托盘发送 OpenConfig"未实现却标注 ✅ |

---

## ⑤ 建议的优化方向 Top 10

1. **修复心流吞提醒(P0-1)**:Flow 停歇 8s 后交付真实提醒——最简单做法是给 `trigger_reminder` 增加 Flow 分支(播放静音级提示音或低打扰通知);更优做法是"停歇 8s → 状态先回 Desktop → 再触发",并让 `reminder.reset()` 只在真正交付提醒后执行。
2. **实现 Gaming 仅提示音(P0-2)**:TimerTick 的 Gaming 分支评估 `should_remind()` 并只播声音;同时把"提醒被吞"改成"提醒顺延"(Gaming/Flow 期间不 reset,退出状态后立即补发),保证"提醒只会迟到、不会消失"。
3. **补上通知层(P0-3)**:用 `Shell_NotifyIcon(NIF_INFO)` 气泡(tray-icon 0.21+ 自带 `notify`/Windows 行为,或手写 winapi 调用),让 `notification_enabled` 真实生效;这是 Desktop 模式的最低成本视觉通道。
4. **建立"护眼闭环"**:到点后弹无边框倒计时小浮窗(25s 环形倒计时)+ 三个动作:「完成了」「延后 10 分钟」「跳过」;浮窗不抢焦点(WS_EX_NOACTIVATE/TopMost 可配),全屏时自动退化为气泡+音效。这是从"发声工具"变成"护眼工具"的关键一步。
5. **接入 global-hotkey 实现 Ctrl+Shift+E(P0-4)**,配置 UI 改为"点击录制热键"控件,而不是可编辑文本。
6. **音频健壮性**:无输出设备时降级为 `MessageBeep`/静默并托盘 tooltip 提示,绝不 panic;按设备实际采样率生成样本(或在 Source 链上加重采样);校准各预设频段至 1.5~2.5kHz 穿透区。
7. **全屏检测加固**:用 `MonitorFromWindow` + `GetMonitorInfo` 按前台窗口所在显示器比较;排除自身/工具窗口;对自动隐藏任务栏留容差;加滞回(连续 N 秒全屏才切换),杜绝"最大化=游戏"误判。
8. **托盘与配置体验**:双击图标开设置;菜单显示当前状态与"下次提醒约 X 分钟后",增加「立即休息」「延后 10 分钟」;开关用 check 菜单项;设置窗口加"恢复默认"、免打扰时间下拉/校验、min≤max 联动、"保存并关闭";配置解析失败改为改名备份+重建。
9. **调好心流默认值**:Medium 阈值从"10 次/30s"提为按打字速率(如 ≥60 键/30s 且持续 2 分钟)判定,或引入"Flow 连续持续 N 分钟才延后提醒";钩子过滤 KF_REPEAT,按 spec 限定键位集合。
10. **工程收尾**:嵌入 exe 图标+版本信息(winres/build.rs);更新安装打包流程并校验产物新鲜度(构建后自动打安装包);合并删除 4 个空 target* 目录;IdleTimeout 改边沿触发;统一心流滑窗实现;补齐 `directories` 死依赖的取舍;考虑切回 MSVC 工具链。

---

## 附:关键证据行号速查

| 结论 | 文件:行号 |
|------|----------|
| 心流提醒被吞 | main.rs:143-151, 155-158, 239 |
| Gaming 零提醒 | main.rs:152;(不可达分支)main.rs:234-238 |
| 通知为 stub | tray.rs:112-116;调用点 main.rs:229-232 |
| 热键未实现 | event.rs:22-25;main.rs:124-130;Cargo.toml(无 global-hotkey) |
| 音频 panic | audio.rs:16-25 |
| 配置删除重建 | config.rs:100-104 |
| 全屏单显示器判定 | detector.rs:31-37 |
| 钩子回调(轻量,无重活) | detector.rs:154-170;消息泵 detector.rs:196-198 |
| IdleTimeout 每秒重发 | main.rs:61-65;reset main.rs:92-96 |
| Flow 阈值语义 | config.rs:14-22;state.rs:127 |
| 托盘无双击 | tray.rs(全文件,无 TrayIconEvent) |
| 自启注册表 | installer.nsi:16, 52(写入);87(卸载清理) |
| 状态机 tick 优先级 | state.rs:99-137(Gaming>Away>回退>Flow>Desktop) |
| 设置窗口阻塞主循环 | main.rs:193-196, 255 |
