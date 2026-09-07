# Rust 技术栈升级调研(05)

调研日期:2026-09-08。基准:`E:\Workspaces\Hanako\eyeflow\Cargo.toml`(eframe/egui 0.31、tray-icon 0.24、rodio 0.22、windows 0.62)。
所有版本号与 API 均以 crates.io API / docs.rs / 官方 CHANGELOG 为准(见文末来源)。

---

## ① 推荐依赖表

| crate | 当前 | 目标版本 | 动作 | 理由 | URL |
|---|---|---|---|---|---|
| eframe / egui | 0.31 | **0.36.1**(2026-08-07) | 升级(跨 0.32→0.36,详见②) | 0.34 起 wgpu 成为 eframe 默认渲染器,轻量托盘应用必须显式选 glow;0.35 移除全部 deprecated API;MSRV 升至 1.95 | https://crates.io/crates/eframe |
| tray-icon | 0.24 | **0.24.2**(2026-07-27) | 补丁升级 | 维护活跃;Windows 端"创建线程 = 消息泵线程"规则不变 | https://crates.io/crates/tray-icon |
| global-hotkey | 未接入 | **0.8.0**(2026-05-01) | **新增** | spec 要求全局快捷键(Ctrl+Shift+E);Windows 后端用 windows-sys,与托盘同泵 | https://crates.io/crates/global-hotkey |
| rodio | 0.22(features=["playback"]) | **0.22.2** + `default-features = false` | 补丁 + 裁剪 feature | `DeviceSinkBuilder`/`MixerDeviceSink`/`Player` 稳定存在;默认 feature 捆绑 Symphonia 解码器(flac/mp3/mp4/vorbis/wav),纯合成音源完全不需要,可省 1~2MB | https://crates.io/crates/rodio |
| windows | 0.62 | **0.62.2**(2025-10-06) | 补丁升级 | 0.62 已是最新大版本,无需迁移 | https://crates.io/crates/windows |
| serde | 1 | 1.0.229 | 补丁 | 无变化 | https://crates.io/crates/serde |
| toml | 0.8 | **1.1.5** | 升级 | toml 1.x 是当前稳定线(0.9 大重构后);`toml::from_str`/`to_string` 主 API 兼容 | https://crates.io/crates/toml |
| directories | 6 | 6.0.0 | 不动 | 已最新 | https://crates.io/crates/directories |
| rand | 0.9 | 0.10.2(2026-08-25) | 可选升级 | 仅用于随机休息文案,迁移成本低;不急可留 0.9 | https://crates.io/crates/rand |
| chrono / log / env_logger | 0.4 / 0.4 / 0.11 | 0.4.45 / 0.4.34 / 0.11.11 | 补丁 | 无变化 | https://crates.io/crates/chrono |
| **dark-light**(可选) | 无 | **3.0.0**(2026-08-12) | 视需求新增 | 系统深/浅色检测(detect/subscribe);但 egui 自带 `ThemePreference::System` 跟随系统,仅当 egui 之外的 UI 也要感知主题时才引入 | https://crates.io/crates/dark-light |
| **winresource**(build-dep) | 无 | **0.1.31**(2026-03-16) | **新增** | exe 图标 + 版本信息嵌入;winres 的维护 fork,纯 API 式,比手写 .rc 简单 | https://crates.io/crates/winresource |
| ~~rdev~~ | 无 | — | **不引入** | 最后发版 0.5.3 停在 2023-06,3 年未维护;其 Windows 后端本就是 WH_KEYBOARD_LL,自写实现无额外收益且可控 | https://crates.io/crates/rdev |
| ~~single-instance~~ | 无 | — | **不引入** | 0.3.3 停在 2021;用现有 windows 依赖 `CreateMutexW` 约 15 行即可 | https://crates.io/crates/single-instance |
| ~~notify-rust / tauri-winrt-notification~~ | 无 | 0.8.1(备选) | 备选 | 推荐原生 `Shell_NotifyIconW + NIF_INFO`(零新依赖,支持 NIIF_*);仅当要 Win10+ 原生 Toast 动作按钮时再选 tauri-winrt-notification(维护活跃,2026-07);notify-rust 4.18 在 Windows 上内部就是包装 winrt-notification | https://crates.io/crates/tauri-winrt-notification |

