//! 调度核心：上下文状态、提醒计时、预告/休息阶段机、用户动作与统计结算。
//!
//! 纯逻辑，不触碰 Win32 / egui，所有时间由调用方以 `Instant` 传入，便于单元测试。
//! 规则来源：docs/adr/0001（默认节奏）、0002（顺延而非丢弃）、0003（温和默认）。

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::stats::{Completed, Stats};

/// 用户当下处境（见 CONTEXT.md）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextState {
    Desktop,
    Flow,
    Gaming,
    Away,
}

impl ContextState {
    pub fn label(self) -> &'static str {
        match self {
            ContextState::Desktop => "桌面",
            ContextState::Flow => "心流",
            ContextState::Gaming => "游戏 / 全屏",
            ContextState::Away => "离开",
        }
    }
}

/// 传感器线程每秒采样一次的快照
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sensors {
    /// 自上次任何输入（键鼠）以来的秒数
    pub idle_secs: u64,
    /// 前台窗口是否全屏且无标题栏
    pub fullscreen: bool,
    /// 系统认为此刻可以打扰用户（SHQueryUserNotificationState == ACCEPTS）
    pub interruptible: bool,
}

impl Default for Sensors {
    fn default() -> Self {
        Self {
            idle_secs: 0,
            fullscreen: false,
            interruptible: true,
        }
    }
}

/// 提醒周期的阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    HeadsUp {
        started: Instant,
        ends: Instant,
        is_long: bool,
    },
    Break {
        started: Instant,
        ends: Instant,
        is_long: bool,
    },
}

/// 核心要求宿主执行的副作用
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// 播放提示音（预设由配置决定）
    PlayCue,
}

/// 停歇多久视为心流已中断，可以投递提醒
const FLOW_PAUSE: Duration = Duration::from_secs(8);
/// 心流滑窗长度
const FLOW_WINDOW: Duration = Duration::from_secs(30);
/// 长休息预告时长
const LONG_HEADS_UP: Duration = Duration::from_secs(30);
/// 长休息被跳过后的再提示间隔
const LONG_REPROMPT: Duration = Duration::from_secs(600);
/// 离开多久后清零用屏累计（AOA：2 小时后休息 15 分钟）
const AWAY_RESETS_ACCUM: Duration = Duration::from_secs(15 * 60);
/// 暂停 / 免打扰结束后，若早已到点，多久后投递
const RESUME_GRACE: Duration = Duration::from_secs(60);

pub struct Core {
    pub cfg: Config,
    pub stats: Stats,
    pub state: ContextState,
    pub phase: Phase,
    pub sensors: Sensors,
    /// 用户主动“暂停”的截止时间
    pub paused_until: Option<Instant>,
    /// 自上次长休息 / 长时间离开以来的活跃用屏时长
    pub screen_accum: Duration,
    /// 休息面板轮换到第几条贴士
    pub tip_index: usize,

    due: Instant,
    last_tick: Instant,
    keys: VecDeque<Instant>,
    last_key: Option<Instant>,
    postponed_this_round: bool,
    cue_played_this_round: bool,
    away_since: Option<Instant>,
    natural_counted: bool,
}

impl Core {
    pub fn new(cfg: Config, stats: Stats, now: Instant) -> Self {
        let mut core = Self {
            cfg,
            stats,
            state: ContextState::Desktop,
            phase: Phase::Idle,
            sensors: Sensors::default(),
            paused_until: None,
            screen_accum: Duration::ZERO,
            tip_index: 0,
            due: now,
            last_tick: now,
            keys: VecDeque::with_capacity(256),
            last_key: None,
            postponed_this_round: false,
            cue_played_this_round: false,
            away_since: None,
            natural_counted: false,
        };
        core.schedule_next(now);
        core
    }

    // ---------------------------------------------------------------- 输入

