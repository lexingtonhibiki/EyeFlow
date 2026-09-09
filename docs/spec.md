# EyeFlow — 规格文档

> **v0.2 · 2026-09**
> 领域用词以 [CONTEXT.md](CONTEXT.md) 为准;默认参数与关键决策的权威来源是 [docs/adr/0001~0006](adr/),实现计划见 [docs/plan-v0.2.md](plan-v0.2.md)。

## 相对 v0.1 的变化

- **默认节奏**改为 AOA 20-20-20 双层休息:短休息 15~25 分钟三角随机(峰值 20 分钟)、持续 30 秒;新增长休息(连续用屏 2 小时 → 15 分钟),取代 v0.1 的 10~18 分钟 / 25 秒(ADR-0001)。
- **提醒形态**从"一声提示音"补全为 预告 → 休息界面 → 结算 闭环;新增预告、护眼贴士、延后(限 1 次)/ 跳过 / 完成。
- 确立"**提醒只顺延、不消失**"规则:心流中顺延到输入停歇 8 秒;游戏/不可打扰仅响提示音、退出后补发预告(ADR-0002/0004)。
- **新增**:坚持统计(stats.toml)、全局热键 Ctrl+Shift+E = 立即休息、单实例、开机自启开关、严格模式(默认关)、5 种提示音预设。
- **UI 宿主**改为按需启动的 eframe 0.36(glow)会话:平时是无窗口、无 GL 上下文的轻量循环,只在显示浮窗 / 设置时进入 eframe,窗口关完即释放;N1 维持 <30 MB(未打开窗口时实测约 16 MB,ADR-0006)。
- **旧配置自动迁移**:检测到 v0.1 字段(`min_interval_secs` 等)时,用户改过的值带过来、旧默认值换成新默认值,原文件备份为 `config.toml.bak-v0.1-<时间戳>`。

---

## 1. 背景

### 1.1 问题描述

长时间面对屏幕工作/游戏,用户容易忘记休息眼睛,导致视疲劳、干眼等健康问题(AAO 将数字视疲劳定位为暂时性、可逆的不适)。
现有护眼工具要么过于 intrusive(弹窗抢焦点、打断游戏),要么配置复杂违背极简原则。
需要一个能感知上下文、不打扰、恰到好处的护眼提醒工具。

### 1.2 现状分析

用户之前设计过类似工具(v0.1)但效果不好。主观感受之外,源码审计([docs/research/01-source-audit.md](research/01-source-audit.md))定位了根因:

- 游戏全屏时弹窗抢夺焦点,破坏体验(全屏检测仅凭窗口尺寸,误判面大)
- 心流状态下突然打断,反而降低效率(实现上心流提醒被静默吞掉并重置计时)
- 提示音容易被游戏音效淹没
- 缺乏上下文感知能力
- v0.1 实现缺口:通知是日志占位、全局热键未实现、无音频设备即崩溃、离开判断漏鼠标

### 1.3 主要使用场景

- **日常桌面使用**:浏览、看视频、写文档
- **编程心流**:长时间专注编码,键盘高频输入
- **全屏游戏**:独占全屏模式,不可被弹窗打断
- **短暂离开**:用户不在电脑前

## 2. 目标

### 2.1 目标

- 在合适的时间、用合适的形态提醒用户护眼;上下文只能改变投递时机与形态,不能取消提醒(ADR-0002)
- 短休息引导用户看远 30 秒,长休息 15 分钟(依据 AOA,见 ADR-0001)
- 零配置开箱即用,参数可配而非必须配
- 系统资源占用低,不影响主任务

### 2.2 非目标

- 不做视力检测
- 不做眼保健操指导
- 不做使用时长统计大屏(只做轻量坚持统计)
- 不做蓝光过滤 / 色温调节(证据不支持且与系统功能重叠,ADR-0003)
- 不做默认强制锁键 / 不可跳过的锁定(ADR-0003)
- 不做账号云同步、订阅付费墙
- 不做跨平台(仅 Windows 10/11 x64)

## 3. 需求

### 3.1 功能性需求

