use chrono::Timelike;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 心流检测灵敏度：30 秒滑动窗口内的有效按键数阈值。
///
/// v0.1 的阈值（10 次/30s）等于 0.33 键/秒，任何打字都算心流（审计 P1-9）。
/// v0.2 面向“持续快速输入”重新标定：普通对话式打字的短爆发不会触发，
/// 需要接近连续录入的节奏（中档 ≈ 2.3 键/秒持续 30 秒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FlowSensitivity {
    Low,
    #[default]
    Medium,
    High,
}

impl FlowSensitivity {
    /// 30 秒窗口内的按键数阈值。
    pub fn threshold(self) -> u32 {
        match self {
            FlowSensitivity::Low => 40,
            FlowSensitivity::Medium => 70,
            FlowSensitivity::High => 100,
        }
    }

    pub const ALL: [FlowSensitivity; 3] = [
        FlowSensitivity::Low,
        FlowSensitivity::Medium,
        FlowSensitivity::High,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FlowSensitivity::Low => "低（30 秒内持续输入 > 40 键）",
            FlowSensitivity::Medium => "中（30 秒内持续输入 > 70 键）",
            FlowSensitivity::High => "高（30 秒内持续输入 > 100 键）",
        }
    }
}

/// 提示音预设（`Custom` 使用 `custom_sound_path` 指向的用户音频文件）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SoundPreset {
    #[default]
    #[serde(rename = "gentle_chime")]
    GentleChime,
    #[serde(rename = "soft_tap")]
    SoftTap,
    #[serde(rename = "water_drop")]
    WaterDrop,
    #[serde(rename = "digital_drop")]
    DigitalDrop,
    #[serde(rename = "triple_beep")]
    TripleBeep,
    #[serde(rename = "custom")]
    Custom,
}

impl SoundPreset {
    pub const ALL: [SoundPreset; 6] = [
        SoundPreset::GentleChime,
        SoundPreset::SoftTap,
        SoundPreset::WaterDrop,
        SoundPreset::DigitalDrop,
        SoundPreset::TripleBeep,
        SoundPreset::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SoundPreset::GentleChime => "风铃",
            SoundPreset::SoftTap => "轻敲",
            SoundPreset::WaterDrop => "水滴",
            SoundPreset::DigitalDrop => "数字降调",
            SoundPreset::TripleBeep => "三连短哔",
            SoundPreset::Custom => "自定义音频…",
        }
    }
}

/// 严格模式背景图片的自适应方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WallpaperFit {
    /// 等比放大铺满屏幕，超出部分居中裁掉
    #[default]
    #[serde(rename = "cover")]
    Cover,
    /// 等比缩放完整显示，四周留暗边
    #[serde(rename = "contain")]
    Contain,
    /// 拉伸铺满（可能变形）
    #[serde(rename = "stretch")]
    Stretch,
}

impl WallpaperFit {
    pub const ALL: [WallpaperFit; 3] = [
        WallpaperFit::Cover,
        WallpaperFit::Contain,
        WallpaperFit::Stretch,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WallpaperFit::Cover => "铺满裁剪",
            WallpaperFit::Contain => "完整显示",
            WallpaperFit::Stretch => "拉伸",
        }
    }
}