### 推荐的 Cargo.toml 片段

```toml
[dependencies]
eframe = { version = "0.36", default-features = false, features = ["glow", "default_fonts"] }
egui = "0.36"
tray-icon = "0.24"
global-hotkey = "0.8"
rodio = { version = "0.22", default-features = false, features = ["playback"] }
toml = "1"

[build-dependencies]
winresource = "0.1"
```

> 注意:`default-features = false` 后不要开启 `accessibility` 除非需要读屏;`persistence`(ron)也不需要——EyeFlow 自己管配置文件。MSRV 要求 **Rust ≥ 1.95**,需核对 `install-rust.ps1`。

---

## ② 破坏性变更与迁移要点

### egui / eframe 0.31 → 0.36(按版本)

**0.32.0(2025-07)— Atoms 与 Popup 重写**
- 新布局原语 `egui::Atom`、`AtomLayout`;`Button`/`Checkbox`/`RadioButton` 重建其上(`ui.button((image, "text"))`)。
- Popup/Tooltip/菜单整体重写:**菜单默认点击项后即关闭**,删掉 `ui.close_menu()` 调用;`Memory::popup` 弃用;`SelectableLabel` 移除。
- 移除"弃用超过一年"的 API;MSRV 1.85;内部改 edition 2024(不影响使用者)。
- epaint:`FontDefinitions.font_data` 类型变为 `BTreeMap<String, Arc<FontData>>`——**字体注册代码必须套 `Arc::new(...)`**。

**0.33.0(2025-10)— Plugin 与文本**
- 新 `egui::Plugin` trait 取代 `on_begin_pass`/`on_end_pass`。
- `screen_rect` 弃用 → `content_rect` / `viewport_rect`。
- `egui::Mutex` 内置超时死锁检测:持锁超过一帧的场景换 std/parking_lot。

**0.34.0(2026-03)— 本项目受影响最大的一版**
- **eframe 默认渲染器改为 wgpu(wgpu 29)**。EyeFlow 是倒计时遮罩 + 配置窗,不需要 wgpu 的通用 GPU 能力:务必 `default-features = false, features = ["glow", "default_fonts"]`(glow 编译快、二进制小、驱动兼容好;wgpu 仅在需要复杂特效时考虑)。
- **"More Ui, less Context"**:`App::update(&mut self, &Context)` 弃用 → `App::ui(&mut self, ui: &mut Ui)`(另有 `App::logic`);`Ui` 现在 deref 到 `Context`,`ui.ctx().input(..)` 可写成 `ui.input(..)`;viewport 回调改发 `&mut Ui` 而非 `&Context`。
- Panel API 统一:`SidePanel`/`TopBottomPanel` 弃用 → `Panel`;`CentralPanel::show` 弃用 → `show_inside`;`Context::style` 改名 `global_style`。
- **字体后端 ab_glyph → skrifa + vello_cpu**:文字更锐、支持 hinting/可变字体。`FontData`/`FontDefinitions` 注册流程不变(`FontData::from_owned`/`from_static` 仍在),`FontTweak` 0.35 新增 hinting、`subpixel_binning` 字段;`AlphaFromCoverage` 改名 `FontColorTransferFunction`。
- MSRV 1.92。

**0.35.0(2026-06)— 清理版**
- **移除所有 `#[deprecated]` API**:0.34 升级后必须把弃用告警清零才能过编译。
- 移除 `impl Into<f32>` 参数(改传显式 `f32`);`Panel` 若干方法改名。
- 新增 `ViewportBuilder::with_monitor(index)` + `ViewportCommand::SetMonitor`(多显示器遮罩定位有用)。
- MSRV 1.95。

**0.36.0/0.36.1(2026-08)**
- `RawInput` 不再携带 `Modifiers`(修饰键变为普通 `egui::Event`);移除 `clip_rect_margin`;`Ui::stack().classes` 类系统;MSRV 1.95。

### 字体注册(中文 UI)迁移示例

```rust
// 0.31 旧写法
let mut fonts = egui::FontDefinitions::default();
fonts.font_data.insert("msyh".into(), egui::FontData::from_owned(bytes));
fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "msyh".into());
ctx.set_fonts(fonts);

// 0.36 新写法:唯一变化是 font_data 存 Arc<FontData>
fonts.font_data.insert("msyh".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
```

