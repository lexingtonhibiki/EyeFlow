use rand::Rng;
use std::time::{Duration, Instant};

/// 加权随机间隔提醒调度器
///
/// 提醒间隔服从三角分布：大多数落在中间区间，偶尔靠近边界。
/// 避免机械式的固定间隔感。
pub struct Scheduler {
    min_secs: f64,
    max_secs: f64,
    next_remind_at: Instant,
}

impl Scheduler {
    pub fn new(min_secs: u64, max_secs: u64) -> Self {
        let min = min_secs as f64;
        let max = max_secs as f64;
        let mut s = Self {
            min_secs: min.max(1.0),
            max_secs: max.max(min + 1.0),
            next_remind_at: Instant::now(),
        };
        s.schedule_next();
        s
    }

    /// 三角分布加权随机间隔
    fn weighted_random(&self) -> Duration {
        let mut rng = rand::rng();
        let range = self.max_secs - self.min_secs;

        // 两个随机数的平均 -> 集中在中间
        let a: f64 = rng.random();
        let b: f64 = rng.random();
        let weighted = (a + b) / 2.0;

        let secs = self.min_secs + weighted * range;
        Duration::from_secs_f64(secs)
    }

    fn schedule_next(&mut self) {
        self.next_remind_at = Instant::now() + self.weighted_random();
    }

    /// 是否该提醒了
    pub fn should_remind(&self) -> bool {
        Instant::now() >= self.next_remind_at
    }

    /// 重置计时器（重新计算下一次提醒时间）
    pub fn reset(&mut self) {
        self.schedule_next();
    }

    /// 更新间隔范围
    pub fn set_interval(&mut self, min_secs: u64, max_secs: u64) {
        self.min_secs = (min_secs as f64).max(1.0);
        self.max_secs = (max_secs as f64).max(self.min_secs + 1.0);
        self.schedule_next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_creates() {
        let s = Scheduler::new(600, 1080);
        // Should schedule into the future
        assert!(s.next_remind_at > Instant::now());
    }

    #[test]
    fn test_reset_sets_future() {
        let mut s = Scheduler::new(600, 1080);
        let before = s.next_remind_at;
        s.reset();
        assert!(s.next_remind_at >= before);
    }

    #[test]
    fn test_should_remind_initially_false() {
        let s = Scheduler::new(600, 1080);
        assert!(!s.should_remind());
    }
}