    pub fn on_key(&mut self, at: Instant) {
        self.keys.push_back(at);
        self.last_key = Some(at);
        self.prune_keys(at);
    }

    pub fn set_sensors(&mut self, s: Sensors) {
        self.sensors = s;
    }

    /// 替换配置；间隔变化时重新排期，其余即时生效。
    pub fn apply_config(&mut self, cfg: Config, now: Instant) {
        let interval_changed = (cfg.short_break_min_secs, cfg.short_break_max_secs)
            != (self.cfg.short_break_min_secs, self.cfg.short_break_max_secs);
        self.cfg = cfg;
        if interval_changed {
            self.schedule_next(now);
        }
    }

    /// 把下一次到点直接排在 `d` 之后（演示 / 调试用，`EYEFLOW_DEMO`）。
    pub fn set_due_in(&mut self, now: Instant, d: Duration) {
        self.due = now + d;
        self.postponed_this_round = false;
        self.cue_played_this_round = false;
    }

    // ------------------------------------------------------------ 用户动作

    /// 立即开始休息（热键 / 托盘 / 预告浮窗“现在开始”）
    pub fn start_break_now(&mut self, now: Instant) {
        let is_long = match self.phase {
            Phase::HeadsUp { is_long, .. } => is_long,
            Phase::Break { .. } => return,
            Phase::Idle => self.long_break_due(),
        };
        self.begin_break(now, is_long);
    }

    /// 延后一次：短休息与长休息都允许，但每个提醒回合只能延后一次（已延后过返回 false）。
    /// 长休息延后后到点仍是长休息（用屏累计未清零），跳过则 10 分钟后再提示。
    pub fn postpone(&mut self, now: Instant) -> bool {
        if !matches!(self.phase, Phase::HeadsUp { .. }) || self.postponed_this_round {
            return false;
        }
        self.postponed_this_round = true;
        self.phase = Phase::Idle;
        self.due = now + Duration::from_secs(self.cfg.postpone_secs);
        self.cue_played_this_round = false;
        self.stats.record_postponed();
        true
    }

    pub fn can_postpone(&self) -> bool {
        matches!(self.phase, Phase::HeadsUp { .. }) && !self.postponed_this_round
    }

    /// 跳过本次休息（预告阶段或休息进行中）
    pub fn skip(&mut self, now: Instant) {
        let is_long = match self.phase {
            Phase::HeadsUp { is_long, .. } | Phase::Break { is_long, .. } => is_long,
            Phase::Idle => return,
        };
        self.stats.record_skipped();
        self.phase = Phase::Idle;
        if is_long {
            // 2 小时是硬性底线（AOA），跳过后 10 分钟再提示
            self.due = now + LONG_REPROMPT;
            self.cue_played_this_round = false;
            self.postponed_this_round = false;
        } else {
            self.schedule_next(now);
        }
    }

    /// 暂停提醒一段时间；进行中的阶段静默取消，不计统计。
    pub fn pause_for(&mut self, now: Instant, duration: Duration) {
        self.paused_until = Some(now + duration);
        self.phase = Phase::Idle;
    }

    pub fn resume(&mut self) {
        self.paused_until = None;
    }

    pub fn is_paused(&self, now: Instant) -> bool {
        self.paused_until.is_some_and(|t| now < t)
    }

    // ---------------------------------------------------------------- 查询

    pub fn next_break_in(&self, now: Instant) -> Duration {
        self.due.saturating_duration_since(now)
    }

    pub fn long_break_due(&self) -> bool {
        self.cfg.long_break_enabled
            && self.screen_accum >= Duration::from_secs(self.cfg.long_break_after_secs)
    }

    pub fn flow_active(&self, now: Instant) -> bool {
        let threshold = self.cfg.flow_sensitivity.threshold() as usize;
        self.keys
            .iter()
            .filter(|t| now.saturating_duration_since(**t) <= FLOW_WINDOW)
            .count()
            >= threshold
    }

