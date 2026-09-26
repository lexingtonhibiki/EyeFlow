#![windows_subsystem = "windows"]

mod app;
mod audio;
mod autostart;
mod config;
mod core;
mod detector;
mod event;
// 与 build.rs / examples 共享；ICO 编码部分在主程序里不会用到
mod i18n;
#[allow(dead_code)]
mod icon;
mod runtime;
mod stats;
mod tips;
mod tr;
mod tray;
mod ui;
mod update;
mod wallpaper;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use global_hotkey::GlobalHotKeyManager;

use crate::config::Config;
use crate::runtime::Runtime;
use crate::stats::Stats;
use crate::tr::Lang;

/// 轻量循环的**兜底超时**（消息队列空时等这么久再醒一次）。
///
/// v0.6 把它当「轮询周期」用，值由 200 ms 抬到 1 s（ADR-0007 判决四），
/// 代价是托盘点击最坏要等满 1 s 才被 `rt.step` 看见
/// （本机实测 p50 593 ms / p90 872 ms / 最坏 937 ms，n=60）。
/// 当时记的收益是「空闲 CPU 0.482% → 0.177%」——**⚠️ 这两个数今天在本机复现不出来**
/// （`sleep` 对照组实测 0.534%），来源条件已无从考证，别再拿它们当依据。
///
/// 现在它不再是轮询周期：轻量循环改用 `MsgWaitForMultipleObjectsEx` 阻塞在**消息队列**上
/// （托盘 HWND、`WM_HOTKEY`、`WM_COMMAND` 全在这条线程的队列里），有输入就立刻醒。
/// 这个 1 s 只在队列一直是空的时候兜底轮转一次，**1 s 的值不动**——它决定的是
/// 「无事发生时多久醒一次做一次维护」，不是「有事发生时多久响应」。
///
/// **⚠️ 这次替换不是零 CPU 代价**（同会话 4 轮交替 A/B，每轮空闲 3 min）：
/// `MsgWaitForMultipleObjectsEx` 均值 **0.777%**，`sleep(1s)` 均值 **0.534%**，
/// **差 +0.243 个百分点，四轮里每一轮都是 `MsgWait` 更高**。折合约合每天多耗 3.5 分钟
/// CPU，换来点击响应 p50 593 ms → 十几毫秒——这笔交易划算，但它不是免费的。
/// 早醒来源未查明（`tray-icon` 那个 15 ms `SetTimer` 只在鼠标悬停托盘时才设，不是它）。
const LIGHT_LOOP_TICK: Duration = Duration::from_secs(1);
/// UI 会话连续失败后的退避时间
const UI_FAILURE_BACKOFF: Duration = Duration::from_secs(60);