/// EyeFlow 配置（%APPDATA%\eyeflow\config.toml）
///
/// 默认值的科学依据见 docs/adr/0001（AOA 20-20-20 + 2h/15min）。
/// 全部字段带 `#[serde(default)]`：旧版配置文件缺少新字段时按默认值补齐，
/// 不再因字段缺失而销毁用户配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// 总开关（托盘“启用提醒”）
    #[serde(default = "d_true")]
    pub enabled: bool,
    /// 短休息间隔下限（秒）
    #[serde(default = "d_short_min")]
    pub short_break_min_secs: u64,
    /// 短休息间隔上限（秒）
    #[serde(default = "d_short_max")]
    pub short_break_max_secs: u64,
    /// 短休息时长（秒）
    #[serde(default = "d_short_break")]
    pub short_break_secs: u64,
    /// 是否启用长休息（连续用屏 2 小时 → 15 分钟）
    #[serde(default = "d_true")]
    pub long_break_enabled: bool,
    /// 连续活跃用屏多久触发长休息（秒）
    #[serde(default = "d_long_after")]
    pub long_break_after_secs: u64,
    /// 长休息时长（秒）
    #[serde(default = "d_long_break")]
    pub long_break_secs: u64,
    /// 休息开始前多少秒给出预告
    #[serde(default = "d_heads_up")]
    pub heads_up_secs: u64,
    /// 用户“延后”一次推迟的秒数
    #[serde(default = "d_postpone")]
    pub postpone_secs: u64,
    /// 提示音开关
    #[serde(default = "d_true")]
    pub sound_enabled: bool,
    /// 提示音预设
    #[serde(default)]
    pub sound_preset: SoundPreset,
    /// 视觉提醒（预告浮窗 + 休息面板）；关闭后只响提示音
    #[serde(default = "d_true")]
    pub visual_enabled: bool,
    /// 严格模式：休息面板变为全屏暗色遮罩
    #[serde(default)]
    pub strict_mode: bool,
    /// 免打扰时段开始（HH:MM）
    #[serde(default = "d_quiet_start")]
    pub quiet_start: String,
    /// 免打扰时段结束（HH:MM）
    #[serde(default = "d_quiet_end")]
    pub quiet_end: String,
    /// 心流检测灵敏度
    #[serde(default)]
    pub flow_sensitivity: FlowSensitivity,
    /// 无输入多久视为离开（秒）
    #[serde(default = "d_away")]
    pub away_secs: u64,
    /// 提示音音量百分比（50~200；>100 为主动放大，适配音乐/视频掩蔽）
    #[serde(default = "d_cue_volume")]
    pub cue_volume_pct: u32,
    /// 提示音总时长（秒，1~5；短图案循环铺满，依据见 docs/report-v0.3.md）
    #[serde(default = "d_cue_duration")]
    pub cue_duration_secs: u64,
    /// 是否启用全局热键 Ctrl+Shift+E（组合键是全局独占的，允许用户关闭）
    #[serde(default = "d_true")]
    pub hotkey_enabled: bool,
    /// 自定义提示音文件（wav / mp3 / ogg / flac / m4a，≤ 5 分钟；`sound_preset = "custom"` 时使用）
    #[serde(default)]
    pub custom_sound_path: Option<String>,
    /// 严格模式背景图片（png / jpg / webp / bmp / gif）
    #[serde(default)]
    pub strict_wallpaper_path: Option<String>,
    /// 严格模式背景图片的自适应方式
    #[serde(default)]
    pub strict_wallpaper_fit: WallpaperFit,
    /// 启动时检查更新（默认关闭；开启后每 24 小时访问一次 GitHub Releases API，不下载任何文件）
    #[serde(default)]
    pub update_check_enabled: bool,
    /// 上次检查更新的 Unix 时间戳（秒）
    #[serde(default)]
    pub update_last_checked: Option<u64>,
    /// 点击『现在开始』/『立即休息』时播放提示音（结束音照常）
    #[serde(default = "d_true")]
    pub start_cue_enabled: bool,
    /// 休息进行中按 Esc 跳过（严格模式下不可用）
    #[serde(default = "d_true")]
    pub esc_skip_enabled: bool,
    /// 严格模式蒙层浓度（0~85，百分比）
    #[serde(default = "d_overlay_pct")]
    pub strict_overlay_pct: u32,
    /// 严格模式蒙层用上深下浅的垂直渐变
    #[serde(default)]
    pub strict_overlay_gradient: bool,
}