    // ---------------------------------------------------------------- 节拍

    /// 每秒调用一次。`quiet` 为“当前处于免打扰时段”。
    pub fn tick(&mut self, now: Instant, quiet: bool) -> Vec<Action> {
        let mut actions = Vec::new();
        let dt = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        self.prune_keys(now);

        // 1. 上下文状态
        let idle = Duration::from_secs(self.sensors.idle_secs);
        self.state = if idle >= Duration::from_secs(self.cfg.away_secs) {
            ContextState::Away
        } else if self.sensors.fullscreen || !self.sensors.interruptible {
            ContextState::Gaming
        } else if self.flow_active(now) {
            ContextState::Flow
        } else {
            ContextState::Desktop
        };

        // 2. 离开：计时暂停；进入离开状态即视为一次自然休息（离开阈值远大于休息时长）；
        //    离开 15 分钟清零用屏累计
        if self.state == ContextState::Away {
            if self.away_since.is_none() {
                self.away_since = Some(now.checked_sub(idle).unwrap_or(now));
                self.natural_counted = false;
            }
            if matches!(self.phase, Phase::Idle) {
                self.due += dt;
            }
            if !self.natural_counted {
                self.natural_counted = true;
                if matches!(self.phase, Phase::HeadsUp { .. }) {
                    self.phase = Phase::Idle;
                }
                self.stats.record_completed(Completed::Natural);
                self.schedule_next(now);
            }
            if idle >= AWAY_RESETS_ACCUM {
                self.screen_accum = Duration::ZERO;
            }
            return actions;
        }
        self.away_since = None;
        self.screen_accum += dt;

        // 3. 暂停 / 免打扰 / 总开关：不投递，到点则推到恢复后 60 秒
        if !self.cfg.enabled || quiet || self.is_paused(now) {
            if !matches!(self.phase, Phase::Idle) {
                self.phase = Phase::Idle;
            }
            if self.due <= now + self.heads_up_len(self.long_break_due()) {
                self.due = now + self.heads_up_len(self.long_break_due()) + RESUME_GRACE;
            }
            return actions;
        }
        if self.paused_until.is_some() {
            self.paused_until = None;
        }

        // 4. 阶段机
        match self.phase {
            Phase::Idle => {
                let is_long = self.long_break_due();
                let heads_up_at = self.due - self.heads_up_len(is_long);
                if now < heads_up_at {
                    return actions;
                }
                match self.state {
                    ContextState::Gaming => {
                        // 只播声音，视觉顺延到退出全屏后补发
                        if now >= self.due && !self.cue_played_this_round {
                            self.cue_played_this_round = true;
                            actions.push(Action::PlayCue);
                        }
                    }
                    ContextState::Flow => {
                        let paused_enough = self
                            .last_key
                            .is_none_or(|k| now.saturating_duration_since(k) >= FLOW_PAUSE);
                        if paused_enough {
                            self.start_heads_up(now, is_long, &mut actions);
                        }
                    }
                    ContextState::Desktop => self.start_heads_up(now, is_long, &mut actions),
                    ContextState::Away => unreachable!("away handled above"),
                }
            }
            Phase::HeadsUp { ends, is_long, .. } => {
                if now >= ends {
                    self.begin_break(now, is_long);
                }
            }
            Phase::Break { ends, is_long, .. } => {
                if now >= ends {
                    self.complete_break(now, is_long);
                    actions.push(Action::PlayCue);
                }
            }
        }
        actions
    }

    // ---------------------------------------------------------------- 内部

    fn heads_up_len(&self, is_long: bool) -> Duration {
        if is_long {
            LONG_HEADS_UP
        } else {
            Duration::from_secs(self.cfg.heads_up_secs)
        }
    }

