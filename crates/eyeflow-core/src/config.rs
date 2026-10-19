//! Platform-neutral scheduler settings, with the desktop defaults from src/config.rs.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum FlowSensitivity {
    Low,
    #[default]
    Medium,
    High,
}
impl FlowSensitivity {
    pub fn threshold(self) -> u32 {
        match self {
            Self::Low => 40,
            Self::Medium => 70,
            Self::High => 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub short_break_min_secs: u64,
    pub short_break_max_secs: u64,
    pub short_break_secs: u64,
    pub long_break_enabled: bool,
    pub long_break_after_secs: u64,
    pub long_break_secs: u64,
    pub heads_up_secs: u64,
    pub postpone_secs: u64,
    pub visual_enabled: bool,
    pub heads_up_cue_enabled: bool,
    pub away_secs: u64,
    pub flow_sensitivity: FlowSensitivity,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            short_break_min_secs: 900,
            short_break_max_secs: 1500,
            short_break_secs: 30,
            long_break_enabled: true,
            long_break_after_secs: 7200,
            long_break_secs: 900,
            heads_up_secs: 15,
            postpone_secs: 300,
            visual_enabled: true,
            heads_up_cue_enabled: true,
            away_secs: 180,
            flow_sensitivity: FlowSensitivity::default(),
        }
    }
}
impl Config {
    pub fn sanitized(mut self) -> Self {
        self.short_break_min_secs = self.short_break_min_secs.clamp(60, 86400);
        self.short_break_max_secs = self
            .short_break_max_secs
            .clamp(self.short_break_min_secs, 86400);
        self.short_break_secs = self.short_break_secs.clamp(1, 3600);
        self.long_break_after_secs = self.long_break_after_secs.clamp(60, 86400);
        self.long_break_secs = self.long_break_secs.clamp(1, 3600);
        self.heads_up_secs = self.heads_up_secs.min(30);
        self.postpone_secs = self.postpone_secs.clamp(1, 86400);
        self.away_secs = self.away_secs.clamp(1, 86400);
        self
    }
}
