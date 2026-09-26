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

    /// 短名（进 `ComboBox::selected_text`）。
    ///
    /// v0.5.2 这里返回的是「低（30 秒内持续输入 > 40 键）」这种**短名 + 括号规则**
    /// 塞进一个宽度受行宽约束的选择框。英文直译是 45 个字符，在 700 px 窗口里
    /// 要么撑爆布局要么被省略号截断，而它是用户打开下拉框之前唯一能看到的那个值。
    /// → 拆成短名（这里）+ 独立的 `desc_key()` 悬停说明（ADR-0008 §b）。
    pub fn key(self) -> &'static str {
        match self {
            FlowSensitivity::Low => "flow.low",
            FlowSensitivity::Medium => "flow.medium",
            FlowSensitivity::High => "flow.high",
        }
    }

    /// 「30 秒内持续输入 > N 键」那条规则，供 `on_hover_text` 用。
    pub fn desc_key(self) -> &'static str {
        match self {
            FlowSensitivity::Low => "flow.low_desc",
            FlowSensitivity::Medium => "flow.medium_desc",
            FlowSensitivity::High => "flow.high_desc",
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
    /// 下拉框的**呈现顺序**，与上面的枚举声明顺序无关（`key()` 的 match 同理）。
    ///
    /// `Custom` 排第一：用户要挑自己的音频文件时，第一项就该是它，省一次滚动。
    /// 默认值仍是 `GentleChime`（`#[default]` 在枚举体上），所以「排序」不改任何人的
    /// 已保存配置——`Config` 全字段 `#[serde(default)]`，老配置里的 `sound_preset`
    /// 原样读回。
    pub const ALL: [SoundPreset; 6] = [
        SoundPreset::Custom,
        SoundPreset::GentleChime,
        SoundPreset::SoftTap,
        SoundPreset::WaterDrop,
        SoundPreset::DigitalDrop,
        SoundPreset::TripleBeep,
    ];

    /// 文案在 locale 表里（`sound.*`），本枚举只给出 key。
    ///
    /// 见 `FlowSensitivity::key` 的说明：文案搬出领域层是为了让 `config.rs`
    /// 保持纯数据、零测试污染（`label()` 内部读 `AtomicU8` 会让这里的 10 个
    /// 测试变成顺序相关的）。
    pub fn key(self) -> &'static str {
        match self {
            SoundPreset::GentleChime => "sound.gentle_chime",
            SoundPreset::SoftTap => "sound.soft_tap",
            SoundPreset::WaterDrop => "sound.water_drop",
            SoundPreset::DigitalDrop => "sound.digital_drop",
            SoundPreset::TripleBeep => "sound.triple_beep",
            SoundPreset::Custom => "sound.custom",
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

    /// 文案在 locale 表里（`fit.*`），见 `SoundPreset::key` 的说明。
    pub fn key(self) -> &'static str {
        match self {
            WallpaperFit::Cover => "fit.cover",
            WallpaperFit::Contain => "fit.contain",
            WallpaperFit::Stretch => "fit.stretch",
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
    /// 界面语言（`zh-CN` / `en-US`；`en` 前缀一律视为英文，见 `Lang::from_config`）
    ///
    /// v0.5.2 写出的 config.toml 没有这一行，走 `d_language()` 走 serde default，
    /// **行为与今天完全一致**：不触发 `migrate_legacy`（那个判据是「没有
    /// `short_break_min_secs`」），不触发解析失败备份重建。
    /// 反向也安全：`Config` **没有** `deny_unknown_fields`，v0.5.2 读到这一行时
    /// 静默忽略，所以降级不会炸。
    #[serde(default = "d_language")]
    pub language: String,
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
fn d_language() -> String {
    "zh-CN".into()
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
            language: d_language(),
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
        write_atomic(&path, toml::to_string_pretty(self)?.as_bytes())?;
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
        write_atomic(path, toml::to_string_pretty(&cfg)?.as_bytes())?;
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
        // `language` **不做规范化**。
        //
        // 其余字段是「越界就夹紧」，而语言越界不是数值问题：README 明确鼓励手改
        // config，`sanitized()` 若把无法识别的原值悄悄换成别的值再落盘，就等于
        // **替用户改了他自己的配置**——他拼错一个字母，界面语言静默变了，且因为
        // 每次加载都会被改回同一个值，他无法从文件里看出发生过什么。
        //
        // 正确形态：能识别的值规范成 BCP-47 写法写回（`en_US` → `en-US`，
        // 让文件自解释）；**无法识别的原值原样保留**，界面上退到中文，
        // 而且这个错误在界面上是可见的、用户能改回。
        let canonical = crate::tr::Lang::from_config(&self.language).code();
        if self.language.trim().eq_ignore_ascii_case(canonical) {
            self.language = canonical.to_string();
        }
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

/// 原子写：先写同目录下的临时文件，再 `rename` 覆盖目标。写法照抄 `stats.rs`。
///
/// `fs::write` 是「打开 → 截断 → 逐块写」，断电或进程被杀会留下半截 toml。
/// 配置被截断的后果比统计更重：`Config::load` 会把它当损坏文件改名备份后
/// **按默认值重建**，于是用户改过的节奏、严格模式壁纸、自定义音频全部回到出厂。
/// `rename` 在同一卷上是原子的，读者要么看到旧文件、要么看到新文件。
fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, bytes)?;
    // rename 在 Windows 上不能覆盖已存在的目标，必须先删。
    // 这留下一个「旧文件已删、新文件未就位」的极窄窗口，
    // 窗口内崩溃最多丢一次配置——远好于配置被截断后静默重置。
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            log::warn!("配置原子写失败（{}）: {e}", path.display());
            Err(e)
        }
    }
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

    // =======================================================================
    // ADR-0008 i18n：T7b（运行时查表）/ T8（老配置兼容）/ T12b（human_duration）
    //
    // 为什么住在这儿：v0.6 的 i18n 实现应该落在 `src/tr.rs` + 生成的 `src/i18n.rs`，
    // 但本轮**不写生产代码**，而 `main.rs` / `tray.rs` / `ui.rs` 正被另一个 agent
    // 并行修改 —— `config.rs` 的测试模块是此刻唯一无人编辑的宿主。等 `src/tr.rs`
    // 落地后，整个 `mod i18n_tests` 可以整体搬过去。
    //
    // 这些测试引用的符号（除 ADR-0008 已钉死的 `Config.language` 外）取自
    // plan-v0.6 §2.3/§2.4 的 API 草案：`Lang::{ZhCn,EnUs}`、`Lang::from_config`、
    // `set_language`、`tr(key)`、生成的 `KEYS` / `ZH_CN` / `EN_US`。
    // **只钉行为，不钉模块名**：实现若把 tr 层放进 `i18n` 而不是 `tr`，改下面 2 行 import 即可。
    // =======================================================================
    mod i18n_tests {
        use super::*;
        use crate::i18n::{EN_US, KEYS, ZH_CN};
        use crate::tr::{set_language, tr, trn, Lang};
        use std::time::Duration;

        /// 语言是进程级全局状态（`AtomicU8`）：所有会切语言的测试共用一把锁，
        /// 结束时恢复成 zh-CN，避免污染同进程里其它测试的默认值。
        static LANG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        fn zh_index(key: &str) -> usize {
            KEYS.iter()
                .position(|k| *k == key)
                .unwrap_or_else(|| panic!("生成表里没有键 {key:?}"))
        }

        // --- T7b 运行时查表：每键在两种语言下都返回非「key 本身」的值 -------
        #[test]
        fn t7b_tr_never_returns_the_key_itself() {
            let _guard = LANG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let orig = crate::tr::language();
            for (lang, table, code) in [
                (Lang::ZhCn, &ZH_CN[..], "zh-CN"),
                (Lang::EnUs, &EN_US[..], "en-US"),
            ] {
                set_language(lang);
                assert_eq!(crate::tr::language(), lang);
                assert_eq!(
                    table.len(),
                    KEYS.len(),
                    "{code} 表长度与 KEYS 不一致（build.rs 生成的数组长度必须等于 KEY_COUNT）"
                );
                for (i, key) in KEYS.iter().enumerate() {
                    let v = tr(key);
                    assert_ne!(
                        v, *key,
                        "{code} 下 tr({key:?}) 返回了 key 本身 —— key 拼错时会静默显示 \
                         「{key}」，这是 i18n-embed 式实现最典型的漏译症状"
                    );
                    assert_eq!(v, table[i], "{code} 下 tr({key:?}) 与生成表第 {i} 项不一致");
                }
            }
            let differs_somewhere = KEYS.iter().enumerate().any(|(i, _)| ZH_CN[i] != EN_US[i]);
            assert!(
                differs_somewhere,
                "两个语言表内容完全相同 —— en-US 根本没翻（ADR-0008 数据流：\
                 以 zh-CN 为唯一事实来源，但内容必须各自独立）"
            );
            set_language(orig);
        }

        // --- T8① 无 language 字段的老 config.toml 解析后为 zh-CN ----------
        #[test]
        fn t8a_config_without_language_field_defaults_to_zh_cn() {
            let cfg: Config = toml::from_str("enabled = false").unwrap();
            assert_eq!(
                cfg.language, "zh-CN",
                "v0.5.2 写出的 config.toml 没有 language 行，加字段后行为必须与今天完全一致"
            );
            assert_eq!(Config::default().language, "zh-CN");
            // 加字段不得触发 v0.1 迁移（config.rs:400 的判据）
            let table: toml::Table = "short_break_min_secs = 900".parse().unwrap();
            let mut probe = Config::default();
            assert!(!migrate_legacy(&table, &mut probe));
        }

        // --- T8②③ 非法值回落 zh-CN，且 sanitized() 不写回 ------------------
        //
        // ⚠️ `en-GB` 已从这份「非法值」名单里移走（2026-09-26）。本测试最初
        // 写的是「严格白名单 en / en-US / en_US」，依据是 ADR-0008 的初版；
        // **ADR-0008 的该条随后被修订**（见其 §「默认语言与配置兼容」的
        // 修订记录）：`en` 前缀一律得英文。初版的问题是 `en-GB`——一个真实存在
        // 的 locale——会拿到中文界面，而用户在界面里看不出原因。
        // 现在 `en-GB` 的断言在下面的 T8c 里。
        #[test]
        fn t8b_invalid_language_falls_back_to_zh_cn_and_is_never_rewritten() {
            let _guard = LANG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let orig = crate::tr::language();
            for raw in [
                "chinese", // 常见误写
                "klingon", // 随手编的
                "zh_CN",   // 分隔符写错（不过 `en` 前缀，落到中文）
                "",        // 空值
            ] {
                assert_eq!(
                    Lang::from_config(raw),
                    Lang::ZhCn,
                    "非法值 {raw:?} 必须回落 zh-CN —— README 鼓励手改 config，拼错一个字母\
                     导致界面静默变英文且无法自动恢复，是违反「不动用户个人配置」硬约束的"
                );
                // ③ sanitized() 不得把非法值悄悄规范化后落盘
                let mut c = Config::default();
                c.language = raw.into();
                assert_eq!(
                    c.clone().sanitized().language,
                    raw,
                    "sanitized() 把非法 language {raw:?} 改写成了别的值 —— 非法值必须原样保留"
                );
                // 界面语言确实是中文
                set_language(Lang::from_config(raw));
                assert_eq!(
                    tr("tray.open_settings"),
                    ZH_CN[zh_index("tray.open_settings")],
                    "language = {raw:?} 时托盘「打开设置」不是中文"
                );
            }
            set_language(orig);
        }

        // --- T8④ 显式 en / en-US / en_US 必须得到英文 ----------------------
        // `en-GB` 也在这里：ADR-0008 修订后认的是 `en` 前缀，不是白名单。
        #[test]
        fn t8c_explicit_english_spellings_resolve_to_english() {
            let _guard = LANG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let orig = crate::tr::language();
            for raw in ["en", "en-US", "en_US", "en-GB"] {
                let cfg: Config = toml::from_str(&format!("language = \"{raw}\"")).unwrap();
                assert_eq!(cfg.language, raw, "合法值被解析层改写了");
                set_language(Lang::from_config(&cfg.language));
                assert_eq!(
                    tr("tray.open_settings"),
                    EN_US[zh_index("tray.open_settings")]
                );
                assert_eq!(tr("tray.quit"), EN_US[zh_index("tray.quit")]);
            }
            set_language(orig);
        }

        // --- T12b human_duration 的英文形态：m == 0 不得输出 "2 h 0 min" ----
        #[test]
        fn t12b_english_human_duration_omits_the_zero_minute_part() {
            let _guard = LANG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let orig = crate::tr::language();
            set_language(Lang::EnUs);

            let two_hours = crate::ui::human_duration(Duration::from_secs(7200));
            assert!(
                !two_hours.contains("0 min"),
                "英文 2 小时输出成 {two_hours:?} —— 必须有一条 m > 0 的分支（ADR-0008 风险 3）"
            );
            let two_h_five = crate::ui::human_duration(Duration::from_secs(7500));
            assert!(
                two_h_five.contains("2") && two_h_five.contains("5"),
                "英文 2 小时 5 分输出成 {two_h_five:?} —— 数字丢了"
            );

            set_language(Lang::ZhCn);
            let zh = crate::ui::human_duration(Duration::from_secs(7200));
            assert!(
                zh.contains("小时"),
                "中文模式输出成 {zh:?} —— 切语言后 human_duration 走的是英文分支"
            );
            set_language(orig);
        }

        // --- T14 状态条「连续坚持」：0 / 1 / 3 三种读法都要成立 --------------
        // 原文案是英文复合名词 `{n}-day streak`，`0-day streak` 对母语读者生硬，
        // 而同一个托盘 tooltip 里的 `stats.streak`（`{n} days in a row`）是对的。
        // 复合名词用不了 `|` 的二元复数约定（ADR-0008 §能力边界），所以改用
        // 「in a row」措辞绕开复合名词，`|` 就能用了。
        #[test]
        fn t14_status_streak_reads_naturally_at_zero_one_and_three() {
            let _guard = LANG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let orig = crate::tr::language();

            set_language(Lang::EnUs);
            let zero = trn("stats.streak_label", 0u32);
            let one = trn("stats.streak_label", 1u32);
            let three = trn("stats.streak_label", 3u32);
            assert_eq!(
                (zero.as_str(), one.as_str(), three.as_str()),
                ("· 0 days in a row", "· 1 day in a row", "· 3 days in a row"),
                "英文状态条「连续坚持」在 0 / 1 / 3 下的读法不对（实测 {zero:?} / \
                 {one:?} / {three:?}）"
            );
            assert!(
                !one.contains("days"),
                "1 天也拼成 {one:?} —— 单数形态没被 `|` 选中"
            );
            for s in [&zero, &one, &three] {
                assert!(
                    !s.contains("-day"),
                    "状态条仍出现复合名词 {s:?} —— 这就是本条要修的 `0-day streak`"
                );
            }

            set_language(Lang::ZhCn);
            let zh = trn("stats.streak_label", 1u32);
            assert_eq!(zh, "· 连续坚持 1 天", "中文状态条输出成 {zh:?}");
            set_language(orig);
        }
    }
}