| ID | 需求 | 说明 |
|----|------|------|
| F1 | 双层休息调度 | 短休息:15~25 分钟三角随机(峰值 20 分钟)、持续 30 秒;长休息:连续用屏累计 2 小时后下一次短休息升级为 15 分钟长休息,默认开启(ADR-0001) |
| F2 | 预告(Heads-up) | 休息开始前 15 秒预告(长休息 30 秒);可选 现在开始 / 延后 5 分钟(每次提醒限 1 次,长休息同样可延后)/ 跳过 |
| F3 | 休息界面(Break panel) | 居中、置顶、不抢焦点;大倒计时 + 一条 AOA/AAO 权威护眼贴士;[继续工作] = 跳过;倒计时结束自动完成并播结束音;严格模式可选全屏遮罩(默认关),可自选背景图片(png/jpg/webp/bmp/gif,三种自适应 + 设置窗内 16:9 实时裁剪预览) |
| F4 | 上下文感知 | 桌面 / 心流 / 游戏 / 离开 四态;上下文只改变提醒的投递时机与形态,不取消提醒(ADR-0002) |
| F5 | 心流顺延 | 30 秒滑窗按键数达阈值(低 40 / 中 70 / 高 100,过滤按键自动重复);到点后顺延到最后按键停歇 8 秒再投递 |
| F6 | 游戏/不可打扰降级 | SHQueryUserNotificationState ≠ ACCEPTS(独占全屏/演示/锁屏等)或前台窗口全屏且无标题栏时:立即仅响提示音,视觉部分顺延,退出后补发预告(ADR-0004) |
| F7 | 离开处理 | GetLastInputInfo(含鼠标与键盘);空闲 ≥ away_secs 计时暂停;进入离开状态即视为一次自然休息,回来后重新计时;离开 ≥ 15 分钟用屏累计归零 |
| F8 | 坚持统计 | 今日完成(短/长/自然休息)、跳过、延后次数、连续坚持天数;持久化到 stats.toml,跨日滚动 |
| F9 | 系统托盘 | 左键单击/双击打开设置;右键菜单:打开设置 · 启用提醒(勾选) · 立即休息 · 暂停 1 小时 · 静音 · 退出;tooltip 显示当前状态与下次休息时间 |
| F10 | 全局热键 | Ctrl+Shift+E = **立即休息**(固定,不可配置;取代 v0.1 的"静音快捷键"语义) |
| F11 | 配置界面 | 分组卡片设置窗(提醒节奏 / 休息内容 / 声音 / 上下文 / 系统);min≤max 联动、时间下拉校验、恢复默认、开机自启开关、今日统计与"立即休息" |
| F12 | 单实例 | CreateMutexW 互斥,二次启动直接退出 |
| F13 | 开机自启开关 | HKCU\Run;安装默认开启、设置界面可开关、便携运行默认不写(ADR-0005) |

### 3.2 非功能性需求

| ID | 需求 | 说明 |
|----|------|------|
| N1 | 内存占用 | **< 30 MB 常驻(未打开窗口时)**,实测约 16 MB 工作集 / 2.5 MB 私有。显示浮窗 / 设置期间因 GL 上下文短暂升至约 200 MB,关闭后回落到 60~90 MB(NVIDIA 驱动堆残留,ADR-0006);仍显著低于 Electron 竞品(100~200 MB 常驻) |
| N2 | CPU 占用 | 空闲时 ≈ 0%(轻量循环 200 ms 轮询;UI 会话期间 1 Hz `request_repaint_after` 节拍,无连续重绘) |
| N3 | 启动速度 | 双击后 < 1 秒托盘出现 |
| N4 | 音频 | 提示音频率设计在 1.5kHz~2.5kHz 谐波区,穿透游戏音效;无音频设备时静音降级,不崩溃 |
| N5 | 打包体积 | 便携 exe 约 7 MB(strip + opt-level=z + LTO + panic=abort;含 egui/eframe 与默认字体) |

### 3.3 配置项清单

配置文件 `%APPDATA%\eyeflow\config.toml`;解析失败时改名备份为 `config.toml.bak-corrupt-<时间戳>` 后以默认值重建;v0.1 旧格式自动迁移并备份为 `config.toml.bak-v0.1-<时间戳>`。

