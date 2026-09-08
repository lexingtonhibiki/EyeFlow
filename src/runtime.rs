//! 运行时：托盘、热键、传感器事件、调度核心与配置持久化的统一“步进”入口。
//!
//! 同一份逻辑在两种模式下运行：
//! - 轻量循环（main.rs）：没有任何窗口、没有 GL 上下文，常驻内存最小；
//! - UI 会话（app.rs）：eframe 运行期间由 `App::logic` 每帧调用。
//!
//! 这样无论窗口开不开，提醒计时、托盘与热键都不会停摆（v0.1 的设置窗会冻结一切）。

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::audio::AudioPlayer;
use crate::autostart;
use crate::config::Config;
use crate::core::{Action, ContextState, Core, Phase};
use crate::event::Event;
use crate::tray::{Tray, TrayCommand};
use crate::ui::{self, SettingsAction, SettingsState};

const PAUSE_DURATION: Duration = Duration::from_secs(3600);
/// Ctrl+Shift+E = 立即休息（组合键由操作系统全局独占，可在设置中关闭）
fn global_hotkey() -> HotKey {
    HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyE)
}
/// 声音是唯一提醒通道（游戏 / 全屏 / 不可打扰）时，提示音至少持续 2 秒
const SOUND_ONLY_MIN_CUE_SECS: u64 = 2;

pub struct Runtime {
    pub core: Core,
    pub audio: AudioPlayer,
    pub tray: Tray,
    pub settings: Option<SettingsState>,
    /// 设置窗已打开时再次请求 → 下一帧把它拉到前台
    pub settings_focus: bool,
    pub quit: bool,
    rx: Receiver<Event>,
    hotkeys: Option<GlobalHotKeyManager>,
    registered_hotkey: Option<HotKey>,
}

impl Runtime {
    pub fn new(
        core: Core,
        rx: Receiver<Event>,
        audio: AudioPlayer,
        tray: Tray,
        hotkeys: Option<GlobalHotKeyManager>,
    ) -> Self {
        let mut rt = Self {
            core,
            audio,
            tray,
            settings: None,
            settings_focus: false,
            quit: false,
            rx,
            hotkeys,
            registered_hotkey: None,
        };
        if let Err(e) = rt.apply_hotkey_enabled(rt.core.cfg.hotkey_enabled) {
            log::warn!("全局热键注册失败: {e}");
        }
        rt
    }

    /// 注册 / 注销全局热键。组合键是操作系统级全局独占资源，
    /// 用户可能与其他软件冲突，因此必须允许关闭（每次提醒限一次的“延后”不受影响）。
    pub fn apply_hotkey_enabled(&mut self, on: bool) -> Result<(), String> {
        let hotkey = global_hotkey();
        if on {
            let Some(mgr) = &self.hotkeys else {
                return Err("热键管理器不可用".to_string());
            };
            if self.registered_hotkey.is_some() {
                return Ok(());
            }
            mgr.register(hotkey).map_err(|e| e.to_string())?;
            self.registered_hotkey = Some(hotkey);
            log::info!("全局热键 Ctrl+Shift+E 已注册");
        } else if self.registered_hotkey.take().is_some() {
            if let Some(mgr) = &self.hotkeys {
                let _ = mgr.unregister(hotkey);
            }
            log::info!("全局热键 Ctrl+Shift+E 已注销");
        }
        self.tray.set_hotkey_hint(on);
        Ok(())
    }

    /// 全局热键当前是否已注册（设置窗显示用）
    pub fn hotkey_active(&self) -> bool {
        self.registered_hotkey.is_some()
    }

    /// 是否需要一个 UI 会话（有窗口要显示）。
    pub fn needs_ui(&self) -> bool {
        self.settings.is_some()
            || (self.core.cfg.visual_enabled && !matches!(self.core.phase, Phase::Idle))
    }