    fn start_heads_up(&mut self, now: Instant, is_long: bool, actions: &mut Vec<Action>) {
        if !self.cfg.visual_enabled {
            // 仅声音模式：一声提示即视为投递完成
            if !self.cue_played_this_round {
                actions.push(Action::PlayCue);
            }
            self.stats.record_completed(if is_long {
                Completed::Long
            } else {
                Completed::Short
            });
            if is_long {
                self.screen_accum = Duration::ZERO;
            }
            self.schedule_next(now);
            return;
        }
        self.phase = Phase::HeadsUp {
            started: now,
            ends: now + self.heads_up_len(is_long),
            is_long,
        };
    }

    fn begin_break(&mut self, now: Instant, is_long: bool) {
        let len = if is_long {
            self.cfg.long_break_secs
        } else {
            self.cfg.short_break_secs
        };
        self.tip_index = self.tip_index.wrapping_add(1);
        self.phase = Phase::Break {
            started: now,
            ends: now + Duration::from_secs(len),
            is_long,
        };
    }

    fn complete_break(&mut self, now: Instant, is_long: bool) {
        self.stats.record_completed(if is_long {
            Completed::Long
        } else {
            Completed::Short
        });
        if is_long {
            self.screen_accum = Duration::ZERO;
        }
        self.phase = Phase::Idle;
        self.schedule_next(now);
    }

    fn schedule_next(&mut self, now: Instant) {
        self.due = now + self.sample_interval();
        self.postponed_this_round = false;
        self.cue_played_this_round = false;
    }

    /// 三角分布：两个均匀随机数取平均，峰值落在区间中点（默认 20 分钟）。
    fn sample_interval(&self) -> Duration {
        use rand::Rng;
        let min = self.cfg.short_break_min_secs.max(60) as f64;
        let max = (self.cfg.short_break_max_secs as f64).max(min);
        let mut rng = rand::rng();
        let a: f64 = rng.random();
        let b: f64 = rng.random();
        Duration::from_secs_f64(min + (a + b) / 2.0 * (max - min))
    }

    fn prune_keys(&mut self, now: Instant) {
        while let Some(&t) = self.keys.front() {
            if now.saturating_duration_since(t) > FLOW_WINDOW {
                self.keys.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: fn(u64) -> Duration = Duration::from_secs;

    fn cfg(interval: u64) -> Config {
        let mut c = Config::default();
        c.short_break_min_secs = interval;
        c.short_break_max_secs = interval; // 零宽区间 → 确定性排期
        c.heads_up_secs = 15;
        c.short_break_secs = 30;
        c.long_break_enabled = false;
        c
    }

    fn core(interval: u64, t0: Instant) -> Core {
        Core::new(cfg(interval), Stats::default(), t0)
    }

    #[test]
    fn desktop_full_cycle_heads_up_then_break_then_complete() {
        let t0 = Instant::now();
        let mut c = core(1200, t0);
        assert!(c.tick(t0 + S(1000), false).is_empty());
        assert_eq!(c.phase, Phase::Idle);

        c.tick(t0 + S(1185), false);
        assert!(matches!(c.phase, Phase::HeadsUp { is_long: false, .. }));

        c.tick(t0 + S(1201), false);
        assert!(matches!(c.phase, Phase::Break { is_long: false, .. }));

        let actions = c.tick(t0 + S(1232), false);
        assert_eq!(c.phase, Phase::Idle);
        assert_eq!(actions, vec![Action::PlayCue]);
        assert_eq!(c.stats.completed_short, 1);
        assert!(c.next_break_in(t0 + S(1232)) >= S(1199));
    }

    #[test]
    fn flow_defers_until_typing_pauses_and_never_drops() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.cfg.flow_sensitivity = crate::config::FlowSensitivity::Low; // 40 键
        let t_due = t0 + S(600);
        for i in 0..50 {
            c.on_key(t_due - S(20) + Duration::from_millis(i * 100));
        }
        // 到点时仍在打字（最后一键 ~15s 前... 用 3s 前再补一键）
        c.on_key(t_due - S(3));
        c.tick(t_due, false);
        assert_eq!(c.state, ContextState::Flow);
        assert_eq!(c.phase, Phase::Idle, "心流中不投递");

        // 最后一键在 due-3s：停歇不足 8 秒时继续等待，满 8 秒后投递，而不是丢弃
        c.tick(t_due + S(4), false);
        assert_eq!(c.phase, Phase::Idle);
        c.tick(t_due + S(9), false);
        assert!(matches!(c.phase, Phase::HeadsUp { .. }), "停歇后必须补发");
    }

    #[test]
    fn gaming_plays_cue_once_then_catches_up_visually_after_exit() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.set_sensors(Sensors {
            fullscreen: true,
            ..Default::default()
        });
        assert!(c.tick(t0 + S(590), false).is_empty(), "预告点不出声");
        let a1 = c.tick(t0 + S(600), false);
        assert_eq!(a1, vec![Action::PlayCue]);
        assert_eq!(c.state, ContextState::Gaming);
        assert_eq!(c.phase, Phase::Idle);
        assert!(c.tick(t0 + S(700), false).is_empty(), "不重复出声");

        c.set_sensors(Sensors::default());
        let a2 = c.tick(t0 + S(701), false);
        assert!(a2.is_empty());
        assert!(
            matches!(c.phase, Phase::HeadsUp { .. }),
            "退出全屏后补发预告"
        );
    }

    #[test]
    fn not_interruptible_behaves_like_gaming() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.set_sensors(Sensors {
            interruptible: false,
            ..Default::default()
        });
        c.tick(t0 + S(600), false);
        assert_eq!(c.state, ContextState::Gaming);
        assert_eq!(c.phase, Phase::Idle);
    }