| 字段 | 默认值 | 说明 |
|------|--------|------|
| enabled | true | 提醒总开关(与托盘"启用提醒"一致) |
| short_break_min_secs | 900 | 短休息间隔下限(15 分钟) |
| short_break_max_secs | 1500 | 短休息间隔上限(25 分钟;三角分布,峰值 20 分钟) |
| short_break_secs | 30 | 短休息时长(秒) |
| long_break_enabled | true | 是否启用长休息 |
| long_break_after_secs | 7200 | 连续用屏多久触发长休息(2 小时) |
| long_break_secs | 900 | 长休息时长(15 分钟) |
| heads_up_secs | 15 | 预告提前量(秒;长休息预告为 30 秒) |
| postpone_secs | 300 | 延后时长(5 分钟;每次提醒最多延后 1 次) |
| sound_enabled | true | 提示音开关 |
| sound_preset | "gentle_chime" | 提示音预设:gentle_chime / soft_tap / water_drop / digital_drop / triple_beep / custom |
| custom_sound_path | (空) | 自定义提示音文件(wav / mp3 / ogg / flac / m4a / aac,≤ 5 分钟),`sound_preset = "custom"` 时使用 |
| cue_volume_pct | 100 | 提示音音量 50~200%(>100 为主动放大) |
| cue_duration_secs | 1 | 提示音时长 1~5 秒;声音是唯一通道(游戏/全屏)时自动至少 2 秒 |
| hotkey_enabled | true | 全局热键 Ctrl+Shift+E 是否注册(可关闭以避免与其他软件冲突) |
| visual_enabled | true | 视觉提醒(预告浮窗 + 休息界面);关闭后仅声音 |
| strict_mode | false | 严格模式:休息界面变为全屏遮罩 |
| strict_wallpaper_path | (空) | 严格模式背景图片;留空为纯暗色遮罩 |
| strict_wallpaper_fit | "cover" | 背景自适应:cover / contain / stretch |
| update_check_enabled | false | 启动时检查更新(每 24 小时至多一次,仅访问 GitHub Releases API) |
| update_last_checked | (空) | 上次检查更新的 Unix 时间戳(程序维护) |
| quiet_start | "00:00" | 免打扰时段开始 |
| quiet_end | "08:00" | 免打扰时段结束 |
| flow_sensitivity | "Medium" | 心流判定灵敏度:Low / Medium / High |
| away_secs | 180 | 无输入多久判定为离开(秒) |

统计文件 `%APPDATA%\eyeflow\stats.toml`:今日完成(短/长/自然)、跳过、延后次数、连续坚持天数。

---

## 4. 设计方案

### 4.1 方案概览

**核心思路:** 一个纯 Rust 编写的 Windows 托盘常驻工具,不使用 WebView。主线程平时跑一个无窗口、无 GL 上下文的轻量循环(托盘 / 热键 / 传感器 / 调度核心照常工作);只有需要显示窗口时才进入 eframe 会话(ADR-0006):根视口是屏幕外的 1×1 锚点窗口,预告浮窗、休息界面、设置窗都是子视口,浮窗用"每次新建窗口 + `with_active(false)` + `WS_EX_NOACTIVATE`"实现"看得见但不抢焦点";所有窗口关闭后会话结束、GL 上下文释放。

**为什么不选 Tauri:** EyeFlow 大部分时间处于无窗口状态,Tauri 即使隐藏窗口也会驻留 WebView2 进程(40-80MB 内存)。eframe(glow)按需会话方案在无窗口时不持有任何渲染上下文。

**技术栈:**

| 层级 | 选型 | 理由 |
|------|------|------|
| 语言 | Rust stable(MSRV 1.95) | 单进程、无 GC、体积小 |
| GUI | eframe / egui **0.36**(glow + default_fonts) | 按需启动的 UI 会话与全部视口;显式选 glow 避免 0.34+ 默认 wgpu 的二进制与内存膨胀(ADR-0006) |
| 系统托盘 | tray-icon **0.24** | Tauri 组织维护;必须创建在 Win32 消息泵线程上 |
| 全局热键 | global-hotkey **0.8** | Ctrl+Shift+E 立即休息;与托盘同泵 |
| 音频 | rodio **0.22**(仅 playback) | 代码合成提示音,无需解码器与音频文件 |
| Windows API | windows **0.62** | 可打扰性、空闲、键盘钩子、注册表、单实例 |
| 配置存储 | serde + toml **1.x** | config.toml / stats.toml |
| 资源嵌入 | winresource **0.1**(build-dep) | exe 图标 + 版本资源(MSVC 需 rc.exe,GNU 需 windres) |
| 打包 | NSIS 3 | 安装 + 卸载,清理:exe、config/stats、注册表 RUN 键、卸载器自身 |

### 4.2 核心模块设计

#### 4.2.1 上下文状态机

EyeFlow 的核心是一个四状态机,根据用户行为自动切换(检测由传感器线程 1 Hz 采样):

```
                ┌──────────────────┐
                │     DESKTOP      │ ← 默认状态:完整投递(预告 + 休息界面 + 声音)
                └───┬─────┬────┬──┘
        键盘高频输入 │     │    │ 全屏 / 不可打扰
                    ▼     │    ▼
             ┌──────────┐ │  ┌──────────────┐
             │   FLOW   │ │  │    GAMING    │
             │ 顺延:停歇 │ │  │ 仅响提示音,   │
             │ 8s 后投递 │ │  │ 视觉顺延+补发  │
             └──────────┘ │  └──────────────┘
                  空闲超时 │
                         ▼
                  ┌──────────┐
                  │   AWAY   │
                  │ 计时暂停  │
                  └──────────┘
```