    /// 处理所有待处理输入并推进调度核心一步。两种模式都调用它。
    pub fn step(&mut self, now: Instant) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::KeyPress(t) => self.core.on_key(t),
                Event::Sensors(s) => self.core.set_sensors(s),
            }
        }
        for cmd in self.tray.poll() {
            self.handle_tray(cmd, now);
        }
        while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
            let active = self
                .registered_hotkey
                .as_ref()
                .is_some_and(|hk| ev.id == hk.id());
            if ev.state == HotKeyState::Pressed && active {
                log::info!("热键：立即休息");
                self.core.start_break_now(Instant::now());
            }
        }

        let quiet = self.core.cfg.in_quiet_hours(chrono::Local::now().time());
        for action in self.core.tick(now, quiet) {
            match action {
                Action::PlayCue => {
                    if self.core.cfg.sound_enabled {
                        let mut secs = self.core.cfg.cue_duration_secs;
                        // 游戏 / 全屏 / 不可打扰时预告面板不可见，声音是唯一通道 → 自动增强
                        if self.core.state == ContextState::Gaming {
                            secs = secs.max(SOUND_ONLY_MIN_CUE_SECS);
                        }
                        self.audio.play_cue(
                            self.core.cfg.sound_preset,
                            self.core.cfg.cue_volume_pct,
                            secs,
                        );
                    }
                }
            }
        }
        if self.core.stats.dirty {
            self.core.stats.save();
        }
        let tip = self.tooltip_text(now);
        self.tray.set_tooltip(&tip);
        self.tray.set_paused(self.core.is_paused(now));
    }

    pub fn open_settings(&mut self) {
        if self.settings.is_none() {
            self.settings = Some(SettingsState::new(
                self.core.cfg.clone(),
                autostart::is_enabled(),
            ));
        } else {
            self.settings_focus = true;
        }
    }
    fn handle_tray(&mut self, cmd: TrayCommand, now: Instant) {
        match cmd {
            TrayCommand::OpenSettings => self.open_settings(),
            TrayCommand::ToggleEnabled => {
                let enabled = !self.core.cfg.enabled;
                self.core.cfg.enabled = enabled;
                self.persist_config();
                self.tray.set_enabled(enabled);
                if let Some(s) = &mut self.settings {
                    s.saved.enabled = enabled;
                    s.draft.enabled = enabled;
                }
            }
            TrayCommand::RestNow => self.core.start_break_now(now),
            TrayCommand::TogglePause => {
                if self.core.is_paused(now) {
                    self.core.resume();
                } else {
                    self.core.pause_for(now, PAUSE_DURATION);
                }
            }
            TrayCommand::ToggleSound => {
                let on = !self.core.cfg.sound_enabled;
                self.core.cfg.sound_enabled = on;
                self.persist_config();
                self.tray.set_sound(on);
                if let Some(s) = &mut self.settings {
                    s.saved.sound_enabled = on;
                    s.draft.sound_enabled = on;
                }
            }
            TrayCommand::Quit => {
                log::info!("用户从托盘退出");
                self.quit = true;
            }
        }
    }

    pub fn apply_settings_action(&mut self, action: SettingsAction, now: Instant) {
        match action {
            SettingsAction::Save(cfg) => {
                self.core.apply_config(cfg.clone(), now);
                if let Err(e) = cfg.save() {
                    log::error!("保存配置失败: {e}");
                }
                self.tray.set_enabled(cfg.enabled);
                self.tray.set_sound(cfg.sound_enabled);
                if cfg.hotkey_enabled != self.registered_hotkey.is_some() {
                    if let Err(e) = self.apply_hotkey_enabled(cfg.hotkey_enabled) {
                        log::error!("应用热键设置失败: {e}");
                    }
                }
                if let Some(s) = &mut self.settings {
                    s.saved = cfg.clone();
                    s.draft = cfg;
                    s.saved_at = Some(now);
                }
                log::info!("配置已更新");
            }
            SettingsAction::RestNow => self.core.start_break_now(now),
            SettingsAction::ResetDefaults => {
                if let Some(s) = &mut self.settings {
                    s.draft = Config::default();
                }
            }
            SettingsAction::SetAutostart(on) => {
                if let Some(s) = &mut self.settings {
                    match autostart::set_enabled(on) {
                        Ok(()) => s.autostart_error = None,
                        Err(e) => {
                            s.autostart_error = Some(e);
                            s.autostart = autostart::is_enabled();
                        }
                    }
                }
            }
            SettingsAction::SetHotkeyEnabled(on) => {
                if let Err(e) = self.apply_hotkey_enabled(on) {
                    log::error!("应用热键设置失败: {e}");
                }
                self.core.cfg.hotkey_enabled = on;
                self.persist_config();
                if let Some(s) = &mut self.settings {
                    s.saved.hotkey_enabled = on;
                    s.draft.hotkey_enabled = on;
                }
            }
            SettingsAction::Preview(preset) => self.audio.play_cue(
                preset,
                self.core.cfg.cue_volume_pct,
                self.core.cfg.cue_duration_secs,
            ),
            SettingsAction::OpenConfigDir => {
                let dir = crate::config::config_dir();
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            }
        }
    }

    fn persist_config(&self) {
        if let Err(e) = self.core.cfg.save() {
            log::error!("保存配置失败: {e}");
        }
    }

    /// 提醒状态一句话：托盘 tooltip 与设置窗共用。
    pub fn reminder_line(&self, now: Instant) -> String {
        if !self.core.cfg.enabled {
            "提醒已关闭".to_string()
        } else if self.core.is_paused(now) {
            "已暂停 1 小时".to_string()
        } else {
            match self.core.phase {
                Phase::Idle => format!(
                    "下次休息约 {}后",
                    ui::human_duration(self.core.next_break_in(now))
                ),
                Phase::HeadsUp { .. } => "即将休息".to_string(),
                Phase::Break { .. } => "休息中".to_string(),
            }
        }
    }

    fn tooltip_text(&self, now: Instant) -> String {
        format!(
            "EyeFlow — {}\n{}\n今日完成 {} 次 · 连续 {} 天",
            self.core.state.label(),
            self.reminder_line(now),
            self.core.stats.completed_today(),
            self.core.stats.streak_days
        )
    }
}
