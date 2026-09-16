# 08 代码级快赢审计(低成本高体验)

> 调研日期:2026-09-10。对象:EyeFlow v0.4(src/ 全部 16 个文件,约 4850 行,44 测试全过)。
> 方法:本地通读源码(只读审计,未改任何 src 文件)+ 少量联网核查(≤8 来源,见 ④)。
> 定位:与 01-source-audit(缺陷向)、04-windows-ux-guidelines(规范向)互补,本文聚焦"改动小、体验提升明显"的机会,并给出可直接落地的 API/函数。
>
> 成本:S ≤ 半天;M ≈ 1~2 天;L > 2 天。收益:H 显著可感;M 明确改善;L 锦上添花。

---

## ① 问题/机会清单(按性价比排序,Top 15)

| # | 位置(文件:行) | 问题/机会 | 建议做法 | 成本 | 收益 |
|---|---|---|---|---|---|
| 1 | core.rs:250-253, 276, 292 | **睡眠/唤醒无处理,且 QPC 跨睡眠累计(已核实)**:进程随系统挂起冻结,醒来后第一个 `tick` 的 `dt` 包含整个睡眠时长。若睡眠跨过 due 点→醒来立即投递预告;`screen_accum += dt` 一次灌满 2 小时→同时误升 15 分钟长休息。微软文档明确 QPC"including the time when the machine was in a sleep state" | `Core::tick` 开头检测 `dt > RESUME_GAP(如 120s)`:视为"离席归来"——`screen_accum = 0`、`schedule_next(now)`(重新采样 15~25 分钟,自然顺延)、记 log;可加纯逻辑方法 `Core::on_wake(now)` 便于单元测试。可选增强:主线程建隐藏顶层窗口收 `WM_POWERBROADCAST/PBT_APMRESUMEAUTOMATIC` → `Event::Resumed` → 同样处理(注意 HWND_MESSAGE 的 message-only 窗口收不到广播,必须是普通隐藏窗口) | S | H |
| 2 | tray.rs:79-80 | **托盘图标固定 32×32,高 DPI 下被 Shell 缩放发糊**:多数笔记本 125%/150% 缩放,任务栏需要 40/48 px | 渲染尺寸跟随系统 DPI:`SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`(main 早期调用,winit 稍后也会设,幂等)→ `GetSystemMetrics(SM_CXICON)` 拿到 DPI 感知的图标尺寸 → `icon::render_rgba(size)`。procedural 渲染零成本,不需要 16/32 双份 ICO 资源 | S | M |
| 3 | app.rs:88-111, 458-468 | **多显示器:预告浮窗/严格全屏遮罩只看"根视口所在显示器"(实为主屏),不看用户正在用的屏幕**;严格模式下用户在副屏工作则遮罩罩错屏,形同虚设。另 `monitor_size` 主路径与 fallback 路径单位疑似不一致(主路径直接用 `viewport().monitor_size`,fallback 把 `GetSystemMetrics` 除以 ppp)——两条路径必有一错,100% DPI 下不可见 | `GetForegroundWindow()` → `MonitorFromWindow` → `GetMonitorInfoW.rcMonitor`(detector.rs:121 `monitor_rect` 现成可复用),用 `ViewportBuilder::with_position(Position::Physical)` / `with_inner_size(Position::Physical)` 直接传物理坐标,绕开逻辑/物理换算歧义;严格遮罩改为"铺满该显示器矩形"而非 `with_fullscreen(true)`。顺手打印一次 `monitor_size` 确认 egui 单位并统一 fallback | M | H |
| 4 | runtime.rs:424-439 | **tooltip"下次休息约 12 分钟后"不如具体钟点直观**,且相对时间导致 tooltip 文本每分钟变化→每分钟一次 NIM_MODIFY | `format!("下次休息 {}", (chrono::Local::now() + self.core.next_break_in(now)).format("%H:%M"))`;副收益:tooltip 只在排期变化时更新。设置窗状态卡可保留"约 X 后"(空间充裕)或统一钟点 | S | M |
| 5 | app.rs:313-408; core.rs:341-346 | **倒计时最后 3 秒无预备**:用户不知道何时"结束、该回神",结束音突兀 | 视觉:`remaining <= 3s` 时倒计时字号/强调色脉冲(`request_repaint_after(50ms)`);听觉:`Action` 加 `Tick` 变体,core 在 Break 剩 3/2/1 秒时各推一次,runtime 播 50ms 短音(复用 `audio::sweep(0.05, 0.2, |_p| 2000.0)`) | S | M |
| 6 | app.rs:471-492(WS_EX_NOACTIVATE 后果) | **面板永远无键盘焦点→收不到 Esc,纯键盘用户无法跳过(无障碍缺口)**;这也回答"严格模式该不该允许 Esc":面板本来就收不到键盘,真正缺口是键盘可达性 | Break 进入时用现成的 global-hotkey crate 临时注册 `HotKey::new(None, Code::Escape)`,Break 结束 unregister;非严格默认开、严格模式默认关(可用性 vs 严格:点击"继续工作"已是逃生门,不必再开 Esc)。风险提示:Break 期间 Esc 被系统级吞掉(全屏视频按 Esc 退出失效 20~30s),设置里给开关 | S/M | M |
| 7 | config.rs:163-165, 353-364; ui.rs:414-428 | **免打扰只有单时段**:午休(12:00-13:30)与夜间(22:00-08:00)无法同时配 | 加 `quiet2_start/quiet2_end`(serde default,复用"起止相同=未启用"约定),`in_quiet_hours` 依次检查两个区间;context_card 加第二行 | S | M |
| 8 | runtime.rs:163-210(step) | **外部手改 toml 不热生效**,且下次托盘开关/保存时被 `persist_config` 静默覆盖 | step 里 1Hz 检查 `Config::path().metadata().modified()`(一次 stat,可忽略),变化则 `Config::load()` + `core.apply_config()` + 托盘/热键同步;自己保存后的 mtime 变化做一次抑制(或直接幂等 reload)。不建议上 notify crate,见 ③ | S | M |
| 9 | config.rs:313-320; stats.rs:45-54 | **配置/统计写入非原子**:`fs::write` 直写,断电/崩溃可能留下半截 toml(虽有备份重建兜底,但丢用户配置) | 写 `config.toml.tmp` → `remove_file(目标)` → `fs::rename(tmp, 目标)`;stats 同样处理 | S | M |
| 10 | app.rs:189-201 | **设置窗直接点 X 会静默丢弃未保存修改**(close_requested → `settings = None`),与"有未保存的修改"提示形成反差 | `close && state.has_unsaved()` 时不关闭,先在窗内显示确认条("放弃修改并关闭 / 返回编辑"),第二次关闭才退出;需给 `SettingsState` 加 `close_confirmed: bool` | S | M |
| 11 | runtime.rs:24; tray.rs:55-56, 158-164 | **暂停只有"1 小时"一档**,菜单文本与 tooltip 写死 | `TrayCommand::PauseFor(Duration)` + 托盘子菜单(30 分钟 / 1 小时 / 到明天);`reminder_line`/tooltip 改"已暂停至 HH:MM"(`Core` 暴露 `paused_until`) | S | M |
| 12 | app.rs:48-61; config.rs | **无障碍:界面字号不可调**;Windows 的"放大文本"对 Win32 桌面应用不自动生效 | config 加 `ui_zoom_pct: u32 = 100`;`UiSession::new` 里 `ctx.set_zoom(zoom)`(egui 0.31+ API;等价地 `set_pixels_per_point(ppp * zoom)`),设置窗"系统"卡加 90~130% 滑条 | S | M |
| 13 | config.rs:10-41; detector.rs:148-169 | **心流判定只有键盘**:画图/设计等高频鼠标场景不算心流;且灵敏度没有"关"档(不想要心流等待的用户无法关闭) | 立即做:`FlowSensitivity` 加 `Off` 档(threshold 无穷),S 成本。重估后可选:加 `WH_MOUSE_LL` 统计移动/点击密度(必须节流:事件里只累计计数、每秒最多投递 1 个 `Event::MouseActivity`,避免事件风暴再开钩子线程),把"键+鼠标"合并进 `flow_active` 计数 | S(Off)/ M(鼠标) | M |
| 14 | core.rs:336-340 | **预告期间切入全屏的边缘**:HeadsUp 进行中用户打开全屏游戏/视频→预告浮窗(ALWAYS_ON_TOP)叠在游戏上,随后 Break 也会照常开始 | `Phase::HeadsUp` 到点时复查 `self.state == Gaming` → 顺延(把 HeadsUp 的 `ends` 后推或退回 Idle 沿用 Gaming 补发路径),与 ADR-0002"顺延而非丢弃"一致 | S | M |
| 15 | runtime.rs:202; tray.rs:158-164; main.rs:196-205 | 小修打包:①`set_paused` 每帧无变化守卫地 `set_text`(UI 会话期间每帧调用 muda API);②日志每次启动截断、长开机不轮转 | ①Tray 存 `paused: bool` 上次值,变化才 set_text(照抄 set_tooltip 模式);②init_logging 前检查 `eyeflow.log` > 1MB 则 rename 为 `.old` | S | L |