    #[test]
    fn away_pauses_timer_and_counts_natural_rest() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.tick(t0 + S(100), false);
        c.set_sensors(Sensors {
            idle_secs: 200,
            ..Default::default()
        });
        c.tick(t0 + S(300), false);
        assert_eq!(c.state, ContextState::Away);
        assert_eq!(c.stats.completed_natural, 1);
        // 计时器已重新排期到 600 秒后
        assert!(c.next_break_in(t0 + S(300)) >= S(599));

        c.set_sensors(Sensors::default());
        c.tick(t0 + S(301), false);
        assert_eq!(c.state, ContextState::Desktop);
        assert_eq!(c.stats.completed_natural, 1, "同一段离开只计一次");
    }

    #[test]
    fn long_away_resets_screen_accumulator() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        for i in 1..=30 {
            c.tick(t0 + S(i * 60), false);
        }
        assert!(c.screen_accum >= S(1700));
        c.set_sensors(Sensors {
            idle_secs: 16 * 60,
            ..Default::default()
        });
        c.tick(t0 + S(31 * 60), false);
        assert_eq!(c.screen_accum, Duration::ZERO);
    }

    #[test]
    fn postpone_allowed_once_per_round() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.tick(t0 + S(586), false);
        assert!(c.can_postpone());
        assert!(c.postpone(t0 + S(590)));
        assert_eq!(c.phase, Phase::Idle);
        assert_eq!(c.stats.postponed, 1);
        let due_in = c.next_break_in(t0 + S(590));
        assert!(due_in <= S(300) && due_in >= S(299));

        c.tick(t0 + S(590 + 286), false);
        assert!(matches!(c.phase, Phase::HeadsUp { .. }));
        assert!(!c.can_postpone());
        assert!(!c.postpone(t0 + S(880)));
    }

    #[test]
    fn skip_during_break_counts_as_skipped_and_reschedules() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.tick(t0 + S(586), false);
        c.tick(t0 + S(601), false);
        assert!(matches!(c.phase, Phase::Break { .. }));
        c.skip(t0 + S(610));
        assert_eq!(c.phase, Phase::Idle);
        assert_eq!(c.stats.skipped, 1);
        assert_eq!(c.stats.completed_short, 0);
        assert!(c.next_break_in(t0 + S(610)) >= S(599));
    }

    #[test]
    fn long_break_after_accumulated_screen_time_can_be_postponed_once() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.cfg.long_break_enabled = true;
        c.cfg.long_break_after_secs = 1800;
        c.cfg.long_break_secs = 900;
        // 两个完整短周期：累计 ≈ 1262s，尚未到 1800s
        let mut t = t0;
        for _ in 0..2 {
            t += S(586);
            c.tick(t, false);
            t += S(15);
            c.tick(t, false);
            t += S(30);
            c.tick(t, false);
        }
        assert_eq!(c.stats.completed_short, 2);
        assert!(!c.long_break_due(), "accum={:?}", c.screen_accum);
        // 下一次到点在 1862s；累计在此之前越过 1800s，因此预告应升级为长休息（提前 30s = 1832s）
        c.tick(t0 + S(1831), false);
        assert!(c.long_break_due());
        assert_eq!(
            c.phase,
            Phase::Idle,
            "长休息预告提前 30 秒，1831 时还差 1 秒"
        );
        c.tick(t0 + S(1833), false);
        assert!(matches!(c.phase, Phase::HeadsUp { is_long: true, .. }));
        let t = t0 + S(1833);

        // 长休息也允许延后一次（+5 分钟），到点仍是长休息
        assert!(c.can_postpone());
        assert!(c.postpone(t));
        assert_eq!(c.stats.postponed, 1);
        assert_eq!(c.phase, Phase::Idle);
        let wait = c.next_break_in(t);
        assert!(wait <= S(300) && wait >= S(299));
        let t2 = t + S(271);
        c.tick(t2, false);
        assert!(matches!(c.phase, Phase::HeadsUp { is_long: true, .. }));
        // 同一回合不能再延后
        assert!(!c.can_postpone());
        assert!(!c.postpone(t2));

        // 跳过长休息 → 10 分钟后再提示，仍是长休息
        c.skip(t2);
        assert_eq!(c.stats.skipped, 1);
        let again = c.next_break_in(t2);
        assert!(again <= S(600) && again >= S(599));
        c.tick(t2 + again - S(29), false);
        assert!(matches!(c.phase, Phase::HeadsUp { is_long: true, .. }));

        // 完成长休息 → 累计清零
        c.tick(t2 + again + S(1), false);
        assert!(matches!(c.phase, Phase::Break { is_long: true, .. }));
        c.tick(t2 + again + S(901), false);
        assert_eq!(c.stats.completed_long, 1);
        assert_eq!(c.screen_accum, Duration::ZERO);
    }

    #[test]
    fn quiet_hours_and_pause_block_delivery_then_grace_after() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.tick(t0 + S(700), true);
        assert_eq!(c.phase, Phase::Idle);
        assert!(c.next_break_in(t0 + S(700)) >= S(74));

        let mut c2 = core(600, t0);
        c2.pause_for(t0, S(3600));
        c2.tick(t0 + S(700), false);
        assert_eq!(c2.phase, Phase::Idle);
        assert!(c2.is_paused(t0 + S(700)));
        c2.tick(t0 + S(3601), false);
        assert!(!c2.is_paused(t0 + S(3601)));
    }

    #[test]
    fn sound_only_mode_delivers_cue_and_counts() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.cfg.visual_enabled = false;
        let a = c.tick(t0 + S(586), false);
        assert_eq!(a, vec![Action::PlayCue]);
        assert_eq!(c.phase, Phase::Idle);
        assert_eq!(c.stats.completed_short, 1);
    }

    #[test]
    fn start_break_now_from_idle() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.start_break_now(t0 + S(10));
        assert!(matches!(c.phase, Phase::Break { is_long: false, .. }));
        c.tick(t0 + S(41), false);
        assert_eq!(c.stats.completed_short, 1);
    }
}