| 状态 | 检测条件 | 到点行为 |
|------|---------|---------|
| 桌面 | 可打扰 + 非全屏 + 键盘活动低 | 完整投递:预告 → 休息界面 |
| 心流 | 30 秒滑窗按键数 ≥ 阈值(低 40 / 中 70 / 高 100,过滤自动重复) | **顺延**:等最后按键停歇 8 秒后完整投递 |
| 游戏 / 不可打扰 | SHQueryUserNotificationState ≠ ACCEPTS(BUSY / D3D 全屏 / 演示 / 锁屏 / QUIET_TIME)或前台窗口矩形 == 所在显示器且无 WS_CAPTION | **立即仅响提示音**;视觉顺延,上下文恢复后**补发**预告 |
| 离开 | GetLastInputInfo 空闲 ≥ away_secs(180s,含鼠标) | 计时**暂停**;进入离开状态即视为一次**自然休息**并重新计时;离开 ≥ 15 分钟用屏累计归零 |

**顺延 / 补发规则(ADR-0002):** 上下文只能改变提醒的投递时机与形态,不能取消提醒。`should_remind` 到点后不再由分支决定是否重置,而是由投递结果决定;计时器区分"重置 / 暂停 / 顺延中"三态。"顺延(Defer)"是系统自动推后,与用户主动的"延后(Postpone)"严格区分(CONTEXT.md)。

**其他投递规则:** 延后仅在预告阶段可用且每次提醒限 1 次(短休息与长休息均可,v0.4 起);长休息跳过后 10 分钟再提示;免打扰时段 / 暂停 1 小时期间不投递,结束后若已到点则 60 秒后投递;游戏/不可打扰中退出后补发一次预告。

#### 4.2.2 运行模式与事件循环(ADR-0006)

同一份 `Runtime::step`(排空事件 → 托盘 / 热键命令 → 调度核心 tick → 统计落盘 → tooltip)在两种模式下运行,因此窗口开不开都不会让提醒、托盘或热键停摆:

```
主线程
  ├── 轻量循环(默认):PeekMessageW 泵 + 200 ms 轮询,无窗口、无 GL
  │     └── needs_ui()(有阶段要显示 / 设置窗被请求)→ 进入 UI 会话
  └── UI 会话:eframe::run_native(run_and_return,复用线程局部 winit 事件循环)
        ├── App::logic:Runtime::step,1 Hz request_repaint_after
        └── App::ui:根视口 = 屏幕外锚点;子视口 = 预告浮窗 / 休息界面 / 设置窗
              └── 所有窗口关闭 → 关闭根视口 → run_native 返回 → 回到轻量循环

后台线程
  ├── 传感器线程(1 Hz):空闲毫秒 / 前台全屏 / SHQueryUserNotificationState → Event::Sensors
  └── 键盘钩子线程(自带 GetMessage 泵):按键时间戳(过滤自动重复与修饰键)→ Event::KeyPress
```

托盘(tray-icon)与热键(global-hotkey)在启动时于主线程创建;后台线程在 UI 会话期间通过全局 `egui::Context` 句柄 `request_repaint()` 唤醒,轻量循环期间该句柄为空、按节拍轮询。

#### 4.2.3 提醒音设计

5 种内置预设(gentle_chime / soft_tap / water_drop / digital_drop / triple_beep),由 rodio 合成,无需音频文件。频率设计在 **1.5kHz~2.5kHz 上行扫描 + 谐波叠加**,高于多数游戏音效(枪声 200-800Hz、爆炸低频),人耳天然能分离。音量跟随系统,不修改系统会话音量;**无音频设备时初始化失败仅告警并静音降级,不崩溃**(v0.1 P0-5)。

### 4.3 关键实现决策