### 已核查为"达标,无需做"的项

- **深色模式下标题栏仍是浅色?** 不是问题:eframe 0.36 底层 winit 0.30 在窗口创建时无条件调 `try_theme(None)`,经 uxtheme ordinal 132(`ShouldAppsUseDarkMode`)检测系统主题后,用 `SetWindowTheme("DarkMode_Explorer")` + `SetWindowCompositionAttribute(WCA_USEDARKMODECOLORS)` 自动上深色标题栏;egui 内容侧 `set_theme(ThemePreference::System)`(app.rs:50)同样跟随。仅极老 Win10 构建可能失效,届时才需要手动 `DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)` 兜底(对子视口 HWND 设置,成本 S)。
- 锁屏:detector.rs:89-95 有意把 `QUNS_NOT_PRESENT` 交给空闲逻辑(锁屏无输入 → Away → 计时暂停+自然休息),语义正确,无需 WTSRegisterSessionNotification;若 #1 做了 WM_POWERBROADCAST,可顺带 `WTSRegisterSessionNotification` 在解锁时给 60s 宽限,属锦上添花。

---

## ② 每条实现要点

1. **睡眠/唤醒(#1)**:`core.rs` 顶部加 `const RESUME_GAP: Duration = Duration::from_secs(120);`。`tick()` 计算 `dt` 后:
   ```rust
   if dt > RESUME_GAP {
       log::info!("检测到 {:?} 的时间跳跃(睡眠/休眠),重新排期", dt);
       self.screen_accum = Duration::ZERO;
       self.phase = Phase::Idle;
       self.schedule_next(now);   // due 顺延为 now + 新采样间隔
       self.last_tick = now;
       return actions;
   }
   ```
   `Away` 分支里的 `self.due += dt`(core.rs:276)不受影响。可选 Win32 增强:`CreateWindowExW` 建隐藏顶层窗口,`WindowProc` 里 `WM_POWERBROADCAST`/`PBT_APMRESUMEAUTOMATIC` 时经 `tx.send(Event::Resumed)` 回主线程;主循环 `pump_win32_messages()` 已经在泵,零新线程。补两条测试:大 dt 跳跃后 `next_break_in` 应 ≥ 最短间隔;`screen_accum` 应为 0。
2. **托盘 DPI 图标(#2)**:`main()` 开头 `SetProcessDpiAwarenessContext(Some(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2))`(windows crate 特性 `Win32_UI_HiDpi`);tray.rs:
   ```rust
   let size = unsafe { GetSystemMetrics(SM_CXICON) } as u32; // per-monitor-v2 下随主屏 DPI 缩放
   let icon = Icon::from_rgba(crate::icon::render_rgba(size), size, size)?;
   ```
3. **多显示器(#3)**:新增 `fn foreground_monitor() -> Option<RECT>`(复用 detector.rs:121 `monitor_rect(GetForegroundWindow())`);构建浮窗时:
   ```rust
   base.with_position(egui::Pos2::from_physical(...)) // 用 Position::Physical(mon.right - w, mon.bottom - h)
   ```
   egui `ViewportBuilder::with_position/with_inner_size` 都接受 `impl Into<Position>`(`Position::Physical(Pos2/Size)`),直接传 `MONITORINFO.rcMonitor` 派生的物理坐标即可,无缩放换算。严格遮罩:放弃 `with_fullscreen(true)`,改 `with_decorations(false).withAlwaysOnTop` + 物理尺寸=rcMonitor 尺寸、位置=rcMonitor 左上角。`monitor_size()` 的单位歧义:先打一条 `log::info!("{:?} ppp={}", size, ctx.pixels_per_point())` 实测定案,再统一两条路径。
4. **tooltip 钟点(#4)**:`reminder_line` 的 Idle 分支改为钟点格式;注意 `next_break_in` 基于单调 `Instant`,钟点 = `Local::now() + next_break_in`,无需改 Core。跨零点(如 23:50 + 20min)显示 "00:10" 即可,不必标"次日"。
5. **最后 3 秒(#5)**:视觉在 `draw_break`:`if remaining <= Duration::from_secs(3)` 时把 `mmss` 的 `.size(...)` 每秒 +10%、`accent_color` 用 `Color32::from_rgb(...).gamma_multiply(0.7 + 呼吸项)`;听觉在 core:`Phase::Break` 分支里对 `remaining.as_secs()` 与 `last_tick` 差值判断跨过 3/2/1 边界时 `actions.push(Action::Tick)`,runtime 匹配后 `audio.play_cue(SoundPreset::SoftTap, cfg.cue_volume_pct, 1)` 或新增专用短音。
6. **Break 全局 Esc(#6)**:`Core` 的 `begin_break`/`complete_break`/`skip` 处无法直接碰热键,由 Runtime 层观察相位:`step()` 里检测 `matches!(phase, Break{..}) && cfg.esc_to_skip && !esc_registered` → `mgr.register(HotKey::new(None, Code::Escape))`;退出 Break 时 unregister。热键回调里仅当 `phase == Break` 才 `skip`。
7. **双免打扰时段(#7)**:字段 `quiet2_start/quiet2_end: String`(serde default 同 d_quiet_* 的"相同=关闭"约定,默认都 "12:00"),`in_quiet_hours` 改为 `self.in_span(now, &qs1, &qe1) || self.in_span(now, &qs2, &qe2)`;抽 `in_span` 小函数,现有测试不变+新增跨段用例。
8. **配置热重载(#8)**:Runtime 加 `cfg_mtime: Option<SystemTime>`;`step()` 里(轻量循环 1Hz 已足够,不必在 UI 帧做):
   ```rust
   let mt = std::fs::metadata(Config::path()).and_then(|m| m.modified()).ok();
   if mt.is_some() && mt != self.cfg_mtime && !self.suppress_reload_once { /* reload+apply */ }
   ```
   reload 后同步:`tray.set_enabled/set_sound/set_hotkey_hint`、`apply_hotkey_enabled`、settings 打开中则刷新 `saved`(不动用户正在编辑的 draft)。
9. **原子写(#9)**:Config::save 与 Stats::save 各改为:写 `path.with_extension("toml.tmp")` → `let _ = std::fs::remove_file(&path);` → `std::fs::rename(&tmp, &path)`。
10. **关闭确认(#10)**:`SettingsState` 加 `close_confirmed: bool`;app.rs:200 改为:
    ```rust
    if close {
        if state.has_unsaved() && !state.close_confirmed { state.close_confirmed = true; }
        else { rt.settings = None; }
    }
    ```
    `close_confirmed` 为 true 时在 footer 上方画红色确认条(两个按钮),任一交互后复位。
11. **暂停多档(#11)**:tray.rs `TogglePause` 项改为三个 `MenuItem`("暂停 30 分钟/1 小时/到今天 24 点"),`TrayCommand::PauseFor(Duration)`;`runtime.handle_tray` 调 `core.pause_for`;`reminder_line` 暂停分支用 `core.paused_until` 格式化钟点。
12. **字号缩放(#12)**:config 字段 + sanitized clamp(80~150);app.rs:50 后 `cc.egui_ctx.set_zoom(f32::from(cfg.ui_zoom_pct) / 100.0)`;ui.rs system_card 加滑条(改 draft,保存后下次会话生效,提示文案注明)。
13. **心流 Off 档 + 鼠标重估(#13)**:Off → `threshold() == u32::MAX`,`flow_active` 恒 false,`state` 永不为 Flow;label"关闭(不因输入延迟提醒)"。鼠标方案(重估后):`start_mouse_hook` 与键盘钩子同线程模式,`WM_MOUSEMOVE` 中仅 `counter += distance` 不发消息,独立 1Hz 定时器把计数打包成 `Event::MouseActivity(u32)`;`flow_active` 判定 `keys + mouse_counts >= threshold`。
14. **HeadsUp 中途全屏(#14)**:core.rs:336 分支改为:
    ```rust
    Phase::HeadsUp { ends, is_long, .. } => {
        if now >= ends {
            if self.state == ContextState::Gaming { self.phase = Phase::Idle; self.cue_played_this_round = false; /* 退出全屏后按 Gaming-Idle 补发路径走 */ }
            else { self.begin_break(now, is_long); }
        }
    }
    ```
15. **小修打包(#15)**:Tray 加 `paused_state: std::cell::Cell<bool>`,`set_paused` 先比较;init_logging 改为先 `fs::metadata(len > 1MB)` → `fs::rename("eyeflow.log", "eyeflow.log.old")` → 再 `File::create`。

---

## ③ 不建议做的(以及为什么)

| 项 | 理由 |
|---|---|
| 周/月统计视图 | 与产品哲学冲突:stats.rs 模块注释明言"不做历史时间线……不是数据面板"。需要 schema 级改造(历史记录),L 成本换低频需求。若坚持,上限是"最近 7 天完成数"一个数字,不做图表 |
| 用 notify crate 监听配置文件 | 托盘常驻程序里引入后台监听线程 + debounce 是负资产;#8 的 1Hz `mtime` stat 每秒一次系统调用,成本可忽略且无新依赖 |
| 严格模式允许 Esc 退出(默认) | 与严格模式目的相悖;且无焦点窗口本来收不到键盘,#6 的全局热键才是正解——非严格默认开、严格默认关,可用性与严格两头都占 |
| WM_DPICHANGED 实时跟随托盘图标 DPI | 变更显示器/DPI 后图标短暂发糊直到重启,可接受;#2 启动时渲染一次已覆盖 99% 场景 |
| 高对比度主题完整适配(SPI_GETHIGHCONTRAST + GetSysColor 全套) | 受众小、egui 无现成支持,需自建整套 visuals;先做 #12 字号缩放,HC 留待有用户反馈 |
| 立即做鼠标心流钩子 | WH_MOUSE_LL 事件风暴需要节流设计 + 第二条钩子线程,M 成本;先用 #13 的 Off 档承接"不想要心流"的用户,鼠标密度按反馈重估 |
| 把预告浮窗改成 Toast 通知 | 04 号文档已论证:预告需要常驻倒计时+按钮,气球/Toast 形态不匹配,现有自绘 NOACTIVATE 窗口是合规且更优的形态 |

---

## ④ 来源

1. Microsoft Learn — Acquiring high-resolution time stamps(QPC 计数**包含** standby/hibernate/connected standby 睡眠时间,即 `std::time::Instant` 会跨睡眠跳变;#1 的依据): <https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps>
2. Microsoft Learn — WM_POWERBROADCAST message(PBT_APMRESUMEAUTOMATIC 每次恢复必发;PBT_APMRESUMESUSPEND 仅用户输入触发恢复时追加;#1 可选增强的依据): <https://learn.microsoft.com/en-us/windows/win32/power/wm-powerbroadcast>
3. winit v0.30.9 源码 — dark_mode.rs(`try_theme(None)` → `should_use_dark_mode()` → uxtheme ordinal 132 `ShouldAppsUseDarkMode` + `SetWindowTheme("DarkMode_Explorer")` + `WCA_USEDARKMODECOLORS`;证明 eframe 0.36 已自动深色标题栏): <https://github.com/rust-windowing/winit/blob/v0.30.9/src/platform_impl/windows/dark_mode.rs>
4. winit v0.30.9 源码 — window.rs(窗口创建时无条件调 `try_theme(window, attributes.preferred_theme)`,注释"if the system theme is dark, we need to set the window theme now"): <https://github.com/rust-windowing/winit/blob/v0.30.9/src/platform_impl/windows/window.rs>
5. docs.rs tray-icon — TrayIconBuilder(文档不含任何 DPI/多尺寸图标处理,佐证 #2 需自行按 DPI 渲染): <https://docs.rs/tray-icon/latest/tray_icon/struct.TrayIconBuilder.html>
6. Microsoft Learn — The Notification Area(16/32 双尺寸 + LoadIconMetric 规范;本文 #2 选择"运行时按 DPI 渲染"的替代路线,规范背景见仓库 docs/research/04): <https://learn.microsoft.com/en-us/windows/win32/shell/notification-area>
7. Microsoft Learn — WTSRegisterSessionNotification(#1 可选增强,锁屏/解锁事件;现状由 QUNS + 空闲逻辑覆盖,仅锦上添花): <https://learn.microsoft.com/en-us/windows/win32/termserv/wtsregistersessionnotification>
