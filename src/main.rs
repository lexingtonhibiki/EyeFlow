#![windows_subsystem = "windows"]

mod config;
mod event;
mod state;
mod tray;
mod reminder;
mod detector;
mod audio;
mod ui;

use config::Config;
use event::Event;
use state::{ContextState, StateMachine};
use tray::{Tray, TrayAction};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn main() {
    env_logger::init();
    log::info!("EyeFlow 启动");

    // 1. 加载配置
    let mut current_config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            log::error!("配置加载失败: {}", e);
            Config::default()
        }
    };

    // 2. 创建事件通道
    let (tx, rx) = mpsc::channel::<Event>();

    // 3. 初始化模块
    let mut state_machine = StateMachine::new(current_config.flow_threshold());
    let mut reminder = reminder::Scheduler::new(current_config.min_interval_secs, current_config.max_interval_secs);
    let audio = audio::AudioPlayer::new();
    let tray = Tray::new(tx.clone(), current_config.enabled, !current_config.sound_enabled);
    let mut flow_detector = detector::FlowDetector::new(current_config.flow_sensitivity);

    // 4. 启动键盘钩子线程（WH_KEYBOARD_LL，需消息泵支撑）
    let kh_tx = tx.clone();
    std::thread::Builder::new()
        .name("keyboard-hook".into())
.spawn(move || {
            detector::start_keyboard_hook(kh_tx);
        })
        .expect("无法启动键盘钩子线程");

    // 5. 启动统一检测线程（合并空闲检测 + 全屏检测 + 定时器节拍）
    let tick_tx = tx.clone();
    std::thread::Builder::new()
        .name("detector-ticker".into())
        .spawn(move || {
            loop {
                // 全屏检测（1s 间隔）
                let is_fs = detector::is_fullscreen();
                let _ = tick_tx.send(Event::FullscreenChanged(is_fs));

                // 空闲检测（15s 采样，超过 3 分钟才发事件）
                let idle_min = detector::idle_minutes();
                if idle_min >= 3 {
                    let _ = tick_tx.send(Event::IdleTimeout);
                }

                // 定时器节拍
                let _ = tick_tx.send(Event::TimerTick);

                std::thread::sleep(Duration::from_secs(1));
            }
        })
        .expect("无法启动检测线程");

    // 6. 状态变量
    let mut config_window_open = false;

    // 7. 主事件循环
    log::info!("EyeFlow 事件循环开始");
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                match event {
                    // ---- 检测器事件 ----
                    Event::KeyboardActivity => {
                        state_machine.on_keyboard_activity();
                        flow_detector.on_key_down();
                    }
                    Event::FullscreenChanged(is_fs) => {
                        state_machine.on_fullscreen_change(is_fs);
                    }
                    Event::IdleTimeout => {
                        state_machine.on_idle_timeout();
                        reminder.reset();
                        log::debug!("检测到空闲超时，进入 Away 状态");
                    }

                    // ---- 托盘事件 ----
                    Event::TrayAction(action) => {
                        match action {
                            TrayAction::OpenSettings => {
                                config_window_open = true;
                            }
                            TrayAction::ToggleEnabled => {
                                current_config.enabled = !current_config.enabled;
                                let _ = current_config.save();
                                tray.set_enabled(current_config.enabled);
                                log::info!("提醒开关: {}", current_config.enabled);
                            }
                            TrayAction::ToggleMute => {
                                current_config.sound_enabled = !current_config.sound_enabled;
                                current_config.notification_enabled = !current_config.notification_enabled;
                                let _ = current_config.save();
                                tray.set_muted(!current_config.sound_enabled);
                                log::info!("静音开关: {}", !current_config.sound_enabled);
                            }
                            TrayAction::Quit => {
                                log::info!("收到退出指令");
                                break;
                            }
                        }
                    }

                    // ---- 全局快捷键 ----
                    Event::GlobalHotkey => {
                        current_config.sound_enabled = !current_config.sound_enabled;
                        current_config.notification_enabled = !current_config.notification_enabled;
                        let _ = current_config.save();
                        log::info!("全局快捷键 - 静音切换: {}", !current_config.sound_enabled);
                    }

                    // ---- 提醒器 ----
                    Event::TimerTick => {
                        let now = Instant::now();
                        let ctx_state = state_machine.tick(now);

                        match ctx_state {
                            ContextState::Desktop => {
                                if reminder.should_remind() {
                                    let _ = tx.send(Event::ReminderTriggered);
                                }
                            }
                            ContextState::Flow => {
                                if let Some(idle) = flow_detector.idle_time() {
                                    if idle >= Duration::from_secs(8) {
                                        if reminder.should_remind() {
                                            let _ = tx.send(Event::ReminderTriggered);
                                        }
                                    }
                                }
                            }
                            ContextState::Gaming | ContextState::Away => {}
                        }
                    }
                    Event::ReminderTriggered => {
                        trigger_reminder(&current_config, &audio, &state_machine, &tray);
                        reminder.reset();
                    }

                    // ---- 设置窗口 ----
                    Event::OpenSettings => {
                        config_window_open = true;
                    }
                    Event::SettingsChanged(new_config) => {
                        state_machine.set_flow_threshold(new_config.flow_threshold());
                        reminder.set_interval(new_config.min_interval_secs, new_config.max_interval_secs);
                        flow_detector.set_sensitivity(new_config.flow_sensitivity);
                        let _ = new_config.save();
                        current_config = new_config;
                        log::info!("配置已更新");
                    }

                                    // ---- 生命周期 ----
                    Event::Quit => {
                        log::info!("收到退出事件");
                        break;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // 超时也是正常状态，继续泵送消息
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                log::info!("事件通道已关闭");
                break;
            }
        }

        // 泵送 Windows 消息（托盘图标隐藏窗口需此处理鼠标事件）
        pump_windows_messages();

        // 按需运行 egui 设置窗口
        if config_window_open {
            run_config_window(&mut current_config, &tx);
            config_window_open = false;
        }
    }

    // 8. 清理
    detector::stop_keyboard_hook();
    log::info!("EyeFlow 退出");
}