fn d_true() -> bool {
    true
}
fn d_short_min() -> u64 {
    900
}
fn d_short_max() -> u64 {
    1500
}
fn d_short_break() -> u64 {
    30
}
fn d_long_after() -> u64 {
    7200
}
fn d_long_break() -> u64 {
    900
}
fn d_heads_up() -> u64 {
    15
}
fn d_postpone() -> u64 {
    300
}
fn d_quiet_start() -> String {
    "00:00".into()
}
fn d_quiet_end() -> String {
    "08:00".into()
}
fn d_away() -> u64 {
    180
}
fn d_cue_volume() -> u32 {
    100
}
fn d_cue_duration() -> u64 {
    1
}
fn d_overlay_pct() -> u32 {
    55
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            short_break_min_secs: d_short_min(),
            short_break_max_secs: d_short_max(),
            short_break_secs: d_short_break(),
            long_break_enabled: true,
            long_break_after_secs: d_long_after(),
            long_break_secs: d_long_break(),
            heads_up_secs: d_heads_up(),
            postpone_secs: d_postpone(),
            sound_enabled: true,
            sound_preset: SoundPreset::default(),
            visual_enabled: true,
            strict_mode: false,
            quiet_start: d_quiet_start(),
            quiet_end: d_quiet_end(),
            flow_sensitivity: FlowSensitivity::default(),
            away_secs: d_away(),
            cue_volume_pct: d_cue_volume(),
            cue_duration_secs: d_cue_duration(),
            hotkey_enabled: true,
            custom_sound_path: None,
            strict_wallpaper_path: None,
            strict_wallpaper_fit: WallpaperFit::default(),
            update_check_enabled: false,
            update_last_checked: None,
            start_cue_enabled: true,
            esc_skip_enabled: true,
            strict_overlay_pct: d_overlay_pct(),
            strict_overlay_gradient: false,
        }
    }
}

impl Config {
    /// 加载配置；解析失败时把旧文件改名备份后用默认值重建，绝不静默销毁。
    /// 遇到 v0.1 格式（`min_interval_secs` 等字段）时自动迁移并备份原文件。
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::path();
        if !path.exists() {
            return Self::create_default(&path);
        }
        let content = std::fs::read_to_string(&path)?;
        let table: toml::Table = match content.parse() {
            Ok(t) => t,
            Err(e) => {
                log::warn!("配置解析失败（{e}），备份旧文件后重建默认配置");
                Self::backup(&path, "corrupt");
                return Self::create_default(&path);
            }
        };
        let mut cfg: Config = match table.clone().try_into() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("配置字段类型不合法（{e}），备份旧文件后重建默认配置");
                Self::backup(&path, "corrupt");
                return Self::create_default(&path);
            }
        };
        if migrate_legacy(&table, &mut cfg) {
            log::info!("检测到 v0.1 配置格式，已迁移到 v0.2 并备份原文件");
            Self::backup(&path, "v0.1");
            let cfg = cfg.sanitized();
            cfg.save()?;
            return Ok(cfg);
        }
        Ok(cfg.sanitized())
    }

    fn backup(path: &std::path::Path, tag: &str) {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let backup = path.with_extension(format!("toml.bak-{tag}-{stamp}"));
        if let Err(e) = std::fs::rename(path, &backup) {
            log::warn!("备份旧配置失败: {e}");
        }
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn path() -> PathBuf {
        config_dir().join("config.toml")
    }

    fn create_default(path: &std::path::Path) -> Result<Self, ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let cfg = Config::default();
        std::fs::write(path, toml::to_string_pretty(&cfg)?)?;
        Ok(cfg)
    }

    /// 把手工编辑可能造成的越界值拉回合理区间（min ≤ max、时长下限等）。
    pub fn sanitized(mut self) -> Self {
        self.short_break_min_secs = self.short_break_min_secs.clamp(60, 6 * 3600);
        self.short_break_max_secs = self
            .short_break_max_secs
            .clamp(self.short_break_min_secs, 6 * 3600);
        self.short_break_secs = self.short_break_secs.clamp(10, 600);
        self.long_break_after_secs = self.long_break_after_secs.clamp(30 * 60, 8 * 3600);
        self.long_break_secs = self.long_break_secs.clamp(60, 3600);
        self.heads_up_secs = self.heads_up_secs.clamp(5, 120);
        self.postpone_secs = self.postpone_secs.clamp(60, 3600);
        self.away_secs = self.away_secs.clamp(60, 3600);
        self.cue_volume_pct = self.cue_volume_pct.clamp(50, 200);
        self.cue_duration_secs = self.cue_duration_secs.clamp(1, 5);
        self.strict_overlay_pct = self.strict_overlay_pct.clamp(0, 85);
        self
    }

    /// 免打扰时段（支持跨零点）。起止相同视为未设置。
    pub fn in_quiet_hours(&self, now: chrono::NaiveTime) -> bool {
        let now_min = now.hour() * 60 + now.minute();
        let (s, e) = (parse_hhmm(&self.quiet_start), parse_hhmm(&self.quiet_end));
        if s == e {
            return false;
        }
        if s < e {
            now_min >= s && now_min < e
        } else {
            now_min >= s || now_min < e
        }
    }
}