### tray-icon 0.24.2(Windows 消息泵共存认证过的规则)

- 官方文档:"Windows 上不需要主线程,但**托盘图标必须创建在跑着 win32 event loop 的那条线程上**"。
- 与 eframe 共存:eframe/winit 在主线程运行完整 win32 消息循环,天然满足条件——**把 `TrayIconBuilder::build()` 放在主线程**(run_native 之前或 `App` 构造时),然后**弃用自写 PeekMessageW 泵**(两个泵会抢消息,这是最大的架构坑)。
- 事件接收:在 `App::ui` 每帧 `TrayIconEvent::receiver().try_recv()`(crossbeam channel)。**不要**再调 `set_event_handler(Some(..))`——一旦设置,事件改走回调、channel 收不到。
- 变体:`Click { id, position, rect, button, button_state }`(单击,含按下/释放)、`DoubleClick`(仅 Windows)、`Enter/Move/Leave`;枚举 `#[non_exhaustive]`,match 要带 `_`。右键菜单用 `menu::MenuEvent::receiver().try_recv()`。

### global-hotkey 0.8.0

```rust
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
let manager = GlobalHotKeyManager::new()?;               // 必须在有 win32 event loop 的线程上创建
let hk = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyE);
manager.register(hk)?;
// 每帧轮询(tray 同款模式):
if let Ok(ev) = global_hotkey::GlobalHotKeyEvent::receiver().try_recv() {
    if ev.state == HotKeyState::Pressed && ev.hotkey == hk { /* 显示遮罩 */ }
}
```
事件含 Pressed/Released 两种状态,按需过滤。退出前 `unregister` 可选(进程退出系统自动回收)。

### Windows 原生气泡通知(推荐,零新依赖)

- `Shell_NotifyIconW(NIM_MODIFY, &nid)` + `nid.uFlags |= NIF_INFO`,`szInfo` 填正文、`szInfoTitle` 填标题、`dwInfoFlags` 填 `NIIF_INFO|NIIF_WARNING|NIIF_ERROR` 等。
- 关键标志:`NIIF_RESPECT_QUIET_TIME`(勿扰模式/专注助手期间不弹)、`NIIF_NOSOUND`(不响铃,与 EyeFlow 自己的提示音配合)、`NIIF_LARGE_ICON`、`NIIF_USER`。
- 常量位置(windows 0.62):`NIF_*` 在 `windows::Win32::UI::Shell::NOTIFY_ICON_DATA_FLAGS`,`NIIF_*` 在 `NOTIFY_ICON_INFOTIP_FLAGS`,均在**已有的 `Win32_UI_Shell` feature** 内,无需加 feature。
- 要点:发送气泡要求托盘图标已用 `NIM_ADD` 添加且持有 `NIF_MESSAGE`/`NIF_ICON`;`szInfo` 最多 256 wchar;Win10+ 系统会自动把旧式气泡转成 Toast 展示,无需额外工作;用户关掉"应用通知"权限后 `Shell_NotifyIconW` 仍返回成功(气泡被吞),可配合 `SHQueryUserNotificationState` 判断时机。
- 结论:用原生;`tauri-winrt-notification 0.8.1` 作为将来要"Toast 按钮/持续通知"时的备选(活跃维护),`notify-rust` 不必要。

### rodio 0.22.2

- `Sink` 在 0.22 已被 **`Player`** 取代(docs 顶层结构体只剩 `Device/Devices/Player/SpatialPlayer/SupportedStreamConfig`)。
- 设备打开:`rodio::DeviceSinkBuilder::open_default_sink() -> Result<DeviceSink, DeviceSinkError>`;**无音频设备时 Err → log::warn + 静音降级**,不要 panic。
- 播放:`let player = rodio::Player::connect_new(&sink.mixer()); player.append(src);`(或 `rodio::play(&sink.mixer(), file)`)。
- 合成音源(短期音效,免解码器):

