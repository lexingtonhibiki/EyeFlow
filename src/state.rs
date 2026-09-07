use std::time::{Duration, Instant};

/// EyeFlow 上下文状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextState {
    /// 日常桌面使用：系统通知 + 提示音
    Desktop,
    /// 编程心流：延后提醒，等停歇再触发
    Flow,
    /// 全屏游戏：仅提示音，不弹窗
    Gaming,
    /// 离开状态（3min 无输入）：重置计时器
    Away,
}

/// 全状态转换规则状态机
///
/// 状态转换凭据：
///   Gaming（全屏覆盖） > Away（3min 空闲） > Flow（超阈值） > Desktop
///
/// 转换由 detector 输入驱动（on_keyboard_activity, on_fullscreen_change, on_idle_timeout），
/// 然后在 tick(now) 中作内聚评估。外部不直接写状态值。
pub struct StateMachine {
    current: ContextState,
    /// 30 秒滑窗内按键计数
    flow_key_count: u32,
    /// 滑窗起始时间
    flow_window_start: Instant,
    /// 心流判定阈值（按键数/30s）
    flow_threshold: u32,
    /// 上次键盘活动时间
    last_activity: Instant,
    /// 是否全屏
    is_fullscreen: bool,
}

impl StateMachine {
    pub fn new(flow_threshold: u32) -> Self {
        let now = Instant::now();
        Self {
            current: ContextState::Desktop,
            flow_key_count: 0,
            flow_window_start: now,
            flow_threshold,
            last_activity: now,
            is_fullscreen: false,
        }
    }

    pub fn current(&self) -> ContextState {
        self.current
    }

    #[allow(dead_code)]
    pub fn last_activity(&self) -> Instant {
        self.last_activity
    }

    /// 更新阈值（当用户更改灵敏度设置时调用）
    pub fn set_flow_threshold(&mut self, threshold: u32) {
        self.flow_threshold = threshold;
    }

    // ---- 事件输入 ----

    pub fn on_keyboard_activity(&mut self) {
        let now = Instant::now();

        // 从 Away 回退到 Desktop
        if self.current == ContextState::Away {
            self.current = ContextState::Desktop;
        }

        self.last_activity = now;

        // 如果窗口已过期，重置滑窗
        if now.duration_since(self.flow_window_start) > Duration::from_secs(30) {
            self.flow_key_count = 0;
            self.flow_window_start = now;
        }

        self.flow_key_count += 1;
    }

    pub fn on_fullscreen_change(&mut self, is_fullscreen: bool) -> bool {
        let changed = self.is_fullscreen != is_fullscreen;
        self.is_fullscreen = is_fullscreen;
        changed
    }

    pub fn on_idle_timeout(&mut self) {
        self.current = ContextState::Away;
    }

    // ---- 周期评估 ----

    /// 每 ~1s 调用一次，根据当前输入做状态转换评估。
    /// 返回当前状态。
    pub fn tick(&mut self, now: Instant) -> ContextState {
        // 1. Gaming（全屏最高优先级）
        if self.is_fullscreen {
            self.current = ContextState::Gaming;
            return self.current;
        }

        let idle_duration = now.duration_since(self.last_activity);

        // 2. Away（3min 无输入）
        if idle_duration >= Duration::from_secs(180) {
            self.current = ContextState::Away;
            return self.current;
        }

        // 3. 如果之前是 Away 但不到 3min，回退到 Desktop
        if self.current == ContextState::Away {
            self.current = ContextState::Desktop;
            return self.current;
        }

        // 4. 重置过期滑窗
        if now.duration_since(self.flow_window_start) > Duration::from_secs(30) {
            self.flow_key_count = 0;
            self.flow_window_start = now;
        }

        // 5. Flow 判断
        if self.flow_key_count >= self.flow_threshold {
            self.current = ContextState::Flow;
        } else if self.current == ContextState::Flow {
            // 按键数低于阈值，回退到 Desktop
            self.current = ContextState::Desktop;
        } else {
            self.current = ContextState::Desktop;
        }

        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sm = StateMachine::new(10);
        assert_eq!(sm.current(), ContextState::Desktop);
    }

    #[test]
    fn test_fullscreen_enters_gaming() {
        let mut sm = StateMachine::new(10);
        sm.on_fullscreen_change(true);
        assert_eq!(sm.tick(Instant::now()), ContextState::Gaming);
    }

    #[test]
    fn test_fullscreen_exit_returns_desktop() {
        let mut sm = StateMachine::new(10);
        sm.on_fullscreen_change(true);
        sm.tick(Instant::now());
        sm.on_fullscreen_change(false);
        assert_eq!(sm.tick(Instant::now()), ContextState::Desktop);
    }

    #[test]
    fn test_idle_timeout_enters_away() {
        let mut sm = StateMachine::new(10);
        let start = Instant::now();
        sm.tick(start);
        sm.on_idle_timeout();
        assert_eq!(sm.current(), ContextState::Away);
    }

    #[test]
    fn test_away_to_desktop_on_keyboard() {
        let mut sm = StateMachine::new(10);
        sm.on_idle_timeout();
        assert_eq!(sm.current(), ContextState::Away);
        sm.on_keyboard_activity();
        assert_eq!(sm.current(), ContextState::Desktop);
    }

    #[test]
    fn test_flow_detection_via_tick() {
        let mut sm = StateMachine::new(3); // lowest threshold for test
        let start = Instant::now();

        // Simulate 5 key presses within 30s window
        for _ in 0..5 {
            sm.on_keyboard_activity();
        }

        sm.tick(start);
        assert_eq!(sm.current(), ContextState::Flow);
    }

    #[test]
    fn test_flow_expires_to_desktop() {
        let mut sm = StateMachine::new(3);
        let start = Instant::now();

        for _ in 0..5 {
            sm.on_keyboard_activity();
        }

        sm.tick(start);
        assert_eq!(sm.current(), ContextState::Flow);

        // After window expires and no keys, should go back to Desktop
        let later = start + Duration::from_secs(31);
        assert_eq!(sm.tick(later), ContextState::Desktop);
    }

    #[test]
    fn test_gaming_overrides_away() {
        let mut sm = StateMachine::new(10);
        sm.on_fullscreen_change(true);
        sm.on_idle_timeout();
        // Gaming should take precedence even if idle
        assert_eq!(sm.tick(Instant::now()), ContextState::Gaming);
    }

    #[test]
    fn test_set_flow_threshold() {
        let mut sm = StateMachine::new(10);
        sm.set_flow_threshold(20);
        // Private field, check via behavior
        assert_eq!(sm.current(), ContextState::Desktop);
    }
}