/// 等消息队列上有输入，或等到兜底超时。返回后一律回到轻量循环顶部重新泵消息。
///
/// 为什么是 `MsgWaitForMultipleObjectsEx` 而不是 `GetMessageW`：前者能**只等输入**、
/// 在超时后正常返回，让 `rt.step` 照常按兜底周期跑；`GetMessageW` 超时不了，
/// 只能另开一个线程去 `PostThreadMessage` 自己叫醒自己。
///
/// `WAIT_FAILED` 的处理是这里唯一容易写错的地方：它表示**等待本身失败**，
/// 此时消息队列里到底有没有东西是未知的。它既不能当成「有消息」盲进泵，
/// 也不能原地重试（会立刻返回，退化成 100% CPU 空转），所以退回等价的 `sleep`。
fn wait_for_input_or_fallback(timeout: Duration) {
    use windows::Win32::Foundation::{WAIT_EVENT, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::UI::WindowsAndMessaging::{
        MsgWaitForMultipleObjectsEx, MSG_WAIT_FOR_MULTIPLE_OBJECTS_EX_FLAGS, QS_ALLINPUT,
        QUEUE_STATUS_FLAGS,
    };

    /// 唤醒掩码 = `QS_ALLINPUT`（`0x04FF` = 投递消息 / 发送消息 / WM_PAINT / WM_TIMER /
    /// WM_HOTKEY / 键鼠输入全都要），语义就是「队列里有任何一条 `PeekMessage` 能取到的消息」。
    ///
    /// ⚠️ **不是任务书写的 `MWAIT_INPUT_AVAILABLE | MWAIT_ALL_COMPLETED`**。本机实测（n=20，
    /// 同一条托盘菜单点击、同一台机器、只换掩码）：`0x0400|0x0001` 的 p50 仍是 609 ms、
    /// p90 891 ms、最坏 958 ms——**和 `sleep(1s)` 一模一样，根本没被唤醒**；换成
    /// `0x04FF` 后 p50 掉到 4.6 ms。原因见下。
    ///
    /// windows 0.62 **不导出 `MWAIT_*` 这组常量**（`windows-0.62.2/src` 全树 grep 不到
    /// `MWAIT`），只生成了 `QS_*` 那一半命名，而 `dwwakemask` 的类型是
    /// `QUEUE_STATUS_FLAGS`（两者是同一个 typedef）。`MWAIT_INPUT_AVAILABLE` 落在 `0x0400`
    /// 这一位上，而该位在 `GetQueueStatus` 里**只在真有键鼠输入时**才置位；
    /// 别的线程 `PostMessage` 来的托盘 / 热键 / `WM_COMMAND` 只置 `QS_POSTMESSAGE (0x0008)`，
    /// 与 `0x0400` 与运算为 0，于是永远等不到。`MWAIT_ALL_COMPLETED (0x0001)` 讲的是
    /// 「句柄列表全部已置位」，我们一个句柄都没传，它不参与唤醒。
    const WAKE_MASK: QUEUE_STATUS_FLAGS = QS_ALLINPUT;
    /// `dwFlags = 0`：不要 `MWAIT_WAIT_ALL`（要立刻返回），不要 `MWAIT_SOLELY_WAIT_FOR_INPUTS`
    /// （那会把内核对象的信号也放进来，而我们一个句柄都没传）。
    const FLAGS: MSG_WAIT_FOR_MULTIPLE_OBJECTS_EX_FLAGS = MSG_WAIT_FOR_MULTIPLE_OBJECTS_EX_FLAGS(0);

    // `None` = 0 个内核对象句柄：我们等的是消息，不是内核对象。
    let r = unsafe {
        MsgWaitForMultipleObjectsEx(
            None,
            timeout.as_millis().min(u128::from(u32::MAX)) as u32,
            WAKE_MASK,
            FLAGS,
        )
    };
    match r {
        // 有输入被投递：回循环顶部泵消息、跑 `rt.step`。
        WAIT_OBJECT_0 => {}
        // 兜底超时：队列一直是空的，回循环顶部做一次常规维护。
        WAIT_TIMEOUT => {}
        // 等待失败：队列状态未知，**不**当成「有消息」；退回 sleep 避免空转。
        other @ (WAIT_FAILED | WAIT_EVENT(_)) => {
            log::warn!(
                "MsgWaitForMultipleObjectsEx 失败（返回值 {}），退回 sleep 兜底",
                other.0
            );
            std::thread::sleep(timeout);
        }
    }
}

fn main() {
    init_logging();
    log::info!("EyeFlow {} 启动", env!("CARGO_PKG_VERSION"));

    if !acquire_single_instance() {
        log::info!("已有 EyeFlow 实例在运行，本次退出");
        return;
    }

    // 首次运行（还没有配置文件）：启动后自动打开设置窗做一次引导
    let first_run = !Config::path().exists();
    let mut cfg = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            log::error!("配置加载失败（{e}），使用默认配置");
            Config::default()
        }
    };
    // 首次运行（还没有配置文件）：用 `GetUserDefaultLocaleName()` 决定初始语言。
    // 之后一律以 config.toml 为准，不再分叉。首跑设置窗已经无条件自动打开，
    // 里面第一行就是语言选择器，所以这个「读系统 locale 当初值」的动作
    // **没有新增验收面**（ADR-0008 §默认语言与配置兼容）。
    if first_run {
        let system = system_locale();
        if Lang::from_config(&system) == Lang::EnUs {
            log::info!("首次运行：系统 locale 为 {system}，初始语言取英文");
            cfg.language = "en-US".into();
        } else {
            log::info!("首次运行：系统 locale 为 {system}，初始语言取中文");
        }
        if let Err(e) = cfg.save() {
            log::error!("首次运行写入配置失败: {e}");
        }
    }
    // 进程级语言状态：托盘在 `Tray::new` 之前就要用 `tr()` 建菜单文案
    tr::set_language(Lang::from_config(&cfg.language));

    let stats = Stats::load();

    let (tx, rx) = mpsc::channel();

    // 启动顺序按「用户多久能看见东西」排，不是按代码依赖排。
    // 托盘图标出现是 spec N3 承诺的「双击后 < 1 秒」，而 `AudioPlayer::new()`
    // 里的 WASAPI 打开默认设备是同步阻塞主线程的（实测 5~50 ms，设备差的机器更久）。
    // 键盘钩子与传感器线程也要先起（否则头几百毫秒的输入不计入心流判定），
    // 但它们只是建线程，很快。
    // 托盘与热键必须在运行 Win32 消息循环的线程上创建：就是主线程；
    // 轻量循环用 PeekMessage 泵，UI 会话期间由 winit 泵，二者都在这条线程上。
    let tray = match tray::Tray::new(cfg.enabled, cfg.sound_enabled) {
        Ok(t) => t,
        Err(e) => {
            log::error!("托盘图标创建失败: {e}");
            return;
        }
    };
    let hotkeys = create_hotkey_manager();

    detector::start_keyboard_hook(tx.clone());
    detector::start_sensor_thread(tx.clone());

    // 最慢的一项，放最后
    let audio = audio::AudioPlayer::new();

    let mut core = core::Core::new(cfg, stats, Instant::now());
    // EYEFLOW_DEMO=1：60 秒后触发一次提醒并打开设置窗，便于演示 / 截图 / 验收
    let demo = std::env::var_os("EYEFLOW_DEMO").is_some();
    if demo {
        core.cfg.heads_up_secs = 45; // 仅内存中生效：把预告窗口拉长便于截图验收
        core.cfg.away_secs = 7200; // 演示时忽略离开判定，保证一定会投递
        core.set_due_in(Instant::now(), Duration::from_secs(60));
        log::info!("演示模式：60 秒后触发提醒，预告 45 秒");
    }

    // 是否注册全局热键由 cfg.hotkey_enabled 决定，见 Runtime::new
    let mut rt = Runtime::new(core, rx, tx, audio, tray, hotkeys);
    if demo || first_run {
        if first_run {
            log::info!("首次运行：打开设置窗引导");
        }
        rt.open_settings();
    }

    // 轻量循环：没有窗口、没有 GL 上下文。只在需要显示窗口时进入 eframe 会话，
    // 窗口全部关闭后回到这里（docs/adr/0006）。
    let mut ui_backoff_until: Option<Instant> = None;
    loop {
        pump_win32_messages();
        let now = Instant::now();
        rt.step(now);
        if rt.quit {
            break;
        }
        let backoff = ui_backoff_until.is_some_and(|t| now < t);
        if rt.needs_ui(now) && !backoff {
            match run_ui_session(&mut rt) {
                Ok(()) => ui_backoff_until = None,
                Err(e) => {
                    log::error!(
                        "UI 会话失败: {e}；{}s 内退化为仅声音",
                        UI_FAILURE_BACKOFF.as_secs()
                    );
                    rt.settings = None;
                    ui_backoff_until = Some(Instant::now() + UI_FAILURE_BACKOFF);
                }
            }
            if rt.quit {
                break;
            }
            continue;
        }
        wait_for_input_or_fallback(LIGHT_LOOP_TICK);
    }

    detector::stop_keyboard_hook();
    log::info!("EyeFlow 退出");
}