```rust
struct Beep { remain: usize }
impl Iterator for Beep {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.remain == 0 { return None; }
        self.remain -= 1;
        let t = (self.remain as f32) / 44_100.0;
        Some((t * 880.0 * std::f32::consts::TAU).sin() * 0.25 * fade)
    }
}
impl rodio::Source for Beep {
    fn current_frame_len(&self) -> Option<usize> { Some(self.remain) }
    fn channels(&self) -> u16 { 1 }
    fn sample_rate(&self) -> u32 { 44_100 }
    fn total_duration(&self) -> Option<Duration> { Some(Duration::from_millis(300)) }
}
```
`MixerDeviceSink` 经 `DeviceSink` 的 `.mixer()` 拿到,`mixer.add(src)` 直播即可;API 在 0.21 引入、0.22 延续,稳定。

### 键盘活动检测

- **保留自写 WH_KEYBOARD_LL**:rdev 3 年未发版且 Windows 后端等价;自写 hook 回调里记得 `CallNextHookEx`、只记录时间戳立刻返回(符合微软"低级钩子回调须快"的要求)。
- **GetLastInputInfo 仍是首选空闲检测**:无需 hook 权限、不受 UIPI 限制、一个调用拿到全局最后输入时间;hook 只在需要"按键级粒度"(如统计键次)时才保留,否则可只用 GetLastInputInfo。

### SHQueryUserNotificationState(windows 0.62)

- feature:**`Win32_UI_Shell`(项目已启用)**,无需新增。
- 调用:

```rust
use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUERY_USER_NOTIFICATION_STATE};
let st = unsafe { SHQueryUserNotificationState()? };
let busy = matches!(st, QUERY_USER_NOTIFICATION_STATE::NQUS_BUSY
    | QUERY_USER_NOTIFICATION_STATE::NQUS_RUNNING_D3D_FULL_SCREEN
    | QUERY_USER_NOTIFICATION_STATE::NQUS_PRESENTATION_MODE);
```
全屏游戏(D3D)/演示模式时跳过遮罩、改入队。

### 单实例 + 主题 + 图标

- 单实例:`windows` 现有 feature(`Win32_System_Threading` + `Win32_Foundation`)里 `CreateMutexW(Some(&HSTRING), false, None)`;`GetLastError() == ERROR_ALREADY_EXISTS` 则直接退出。不要引停更的 single-instance crate。
- 主题:egui `Options` 的 `ThemePreference::System` 已跟随系统;dark-light 3.0(`detect()/subscribe()`,Windows 用注册表)仅在 egui 之外需要主题信息时引入。
- exe 图标:`winresource`(build-dependencies)——`WindowsResource::new().set_icon("assets/icon.ico").compile()?`;需要 MSVC 的 rc.exe 或 llvm-rc。embed-resource 3.0.11 同样维护活跃(直接编译 .rc 文件,适合要写自定义 rc 的场景);二者皆主流,**简单图标/版本信息选 winresource**。

---

## ③ 全屏倒计时遮罩窗口实现模式建议

**推荐:eframe 多 viewport(单进程单渲染器)**

- 主 viewport = 配置窗口;遮罩作为独立 viewport 按需创建/销毁:

```rust
ctx.show_viewport_deferred(
    egui::ViewportId::from_hash_of("countdown_overlay"),
    egui::ViewportBuilder::new()
        .with_fullscreen(true)        // winit borderless 全屏(0.36 存在,非独占)
        .with_decorations(false)
        .with_always_on_top()         // 置顶
        .with_taskbar(false)          // 不占任务栏
        .with_active(false),          // 不抢焦点(希望"点击遮罩跳过"则设 true)
    Arc::new(Mutex::new(state)),      // deferred 需要可共享状态
    |ctx, state| {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_black_alpha(200)))
            .show_inside(ctx, |ui| { ui.heading(倒计时文本); });
        ctx.request_repaint_after(Duration::from_millis(200)); // 空闲 0 CPU/GPU
    },
);
```
- 多显示器:0.35+ 的 `ViewportBuilder::with_monitor(index)` / `ViewportCommand::SetMonitor`;`ViewportCommand::Focus`、`ViewportCommand::Fullscreen(bool)` 也已确认存在。
- 倒计时结束:发 `ViewportCommand::Close`(或不再 show 该 viewport)释放资源。
- 运行时机受 `SHQueryUserNotificationState` 门控(NQUS_BUSY/D3D 全屏/演示时改入队),这就是"上下文感知"的核心。
- **点击穿透遮罩**(用户可操作下层窗口)egui/winit 无跨平台 API,Windows 需 `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE`;若确需穿透,用 `SetWindowLongPtrW(GWL_EXSTYLE, ...)` 对 viewport 的 HWND 后处理,或放弃穿透。
- 备选方案(仅在 eframe viewport 无法满足时):遮罩窗口脱离 eframe,用 windows crate 直建 WS_POPUP layered 窗口自绘(Direct2D/GDI)。代价是两套渲染与两条消息路径,维护成本高,不推荐首选。
- 与托盘/热键的线程模型:一切以 **eframe 的 winit 主线程循环**为唯一泵——tray、global-hotkey 都在该线程创建,`try_recv` 全部挪进 `App::ui`;自写 PeekMessageW 泵退役。

