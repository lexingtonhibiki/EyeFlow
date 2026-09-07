use chrono::Timelike;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 心流检测灵敏度阈值
/// 30秒窗口内按键次数超过该值判定为心流
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowSensitivity {
    Low,
    Medium,
    High,
}

impl FlowSensitivity {
    pub fn threshold(self) -> u32 {
        match self {
            FlowSensitivity::Low => 6,
            FlowSensitivity::Medium => 10,
            FlowSensitivity::High => 15,
        }
    }
}

/// 提示音预设
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SoundPreset {
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
}

impl Default for SoundPreset {
    fn default() -> Self {
        SoundPreset::GentleChime
    }
}

/// EyeFlow 全部配置项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 全局提醒开关
    pub enabled: bool,
    /// 最小提醒间隔（秒）
    pub min_interval_secs: u64,
    /// 最大提醒间隔（秒）
    pub max_interval_secs: u64,
    /// 护眼时长（秒）
    pub eye_rest_secs: u64,
    /// 提示音开关
    pub sound_enabled: bool,
    /// 提示音预设
    pub sound_preset: SoundPreset,
    /// 系统通知开关
    pub notification_enabled: bool,
    /// 免打扰时段开始 (HH:MM)
    pub dnd_start: String,
    /// 免打扰时段结束 (HH:MM)
    pub dnd_end: String,
    /// 心流检测灵敏度
    pub flow_sensitivity: FlowSensitivity,
    /// 全局静音快捷键
    pub global_mute_hotkey: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            min_interval_secs: 600,
            max_interval_secs: 1080,
            eye_rest_secs: 25,
            sound_enabled: true,
            sound_preset: SoundPreset::default(),
            notification_enabled: true,
            dnd_start: "00:00".into(),
            dnd_end: "08:00".into(),
            flow_sensitivity: FlowSensitivity::Medium,
            global_mute_hotkey: "Ctrl+Shift+E".into(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::path();
        if !path.exists() {
            return Self::create_default(&path);
        }
        let content = std::fs::read_to_string(&path)?;
        match toml::from_str(&content) {
            Ok(cfg) => Ok(cfg),
            Err(e) => {
                log::warn!("配置解析失败 ({}), 使用默认配置覆盖", e);
                // 配置文件格式可能来自旧版本，覆盖重建
                let _ = std::fs::remove_file(&path);
                Self::create_default(&path)
            }
        }
    }

    /// 创建并保存默认配置文件
    fn create_default(path: &std::path::Path) -> Result<Self, ConfigError> {
        let config = Config::default();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(&config)?;
        std::fs::write(path, content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    fn path() -> PathBuf {
        let base = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        base.join("eyeflow").join("config.toml")
    }

    pub fn flow_threshold(&self) -> u32 {
        self.flow_sensitivity.threshold()
    }

    pub fn is_in_dnd(&self) -> bool {
        let now = chrono::Local::now();
        let now_minutes = now.hour() as u32 * 60 + now.minute() as u32;

        let start = parse_time(&self.dnd_start);
        let end = parse_time(&self.dnd_end);

        if start <= end {
            now_minutes >= start && now_minutes < end
        } else {
            now_minutes >= start || now_minutes < end
        }
    }
}

fn parse_time(s: &str) -> u32 {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return 0;
    }
    let hour: u32 = parts[0].parse().unwrap_or(0);
    let minute: u32 = parts[1].parse().unwrap_or(0);
    hour.min(23) * 60 + minute.min(59)
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    TomlParse(toml::de::Error),
    TomlSerialize(toml::ser::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "IO 错误: {}", e),
            ConfigError::TomlParse(e) => write!(f, "TOML 解析错误: {}", e),
            ConfigError::TomlSerialize(e) => write!(f, "TOML 序列化错误: {}", e),
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
        ConfigError::TomlParse(e)
    }
}

impl From<toml::ser::Error> for ConfigError {
    fn from(e: toml::ser::Error) -> Self {
        ConfigError::TomlSerialize(e)
    }
}