/// 进入一次 eframe 会话，直到所有窗口关闭。`run_and_return`（默认开启）让
/// eframe 复用线程局部的 winit 事件循环，因此可以反复调用。
fn run_ui_session(rt: &mut Runtime) -> eframe::Result {
    log::debug!("UI 会话开始");
    // 根视口是屏幕外的 1×1 锚点窗口（见 app.rs 模块说明）。
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("EyeFlow")
            .with_inner_size([1.0, 1.0])
            .with_position([-30000.0, -30000.0])
            .with_decorations(false)
            .with_taskbar(false)
            .with_active(false)
            .with_resizable(false),
        centered: false,
        persist_window: false,
        run_and_return: true,
        ..Default::default()
    };
    let result = eframe::run_native(
        "EyeFlow",
        options,
        Box::new(|cc| Ok(Box::new(app::UiSession::new(cc, rt)))),
    );
    crate::event::set_ui_context(None);
    log::debug!("UI 会话结束");
    result
}

/// 泵送主线程消息队列：托盘图标与全局热键的隐藏窗口都在这条线程上。
fn pump_win32_messages() {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };
    let mut msg = MSG::default();
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// 创建全局热键管理器；具体注册与否由 `Runtime::apply_hotkey_enabled` 按配置决定。
fn create_hotkey_manager() -> Option<GlobalHotKeyManager> {
    match GlobalHotKeyManager::new() {
        Ok(m) => Some(m),
        Err(e) => {
            log::warn!("全局热键管理器创建失败: {e}");
            None
        }
    }
}

