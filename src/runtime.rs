//! 运行时：托盘、热键、传感器事件、调度核心与配置持久化的统一“步进”入口。
//!
//! 同一份逻辑在两种模式下运行：
//! - 轻量循环（main.rs）：没有任何窗口、没有 GL 上下文，常驻内存最小；
//! - UI 会话（app.rs）：eframe 运行期间由 `App::logic` 每帧调用。
//!
//! 这样无论窗口开不开，提醒计时、托盘与热键都不会停摆（v0.1 的设置窗会冻结一切）。

use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::audio::{self, AudioPlayer};
use crate::autostart;
use crate::config::{Config, SoundPreset};
use crate::core::{Action, ContextState, Core, Phase};
use crate::event::Event;
use crate::tray::{Tray, TrayCommand};
use crate::ui::{self, SettingsAction, SettingsState, UpdateStatus};
use crate::update;

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
    pub update_status: UpdateStatus,
    rx: Receiver<Event>,
    tx: Sender<Event>,
    hotkeys: Option<GlobalHotKeyManager>,
    registered_hotkey: Option<HotKey>,
}

impl Runtime {
    pub fn new(
        core: Core,
        rx: Receiver<Event>,
        tx: Sender<Event>,
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
            update_status: UpdateStatus::Idle,
            rx,
            tx,
            hotkeys,
            registered_hotkey: None,
        };
        if let Err(e) = rt.apply_hotkey_enabled(rt.core.cfg.hotkey_enabled) {
            log::warn!("全局热键注册失败: {e}");
        }
        if update::is_due(&rt.core.cfg, update::now_unix()) {
            rt.spawn_update_check();
        }
        rt
    }

    /// 在后台线程查询 GitHub Releases，结果经事件总线回到主线程。
    pub fn spawn_update_check(&mut self) {
        if matches!(self.update_status, UpdateStatus::Checking) {
            return;
        }
        self.update_status = UpdateStatus::Checking;
        let tx = self.tx.clone();
        std::thread::Builder::new()
            .name("eyeflow-update-check".into())
            .spawn(move || {
                let result = update::check().map(|info| {
                    if info.is_newer {
                        format!("{}|{}", info.latest, info.url)
                    } else {
                        String::new()
                    }
                });
                let _ = tx.send(Event::UpdateChecked(result));
                crate::event::wake_ui();
            })
            .ok();
    }

    fn on_update_checked(&mut self, result: Result<String, String>) {
        self.update_status = match result {
            Ok(s) if s.is_empty() => UpdateStatus::UpToDate,
            Ok(s) => {
                let (latest, url) = s
                    .split_once('|')
                    .unwrap_or((s.as_str(), update::RELEASES_URL));
                log::info!("发现新版本 v{latest}: {url}");
                UpdateStatus::Available {
                    latest: latest.to_string(),
                    url: url.to_string(),
                }
            }
            Err(e) => {
                log::warn!("更新检查失败: {e}");
                UpdateStatus::Failed(e)
            }
        };
        self.core.cfg.update_last_checked = Some(update::now_unix());
        self.persist_config();
        if let Some(s) = &mut self.settings {
            s.saved.update_last_checked = self.core.cfg.update_last_checked;
            s.draft.update_last_checked = self.core.cfg.update_last_checked;
        }
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
                Event::UpdateChecked(result) => self.on_update_checked(result),
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
                        // 游戏 / 全屏 / 不可打扰时预告面板不可见，声音是唯一通道 → 自动增强
                        let sound_only = self.core.state == ContextState::Gaming;
                        self.play_cue(sound_only);
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
        let st = &self.core.stats;
        self.tray.set_stats_line(&format!(
            "今日休息 {} 次 · 跳过 {} · 延后 {}",
            st.completed_today(),
            st.skipped,
            st.postponed
        ));
    }

    /// 按当前配置播放提示音：自定义音频优先，失败或未设置时回退到合成预设。
    fn play_cue(&self, sound_only: bool) {
        let cfg = &self.core.cfg;
        let mut secs = cfg.cue_duration_secs;
        if sound_only {
            secs = secs.max(SOUND_ONLY_MIN_CUE_SECS);
        }
        if cfg.sound_preset == SoundPreset::Custom {
            if let Some(path) = cfg.custom_sound_path.as_deref() {
                match self
                    .audio
                    .play_custom(std::path::Path::new(path), cfg.cue_volume_pct)
                {
                    Ok(()) => return,
                    Err(e) => log::warn!("自定义提示音播放失败（{e}），回退默认提示音"),
                }
            }
            self.audio
                .play_cue(SoundPreset::GentleChime, cfg.cue_volume_pct, secs);
        } else {
            self.audio
                .play_cue(cfg.sound_preset, cfg.cue_volume_pct, secs);
        }
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
                let check_now = cfg.update_check_enabled && !self.core.cfg.update_check_enabled;
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
                if check_now {
                    self.spawn_update_check();
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
            SettingsAction::PreviewCustom(path) => {
                let volume = self
                    .settings
                    .as_ref()
                    .map(|s| s.draft.cue_volume_pct)
                    .unwrap_or(self.core.cfg.cue_volume_pct);
                if let Err(e) = self.audio.play_custom(std::path::Path::new(&path), volume) {
                    if let Some(s) = &mut self.settings {
                        s.custom_sound_error = Some(format!("播放失败：{e}"));
                    }
                }
            }
            SettingsAction::PickCustomSound => {
                let picked = rfd::FileDialog::new()
                    .set_title("选择提示音文件（≤ 5 分钟）")
                    .add_filter("音频文件", audio::CUSTOM_EXTENSIONS)
                    .pick_file();
                let Some(path) = picked else {
                    return;
                };
                let Some(s) = &mut self.settings else {
                    return;
                };
                match audio::probe_custom(&path) {
                    Ok(len) => {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        s.custom_sound_info =
                            Some(format!("{name}（{}）", ui::human_duration(len)));
                        s.custom_sound_error = None;
                        s.draft.custom_sound_path = Some(path.display().to_string());
                        s.draft.sound_preset = SoundPreset::Custom;
                    }
                    Err(e) => {
                        s.custom_sound_error = Some(e);
                    }
                }
            }
            SettingsAction::ClearCustomSound => {
                if let Some(s) = &mut self.settings {
                    s.draft.custom_sound_path = None;
                    s.custom_sound_info = None;
                    s.custom_sound_error = None;
                    if s.draft.sound_preset == SoundPreset::Custom {
                        s.draft.sound_preset = SoundPreset::GentleChime;
                    }
                }
            }
            SettingsAction::PickWallpaper => {
                let picked = rfd::FileDialog::new()
                    .set_title("选择严格模式背景图片")
                    .add_filter("图片", crate::wallpaper::WALLPAPER_EXTENSIONS)
                    .pick_file();
                if let (Some(path), Some(s)) = (picked, &mut self.settings) {
                    s.draft.strict_wallpaper_path = Some(path.display().to_string());
                }
            }
            SettingsAction::ClearWallpaper => {
                if let Some(s) = &mut self.settings {
                    s.draft.strict_wallpaper_path = None;
                    s.wallpaper.invalidate();
                }
            }
            SettingsAction::CheckUpdateNow => self.spawn_update_check(),
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
        let st = &self.core.stats;
        let postponed = if st.postponed > 0 {
            format!(" · 延后 {} 次", st.postponed)
        } else {
            String::new()
        };
        format!(
            "EyeFlow — {}\n{}\n今日完成 {} 次{} · 连续 {} 天",
            self.core.state.label(),
            self.reminder_line(now),
            st.completed_today(),
            postponed,
            st.streak_days
        )
    }
}
