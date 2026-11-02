use eyeflow::{
    config::Config,
    core::{Core, Phase, Sensors},
    platform::sensors::Reading,
    stats::Stats,
};
use std::time::{Duration, Instant};

fn sample(now: Instant, idle: u64, locked: bool) -> Sensors {
    Sensors {
        sampled_at: now,
        idle: Reading::Known(Duration::from_secs(idle)),
        fullscreen: Reading::Known(false),
        interruptible: if locked {
            Reading::Unavailable("Windows reports that the user is not present")
        } else {
            Reading::Known(true)
        },
    }
}

fn scenario(heads_up: bool, long: bool, rest: u64, disabled: bool) {
    let t0 = Instant::now();
    let mut core = Core::new(
        Config {
            short_break_min_secs: 1200,
            short_break_max_secs: 1200,
            ..Config::default()
        },
        Stats::default(),
        t0,
    );
    let lock_at = if long { 1170 } else { 1185 } - u64::from(!heads_up);
    for s in 0..=lock_at {
        let now = t0 + Duration::from_secs(s);
        core.set_sensors(sample(now, 0, false));
        if long {
            core.screen_accum = Duration::from_secs(core.cfg.long_break_after_secs);
        }
        core.tick(now, false);
    }
    assert_eq!(matches!(core.phase, Phase::HeadsUp { .. }), heads_up);
    if heads_up {
        assert!(matches!(core.phase, Phase::HeadsUp { is_long, .. } if is_long == long));
    }
    let before = core.screen_accum;
    core.cfg.enabled = !disabled;
    for s in 1..=rest {
        let now = t0 + Duration::from_secs(lock_at + s);
        core.set_sensors(sample(now, s, true));
        assert!(core.tick(now, false).is_empty());
        assert_eq!(core.phase, Phase::Idle);
    }
    let returned = t0 + Duration::from_secs(lock_at + rest + 1);
    core.set_sensors(sample(returned, 0, false));
    assert!(core.tick(returned, false).is_empty());
    assert_eq!(core.phase, Phase::Idle);
    if rest < 900 {
        assert_eq!(
            core.screen_accum, before,
            "short lock must retain continuous use"
        );
        assert!(core.next_break_in(returned) <= Duration::from_secs(31));
        return;
    }
    assert_eq!(core.screen_accum, Duration::ZERO);
    // Test a whole minute, not just the deliberately suppressed recovery tick.
    for s in 1..=60 {
        let now = returned + Duration::from_secs(s);
        core.set_sensors(sample(now, 0, false));
        assert!(
            core.tick(now, false).is_empty(),
            "old reminder replayed at unlock second {}",
            s + 1
        );
        assert_eq!(core.phase, Phase::Idle);
    }
    assert!(core.next_break_in(returned + Duration::from_secs(60)) >= Duration::from_secs(1140));
    assert!(!core.long_break_due());
    assert_eq!(core.stats.completed_short, 0);
    assert_eq!(core.stats.completed_long, 0);
    assert_eq!(core.stats.completed_natural, 0);
    assert_eq!(core.stats.skipped, 0);
    // Re-enabling and continuing actual use must eventually deliver a fresh short
    // reminder, rather than suppressing the old round forever.
    core.cfg.enabled = true;
    for s in 61..1185 {
        let now = returned + Duration::from_secs(s);
        core.set_sensors(sample(now, 0, false));
        assert!(core.tick(now, false).is_empty());
        assert_eq!(core.phase, Phase::Idle);
    }
    let next = returned + Duration::from_secs(1185);
    core.set_sensors(sample(next, 0, false));
    core.tick(next, false);
    assert!(matches!(core.phase, Phase::HeadsUp { is_long: false, .. }));
}

#[test]
fn long_locked_rest_does_not_replay_old_heads_up_after_unlock() {
    scenario(true, false, 900, false);
}

#[test]
fn long_locked_rest_refreshes_imminent_reminder_without_heads_up() {
    scenario(false, false, 900, false);
}

#[test]
fn longer_lock_stays_silent_after_unlock() {
    scenario(true, false, 1800, false);
}

#[test]
fn short_lock_does_not_discard_pending_reminder_or_continuous_use() {
    for heads_up in [false, true] {
        for long in [false, true] {
            for rest in [60, 899] {
                scenario(heads_up, long, rest, false);
            }
        }
    }
}

#[test]
fn long_rest_resets_pending_long_reminder_too() {
    for heads_up in [false, true] {
        scenario(heads_up, true, 900, false);
    }
}

#[test]
fn disabled_reminders_do_not_replay_after_long_locked_rest() {
    scenario(true, false, 900, true);
}

#[test]
fn stale_or_unknown_idle_does_not_discard_pending_deadline() {
    let t0 = Instant::now();
    for (idle, age) in [
        (Reading::Unavailable("idle unavailable"), 0),
        (Reading::Failed("idle failed"), 0),
        (Reading::Known(Duration::from_secs(900)), 6),
    ] {
        let mut core = Core::new(Config::default(), Stats::default(), t0);
        core.screen_accum = Duration::from_secs(1000);
        core.set_due_in(t0, Duration::from_secs(40));
        let now = t0 + Duration::from_secs(1);
        let mut sensors = sample(now - Duration::from_secs(age), 900, true);
        sensors.idle = idle;
        core.set_sensors(sensors);
        assert!(core.tick(now, false).is_empty());
        assert_eq!(core.next_break_in(now), Duration::from_secs(40));
        assert_eq!(core.screen_accum, Duration::from_secs(1000));
        assert_eq!(core.stats.completed_natural, 0);
    }
}
