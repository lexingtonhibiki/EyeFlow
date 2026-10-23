//! Platform sensor contract. Unknown readings must never become synthetic zero/false.

use std::time::{Duration, Instant};

/// Native capability or reading, with a displayable diagnostic for unavailable data.
/// Reasons are static diagnostic descriptions, not evidence that access was granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading<T> {
    Known(T),
    /// The backend explicitly does not implement this capability.
    Unsupported(&'static str),
    Unavailable(&'static str),
    // Some OS backends do not request permissions, but the shared contract must
    // distinguish refusal/pending permission from unavailable APIs.
    #[allow(dead_code)]
    PermissionRequired(&'static str),
    Failed(&'static str),
}

impl<T> Reading<T> {
    pub fn reason(&self) -> Option<&'static str> {
        match self {
            Self::Known(_) => None,
            Self::Unsupported(reason)
            | Self::Unavailable(reason)
            | Self::PermissionRequired(reason)
            | Self::Failed(reason) => Some(reason),
        }
    }
}

/// The detector is sampled every second; after five seconds its state is not trusted.
pub const MAX_SENSOR_AGE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorSnapshot {
    pub sampled_at: Instant,
    pub idle: Reading<Duration>,
    pub fullscreen: Reading<bool>,
    /// True means the backend has positively established that automatic reminders
    /// are permitted. Missing lock/session/presentation information is not true.
    pub interruptible: Reading<bool>,
}

impl SensorSnapshot {
    pub fn unavailable(now: Instant, reason: &'static str) -> Self {
        Self {
            sampled_at: now,
            idle: Reading::Unavailable(reason),
            fullscreen: Reading::Unavailable(reason),
            interruptible: Reading::Unavailable(reason),
        }
    }

    pub fn is_fresh(&self, now: Instant) -> bool {
        now.checked_duration_since(self.sampled_at)
            .is_some_and(|age| age <= MAX_SENSOR_AGE)
    }

    /// A reason automatic monitoring is paused. Optional backend limitations must
    /// be surfaced, never silently reinterpreted as a safe desktop state.
    pub fn automatic_pause_reason(&self, now: Instant) -> Option<&'static str> {
        if !self.is_fresh(now) {
            return Some("Sensor reading is stale or has an invalid timestamp");
        }
        self.idle
            .reason()
            .or_else(|| self.fullscreen.reason())
            .or_else(|| self.interruptible.reason())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_rejects_stale_and_future_samples() {
        let now = Instant::now();
        let snapshot = SensorSnapshot::unavailable(now, "unavailable");
        assert!(snapshot.is_fresh(now));
        assert!(snapshot.is_fresh(now + MAX_SENSOR_AGE));
        assert!(!snapshot.is_fresh(now + MAX_SENSOR_AGE + Duration::from_nanos(1)));
        assert!(!snapshot.is_fresh(now - Duration::from_secs(1)));
    }

    #[test]
    fn absence_and_valid_false_are_distinct() {
        let now = Instant::now();
        let mut snapshot = SensorSnapshot {
            sampled_at: now,
            idle: Reading::Known(Duration::ZERO),
            fullscreen: Reading::Known(false),
            interruptible: Reading::Known(true),
        };
        assert_eq!(snapshot.automatic_pause_reason(now), None);
        snapshot.fullscreen = Reading::Unavailable("Fullscreen detection unavailable");
        assert_eq!(
            snapshot.automatic_pause_reason(now),
            Some("Fullscreen detection unavailable")
        );
    }
}
