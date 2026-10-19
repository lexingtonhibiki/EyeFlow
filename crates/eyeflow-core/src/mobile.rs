//! Conservative mobile scheduling based on explicitly observed elapsed intervals.
//!
//! Unlike the desktop adapter, an unselected app is not rest, a missing sample is
//! not rest, and reminder delivery is not rest. Hosts reconstruct the timeline and
//! supply disjoint chronological intervals. No clock, app sensing, notifications,
//! persistence I/O, or operating-system assumptions live in this engine.

use serde::{Deserialize, Serialize};

pub const MAX_ELAPSED_MS: u64 = 100 * 366 * 86_400 * 1000;
const MAX_SETTING_MS: u64 = 86_400 * 1000;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MobileState {
    Tracking = 0,
    OutOfScope = 1,
    Locked = 2,
    Unknown = 3,
    Stopped = 4,
    /// An explicitly started, foreground, user-active guided rest session.
    /// This is a participation signal, not proof that the user looked away.
    VoluntaryRest = 5,
}

impl TryFrom<u32> for MobileState {
    type Error = ();
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Tracking),
            1 => Ok(Self::OutOfScope),
            2 => Ok(Self::Locked),
            3 => Ok(Self::Unknown),
            4 => Ok(Self::Stopped),
            5 => Ok(Self::VoluntaryRest),
            _ => Err(()),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MobilePhase {
    Idle = 0,
    ShortHeadsUp = 1,
    ShortDue = 2,
    LongHeadsUp = 3,
    LongDue = 4,
    ShortRest = 5,
    LongRest = 6,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MobileConfig {
    pub short_interval_min_ms: u64,
    pub short_interval_max_ms: u64,
    pub short_rest_ms: u64,
    /// Zero disables the long reminder (not short reminders or natural rest).
    pub long_after_ms: u64,
    pub long_rest_ms: u64,
    pub short_heads_up_ms: u64,
    pub long_heads_up_ms: u64,
    pub postpone_ms: u64,
    pub long_retry_ms: u64,
}

impl Default for MobileConfig {
    fn default() -> Self {
        Self {
            short_interval_min_ms: 900_000,
            short_interval_max_ms: 1_500_000,
            short_rest_ms: 30_000,
            long_after_ms: 7_200_000,
            long_rest_ms: 900_000,
            short_heads_up_ms: 15_000,
            long_heads_up_ms: 30_000,
            postpone_ms: 300_000,
            long_retry_ms: 600_000,
        }
    }
}

impl MobileConfig {
    pub fn is_valid(&self) -> bool {
        self.short_interval_min_ms > 0
            && self.short_interval_min_ms <= self.short_interval_max_ms
            && self.short_interval_max_ms <= MAX_SETTING_MS
            && (1..=3_600_000).contains(&self.short_rest_ms)
            && (1..=3_600_000).contains(&self.long_rest_ms)
            && self.long_rest_ms >= self.short_rest_ms
            && self.long_after_ms <= MAX_SETTING_MS
            && self.short_heads_up_ms <= self.short_interval_min_ms
            && (self.long_after_ms == 0 || self.long_heads_up_ms <= self.long_after_ms)
            && self.long_heads_up_ms <= MAX_SETTING_MS
            && (1..=MAX_SETTING_MS).contains(&self.postpone_ms)
            && (1..=MAX_SETTING_MS).contains(&self.long_retry_ms)
    }

    fn sample_short_interval(&self) -> u64 {
        use rand::Rng;
        let mut rng = rand::rng();
        triangular_interval(
            self.short_interval_min_ms,
            self.short_interval_max_ms,
            rng.random(),
            rng.random(),
        )
    }
}

fn triangular_interval(min: u64, max: u64, a: f64, b: f64) -> u64 {
    // The mean of two independent uniform samples: triangular, peak at midpoint.
    min + (((max - min) as f64 * (a + b) / 2.0) as u64)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MobileStats {
    pub day: String,
    /// Foreground, explicitly started guided sessions; not verified eye movement.
    pub guided_short: u32,
    pub guided_long: u32,
    /// One count at the short threshold per uninterrupted proven lock chain.
    pub natural: u32,
    /// An upgrade of a natural lock chain, not an additional completed session.
    pub natural_long: u32,
    pub postponed: u32,
    pub skipped: u32,
}

impl MobileStats {
    pub fn completed(&self) -> u32 {
        self.guided_short
            .saturating_add(self.guided_long)
            .saturating_add(self.natural)
    }

    pub fn roll_day(&mut self, day: &str) {
        if self.day != day {
            *self = Self {
                day: day.to_owned(),
                ..Self::default()
            };
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum RestKind {
    Short,
    Long,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GuidedRest {
    kind: RestKind,
    elapsed_ms: u64,
}

/// Serializable checkpoint. It has no process-specific clock timestamps.
/// Restore deliberately invalidates lock continuity and an active guided session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MobileSnapshot {
    version: u32,
    config: MobileConfig,
    stats: MobileStats,
    enabled: bool,
    short_debt_ms: u64,
    long_debt_ms: u64,
    short_due_in_ms: u64,
    long_due_in_ms: u64,
    short_postponed: bool,
    long_postponed: bool,
    long_engaged: bool,
    pause_remaining_ms: u64,
    short_round: u64,
    long_round: u64,
    next_round: u64,
    // Saved for diagnosis, but never trusted as continuity across a process gap.
    lock_ms: u64,
    lock_short_counted: bool,
    lock_long_counted: bool,
    guided_rest: Option<GuidedRest>,
}

#[derive(Debug, Clone)]
pub struct MobileEngine {
    config: MobileConfig,
    stats: MobileStats,
    enabled: bool,
    state: MobileState,
    delivery_allowed: bool,
    short_debt_ms: u64,
    long_debt_ms: u64,
    short_due_in_ms: u64,
    long_due_in_ms: u64,
    short_postponed: bool,
    long_postponed: bool,
    long_engaged: bool,
    pause_remaining_ms: u64,
    lock_ms: u64,
    lock_short_counted: bool,
    lock_long_counted: bool,
    guided_rest: Option<GuidedRest>,
    short_round: u64,
    long_round: u64,
    next_round: u64,
}

impl Default for MobileEngine {
    fn default() -> Self {
        Self::new(MobileConfig::default()).expect("default mobile config is valid")
    }
}

impl MobileEngine {
    pub fn new(config: MobileConfig) -> Option<Self> {
        if !config.is_valid() {
            return None;
        }
        Some(Self {
            short_due_in_ms: config.sample_short_interval(),
            long_due_in_ms: config.long_after_ms,
            config,
            stats: MobileStats::default(),
            enabled: true,
            state: MobileState::Unknown,
            delivery_allowed: false,
            short_debt_ms: 0,
            long_debt_ms: 0,
            short_postponed: false,
            long_postponed: false,
            long_engaged: false,
            pause_remaining_ms: 0,
            lock_ms: 0,
            lock_short_counted: false,
            lock_long_counted: false,
            guided_rest: None,
            short_round: 1,
            long_round: 2,
            next_round: 3,
        })
    }

    /// Consume one complete known interval, including a zero-length state boundary.
    /// `delta_ms` is elapsed duration, never a cumulative synthetic active clock.
    /// The host must label gaps Unknown rather than extending a previous sample.
    /// Rejected intervals leave the engine unchanged.
    pub fn advance(
        &mut self,
        delta_ms: u64,
        state: MobileState,
        delivery_allowed: bool,
    ) -> Result<MobilePhase, &'static str> {
        if delta_ms > MAX_ELAPSED_MS {
            return Err("elapsed interval exceeds the supported range");
        }
        self.pause_remaining_ms = self.pause_remaining_ms.saturating_sub(delta_ms);
        self.state = if self.enabled {
            state
        } else {
            MobileState::Stopped
        };
        self.delivery_allowed = delivery_allowed;

        if self.state != MobileState::Locked {
            self.break_lock_chain();
        }
        match self.state {
            MobileState::Tracking => {
                self.guided_rest = None;
                self.short_debt_ms = bounded_add(self.short_debt_ms, delta_ms);
                self.long_debt_ms = bounded_add(self.long_debt_ms, delta_ms);
                self.short_due_in_ms = self.short_due_in_ms.saturating_sub(delta_ms);
                if self.config.long_after_ms != 0 {
                    self.long_due_in_ms = self.long_due_in_ms.saturating_sub(delta_ms);
                    self.long_engaged |= self.long_due_in_ms <= self.config.long_heads_up_ms;
                }
            }
            MobileState::Locked => {
                // Do not combine own-app participation with device lock evidence.
                // Each source must independently establish an uninterrupted rest.
                if let Some(rest) = &mut self.guided_rest {
                    rest.elapsed_ms = 0;
                }
                self.advance_lock(delta_ms);
            }
            MobileState::VoluntaryRest => self.advance_guided(delta_ms),
            MobileState::OutOfScope | MobileState::Unknown | MobileState::Stopped => {
                self.guided_rest = None;
            }
        }
        Ok(self.phase())
    }

    fn advance_lock(&mut self, delta_ms: u64) {
        self.lock_ms = bounded_add(self.lock_ms, delta_ms);
        if !self.lock_short_counted && self.lock_ms >= self.config.short_rest_ms {
            self.lock_short_counted = true;
            self.stats.natural = self.stats.natural.saturating_add(1);
            self.reset_short();
            if self
                .guided_rest
                .as_ref()
                .is_some_and(|r| r.kind == RestKind::Short)
            {
                self.guided_rest = None;
            }
        }
        if !self.lock_long_counted && self.lock_ms >= self.config.long_rest_ms {
            self.lock_long_counted = true;
            self.stats.natural_long = self.stats.natural_long.saturating_add(1);
            self.reset_long();
            self.guided_rest = None;
        }
    }

    fn advance_guided(&mut self, delta_ms: u64) {
        let Some(rest) = &mut self.guided_rest else {
            return;
        };
        rest.elapsed_ms = bounded_add(rest.elapsed_ms, delta_ms);
        let duration = match rest.kind {
            RestKind::Short => self.config.short_rest_ms,
            RestKind::Long => self.config.long_rest_ms,
        };
        if rest.elapsed_ms < duration {
            return;
        }
        match rest.kind {
            RestKind::Short => {
                self.stats.guided_short = self.stats.guided_short.saturating_add(1);
                self.reset_short();
            }
            RestKind::Long => {
                self.stats.guided_long = self.stats.guided_long.saturating_add(1);
                self.reset_short();
                self.reset_long();
            }
        }
        self.guided_rest = None;
    }

    fn break_lock_chain(&mut self) {
        self.lock_ms = 0;
        self.lock_short_counted = false;
        self.lock_long_counted = false;
    }

    fn round_id(&mut self) -> u64 {
        let result = self.next_round;
        self.next_round = self.next_round.saturating_add(1);
        result
    }

    fn reset_short(&mut self) {
        self.short_debt_ms = 0;
        self.short_due_in_ms = self.config.sample_short_interval();
        self.short_postponed = false;
        self.short_round = self.round_id();
    }

    fn reset_long(&mut self) {
        self.long_debt_ms = 0;
        self.long_due_in_ms = self.config.long_after_ms;
        self.long_postponed = false;
        self.long_engaged = false;
        self.long_round = self.round_id();
    }

    fn long_has_priority(&self) -> bool {
        self.config.long_after_ms != 0
            && (self.long_engaged
                || self.long_due_in_ms <= self.config.long_heads_up_ms
                || self.long_debt_ms >= self.config.long_after_ms)
    }

    fn pending_kind(&self) -> Option<RestKind> {
        if self.long_has_priority() {
            (self.long_due_in_ms <= self.config.long_heads_up_ms).then_some(RestKind::Long)
        } else {
            (self.short_due_in_ms <= self.config.short_heads_up_ms).then_some(RestKind::Short)
        }
    }

    pub fn phase(&self) -> MobilePhase {
        if let Some(rest) = &self.guided_rest {
            return match rest.kind {
                RestKind::Short => MobilePhase::ShortRest,
                RestKind::Long => MobilePhase::LongRest,
            };
        }
        match self.pending_kind() {
            Some(RestKind::Long) if self.long_due_in_ms == 0 => MobilePhase::LongDue,
            Some(RestKind::Long) => MobilePhase::LongHeadsUp,
            Some(RestKind::Short) if self.short_due_in_ms == 0 => MobilePhase::ShortDue,
            Some(RestKind::Short) => MobilePhase::ShortHeadsUp,
            None => MobilePhase::Idle,
        }
    }

    /// The deadline countdown, or rest remaining. Round up at the C boundary.
    pub fn remaining_ms(&self) -> u64 {
        if let Some(rest) = &self.guided_rest {
            let duration = match rest.kind {
                RestKind::Short => self.config.short_rest_ms,
                RestKind::Long => self.config.long_rest_ms,
            };
            // Locked rest uses only the independently proven lock chain.
            let elapsed = if self.state == MobileState::Locked {
                self.lock_ms
            } else {
                rest.elapsed_ms
            };
            return duration.saturating_sub(elapsed);
        }
        if self.long_has_priority() {
            self.long_due_in_ms
        } else if self.config.long_after_ms == 0 {
            self.short_due_in_ms
        } else {
            self.short_due_in_ms.min(self.long_due_in_ms)
        }
    }

    pub fn can_deliver(&self) -> bool {
        self.enabled
            && self.pause_remaining_ms == 0
            && self.state == MobileState::Tracking
            && self.delivery_allowed
            && self.pending_kind().is_some()
            && self.guided_rest.is_none()
    }

    pub fn reminder_id(&self) -> u64 {
        let kind = self
            .guided_rest
            .as_ref()
            .map(|rest| rest.kind)
            .or_else(|| self.pending_kind());
        match kind {
            Some(RestKind::Short) => self.short_round,
            Some(RestKind::Long) => self.long_round,
            None => 0,
        }
    }

    pub fn start_rest(&mut self) -> bool {
        if !self.enabled || self.guided_rest.is_some() {
            return false;
        }
        let kind = if self.long_has_priority() {
            RestKind::Long
        } else {
            RestKind::Short
        };
        self.guided_rest = Some(GuidedRest {
            kind,
            elapsed_ms: 0,
        });
        true
    }

    pub fn can_postpone(&self) -> bool {
        if self.guided_rest.is_some() || !self.enabled {
            return false;
        }
        match self.pending_kind() {
            Some(RestKind::Short) => !self.short_postponed,
            Some(RestKind::Long) => !self.long_postponed,
            None => false,
        }
    }

    pub fn postpone(&mut self) -> bool {
        if !self.can_postpone() {
            return false;
        }
        match self
            .pending_kind()
            .expect("can_postpone requires a pending reminder")
        {
            RestKind::Short => {
                self.short_due_in_ms = self.config.postpone_ms;
                self.short_postponed = true;
                self.short_round = self.round_id();
            }
            RestKind::Long => {
                self.long_due_in_ms = self.config.postpone_ms;
                self.long_postponed = true;
                self.long_round = self.round_id();
            }
        }
        self.stats.postponed = self.stats.postponed.saturating_add(1);
        true
    }

    pub fn skip(&mut self) -> bool {
        if !self.enabled {
            return false;
        }
        let kind = self
            .guided_rest
            .as_ref()
            .map(|r| r.kind)
            .or_else(|| self.pending_kind());
        let Some(kind) = kind else { return false };
        self.guided_rest = None;
        self.stats.skipped = self.stats.skipped.saturating_add(1);
        match kind {
            RestKind::Short => {
                self.short_due_in_ms = self.config.sample_short_interval();
                self.short_postponed = false;
                self.short_round = self.round_id();
            }
            RestKind::Long => {
                self.long_due_in_ms = self.config.long_retry_ms;
                self.long_postponed = false;
                self.long_round = self.round_id();
            }
        }
        true
    }

    /// Commands do not manufacture elapsed time or evidence of rest.
    /// 1 start rest, 2 postpone once, 3 skip, 4 pause reminders for one hour,
    /// 5 resume, 6 stop tracking, 7 cancel the guided session without skipping.
    pub fn command(&mut self, command: u32) -> bool {
        match command {
            1 => self.start_rest(),
            2 => self.postpone(),
            3 => self.skip(),
            4 => {
                self.pause_remaining_ms = 3_600_000;
                self.guided_rest = None;
                true
            }
            5 => {
                self.enabled = true;
                self.pause_remaining_ms = 0;
                self.state = MobileState::Unknown;
                self.delivery_allowed = false;
                self.break_lock_chain();
                self.guided_rest = None;
                true
            }
            6 => {
                self.enabled = false;
                self.state = MobileState::Stopped;
                self.delivery_allowed = false;
                self.break_lock_chain();
                self.guided_rest = None;
                true
            }
            7 => self.guided_rest.take().is_some(),
            _ => false,
        }
    }

    pub fn state(&self) -> MobileState {
        self.state
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn stats(&self) -> &MobileStats {
        &self.stats
    }
    pub fn config(&self) -> &MobileConfig {
        &self.config
    }
    pub fn short_debt_ms(&self) -> u64 {
        self.short_debt_ms
    }
    pub fn long_debt_ms(&self) -> u64 {
        self.long_debt_ms
    }
    pub fn lock_ms(&self) -> u64 {
        self.lock_ms
    }
    pub fn pause_remaining_ms(&self) -> u64 {
        self.pause_remaining_ms
    }
    pub fn roll_day(&mut self, day: &str) {
        self.stats.roll_day(day);
    }

    /// Reconfigure without erasing exposure or an already pending obligation.
    /// A fresh engine adopts the interval range immediately. Existing deadlines
    /// may move earlier, never later merely because configuration changed.
    pub fn configure(&mut self, config: MobileConfig) -> bool {
        if !config.is_valid() {
            return false;
        }
        let short_remaining = config
            .sample_short_interval()
            .saturating_sub(self.short_debt_ms);
        self.short_due_in_ms = if self.short_debt_ms == 0 {
            short_remaining
        } else {
            self.short_due_in_ms.min(short_remaining)
        };
        self.long_due_in_ms = if config.long_after_ms == 0 {
            0
        } else if self.config.long_after_ms == 0 {
            config.long_after_ms.saturating_sub(self.long_debt_ms)
        } else {
            self.long_due_in_ms
                .min(config.long_after_ms.saturating_sub(self.long_debt_ms))
        };
        self.long_engaged = config.long_after_ms != 0
            && (self.long_engaged || self.long_due_in_ms <= config.long_heads_up_ms);
        self.config = config;
        self.guided_rest = None;
        // Threshold edits cannot credit previously observed lock time as a new rest.
        self.break_lock_chain();
        true
    }

    pub fn snapshot(&self) -> MobileSnapshot {
        MobileSnapshot {
            version: 1,
            config: self.config.clone(),
            stats: self.stats.clone(),
            enabled: self.enabled,
            short_debt_ms: self.short_debt_ms,
            long_debt_ms: self.long_debt_ms,
            short_due_in_ms: self.short_due_in_ms,
            long_due_in_ms: self.long_due_in_ms,
            short_postponed: self.short_postponed,
            long_postponed: self.long_postponed,
            long_engaged: self.long_engaged,
            pause_remaining_ms: self.pause_remaining_ms,
            short_round: self.short_round,
            long_round: self.long_round,
            next_round: self.next_round,
            lock_ms: self.lock_ms,
            lock_short_counted: self.lock_short_counted,
            lock_long_counted: self.lock_long_counted,
            guided_rest: self.guided_rest.clone(),
        }
    }

    /// Import is an observation discontinuity, not elapsed rest. Supply any
    /// reliably measured downtime separately as Unknown (or replay proved events).
    pub fn restore(saved: MobileSnapshot) -> Option<Self> {
        let bounded = [saved.short_debt_ms, saved.long_debt_ms, saved.lock_ms]
            .iter()
            .all(|value| *value <= MAX_ELAPSED_MS);
        if saved.version != 1
            || !saved.config.is_valid()
            || !bounded
            || saved.short_due_in_ms > MAX_SETTING_MS
            || saved.long_due_in_ms > MAX_SETTING_MS
            || saved.pause_remaining_ms > 3_600_000
            || saved.stats.day.len() > 32
            || saved.short_round == 0
            || saved.long_round == 0
            || saved.next_round <= saved.short_round
            || saved.next_round <= saved.long_round
            || saved
                .guided_rest
                .as_ref()
                .is_some_and(|rest| rest.elapsed_ms > 3_600_000)
        {
            return None;
        }
        Some(Self {
            config: saved.config,
            stats: saved.stats,
            enabled: saved.enabled,
            state: if saved.enabled {
                MobileState::Unknown
            } else {
                MobileState::Stopped
            },
            delivery_allowed: false,
            short_debt_ms: saved.short_debt_ms,
            long_debt_ms: saved.long_debt_ms,
            short_due_in_ms: saved.short_due_in_ms,
            long_due_in_ms: saved.long_due_in_ms,
            short_postponed: saved.short_postponed,
            long_postponed: saved.long_postponed,
            long_engaged: saved.long_engaged,
            pause_remaining_ms: saved.pause_remaining_ms,
            short_round: saved.short_round,
            long_round: saved.long_round,
            next_round: saved.next_round,
            lock_ms: 0,
            lock_short_counted: false,
            lock_long_counted: false,
            guided_rest: None,
        })
    }
}

fn bounded_add(a: u64, b: u64) -> u64 {
    a.saturating_add(b).min(MAX_ELAPSED_MS)
}
pub(crate) fn seconds_ceil(ms: u64) -> u64 {
    ms / 1000 + u64::from(ms % 1000 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangular_sampler_has_original_support_and_midpoint_peak() {
        assert_eq!(triangular_interval(900_000, 1_500_000, 0.0, 0.0), 900_000);
        assert_eq!(triangular_interval(900_000, 1_500_000, 1.0, 1.0), 1_500_000);
        assert_eq!(triangular_interval(900_000, 1_500_000, 0.0, 1.0), 1_200_000);
        // A deterministic uniform grid avoids a flaky stochastic test. The
        // central bin has more outcomes than either edge, with mean 20 minutes.
        let mut bins = [0u64; 6];
        let mut total = 0u64;
        for a in 0..100 {
            for b in 0..100 {
                let value = triangular_interval(
                    900_000,
                    1_500_000,
                    (f64::from(a) + 0.5) / 100.0,
                    (f64::from(b) + 0.5) / 100.0,
                );
                bins[((value - 900_000) / 100_000) as usize] += 1;
                total += value;
            }
        }
        assert!(bins[2] > bins[0] * 3 && bins[3] > bins[5] * 3);
        assert!((1_199_999..=1_200_001).contains(&(total / 10_000)));
        assert_eq!(seconds_ceil(1), 1);
        assert_eq!(seconds_ceil(1000), 1);
        assert_eq!(seconds_ceil(1001), 2);
    }
}
