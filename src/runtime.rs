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
use crate::tr::{tr, tr_fill, trn, Lang};
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
    /// 休息期间临时注册的 Esc（面板不抢焦点收不到键盘，只能走全局热键）
    esc_registered: bool,
    /// 上一次推给托盘的 tooltip 的「廉价指纹」+ 已推送文本。
    /// `tooltip_text()` 每个 tick 要做一次 `chrono::Local::now()` 的时区查询
    /// 加 3~4 次 `format!`（约 5 次堆分配），而托盘 tooltip 并不需要每 tick 刷新。
    /// 指纹覆盖了 `tooltip_text()` 的全部输入（见 `tooltip_fingerprint`），
    /// 指纹不变就整段跳过。
    last_tooltip: Option<(TooltipFingerprint, String)>,
    /// 同理，托盘菜单里的今日统计行：只由三个计数器决定。
    last_stats_line: Option<(u32, u32, u32)>,
}

/// `tooltip_text()` 所有输入的廉价指纹。
///
/// 逐项对应关系（改动 `tooltip_text` 时必须同步改这里，否则会漏刷新）：
/// - `state` / `enabled` / `paused` / `phase` → 第 1、2 行的文案；
/// - `phase` 只取判别式：`HeadsUp` / `Break` 的文案是常量，带上的时间载荷不影响输出；
/// - `minute` → `Idle` 分支里 `%H:%M` 的钟点，**只会在分钟边界变**；
/// - `completed` / `postponed` / `streak` → 第 3 行的计数。
type TooltipFingerprint = (ContextState, bool, bool, u8, i64, u32, u32, u32);

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
            esc_registered: false,
            last_tooltip: None,
            last_stats_line: None,
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
                return Err(tr("runtime.hotkey_unavailable").to_string());
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

    /// 手动开始休息（预告“现在开始”/ 托盘 / 热键）：可选播放一声开始提示音。
    pub fn on_manual_break_start(&mut self, now: Instant) {
        self.core.start_break_now(now);
        if self.core.cfg.sound_enabled && self.core.cfg.start_cue_enabled {
            self.play_cue(false);
        }
    }

    /// 休息期间临时注册 / 注销 Esc 全局热键（每帧同步，开销可忽略）。
    fn sync_esc_hotkey(&mut self) {
        let wanted = self.core.cfg.esc_skip_enabled
            && !self.core.cfg.strict_mode
            && matches!(self.core.phase, Phase::Break { .. });
        if wanted == self.esc_registered {
            return;
        }
        let Some(mgr) = &self.hotkeys else {
            return;
        };
        let esc = HotKey::new(None, Code::Escape);
        if wanted {
            match mgr.register(esc) {
                Ok(()) => self.esc_registered = true,
                Err(e) => log::debug!("Esc 热键注册失败: {e}"),
            }
        } else if self.esc_registered {
            let _ = mgr.unregister(esc);
            self.esc_registered = false;
        }
    }

    /// 是否需要一个 UI 会话（有窗口要显示）。
    ///
    /// 最后一个分支是**预热**（`Core::ui_warmup_in`）：浮窗出现前的几秒就把
    /// 会话建好，等 `Phase::HeadsUp` 一到，窗口在同一帧出现。没有它时，会话
    /// 是在「预告该出现」的那一刻才从零开始建的（GL 上下文 + 字体 + 着色器），
    /// 于是用户先听到提示音、再看着窗口从白底上一点点画出来。
    pub fn needs_ui(&self, now: Instant) -> bool {
        self.settings.is_some()
            || (self.core.cfg.visual_enabled && !matches!(self.core.phase, Phase::Idle))
            || (self.core.cfg.visual_enabled && self.core.flash_active(now))
            || self.core.ui_warmup_in(now).is_some()
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
        self.sync_esc_hotkey();
        while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
            let pressed = ev.state == HotKeyState::Pressed;
            let user_hotkey = self
                .registered_hotkey
                .as_ref()
                .is_some_and(|hk| ev.id == hk.id());
            if pressed && user_hotkey {
                log::info!("热键：立即休息");
                self.on_manual_break_start(now);
            } else if pressed && self.esc_registered {
                log::info!("Esc：跳过本次休息");
                self.core.skip(now);
            }
        }

        let quiet = self.core.cfg.in_quiet_hours(chrono::Local::now().time());
        for action in self.core.tick(now, quiet) {
            match action {
                Action::PlayCue => {
                    if self.core.cfg.sound_enabled {
                        // 游戏 / 全屏 / 不可打扰时预告面板不可见，声音是唯一通道 → 自动增强
                        let sound_only = self.core.state == ContextState::Gaming;
                        log::debug!("播放提示音（sound_only={sound_only}）");
                        self.play_cue(sound_only);
                    }
                }
            }
        }
        if self.core.stats.dirty {
            self.core.stats.save();
        }
        self.sync_tooltip(now);
        self.tray.set_paused(self.core.is_paused(now));
        self.sync_stats_line();
    }

    /// 托盘 tooltip：先比指纹、后构造文本。
    ///
    /// 顺序很重要——旧写法无条件先 `tooltip_text()` 再交给托盘内部的守卫，
    /// 守卫只挡住了 Win32 调用，昂贵的那几次分配一次都没省下。
    fn sync_tooltip(&mut self, now: Instant) {
        let fp = self.tooltip_fingerprint(now);
        if self
            .last_tooltip
            .as_ref()
            .is_some_and(|(prev, _)| *prev == fp)
        {
            return;
        }
        let tip = self.tooltip_text(now);
        self.tray.set_tooltip(&tip);
        self.last_tooltip = Some((fp, tip));
    }

    /// 托盘统计行：先比三个计数器、后 `format!`。
    fn sync_stats_line(&mut self) {
        let st = &self.core.stats;
        let key = (st.completed_today(), st.skipped, st.postponed);
        if self.last_stats_line == Some(key) {
            return;
        }
        self.last_stats_line = Some(key);
        let (completed, skipped, postponed) = key;
        self.tray
            .set_stats_line(&crate::tr::tray_stats_line(completed, skipped, postponed));
    }

    /// 见 `TooltipFingerprint`。
    fn tooltip_fingerprint(&self, now: Instant) -> TooltipFingerprint {
        (
            self.core.state,
            self.core.cfg.enabled,
            self.core.is_paused(now),
            phase_discriminant(&self.core.phase),
            // 钟点只按分钟变；这是本函数唯一一次时区查询
            chrono::Local::now().timestamp() / 60,
            self.core.stats.completed_today(),
            self.core.stats.postponed,
            self.core.stats.streak_days,
        )
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
            TrayCommand::RestNow => self.on_manual_break_start(now),
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
                let cfg = *cfg;
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
            SettingsAction::RestNow => self.on_manual_break_start(now),
            SettingsAction::ResetDefaults => {
                let defaults = Config::default();
                if let Some(s) = &mut self.settings {
                    s.draft = defaults.clone();
                }
                // 即时保存语义：恢复默认也立即落盘
                self.core.apply_config(defaults.clone(), now);
                if let Err(e) = defaults.save() {
                    log::error!("保存默认配置失败: {e}");
                }
                self.tray.set_enabled(defaults.enabled);
                self.tray.set_sound(defaults.sound_enabled);
                if defaults.hotkey_enabled != self.registered_hotkey.is_some() {
                    if let Err(e) = self.apply_hotkey_enabled(defaults.hotkey_enabled) {
                        log::error!("应用热键设置失败: {e}");
                    }
                }
                if let Some(s) = &mut self.settings {
                    s.saved = defaults;
                    s.saved_at = Some(now);
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
            SettingsAction::Preview {
                preset,
                volume_pct,
                duration_secs,
            } => self.audio.play_cue(preset, volume_pct, duration_secs),
            SettingsAction::PreviewCustom { path, volume_pct } => {
                if let Err(e) = self
                    .audio
                    .play_custom(std::path::Path::new(&path), volume_pct)
                {
                    if let Some(s) = &mut self.settings {
                        s.custom_sound_error =
                            Some(tr_fill("runtime.custom_play_failed", "{err}", e));
                    }
                }
            }
            SettingsAction::PickCustomSound => {
                let picked = rfd::FileDialog::new()
                    .set_title(tr("dialog.pick_sound"))
                    .add_filter(tr("dialog.audio_filter"), audio::CUSTOM_EXTENSIONS)
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
                        s.custom_sound_info = Some(
                            tr("ui.sound_file_info")
                                .replace("{name}", &name)
                                .replace("{len}", &ui::human_duration(len)),
                        );
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
                    .set_title(tr("dialog.pick_wallpaper"))
                    .add_filter(
                        tr("dialog.image_filter"),
                        crate::wallpaper::WALLPAPER_EXTENSIONS,
                    )
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
            SettingsAction::SetLanguage(code) => self.apply_language(code),
            SettingsAction::OpenConfigDir => {
                let dir = crate::config::config_dir();
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            }
        }
    }

    /// 切换界面语言：更新全局语言状态 → 托盘逐项重写 → 落盘。
    ///
    /// 视口内容与字体不需要这里管：egui 每帧从 `tr()` 现取，`app.rs` 的
    /// `App::logic` 每帧比对 `tr::language()`，变了就 `ctx.set_fonts()`。
    pub fn apply_language(&mut self, code: &str) {
        let lang = Lang::from_config(code);
        if lang == crate::tr::language() && self.core.cfg.language == lang.code() {
            return;
        }
        self.core.cfg.language = lang.code().to_string();
        self.tray.apply_language(lang);
        // 统计行与 tooltip 的指纹不含语言，必须强制重推一次
        self.last_stats_line = None;
        self.last_tooltip = None;
        self.persist_config();
        if let Some(s) = &mut self.settings {
            s.saved.language = self.core.cfg.language.clone();
            s.draft.language = self.core.cfg.language.clone();
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
            tr("runtime.reminder_disabled").to_string()
        } else if self.core.is_paused(now) {
            tr("runtime.reminder_paused").to_string()
        } else {
            match self.core.phase {
                // 具体钟点比“约 12 分钟后”更直观（快赢审计 #4）
                Phase::Idle => {
                    let at = chrono::Local::now()
                        + chrono::Duration::from_std(self.core.next_break_in(now))
                            .unwrap_or_default();
                    tr_fill("runtime.reminder_next", "{t}", at.format("%H:%M"))
                }
                Phase::HeadsUp { .. } => tr("runtime.reminder_imminent").to_string(),
                Phase::Break { .. } => tr("runtime.reminder_in_break").to_string(),
            }
        }
    }

    fn tooltip_text(&self, now: Instant) -> String {
        let st = &self.core.stats;
        // 延后为 0 时整段留空（v0.5.2 的行为），不为 0 时用复数 key
        let postponed = if st.postponed > 0 {
            trn("stats.postponed_suffix", st.postponed)
        } else {
            String::new()
        };
        format!(
            "{}\n{}\n{}{}{}{}",
            tr_fill("runtime.tooltip_head", "{state}", tr(self.core.state.key())),
            self.reminder_line(now),
            trn("stats.completed", st.completed_today()),
            postponed,
            tr("stats.separator"),
            trn("stats.streak", st.streak_days),
        )
    }
}

/// `Phase` 的判别式（0/1/2）。见 `TooltipFingerprint`：HeadsUp / Break 的
/// tooltip 文案是常量，载荷里的时间戳不影响输出，因此指纹只需要判别式。
fn phase_discriminant(p: &Phase) -> u8 {
    match p {
        Phase::Idle => 0,
        Phase::HeadsUp { .. } => 1,
        Phase::Break { .. } => 2,
    }
}
