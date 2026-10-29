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
        self.save_to(&Self::path());
    }

    /// Make one final attempt for pending statistics, without automatic retry delay.
    pub fn flush_if_dirty(&mut self) {
        self.flush_if_dirty_with(Self::save);
    }

    fn flush_if_dirty_with(&mut self, save: impl FnOnce(&mut Self)) {
        if self.dirty {
            save(self);
        }
    }

    fn save_to(&mut self, path: &std::path::Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = toml::to_string_pretty(self) {
            match write_atomic(path, s.as_bytes()) {
                Ok(()) => self.dirty = false,
                Err(e) => log::warn!("统计落盘失败: {e}"),
            }
        }
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

/// Host-owned retry state; never serialized with the statistics.
/// Only failed automatic saves are delayed. Explicit `Stats::save` bypasses this
/// gate so shutdown can make a final attempt without waiting for another frame.
#[derive(Debug, Default)]
pub struct StatsSaveRetry {
    retry_after: Option<std::time::Instant>,
}

impl StatsSaveRetry {
    pub fn save_if_due(&mut self, stats: &mut Stats, now: std::time::Instant) {
        self.save_if_due_with(stats, now, Stats::save);
    }

    fn save_if_due_with(
        &mut self,
        stats: &mut Stats,
        now: std::time::Instant,
        save: impl FnOnce(&mut Stats),
    ) {
        if !stats.dirty {
            self.retry_after = None;
            return;
        }
        if self.retry_after.is_some_and(|deadline| now < deadline) {
            return;
        }
        save(stats);
        self.retry_after = stats.dirty.then(|| now + std::time::Duration::from_secs(5));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completed {
    Short,
    Long,
    Natural,
}

/// 原子写：先写同目录下的临时文件，再 `rename` 覆盖目标。
///
/// `fs::write` 是「打开 → 截断 → 逐块写」，断电或进程被杀会留下半截 toml；
/// 统计文件被截断后 `Stats::load` 会静默回落到全零，用户的坚持记录就没了。
/// 同目录替换不先删除旧文件；未执行 fsync，不提供断电持久性保证。
fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    write_atomic_with_replace(path, bytes, |from, to| std::fs::rename(from, to))
}

// Injectable replacement operation keeps failure preservation testable without
// changing filesystem permissions or depending on a timing race.
fn write_atomic_with_replace(
    path: &std::path::Path,
    bytes: &[u8],
    replace: impl FnOnce(&std::path::Path, &std::path::Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, bytes)?;
    // rename replaces an existing file on supported Unix and Windows filesystems.
    // Keep the old destination until replacement succeeds; never delete it first.
    match replace(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
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

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "eyeflow-stats-dirty-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn dirty_save_retries_after_temporary_file_obstruction_is_removed() {
        let dir = TempDir::new();
        let path = dir.0.join("stats.toml");
        let tmp = path.with_extension("toml.tmp");
        let mut stats = Stats::default();
        stats.record_skipped();
        // A directory at the temporary-file path fails even for privileged users.
        std::fs::create_dir(&tmp).unwrap();
        stats.save_to(&path);
        assert!(stats.dirty, "failed persistence must remain pending");
        assert!(!path.exists());
        std::fs::remove_dir(&tmp).unwrap();
        // Match the host's dirty-gated next save, with no new recorded event.
        if stats.dirty {
            stats.save_to(&path);
        }
        assert!(!stats.dirty);
        let saved: Stats = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(saved.skipped, 1);
    }

    #[test]
    fn dirty_survives_repeated_save_failures_and_keeps_latest_counts() {
        let dir = TempDir::new();
        let path = dir.0.join("stats.toml");
        let tmp = path.with_extension("toml.tmp");
        std::fs::create_dir(&tmp).unwrap();
        let mut stats = Stats::default();
        stats.record_skipped();
        for _ in 0..3 {
            stats.save_to(&path);
            assert!(stats.dirty);
        }
        stats.record_postponed();
        stats.save_to(&path);
        assert!(stats.dirty);
        std::fs::remove_dir(tmp).unwrap();
        stats.save_to(&path);
        assert!(!stats.dirty);
        let saved: Stats = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!((saved.skipped, saved.postponed), (1, 1));
    }

    #[test]
    fn clean_host_guard_does_not_write_and_explicit_save_still_works() {
        let dir = TempDir::new();
        let path = dir.0.join("stats.toml");
        let mut stats = Stats::default();
        if stats.dirty {
            stats.save_to(&path);
        }
        assert!(!path.exists());
        // save itself has historically been unconditional; retain that API.
        stats.save_to(&path);
        assert!(path.exists());
        assert!(!stats.dirty);
    }

    #[test]
    fn failed_save_does_not_mark_clean_stats_dirty() {
        let dir = TempDir::new();
        let path = dir.0.join("stats.toml");
        std::fs::create_dir(path.with_extension("toml.tmp")).unwrap();
        let mut stats = Stats::default();
        stats.save_to(&path);
        assert!(!stats.dirty);
    }

    #[test]
    fn automatic_retry_is_throttled_and_recovers_without_a_new_event() {
        use std::time::{Duration, Instant};
        let dir = TempDir::new();
        let path = dir.0.join("stats.toml");
        let tmp = path.with_extension("toml.tmp");
        std::fs::create_dir(&tmp).unwrap();
        let mut stats = Stats::default();
        stats.record_skipped();
        let mut retry = StatsSaveRetry::default();
        let start = Instant::now();
        let mut attempts = 0;
        let mut save = |s: &mut Stats| {
            attempts += 1;
            s.save_to(&path);
        };
        retry.save_if_due_with(&mut stats, start, &mut save);
        for frame in 1..250 {
            retry.save_if_due_with(
                &mut stats,
                start + Duration::from_millis(frame * 20),
                &mut save,
            );
        }
        // An earlier supplied timestamp neither panics nor bypasses the gate.
        retry.save_if_due_with(&mut stats, start - Duration::from_secs(1), &mut save);
        assert!(stats.dirty);
        std::fs::remove_dir(tmp).unwrap();
        retry.save_if_due_with(&mut stats, start + Duration::from_secs(5), &mut save);
        assert!(!stats.dirty);
        // A new event following a successful save is not artificially delayed.
        stats.record_postponed();
        retry.save_if_due_with(&mut stats, start + Duration::from_millis(5020), &mut save);
        assert!(!stats.dirty);
        assert_eq!(attempts, 3);
        let saved: Stats = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!((saved.skipped, saved.postponed), (1, 1));
    }

    #[test]
    fn repeated_failures_get_new_cooldown_and_explicit_exit_save_bypasses_it() {
        use std::time::{Duration, Instant};
        let dir = TempDir::new();
        let path = dir.0.join("stats.toml");
        let tmp = path.with_extension("toml.tmp");
        std::fs::create_dir(&tmp).unwrap();
        let mut stats = Stats::default();
        stats.record_skipped();
        let mut retry = StatsSaveRetry::default();
        let start = Instant::now();
        let mut attempts = 0;
        for millis in [0, 20, 5000, 5020, 9980] {
            retry.save_if_due_with(&mut stats, start + Duration::from_millis(millis), |s| {
                attempts += 1;
                s.save_to(&path);
            });
        }
        assert_eq!(attempts, 2);
        assert!(stats.dirty);
        std::fs::remove_dir(tmp).unwrap();
        // The application shutdown hook runs without another runtime step.
        stats.flush_if_dirty_with(|s| s.save_to(&path));
        assert!(!stats.dirty);
        stats.flush_if_dirty_with(|_| panic!("successful final save must not repeat"));
        retry.save_if_due_with(&mut stats, start + Duration::from_millis(9990), |_| {
            panic!("clean statistics must not be written");
        });
        assert!(retry.retry_after.is_none());
    }

    #[test]
    fn any_unsaved_result_including_serialization_failure_is_throttled() {
        use std::time::{Duration, Instant};
        let mut stats = Stats {
            dirty: true,
            ..Default::default()
        };
        let mut retry = StatsSaveRetry::default();
        let start = Instant::now();
        // Stats currently contains only TOML-compatible strings/u32 fields.
        // Model a save that cannot serialize and leaves pending data unchanged.
        retry.save_if_due_with(&mut stats, start, |_| {});
        retry.save_if_due_with(&mut stats, start + Duration::from_secs(1), |_| {
            panic!("an unsuccessful save must be throttled");
        });
        assert!(stats.dirty);
        retry.save_if_due_with(&mut stats, start + Duration::from_secs(5), |s| {
            s.dirty = false;
        });
        assert!(retry.retry_after.is_none());
    }

    #[test]
    fn final_flush_failure_keeps_pending_data_without_looping() {
        let dir = TempDir::new();
        let path = dir.0.join("stats.toml");
        std::fs::create_dir(path.with_extension("toml.tmp")).unwrap();
        let mut stats = Stats::default();
        stats.record_skipped();
        let mut attempts = 0;
        stats.flush_if_dirty_with(|s| {
            attempts += 1;
            s.save_to(&path);
        });
        assert_eq!(attempts, 1);
        assert!(stats.dirty);
    }

    #[test]
    fn final_flush_of_clean_stats_never_calls_writer() {
        Stats::default().flush_if_dirty_with(|_| panic!("no pending data"));
    }

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

#[cfg(test)]
mod atomic_write_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_path() -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "eyeflow_stats_atomic_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("stats.toml")
    }

    #[test]
    fn replacement_failure_preserves_existing_file() {
        let path = temp_path();
        std::fs::write(&path, b"old complete data").unwrap();
        let err = write_atomic_with_replace(&path, b"new complete data", |tmp, target| {
            assert_eq!(std::fs::read(tmp).unwrap(), b"new complete data");
            assert_eq!(std::fs::read(target).unwrap(), b"old complete data");
            Err(std::io::Error::other("injected replacement failure"))
        })
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Other);
        assert_eq!(std::fs::read(&path).unwrap(), b"old complete data");
        assert!(!path.with_extension("toml.tmp").exists());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn repeated_replacement_and_initial_creation_work() {
        let path = temp_path();
        for contents in [
            b"first".as_slice(),
            b"second longer".as_slice(),
            b"third".as_slice(),
        ] {
            write_atomic(&path, contents).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), contents);
            assert!(!path.with_extension("toml.tmp").exists());
        }
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