pub fn config_dir() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("eyeflow")
}

/// v0.1 → v0.2 字段迁移。返回是否检测到旧格式。
///
/// 只有用户**改过**的旧值才带过来：旧默认值（10~18 分钟 / 25 秒）会被 v0.2 的
/// 循证默认值（15~25 分钟 / 30 秒，docs/adr/0001）取代。免打扰时段原样保留。
/// 旧的 `notification_enabled` 指的是从未实现的气球通知，不映射到新的视觉提醒。
fn migrate_legacy(table: &toml::Table, cfg: &mut Config) -> bool {
    if table.contains_key("short_break_min_secs") {
        return false;
    }
    let legacy_keys = [
        "min_interval_secs",
        "max_interval_secs",
        "eye_rest_secs",
        "dnd_start",
        "dnd_end",
        "dnd_enabled",
        "hotkey_enabled",
    ];
    if !legacy_keys.iter().any(|k| table.contains_key(*k)) {
        return false;
    }
    let int = |k: &str| {
        table
            .get(k)
            .and_then(|v| v.as_integer())
            .map(|v| v.max(0) as u64)
    };
    let text = |k: &str| table.get(k).and_then(|v| v.as_str()).map(str::to_owned);
    let flag = |k: &str| table.get(k).and_then(|v| v.as_bool());

    if let Some(v) = int("min_interval_secs").filter(|v| *v != 600) {
        cfg.short_break_min_secs = v;
    }
    if let Some(v) = int("max_interval_secs").filter(|v| *v != 1080) {
        cfg.short_break_max_secs = v.max(cfg.short_break_min_secs);
    }
    if let Some(v) = int("eye_rest_secs").filter(|v| *v != 25) {
        cfg.short_break_secs = v;
    }
    if let Some(v) = text("dnd_start") {
        cfg.quiet_start = v;
    }
    if let Some(v) = text("dnd_end") {
        cfg.quiet_end = v;
    }
    // 某些旧版本有独立的免打扰总开关；关掉过就用“起止相同”表示未启用
    if flag("dnd_enabled") == Some(false) {
        cfg.quiet_end = cfg.quiet_start.clone();
    }
    // 旧版本允许关闭全局热键，尊重该偏好
    if flag("hotkey_enabled") == Some(false) {
        cfg.hotkey_enabled = false;
    }
    true
}

/// "HH:MM" → 当日分钟数；格式错误按 0 处理。
pub fn parse_hhmm(s: &str) -> u32 {
    let mut parts = s.split(':');
    let h: u32 = parts
        .next()
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or(0);
    let m: u32 = parts
        .next()
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or(0);
    h.min(23) * 60 + m.min(59)
}

