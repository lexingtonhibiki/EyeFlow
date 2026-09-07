use serde::{Deserialize, Serialize};

/// 坚持统计（%APPDATA%\eyeflow\stats.toml）
///
/// 只记录当天计数与连续天数，不做历史时间线——统计的意义是“欠账可视化”
/// 与留存钩子（docs/research/03-competitor-ux.md），不是数据面板。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    /// 计数归属的日期（YYYY-MM-DD）
    #[serde(default)]
    pub day: String,
    #[serde(default)]
    pub completed_short: u32,
    #[serde(default)]
    pub completed_long: u32,
    /// 自然休息：用户自行离开屏幕达到休息时长，计时器因此重置
    #[serde(default)]
    pub completed_natural: u32,
    #[serde(default)]
    pub skipped: u32,
    #[serde(default)]
    pub postponed: u32,
    /// 连续有完成记录的天数
    #[serde(default)]
    pub streak_days: u32,
    /// 最近一次有完成记录的日期
    #[serde(default)]
    pub last_completed_day: String,
    /// 内存中有未落盘的变更；由宿主在合适时机调用 `save()` 清除
    #[serde(skip)]
    pub dirty: bool,
}

impl Stats {
    pub fn load() -> Self {
        let path = Self::path();
        let mut stats = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str::<Stats>(&s).ok())
            .unwrap_or_default();
        stats.roll_day(&today());
        stats
    }

    pub fn save(&mut self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = toml::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
        self.dirty = false;
    }

    fn path() -> std::path::PathBuf {
        crate::config::config_dir().join("stats.toml")
    }

    /// 跨日时清零当天计数；连续天数在记录完成时维护。
    pub fn roll_day(&mut self, today: &str) {
        if self.day != today {
            self.day = today.to_string();
            self.completed_short = 0;
            self.completed_long = 0;
            self.completed_natural = 0;
            self.skipped = 0;
            self.postponed = 0;
        }
    }

    pub fn record_completed(&mut self, kind: Completed) {
        let today = today();
        self.roll_day(&today);
        match kind {
            Completed::Short => self.completed_short += 1,
            Completed::Long => self.completed_long += 1,
            Completed::Natural => self.completed_natural += 1,
        }
        self.bump_streak(&today, &yesterday());
        self.dirty = true;
    }

    pub fn record_skipped(&mut self) {
        self.roll_day(&today());
        self.skipped += 1;
        self.dirty = true;
    }

    pub fn record_postponed(&mut self) {
        self.roll_day(&today());
        self.postponed += 1;
        self.dirty = true;
    }

    fn bump_streak(&mut self, today: &str, yesterday: &str) {
        if self.last_completed_day == today {
            return;
        }
        self.streak_days = if self.last_completed_day == yesterday {
            self.streak_days + 1
        } else {
            1
        };
        self.last_completed_day = today.to_string();
    }

    pub fn completed_today(&self) -> u32 {
        self.completed_short + self.completed_long + self.completed_natural
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completed {
    Short,
    Long,
    Natural,
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn yesterday() -> String {
    (chrono::Local::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_roll_resets_counters_but_keeps_streak() {
        let mut s = Stats {
            day: "2026-09-07".into(),
            completed_short: 5,
            skipped: 2,
            streak_days: 3,
            last_completed_day: "2026-09-07".into(),
            ..Default::default()
        };
        s.roll_day("2026-09-08");
        assert_eq!(s.completed_short, 0);
        assert_eq!(s.skipped, 0);
        assert_eq!(s.streak_days, 3);
    }

    #[test]
    fn streak_increments_on_consecutive_days_only() {
        let mut s = Stats::default();
        s.bump_streak("2026-09-07", "2026-09-06");
        assert_eq!(s.streak_days, 1);
        s.bump_streak("2026-09-07", "2026-09-06"); // same day: no change
        assert_eq!(s.streak_days, 1);
        s.bump_streak("2026-09-08", "2026-09-07");
        assert_eq!(s.streak_days, 2);
        s.bump_streak("2026-09-12", "2026-09-11"); // gap: reset
        assert_eq!(s.streak_days, 1);
    }
}
