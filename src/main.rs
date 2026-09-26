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

/// 轻量循环的轮询周期（托盘点击的最大响应延迟）。
///
/// v0.6 由 200 ms 改为 1 s（ADR-0007 判决四）：收益上限就是全部空闲 CPU
/// （实测 0.482% → 0.177%），代价是托盘菜单最坏响应延迟 200 ms → 1 s。
/// 彻底解法是把下面的 `sleep` 换成 `MsgWaitForMultipleObjectsEx`
/// （托盘 HWND 与 `WM_HOTKEY` 都走主线程消息队列，技术上可行），推到 v0.7 单独做。
const LIGHT_LOOP_TICK: Duration = Duration::from_secs(1);
/// UI 会话连续失败后的退避时间
const UI_FAILURE_BACKOFF: Duration = Duration::from_secs(60);

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
        std::thread::sleep(LIGHT_LOOP_TICK);
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
