#![windows_subsystem = "windows"]

mod app;
mod audio;
mod autostart;
mod config;
mod core;
mod detector;
mod event;
// 与 build.rs / examples 共享；ICO 编码部分在主程序里不会用到
#[allow(dead_code)]
mod icon;
mod runtime;
mod stats;
mod tips;
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

/// 轻量循环的轮询周期（托盘点击的最大响应延迟）
const LIGHT_LOOP_TICK: Duration = Duration::from_millis(200);
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
    let cfg = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            log::error!("配置加载失败（{e}），使用默认配置");
            Config::default()
        }
    };
    let stats = Stats::load();

    let (tx, rx) = mpsc::channel();
    detector::start_keyboard_hook(tx.clone());
    detector::start_sensor_thread(tx.clone());

    let audio = audio::AudioPlayer::new();

    // 托盘与热键都必须在运行 Win32 消息循环的线程上创建：就是主线程。
    // 轻量循环用 PeekMessage 泵，UI 会话期间由 winit 泵，二者都在这条线程上。
    let tray = match tray::Tray::new(cfg.enabled, cfg.sound_enabled) {
        Ok(t) => t,
        Err(e) => {
            log::error!("托盘图标创建失败: {e}");
            return;
        }
    };
    let hotkeys = create_hotkey_manager();

    let mut core = core::Core::new(cfg, stats, Instant::now());
    // EYEFLOW_DEMO=1：60 秒后触发一次提醒并打开设置窗，便于演示 / 截图 / 验收
    let demo = std::env::var_os("EYEFLOW_DEMO").is_some();
    if demo {
        core.cfg.heads_up_secs = 45; // 仅内存中生效：把预告窗口拉长便于截图验收
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
        if rt.needs_ui() && !backoff {
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
    }));
}