| 决策 | 方案 | 依据 / 替代方案 |
|------|------|----------------|
| 默认节奏 | AOA 20-20-20 双层休息(短 15~25min/30s,长 2h/15min) | ADR-0001;替代:保留 10~18min(证据弱)或 30min/5min(作为长休息可选档) |
| 顺延不丢弃 | 心流/游戏中顺延 + 补发 | ADR-0002;替代:上下文中不提醒(等于让最需要护眼的场景失去保护) |
| 提醒形态 | 预告 + 不抢焦点休息界面,严格模式可选;不做蓝光滤镜 / 强制锁定 | ADR-0003 |
| 可打扰性 | SHQueryUserNotificationState 为主,前台窗口矩形 + 无标题栏比对兜底 | ADR-0004;替代:纯窗口尺寸比对(自动隐藏任务栏时误判) |
| 离开判定 | GetLastInputInfo(含鼠标) | 替代:仅键盘钩子(鼠标用户被误判,v0.1 缺陷) |
| 开机自启 | HKCU\Run,安装默认开 + 应用内开关 | ADR-0005;替代:任务计划(脱离任务管理器管控,违背用户可控) |
| UI 宿主 | 按需启动的 eframe(glow)会话 + 子视口,空闲时无 GL | ADR-0006;替代:常驻 eframe(实测 NVIDIA 机器上 200 MB 常驻)、v0.1 式阻塞 run_native(托盘停摆) |
| 全屏检测 | SHQueryUserNotificationState + 前台窗口比对 | 替代:GetForegroundWindow + SM_CXSCREEN(v0.1 误判面大) |
| 键盘钩子 | SetWindowsHookEx WH_KEYBOARD_LL(仅记录时间戳,回调即返) | 替代:GetAsyncKeyState 轮询(漏键 + CPU 开销)、rdev(停更) |
| 空闲检测 | GetLastInputInfo | 20 年验证的 Win32 API,无需钩子权限 |
| 单实例 | CreateMutexW + ERROR_ALREADY_EXISTS | 替代:single-instance crate(停更) |
| 配置存储 | TOML,%APPDATA%\eyeflow\config.toml | JSON / YAML |
| 提示音生成 | rodio 合成音源(编译期嵌入样本参数) | 运行时解码 WAV 文件(需解码器,体积膨胀) |
| 卸载清理 | NSIS:exe + config/stats(+bak)+ HKCU\Run + 卸载键 + 卸载器自删 | 手动删除 |

### 4.4 项目结构

```
eyeflow/
├── Cargo.toml / Cargo.lock
├── build.rs               # 程序化生成 ICO + winresource 嵌入图标与版本资源
├── src/
│   ├── main.rs            # 入口:单实例、日志、线程启动、轻量循环 ⇄ UI 会话切换
│   ├── runtime.rs         # Runtime::step:托盘 / 热键 / 传感器事件 + 调度核心推进 + 落盘
│   ├── core.rs            # 调度核心(纯逻辑,可单元测试):上下文状态、计时、阶段机、统计结算
│   ├── app.rs             # UI 会话:锚点根视口、预告浮窗 / 休息界面 / 设置窗子视口
│   ├── ui.rs              # 设置窗内容(egui)
│   ├── detector.rs        # 键盘钩子 + 空闲 / 全屏 / 可打扰性采样线程
│   ├── audio.rs           # 提示音合成与播放(失败降级静音)
│   ├── config.rs          # config.toml 读写、越界修正、v0.1 迁移与备份
│   ├── stats.rs           # stats.toml 坚持统计
│   ├── tips.rs            # 护眼贴士文案(AOA / AAO)
│   ├── tray.rs            # 托盘图标与菜单
│   ├── autostart.rs       # HKCU\Run 读写
│   ├── icon.rs            # 程序化图标(与 build.rs / examples 共享)
│   └── event.rs           # 线程间事件与 UI 唤醒句柄
├── examples/              # gen-icon(生成 assets/icon.ico)、eframe_min(内存基线对照)
├── assets/                # icon.ico、icon-preview.png、screenshots/
├── installer/installer.nsi
└── docs/                  # spec、adr/、plan-v0.2、research/
```

### 4.5 v0.2 范围清单

- [x] 按需 eframe 会话 + 轻量循环,托盘/热键同泵(ADR-0006)
- [x] 调度核心:短/长休息计时、用屏累计、离开暂停、三角随机(12 个单元测试)
- [x] 上下文状态机与顺延/补发(ADR-0002/0004)
- [x] 预告浮窗 + 休息界面 + 严格模式
- [x] 坚持统计(stats.toml)
- [x] 设置窗(分组卡片、校验、恢复默认、自启开关)
- [x] 托盘菜单与 tooltip
- [x] 全局热键 Ctrl+Shift+E = 立即休息
- [x] 5 种提示音预设 + 无音频设备降级
- [x] 单实例、配置备份重建与 v0.1 迁移、exe 图标与版本资源
- [x] MIT、CI、tag 触发 release(NSIS + portable zip + sha256)

后续候选:多显示器遮罩覆盖、自定义热键、Toast 辅助通知、winget 包、浮窗 Win32 自绘以消除显卡驱动内存残留。
