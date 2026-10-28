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
    /// 文案在 locale 表里（`core.state_*`），本枚举只给出 key。
    ///
    /// `label()` 原本直接返回中文字面量。改成 key 之后 `core.rs` 不再需要
    /// 引用 i18n 层，14 个调度测试仍然是纯逻辑、与语言无关（ADR-0008）。
    pub fn key(self) -> &'static str {
        match self {
            ContextState::Desktop => "core.state_desktop",
            ContextState::Flow => "core.state_flow",
            ContextState::Gaming => "core.state_gaming",
            ContextState::Away => "core.state_away",
        }
    }
}

use crate::platform::sensors::Reading;
pub use crate::platform::sensors::SensorSnapshot as Sensors;

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
/// 相邻两次 tick 超过该间隔视为经历了睡眠 / 挂起。
/// 10 分钟以内的挂起较罕见且只影响一条预告；完整的电源事件桥在 Roadmap。
const SLEEP_GAP: Duration = Duration::from_secs(600);
/// 休息完成后的“欢迎回来”闪屏时长
const COMPLETED_FLASH: Duration = Duration::from_secs(3);
/// 预告浮窗出现前多久开始预热 UI 会话：宿主提前把 GL 上下文 / 字体 / 着色器
/// 建好，让「到点响一声」与「浮窗可见」落在同一帧（v0.7.1 观感修复）。
const UI_WARMUP: Duration = Duration::from_secs(5);

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
    /// 刚完成一次休息的“欢迎回来”闪屏截止时间（None = 无）
    pub completed_flash_until: Option<Instant>,

    due: Instant,
    last_tick: Instant,
    /// 最近一次 `tick` 传入的免打扰状态（`ui_warmup_in` 要用：免打扰里
    /// 到点会被顺延，此时预热出的会话 5 秒后就得退出，纯属白开）。
    last_quiet: bool,
    monitoring_ready: bool,
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
            sensors: Sensors::unavailable(now, "Waiting for the first native sensor sample"),
            paused_until: None,
            screen_accum: Duration::ZERO,
            tip_index: 0,
            completed_flash_until: None,
            due: now,
            last_tick: now,
            last_quiet: false,
            monitoring_ready: false,
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

    /// Display this reason rather than representing unavailable native data as Desktop.
    pub fn automatic_pause_reason(&self, now: Instant) -> Option<&'static str> {
        if !self.cfg.compatibility_reminders {
            return self.sensors.automatic_pause_reason(now);
        }
        if !self.sensors.is_fresh(now) {
            return Some("Sensor reading is stale or has an invalid timestamp");
        }
        if let Some(reason) = self.sensors.idle.reason() {
            return Some(reason);
        }
        if self.sensors.interruptible == Reading::Known(false) {
            return Some("The desktop session is not interruptible");
        }
        // Only explicitly unsupported capabilities, not a blanket Unknown bypass.
        // Other Unavailable readings may mean a disconnected session or malformed
        // native data. Failed and PermissionRequired always remain blockers.
        let fullscreen_reason = match self.sensors.fullscreen {
            Reading::Unsupported(_) => None,
            other => other.reason(),
        };
        let session_reason = match self.sensors.interruptible {
            Reading::Unsupported(_) => None,
            other => other.reason(),
        };
        fullscreen_reason.or(session_reason)
    }

    pub fn set_sensors(&mut self, s: Sensors) {
        self.sensors = s;
    }

    /// 替换配置；间隔变化时重新排期，其余即时生效。
    pub fn apply_config(&mut self, cfg: Config, now: Instant) {
        let interval_changed = (cfg.short_break_min_secs, cfg.short_break_max_secs)
            != (self.cfg.short_break_min_secs, self.cfg.short_break_max_secs);
        let policy_changed = cfg.compatibility_reminders != self.cfg.compatibility_reminders;
        self.cfg = cfg;
        if policy_changed {
            // Changing trust policy cannot retroactively establish observed use.
            self.monitoring_ready = false;
            self.last_tick = now;
            // A policy change is not evidence that the user returned. Preserve
            // the existing Away episode and its credit until actual active input.
            self.keys.clear();
            self.last_key = None;
            if matches!(self.phase, Phase::HeadsUp { .. }) {
                self.phase = Phase::Idle;
            }
        }
        if interval_changed || policy_changed {
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

    /// 下一次预告浮窗应该出现的时刻（`due` 提前一个预告时长）。
    fn next_heads_up_at(&self) -> Instant {
        let is_long = self.long_break_due();
        self.due
            .checked_sub(self.heads_up_len(is_long))
            .unwrap_or(self.due)
    }

    /// 是否该为即将出现的预告浮窗**预热** UI 会话；窗口内返回「还有多久出现」。
    ///
    /// 宿主（轻量循环）据此提前进入 eframe 会话：GL 上下文、字体、着色器都在
    /// 这里建好，`Phase::HeadsUp` 一到就能在**同一帧**把浮窗画出来，而不是
    /// 「先响一声、随后窗口才从白底上一点点构建出来」。
    ///
    /// 窗口故意开得很窄（[`UI_WARMUP`]，5 秒），并且只在真的会按点投递时打开：
    /// 一旦错过（心流 / 全屏 / 暂停里暂不投递），浮窗时刻已经到了身后，这里
    /// 立刻回到 `None`，会话随之退出、GL 上下文释放——**不能为了预热让
    /// 上下文常驻**。手动触发（热键 / 托盘）没有提前量，仍然走冷启动路径。
    pub fn ui_warmup_in(&self, now: Instant) -> Option<Duration> {
        if self.automatic_pause_reason(now).is_some() {
            return None;
        }
        if !self.cfg.enabled || !self.cfg.visual_enabled || self.is_paused(now) {
            return None;
        }
        // 免打扰里到点会被顺延（见 tick 第 3 步），此刻预热出的会话很快就要退出
        if self.last_quiet {
            return None;
        }
        if !matches!(self.phase, Phase::Idle) {
            return None;
        }
        self.next_heads_up_at()
            .checked_duration_since(now)
            .filter(|d| *d <= UI_WARMUP)
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
        self.last_quiet = quiet;
        self.prune_keys(now);

        // A break already started by the user/scheduler owns its countdown.
        // Resolve it before sensor recovery/Away handling, otherwise a legitimate
        // idle reading can strand a manual break forever or miscount it as natural.
        if let Phase::Break { ends, is_long, .. } = self.phase {
            let observed = self.automatic_pause_reason(now).is_none();
            if observed
                && self.monitoring_ready
                && dt <= SLEEP_GAP
                && matches!(self.sensors.idle, Reading::Known(idle) if idle < Duration::from_secs(self.cfg.away_secs))
            {
                // Preserve active-use accounting while a user keeps working
                // through a break; unknown/idle periods are never added.
                self.screen_accum += dt;
            }
            self.monitoring_ready = observed;
            if !self.cfg.enabled || quiet || self.is_paused(now) {
                self.phase = Phase::Idle;
            } else if now >= ends {
                self.complete_break(now, is_long);
                actions.push(Action::PlayCue);
            }
            return actions;
        }

        // Unknown or stale native state cannot establish continuous screen use.
        // Cancel an automatic pre-warning, but allow an already-started (including
        // manually requested) break to finish. Do not invent a natural break credit.
        if self.automatic_pause_reason(now).is_some() {
            // Other capabilities may be unavailable while a locked session still
            // reports a fresh, known idle duration. That independent observation
            // can end continuous use, but does not establish a natural-break credit
            // or permission to deliver reminders.
            if self.sensors.is_fresh(now)
                && matches!(self.sensors.idle, Reading::Known(idle) if idle >= AWAY_RESETS_ACCUM)
            {
                self.screen_accum = Duration::ZERO;
            }
            self.monitoring_ready = false;
            self.due += dt;
            if matches!(self.phase, Phase::HeadsUp { .. }) {
                self.phase = Phase::Idle;
            }
            if !self.cfg.enabled || quiet || self.is_paused(now) {
                self.phase = Phase::Idle;
                return actions;
            }
            return actions;
        }
        if !self.monitoring_ready {
            // The interval before recovery was unobserved. Never count it as active
            // use or immediately deliver an overdue automatic reminder.
            self.monitoring_ready = true;
            self.due += dt;
            return actions;
        }

        // 0. 睡眠 / 挂起会把整段时长算进 dt（Windows 计时含睡眠时间）。
        //    超过 10 分钟视为“离席归来”：当作一次自然休息，清零用屏累计并顺延，
        //    避免醒来瞬间被轰炸预告、或误升 15 分钟长休息。
        if dt > SLEEP_GAP {
            self.screen_accum = Duration::ZERO;
            self.phase = Phase::Idle;
            self.stats.record_completed(Completed::Natural);
            self.schedule_next(now);
            return actions;
        }

        // 1. 上下文状态
        let Reading::Known(idle) = self.sensors.idle else {
            unreachable!("unknown readings are handled above")
        };
        self.state = if idle >= Duration::from_secs(self.cfg.away_secs) {
            ContextState::Away
        } else if self.sensors.fullscreen == Reading::Known(true)
            || self.sensors.interruptible == Reading::Known(false)
        {
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
        // 预告浮窗弹出时的提示音（`heads_up_cue_enabled`，默认开）。
        // 这一声与「结束音」「全屏时的唯一通道音」是三回事，不共用
        // `cue_played_this_round` 的**含义**，但要共用它的**状态**：
        // 游戏 / 全屏里到点时那一声（声音是唯一通道）已经替这一回合
        // 「响过了」，退出全屏后补发的预告不该再响第二声
        // （`gaming_plays_cue_once_then_catches_up_visually_after_exit` 钉着这条）。
        // `sound_enabled` 是总开关，由宿主的 `Action::PlayCue` 分支把关。
        if self.cfg.heads_up_cue_enabled && !self.cue_played_this_round {
            actions.push(Action::PlayCue);
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
        // An immediate Away reading can describe the same rest just completed.
        // Do not credit it again as natural until active use has been observed.
        self.away_since = Some(now);
        self.natural_counted = true;
        self.stats.record_completed(if is_long {
            Completed::Long
        } else {
            Completed::Short
        });
        if is_long {
            self.screen_accum = Duration::ZERO;
        }
        self.completed_flash_until = Some(now + COMPLETED_FLASH);
        self.phase = Phase::Idle;
        self.schedule_next(now);
    }

    /// 休息刚完成的“欢迎回来”闪屏是否仍应显示。
    pub fn flash_active(&self, now: Instant) -> bool {
        self.completed_flash_until.is_some_and(|t| now < t)
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

    fn known_sensors(now: Instant) -> Sensors {
        Sensors {
            sampled_at: now,
            idle: Reading::Known(Duration::ZERO),
            fullscreen: Reading::Known(false),
            interruptible: Reading::Known(true),
        }
    }

    // Existing scheduler scenarios assume the detector continues sampling between
    // assertions. Freshness and unknown-data tests below call tick directly.
    fn sampled_warmup(c: &mut Core, now: Instant) -> Option<Duration> {
        c.sensors.sampled_at = now;
        c.ui_warmup_in(now)
    }

    fn sampled_tick(c: &mut Core, now: Instant, quiet: bool) -> Vec<Action> {
        c.sensors.sampled_at = now;
        c.tick(now, quiet)
    }

    fn cfg(interval: u64) -> Config {
        Config {
            short_break_min_secs: interval,
            short_break_max_secs: interval, // 零宽区间 → 确定性排期
            heads_up_secs: 15,
            short_break_secs: 30,
            long_break_enabled: false,
            ..Config::default()
        }
    }

    fn core(interval: u64, t0: Instant) -> Core {
        {
            let mut c = Core::new(cfg(interval), Stats::default(), t0);
            c.set_sensors(known_sensors(t0));
            sampled_tick(&mut c, t0, false);
            c
        }
    }

    #[test]
    fn desktop_full_cycle_heads_up_then_break_then_complete() {
        let t0 = Instant::now();
        let mut c = core(1200, t0);
        // 相邻 tick 间隔都保持在 10 分钟内（否则被判定为睡眠 / 挂起）
        assert!(sampled_tick(&mut c, t0 + S(100), false).is_empty());
        assert!(sampled_tick(&mut c, t0 + S(500), false).is_empty());
        assert!(sampled_tick(&mut c, t0 + S(1000), false).is_empty());
        assert_eq!(c.phase, Phase::Idle);

        sampled_tick(&mut c, t0 + S(1185), false);
        assert!(matches!(c.phase, Phase::HeadsUp { is_long: false, .. }));

        sampled_tick(&mut c, t0 + S(1201), false);
        assert!(matches!(c.phase, Phase::Break { is_long: false, .. }));

        let actions = sampled_tick(&mut c, t0 + S(1232), false);
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
        sampled_tick(&mut c, t_due, false);
        assert_eq!(c.state, ContextState::Flow);
        assert_eq!(c.phase, Phase::Idle, "心流中不投递");

        // 最后一键在 due-3s：停歇不足 8 秒时继续等待，满 8 秒后投递，而不是丢弃
        sampled_tick(&mut c, t_due + S(4), false);
        assert_eq!(c.phase, Phase::Idle);
        sampled_tick(&mut c, t_due + S(9), false);
        assert!(matches!(c.phase, Phase::HeadsUp { .. }), "停歇后必须补发");
    }

    #[test]
    fn gaming_plays_cue_once_then_catches_up_visually_after_exit() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.set_sensors(Sensors {
            fullscreen: Reading::Known(true),
            ..known_sensors(t0)
        });
        assert!(
            sampled_tick(&mut c, t0 + S(590), false).is_empty(),
            "预告点不出声"
        );
        let a1 = sampled_tick(&mut c, t0 + S(600), false);
        assert_eq!(a1, vec![Action::PlayCue]);
        assert_eq!(c.state, ContextState::Gaming);
        assert_eq!(c.phase, Phase::Idle);
        assert!(
            sampled_tick(&mut c, t0 + S(700), false).is_empty(),
            "不重复出声"
        );

        c.set_sensors(known_sensors(t0));
        let a2 = sampled_tick(&mut c, t0 + S(701), false);
        // 补发预告**不再出声**：这一回合的那一声已经在全屏里响过（v0.7.0 的
        // `heads_up_cue_enabled` 用 `cue_played_this_round` 兜住这条，别把这里
        // 改成允许第二声——用户会听到同一个提醒响两遍）。
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
            interruptible: Reading::Known(false),
            ..known_sensors(t0)
        });
        sampled_tick(&mut c, t0 + S(600), false);
        assert_eq!(c.state, ContextState::Gaming);
        assert_eq!(c.phase, Phase::Idle);
    }

    #[test]
    fn away_pauses_timer_and_counts_natural_rest() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        sampled_tick(&mut c, t0 + S(100), false);
        c.set_sensors(Sensors {
            idle: Reading::Known(S(200)),
            ..known_sensors(t0)
        });
        sampled_tick(&mut c, t0 + S(300), false);
        assert_eq!(c.state, ContextState::Away);
        assert_eq!(c.stats.completed_natural, 1);
        // 计时器已重新排期到 600 秒后
        assert!(c.next_break_in(t0 + S(300)) >= S(599));

        c.set_sensors(known_sensors(t0));
        sampled_tick(&mut c, t0 + S(301), false);
        assert_eq!(c.state, ContextState::Desktop);
        assert_eq!(c.stats.completed_natural, 1, "同一段离开只计一次");
    }

    #[test]
    fn long_away_resets_screen_accumulator() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        for i in 1..=30 {
            sampled_tick(&mut c, t0 + S(i * 60), false);
        }
        assert!(c.screen_accum >= S(1700));
        c.set_sensors(Sensors {
            idle: Reading::Known(S(16 * 60)),
            ..known_sensors(t0)
        });
        sampled_tick(&mut c, t0 + S(31 * 60), false);
        assert_eq!(c.screen_accum, Duration::ZERO);
    }

    #[test]
    fn postpone_allowed_once_per_round() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        sampled_tick(&mut c, t0 + S(586), false);
        assert!(c.can_postpone());
        assert!(c.postpone(t0 + S(590)));
        assert_eq!(c.phase, Phase::Idle);
        assert_eq!(c.stats.postponed, 1);
        let due_in = c.next_break_in(t0 + S(590));
        assert!(due_in <= S(300) && due_in >= S(299));

        sampled_tick(&mut c, t0 + S(590 + 286), false);
        assert!(matches!(c.phase, Phase::HeadsUp { .. }));
        assert!(!c.can_postpone());
        assert!(!c.postpone(t0 + S(880)));
    }

    #[test]
    fn skip_during_break_counts_as_skipped_and_reschedules() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        sampled_tick(&mut c, t0 + S(586), false);
        sampled_tick(&mut c, t0 + S(601), false);
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
            sampled_tick(&mut c, t, false);
            t += S(15);
            sampled_tick(&mut c, t, false);
            t += S(30);
            sampled_tick(&mut c, t, false);
        }
        assert_eq!(c.stats.completed_short, 2);
        assert!(!c.long_break_due(), "accum={:?}", c.screen_accum);
        // 下一次到点在 1862s；累计在此之前越过 1800s，因此预告应升级为长休息（提前 30s = 1832s）
        sampled_tick(&mut c, t0 + S(1831), false);
        assert!(c.long_break_due());
        assert_eq!(
            c.phase,
            Phase::Idle,
            "长休息预告提前 30 秒，1831 时还差 1 秒"
        );
        sampled_tick(&mut c, t0 + S(1833), false);
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
        sampled_tick(&mut c, t2, false);
        assert!(matches!(c.phase, Phase::HeadsUp { is_long: true, .. }));
        // 同一回合不能再延后
        assert!(!c.can_postpone());
        assert!(!c.postpone(t2));

        // 跳过长休息 → 10 分钟后再提示，仍是长休息
        c.skip(t2);
        assert_eq!(c.stats.skipped, 1);
        let again = c.next_break_in(t2);
        assert!(again <= S(600) && again >= S(599));
        sampled_tick(&mut c, t2 + again - S(29), false);
        assert!(matches!(c.phase, Phase::HeadsUp { is_long: true, .. }));

        // 完成长休息 → 累计清零（中途补一次 tick，避免间隔超过 10 分钟被判睡眠）
        sampled_tick(&mut c, t2 + again + S(1), false);
        assert!(matches!(c.phase, Phase::Break { is_long: true, .. }));
        sampled_tick(&mut c, t2 + again + S(451), false);
        sampled_tick(&mut c, t2 + again + S(901), false);
        assert_eq!(c.stats.completed_long, 1);
        assert_eq!(c.screen_accum, Duration::ZERO);
    }

    #[test]
    fn quiet_hours_and_pause_block_delivery_then_grace_after() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        sampled_tick(&mut c, t0 + S(700), true);
        assert_eq!(c.phase, Phase::Idle);
        assert!(c.next_break_in(t0 + S(700)) >= S(74));

        let mut c2 = core(600, t0);
        c2.pause_for(t0, S(3600));
        sampled_tick(&mut c2, t0 + S(700), false);
        assert_eq!(c2.phase, Phase::Idle);
        assert!(c2.is_paused(t0 + S(700)));
        sampled_tick(&mut c2, t0 + S(3601), false);
        assert!(!c2.is_paused(t0 + S(3601)));
    }

    #[test]
    fn sleep_gap_treated_as_natural_rest_and_reschedules() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        sampled_tick(&mut c, t0 + S(100), false);
        // 相邻 tick 间隔 900 秒 => 睡眠 / 挂起
        sampled_tick(&mut c, t0 + S(100) + S(900), false);
        assert_eq!(c.phase, Phase::Idle);
        assert_eq!(c.stats.completed_natural, 1);
        assert_eq!(c.screen_accum, Duration::ZERO);
        let now = t0 + S(1001);
        assert!(c.next_break_in(now) >= S(599), "必须顺延而不是立即轰炸");
        assert!(!c.flash_active(now));
    }

    #[test]
    fn completed_break_flashes_welcome_back() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        sampled_tick(&mut c, t0 + S(586), false);
        sampled_tick(&mut c, t0 + S(601), false);
        assert!(matches!(c.phase, Phase::Break { .. }));
        sampled_tick(&mut c, t0 + S(632), false);
        assert!(c.flash_active(t0 + S(633)), "完成后 3 秒内显示欢迎回来");
        assert!(!c.flash_active(t0 + S(663)), "3 秒后自动消失");
    }

    #[test]
    fn sound_only_mode_delivers_cue_and_counts() {
        let t0 = Instant::now();
        let mut c = core(600, t0);
        c.cfg.visual_enabled = false;
        let a = sampled_tick(&mut c, t0 + S(586), false);
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
        sampled_tick(&mut c, t0 + S(41), false);
        assert_eq!(c.stats.completed_short, 1);
    }

    #[test]
    fn heads_up_cue_follows_its_switch() {
        let t0 = Instant::now();
        // 默认开：预告浮窗弹出的那一 tick 必须带一声提示音
        let mut c = core(600, t0);
        let a = sampled_tick(&mut c, t0 + S(586), false);
        assert!(matches!(c.phase, Phase::HeadsUp { .. }));
        assert_eq!(
            a,
            vec![Action::PlayCue],
            "预告浮窗弹出时应有一声提示音（heads_up_cue_enabled 默认开）"
        );

        // 关掉之后：同一 tick 不再出声，但预告阶段照常建立（视觉提醒不受影响）
        let mut c2 = core(600, t0);
        c2.cfg.heads_up_cue_enabled = false;
        let a2 = sampled_tick(&mut c2, t0 + S(586), false);
        assert!(matches!(c2.phase, Phase::HeadsUp { .. }));
        assert!(a2.is_empty(), "关闭开关后预告不得再出声");

        // 仅声音模式（visual_enabled = false）不受这个开关影响：
        // 那一声是本次投递的唯一通道，关掉它等于取消提醒
        let mut c3 = core(600, t0);
        c3.cfg.visual_enabled = false;
        c3.cfg.heads_up_cue_enabled = false;
        assert_eq!(
            sampled_tick(&mut c3, t0 + S(586), false),
            vec![Action::PlayCue]
        );
    }

    #[test]
    fn ui_warmup_opens_a_narrow_window_before_heads_up() {
        let t0 = Instant::now();
        // heads_up_secs = 15：把到点排在 100 秒后 → 浮窗应在 t0+85s 出现
        let mut c = core(600, t0);
        c.set_due_in(t0, S(100));

        assert!(
            sampled_warmup(&mut c, t0).is_none(),
            "离浮窗还有 85 秒，不该预热"
        );
        assert!(
            sampled_warmup(&mut c, t0 + S(79)).is_none(),
            "差 6 秒仍差一点：预热窗口只有 5 秒"
        );
        assert!(
            sampled_warmup(&mut c, t0 + S(80)).is_some(),
            "差 5 秒：预热窗口打开"
        );
        assert!(
            sampled_warmup(&mut c, t0 + S(85)).is_some(),
            "正点仍应是 Some(0)"
        );
        assert!(
            sampled_warmup(&mut c, t0 + S(86)).is_none(),
            "已错过浮窗时刻（心流 / 全屏里暂不投递）应立即回到 None，释放会话"
        );

        // 到点且会投递：phase 转为 HeadsUp，此后由 needs_ui 的阶段分支接管
        sampled_tick(&mut c, t0 + S(85), false);
        assert!(matches!(c.phase, Phase::HeadsUp { .. }));
        assert!(
            sampled_warmup(&mut c, t0 + S(85)).is_none(),
            "已在预告阶段，预热判定必须让位给阶段分支"
        );
    }

    #[test]
    fn ui_warmup_stays_off_when_no_panel_can_appear() {
        let t0 = Instant::now();
        let in_window = t0 + S(80);

        let mut c = core(600, t0);
        c.set_due_in(t0, S(100));
        c.cfg.visual_enabled = false;
        assert!(
            sampled_warmup(&mut c, in_window).is_none(),
            "仅声音模式没有浮窗可预热"
        );

        let mut c = core(600, t0);
        c.set_due_in(t0, S(100));
        c.cfg.enabled = false;
        assert!(
            sampled_warmup(&mut c, in_window).is_none(),
            "提醒关掉后不必预热"
        );

        let mut c = core(600, t0);
        c.set_due_in(t0, S(100));
        c.pause_for(t0, S(3600));
        assert!(
            sampled_warmup(&mut c, in_window).is_none(),
            "暂停期间不预热"
        );

        // 免打扰：连预热都不该开——会话建好 5 秒后到点会被顺延，等于白开
        let mut c = core(600, t0);
        c.set_due_in(t0, S(100));
        assert!(
            sampled_warmup(&mut c, t0 + S(80)).is_some(),
            "先确认这个时刻本来就在预热窗口里"
        );
        sampled_tick(&mut c, t0 + S(80), true);
        assert!(
            sampled_warmup(&mut c, t0 + S(80)).is_none(),
            "免打扰时段里不得预热：到点会被顺延，预热出的会话随即退出"
        );
    }
    fn unsupported_sensors(now: Instant) -> Sensors {
        Sensors {
            fullscreen: Reading::Unsupported("Fullscreen unsupported"),
            interruptible: Reading::Unsupported("Lock unsupported"),
            ..known_sensors(now)
        }
    }

    #[test]
    fn compatibility_is_opt_in_and_keeps_sensor_evidence_unknown() {
        let t0 = Instant::now();
        let mut c = core(60, t0);
        c.set_sensors(unsupported_sensors(t0));
        assert!(c.automatic_pause_reason(t0).is_some());
        let mut config = c.cfg.clone();
        config.compatibility_reminders = true;
        c.apply_config(config, t0);
        assert!(c.automatic_pause_reason(t0).is_none());
        sampled_tick(&mut c, t0, false);
        sampled_tick(&mut c, t0 + S(1), false);
        assert_eq!(c.screen_accum, S(1));
        sampled_tick(&mut c, t0 + S(45), false);
        assert!(matches!(c.phase, Phase::HeadsUp { .. }));
        assert!(matches!(c.sensors.interruptible, Reading::Unsupported(_)));
    }

    #[test]
    fn compatibility_rejects_failures_permissions_and_unavailable_state() {
        let t0 = Instant::now();
        for blocked in [
            Reading::Failed("failed"),
            Reading::PermissionRequired("permission"),
            Reading::Unavailable("no session"),
        ] {
            for session in [false, true] {
                let mut c = core(60, t0);
                c.cfg.compatibility_reminders = true;
                let mut sensors = unsupported_sensors(t0);
                if session {
                    sensors.interruptible = blocked;
                } else {
                    sensors.fullscreen = blocked;
                }
                c.set_sensors(sensors);
                assert!(c.automatic_pause_reason(t0).is_some());
                assert!(sampled_tick(&mut c, t0 + S(90), false).is_empty());
                assert_eq!(c.screen_accum, Duration::ZERO);
                assert_eq!(c.phase, Phase::Idle);
            }
        }
    }

    #[test]
    fn compatibility_never_overrides_idle_freshness_or_detected_inactive_session() {
        let t0 = Instant::now();
        let mut c = core(60, t0);
        c.cfg.compatibility_reminders = true;
        let valid = unsupported_sensors(t0);
        for idle in [
            Reading::Unavailable("Wayland idle"),
            Reading::Unsupported("idle"),
            Reading::Failed("invalid idle"),
            Reading::PermissionRequired("idle permission"),
        ] {
            c.set_sensors(Sensors { idle, ..valid });
            assert!(c.automatic_pause_reason(t0).is_some());
        }
        c.set_sensors(Sensors {
            interruptible: Reading::Known(false),
            ..valid
        });
        assert!(c.automatic_pause_reason(t0).is_some());
        assert!(sampled_tick(&mut c, t0 + S(60), false).is_empty());
        assert_eq!(c.screen_accum, Duration::ZERO);
        c.set_sensors(valid);
        assert!(c.automatic_pause_reason(t0 + S(6)).is_some());
        assert!(c.automatic_pause_reason(t0 - S(1)).is_some());
    }

    #[test]
    fn compatibility_preserves_detected_fullscreen_visual_avoidance() {
        let t0 = Instant::now();
        let mut c = core(60, t0);
        c.cfg.compatibility_reminders = true;
        c.set_sensors(Sensors {
            fullscreen: Reading::Known(true),
            ..unsupported_sensors(t0)
        });
        assert!(sampled_tick(&mut c, t0 + S(45), false).is_empty());
        assert_eq!(c.state, ContextState::Gaming);
        assert_eq!(c.phase, Phase::Idle);
        assert_eq!(
            sampled_tick(&mut c, t0 + S(60), false),
            vec![Action::PlayCue]
        );
        assert_eq!(c.phase, Phase::Idle);
        assert!(sampled_tick(&mut c, t0 + S(61), false).is_empty());
    }

    #[test]
    fn compatibility_toggle_does_not_recount_an_existing_away_episode() {
        let t0 = Instant::now();
        for already_counted in [false, true] {
            let mut c = core(60, t0);
            c.set_sensors(Sensors {
                idle: Reading::Known(S(200)),
                ..known_sensors(t0)
            });
            if already_counted {
                sampled_tick(&mut c, t0, false);
                assert_eq!(c.stats.completed_natural, 1);
            }
            // Both policy directions, while the same continuous Away episode
            // persists. The uncounted control may earn its first credit, not two.
            for (i, enabled) in [true, false].into_iter().enumerate() {
                let now = t0 + S(1 + i as u64 * 3);
                let mut config = c.cfg.clone();
                config.compatibility_reminders = enabled;
                c.apply_config(config, now);
                sampled_tick(&mut c, now, false);
                sampled_tick(&mut c, now + S(1), false);
                assert_eq!(c.stats.completed_natural, 1);
            }
            // A real return and a later Away episode may earn another credit.
            c.set_sensors(known_sensors(t0 + S(8)));
            c.tick(t0 + S(8), false);
            c.set_sensors(Sensors {
                idle: Reading::Known(S(200)),
                ..known_sensors(t0 + S(208))
            });
            c.tick(t0 + S(208), false);
            assert_eq!(c.stats.completed_natural, 2);
        }
    }

    #[test]
    fn compatibility_changes_preserve_an_already_started_manual_break() {
        let t0 = Instant::now();
        let mut c = core(60, t0);
        c.set_sensors(unsupported_sensors(t0));
        c.start_break_now(t0);
        let original = c.phase;
        for enabled in [true, false] {
            let mut config = c.cfg.clone();
            config.compatibility_reminders = enabled;
            c.apply_config(config, t0 + S(1));
            assert_eq!(c.phase, original);
        }
        assert_eq!(c.tick(t0 + S(31), false), vec![Action::PlayCue]);
        assert_eq!(c.stats.completed_short, 1);
    }

    #[test]
    fn compatibility_toggle_and_recovery_never_catch_up() {
        let t0 = Instant::now();
        let mut c = core(60, t0);
        c.set_sensors(unsupported_sensors(t0));
        sampled_tick(&mut c, t0 + S(120), false);
        let mut config = c.cfg.clone();
        config.compatibility_reminders = true;
        c.apply_config(config.clone(), t0 + S(240));
        assert!(sampled_tick(&mut c, t0 + S(250), false).is_empty());
        assert_eq!(c.screen_accum, Duration::ZERO);
        assert_eq!(c.next_break_in(t0 + S(250)), S(60));
        sampled_tick(&mut c, t0 + S(251), false);
        assert_eq!(c.screen_accum, S(1));
        c.tick(t0 + S(260), false); // stale
        assert!(sampled_tick(&mut c, t0 + S(300), false).is_empty());
        assert_eq!(c.screen_accum, S(1));
        c.phase = Phase::HeadsUp {
            started: t0 + S(300),
            ends: t0 + S(320),
            is_long: false,
        };
        config.compatibility_reminders = false;
        c.apply_config(config, t0 + S(301));
        assert_eq!(c.phase, Phase::Idle);
        assert!(sampled_tick(&mut c, t0 + S(400), false).is_empty());
        assert_eq!(c.screen_accum, S(1));
        c.start_break_now(t0 + S(400));
        assert_eq!(c.tick(t0 + S(431), false), vec![Action::PlayCue]);
    }

    #[test]
    fn no_native_sample_never_creates_activity_or_natural_rest() {
        let t0 = Instant::now();
        let mut c = Core::new(cfg(60), Stats::default(), t0);
        assert!(c.tick(t0 + S(900), false).is_empty());
        assert_eq!(c.screen_accum, Duration::ZERO);
        assert_eq!(c.phase, Phase::Idle);
        assert_eq!(c.stats.completed_natural, 0);
        assert!(c.automatic_pause_reason(t0 + S(900)).is_some());
        assert!(c.ui_warmup_in(t0 + S(900)).is_none());
    }

    #[test]
    fn every_missing_capability_blocks_automatic_delivery() {
        let t0 = Instant::now();
        let mut snapshots = [known_sensors(t0); 3];
        snapshots[0].idle = Reading::Unavailable("No global idle API");
        snapshots[1].fullscreen = Reading::PermissionRequired("Window metadata unavailable");
        snapshots[2].interruptible = Reading::Failed("Session query failed");
        for mut snapshot in snapshots {
            let mut c = core(60, t0);
            snapshot.sampled_at = t0 + S(60);
            c.set_sensors(snapshot);
            assert!(c.tick(t0 + S(60), false).is_empty());
            assert_eq!(c.phase, Phase::Idle);
            assert_eq!(c.screen_accum, Duration::ZERO);
            assert!(c.automatic_pause_reason(t0 + S(60)).is_some());
        }
    }

    #[test]
    fn stale_sample_cancels_heads_up_without_credit() {
        let t0 = Instant::now();
        let mut c = core(60, t0);
        c.set_sensors(known_sensors(t0 + S(45)));
        c.tick(t0 + S(45), false);
        assert!(matches!(c.phase, Phase::HeadsUp { .. }));
        let accumulated = c.screen_accum;
        assert!(c.tick(t0 + S(51), false).is_empty());
        assert_eq!(c.phase, Phase::Idle);
        assert_eq!(c.screen_accum, accumulated);
        assert_eq!(c.stats.completed_natural, 0);
    }

    #[test]
    fn recovery_does_not_count_unobserved_time_or_deliver_overdue_reminder() {
        let t0 = Instant::now();
        let mut c = Core::new(cfg(60), Stats::default(), t0);
        c.tick(t0 + S(120), false);
        c.set_sensors(known_sensors(t0 + S(240)));
        assert!(c.tick(t0 + S(240), false).is_empty());
        assert_eq!(c.screen_accum, Duration::ZERO);
        assert_eq!(c.phase, Phase::Idle);
        assert_eq!(c.next_break_in(t0 + S(240)), S(60));
        c.set_sensors(known_sensors(t0 + S(241)));
        c.tick(t0 + S(241), false);
        assert_eq!(c.screen_accum, S(1));
    }

    #[test]
    fn manual_break_completes_when_native_monitoring_is_unavailable() {
        let t0 = Instant::now();
        let mut c = Core::new(cfg(60), Stats::default(), t0);
        c.start_break_now(t0);
        assert!(matches!(c.phase, Phase::Break { .. }));
        assert_eq!(c.tick(t0 + S(31), false), vec![Action::PlayCue]);
        assert_eq!(c.phase, Phase::Idle);
        assert_eq!(c.stats.completed_short, 1);
        assert_eq!(c.screen_accum, Duration::ZERO);
    }
    #[test]
    fn manual_break_settles_at_recovery_even_when_sensor_reports_away() {
        let t0 = Instant::now();
        for long in [false, true] {
            let mut c = Core::new(cfg(60), Stats::default(), t0);
            c.cfg.long_break_enabled = long;
            if long {
                c.screen_accum = S(c.cfg.long_break_after_secs);
            }
            c.start_break_now(t0);
            let Phase::Break { ends, .. } = c.phase else {
                panic!("break must start")
            };
            c.tick(ends - S(1), false);
            c.set_sensors(Sensors {
                idle: Reading::Known(S(200)),
                ..known_sensors(ends)
            });
            assert_eq!(c.tick(ends, false), vec![Action::PlayCue]);
            assert_eq!(c.phase, Phase::Idle);
            assert_eq!(c.stats.completed_natural, 0);
            assert_eq!(c.stats.completed_short, u32::from(!long));
            assert_eq!(c.stats.completed_long, u32::from(long));
            c.set_sensors(Sensors {
                idle: Reading::Known(S(201)),
                ..known_sensors(ends + S(1))
            });
            c.tick(ends + S(1), false);
            assert_eq!(
                c.stats.completed_natural, 0,
                "same rest must not count twice"
            );
        }
    }
}
// 追加测试到 tests 模块（由集成脚本定位插入）