---

## ④ 体积 / 内存优化要点(现 opt-level=z + LTO 之外)

1. **eframe 关默认 feature 选 glow**:0.34 起 default 含 wgpu 29 全家桶,是最大的二进制膨胀源;`default-features = false, features = ["glow", "default_fonts"]`。
2. **rodio `default-features = false`**:默认捆绑 Symphonia(flac/mp3/mp4/vorbis/wav)解码器;合成音源只需 `playback`。
3. eframe 的 `persistence`(ron)、`accesskit` 平台桥按需关闭(注意 egui 0.34+ 核心 always-on accesskit,关的是平台集成)。
4. windows crate 维持精确 feature 列表(已做到)。
5. 运行时常驻内存:遮罩 viewport 用完即销毁;`request_repaint_after` 代替连续重绘(空闲接近 0% CPU);托盘图标单实例复用。
6. `default_fonts` 若确定自带 CJK 字体子集可关闭(省约 2MB,代价是丢 emoji/默认 fallback,需测试中文回退)。
7. 不建议 UPX 等壳(杀软误报率高)。

---

## ⑤ 来源列表

- eframe/egui 版本:https://crates.io/crates/eframe (0.36.1, 2026-08-07)
- egui CHANGELOG(0.32~0.36 破坏性变更):https://github.com/emilk/egui/blob/master/CHANGELOG.md
- eframe CHANGELOG(wgpu 默认化):https://github.com/emilk/egui/blob/master/crates/eframe/CHANGELOG.md
- eframe 0.36.1 features:https://github.com/emilk/egui/blob/0.36.1/crates/eframe/Cargo.toml
- egui FontDefinitions(Arc<FontData>):https://docs.rs/egui/latest/egui/struct.FontDefinitions.html
- egui 0.36 ViewportBuilder/ViewportCommand(源码确认):https://github.com/emilk/egui/blob/0.36.1/crates/egui/src/viewport.rs
- tray-icon:https://docs.rs/tray-icon/latest/tray_icon/ 、https://docs.rs/tray-icon/latest/tray_icon/enum.TrayIconEvent.html 、https://crates.io/crates/tray-icon
- global-hotkey:https://docs.rs/global-hotkey/latest/global_hotkey/ 、https://crates.io/crates/global-hotkey
- rodio:https://docs.rs/rodio/latest/rodio/ 、https://docs.rs/rodio/0.22.2/rodio/ 、https://crates.io/crates/rodio
- windows crate:https://crates.io/crates/windows (0.62.2)
- SHQueryUserNotificationState:https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/UI/Shell/fn.SHQueryUserNotificationState.html
- NIF_*/NIIF_* 常量(Win32_UI_Shell):https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/UI/Shell/index.html
- tauri-winrt-notification:https://crates.io/crates/tauri-winrt-notification (0.8.1)
- notify-rust:https://crates.io/crates/notify-rust (4.18.0)
- rdev(2023 停更):https://crates.io/crates/rdev
- dark-light:https://docs.rs/dark-light/latest/dark_light/ 、https://crates.io/crates/dark-light
- winresource:https://crates.io/crates/winresource (0.1.31);embed-resource:https://crates.io/crates/embed-resource (3.0.11)
- single-instance(2021 停更):https://crates.io/crates/single-instance
- toml:https://crates.io/crates/toml (1.1.5);rand:https://crates.io/crates/rand (0.10.2)