pub fn format_hhmm(minutes: u32) -> String {
    format!("{:02}:{:02}", (minutes / 60) % 24, minutes % 60)
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Toml(toml::de::Error),
    TomlSerialize(toml::ser::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "IO 错误: {e}"),
            ConfigError::Toml(e) => write!(f, "TOML 解析错误: {e}"),
            ConfigError::TomlSerialize(e) => write!(f, "TOML 序列化错误: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}
impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Toml(e)
    }
}
impl From<toml::ser::Error> for ConfigError {
    fn from(e: toml::ser::Error) -> Self {
        ConfigError::TomlSerialize(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_hours_crossing_midnight() {
        let mut c = Config::default();
        c.quiet_start = "23:00".into();
        c.quiet_end = "07:00".into();
        assert!(c.in_quiet_hours(chrono::NaiveTime::from_hms_opt(23, 30, 0).unwrap()));
        assert!(c.in_quiet_hours(chrono::NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
        assert!(!c.in_quiet_hours(chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap()));
    }

    #[test]
    fn quiet_hours_same_bounds_means_disabled() {
        let mut c = Config::default();
        c.quiet_start = "08:00".into();
        c.quiet_end = "08:00".into();
        assert!(!c.in_quiet_hours(chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap()));
    }

    #[test]
    fn defaults_survive_partial_toml() {
        let cfg: Config = toml::from_str("enabled = false").unwrap();
        assert!(!cfg.enabled);
        assert_eq!(cfg.short_break_secs, 30);
        assert_eq!(cfg.short_break_min_secs, 900);
        assert_eq!(cfg.flow_sensitivity, FlowSensitivity::Medium);
    }

    #[test]
    fn sanitize_fixes_inverted_interval() {
        let mut c = Config::default();
        c.short_break_min_secs = 1800;
        c.short_break_max_secs = 600;
        let c = c.sanitized();
        assert!(c.short_break_min_secs <= c.short_break_max_secs);
    }

    #[test]
    fn hhmm_roundtrip() {
        assert_eq!(parse_hhmm("07:30"), 450);
        assert_eq!(format_hhmm(450), "07:30");
        assert_eq!(parse_hhmm("garbage"), 0);
    }

    #[test]
    fn legacy_v01_config_migrates_customized_values_only() {
        let legacy = r#"
enabled = true
min_interval_secs = 600
max_interval_secs = 2400
eye_rest_secs = 25
sound_enabled = false
sound_preset = "water_drop"
notification_enabled = false
dnd_start = "23:00"
dnd_end = "07:30"
flow_sensitivity = "High"
global_mute_hotkey = "Ctrl+Shift+E"
"#;
        let table: toml::Table = legacy.parse().unwrap();
        let mut cfg: Config = table.clone().try_into().unwrap();
        assert!(migrate_legacy(&table, &mut cfg));
        // 旧默认值 600 不带过来 → 新默认 900；用户改过的 2400 带过来
        assert_eq!(cfg.short_break_min_secs, 900);
        assert_eq!(cfg.short_break_max_secs, 2400);
        // 旧默认 25 秒 → 新默认 30 秒
        assert_eq!(cfg.short_break_secs, 30);
        assert_eq!(cfg.quiet_start, "23:00");
        assert_eq!(cfg.quiet_end, "07:30");
        // 同名字段直接解析
        assert!(!cfg.sound_enabled);
        assert_eq!(cfg.sound_preset, SoundPreset::WaterDrop);
        assert_eq!(cfg.flow_sensitivity, FlowSensitivity::High);
        // 旧的气球通知开关不影响新的视觉提醒
        assert!(cfg.visual_enabled);
    }

    #[test]
    fn v02_config_is_not_treated_as_legacy() {
        let table: toml::Table = "short_break_min_secs = 900\ndnd_start = \"x\""
            .parse()
            .unwrap();
        let mut cfg = Config::default();
        assert!(!migrate_legacy(&table, &mut cfg));
    }

    #[test]
    fn legacy_disabled_dnd_switch_disables_quiet_hours() {
        let table: toml::Table = "dnd_start = \"00:00\"\ndnd_end = \"08:00\"\ndnd_enabled = false"
            .parse()
            .unwrap();
        let mut cfg = Config::default();
        assert!(migrate_legacy(&table, &mut cfg));
        assert_eq!(cfg.quiet_start, cfg.quiet_end);
        assert!(!cfg.in_quiet_hours(chrono::NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
    }

    #[test]
    fn legacy_disabled_hotkey_is_carried_over() {
        let table: toml::Table = "hotkey_enabled = false".parse().unwrap();
        let mut cfg = Config::default();
        assert!(migrate_legacy(&table, &mut cfg));
        assert!(!cfg.hotkey_enabled);
    }

    #[test]
    fn sanitize_clamps_cue_volume_and_duration() {
        let mut c = Config::default();
        c.cue_volume_pct = 500;
        c.cue_duration_secs = 30;
        let c = c.sanitized();
        assert_eq!(c.cue_volume_pct, 200);
        assert_eq!(c.cue_duration_secs, 5);
    }
}