/// 泵送 Windows 消息队列（托盘图标依赖此处理鼠标事件）
fn pump_windows_messages() {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, MSG, PM_REMOVE,
    };
    let mut msg = MSG::default();
    // SAFETY: PeekMessageW/DispatchMessageW 是无副作用的 Win32 消息泵
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            DispatchMessageW(&msg);
        }
    }
}

/// 触发护眼提醒
fn trigger_reminder(config: &Config, audio: &audio::AudioPlayer, state: &StateMachine, tray: &Tray) {
    if !config.enabled || config.is_in_dnd() {
        return;
    }

    match state.current() {
        ContextState::Desktop => {
            if config.sound_enabled {
                audio.play_preset(&config.sound_preset);
            }
            if config.notification_enabled {
                let msg = format!("已经工作了，看看远处 {} 秒吧 👀", config.eye_rest_secs);
                let _ = tray.show_notification("EyeFlow", &msg);
            }
        }
        ContextState::Gaming => {
            if config.sound_enabled {
                audio.play_preset(&config.sound_preset);
            }
        }
        ContextState::Flow | ContextState::Away => {}
    }
}

/// 运行 egui 配置窗口
fn run_config_window(config: &mut Config, tx: &mpsc::Sender<Event>) {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([540.0, 580.0])
            .with_resizable(false),
        ..Default::default()
    };

    let config_clone = config.clone();
    let tx_clone = tx.clone();

    if let Err(e) = eframe::run_native(
        "EyeFlow 设置",
        options,
        Box::new(|cc| {
            // 加载中文字体（Microsoft YaHei）以正确显示中文
            let font_path = "C:\\Windows\\Fonts\\msyh.ttc";
            if let Ok(font_data) = std::fs::read(font_path) {
                use std::sync::Arc;
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert("msyh".to_string(), Arc::new(egui::FontData::from_owned(font_data)));
                fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap()
                    .insert(0, "msyh".to_string());
                cc.egui_ctx.set_fonts(fonts);
            } else {
                log::warn!("未找到中文字体文件: {}", font_path);
            }
            Ok(Box::new(ConfigWindowApp {
                config_window: ui::ConfigWindow::new(config_clone, tx_clone),
            }))
        }),
    ) {
        log::error!("设置窗口运行失败: {:?}", e);
    }
}

struct ConfigWindowApp {
    config_window: ui::ConfigWindow,
}

impl eframe::App for ConfigWindowApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.config_window.show(ctx);
    }
}