/// 系统的默认 UI 语言（`GetUserDefaultLocaleName`），例如 `en-US` / `zh-CN`。
/// 拿不到就返回空串，调用方按「非 `en` 前缀即中文」处理。
fn system_locale() -> String {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    // Win32 的 LOCALE_NAME_MAX_LENGTH = 85；windows 0.62 的 Globalization 里没有
    // 导出这个常量（它是 `GetUserDefaultLocaleName` 的调用方约定，不是 API 常量），
    // 所以这里按 Win32 头文件写死并在下面按返回值截断。
    let mut buf = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if len <= 1 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize - 1])
}

/// 命名互斥量保证单实例；句柄有意不关闭，随进程存活。
fn acquire_single_instance() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    unsafe {
        match CreateMutexW(None, false, w!("Local\\EyeFlow.SingleInstance")) {
            Ok(_handle) => GetLastError() != ERROR_ALREADY_EXISTS,
            Err(e) => {
                log::warn!("创建单实例互斥量失败: {e}");
                true
            }
        }
    }
}

/// 无控制台的托盘程序把日志写到 %APPDATA%\eyeflow\eyeflow.log（每次启动覆盖）。
fn init_logging() {
    let dir = config::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let mut builder = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,eframe=warn,egui_glow=warn"),
    );
    if let Ok(file) = std::fs::File::create(dir.join("eyeflow.log")) {
        builder.target(env_logger::Target::Pipe(Box::new(file)));
    }
    let _ = builder.try_init();

    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
        show_panic_message(info);
    }));
}

/// 崩溃时弹一个消息框。
///
/// `panic = "abort"` 只取消 unwind，panic hook 仍由 panic runtime 在 abort
/// 之前调用，所以这里不是死代码。真正的问题是本程序是 `#![windows_subsystem]`
/// 的无控制台托盘应用：只写日志的话崩溃时用户在屏幕上什么都看不到，而日志
/// 每次启动就被覆盖一次。护眼工具崩溃在用户眼里等于「这软件有问题」。
///
/// Win32 调用本身是 unsafe 且可能重入的（弹窗期间再 panic 会递归调 hook），
/// 因此用静态标志挡住重入。
fn show_panic_message(info: &std::panic::PanicHookInfo<'_>) {
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK, MB_TOPMOST};

    static IN_HOOK: AtomicBool = AtomicBool::new(false);
    if IN_HOOK.swap(true, Ordering::SeqCst) {
        return;
    }

    let text = format!(
        "{}\n\n{info}\n\n{}\n{}",
        tr::tr("app.panic_body"),
        tr::tr("app.panic_advice"),
        tr::tr("app.panic_log"),
    );
    let text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = tr::tr("app.panic_title")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR | MB_TOPMOST,
        );
    }
}
