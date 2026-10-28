use eyeflow::{
    config::Config,
    core::{Core, Phase, Sensors},
    platform::sensors::Reading,
    stats::Stats,
};
use std::time::{Duration, Instant};
fn sample(now: Instant, idle: u64, absent: bool) -> Sensors {
    Sensors {
        sampled_at: now,
        idle: Reading::Known(Duration::from_secs(idle)),
        fullscreen: Reading::Known(false),
        interruptible: if absent {
            Reading::Unavailable("Windows reports that the user is not present")
        } else {
            Reading::Known(true)
        },
    }
}
fn scenario(seconds: u64, heads_up: bool, absent: bool) -> Duration {
    let t0 = Instant::now();
    let cfg = Config {
        short_break_min_secs: 3600,
        short_break_max_secs: 3600,
        ..Config::default()
    };
    let mut c = Core::new(cfg, Stats::default(), t0);
    c.set_sensors(sample(t0, 0, false));
    c.tick(t0, false);
    c.screen_accum = Duration::from_secs(1000);
    if heads_up {
        c.phase = Phase::HeadsUp {
            started: t0,
            ends: t0 + Duration::from_secs(15),
            is_long: false,
        };
    }
    for s in 1..=seconds {
        let now = t0 + Duration::from_secs(s);
        c.set_sensors(sample(now, s, absent));
        assert!(
            c.tick(now, false).is_empty(),
            "unexpected cue during absence at {s}"
        );
    }
    if absent {
        assert_eq!(c.stats.completed_natural, 0);
        assert_eq!(c.phase, Phase::Idle);
    }
    for s in [seconds + 1, seconds + 2] {
        let now = t0 + Duration::from_secs(s);
        c.set_sensors(sample(now, 0, false));
        assert!(c.tick(now, false).is_empty(), "unexpected cue after return");
    }
    println!(
        "seconds={seconds},heads_up={heads_up},absent={absent},accum={:?}",
        c.screen_accum
    );
    c.screen_accum
}
#[test]
fn long_absence_from_idle_resets_after_return() {
    assert!(scenario(900, false, true) <= Duration::from_secs(2));
}
#[test]
fn long_absence_from_headsup_resets_after_return() {
    assert!(scenario(900, true, true) <= Duration::from_secs(2));
}
#[test]
fn short_absence_preserves_accum() {
    assert_eq!(scenario(60, false, true), Duration::from_secs(1001));
}
#[test]
fn observed_long_idle_control_resets() {
    assert!(scenario(900, false, false) <= Duration::from_secs(2));
}
#[test]
fn short_absence_from_headsup_preserves_accum() {
    assert_eq!(scenario(60, true, true), Duration::from_secs(1001));
}
#[test]
fn below_reset_threshold_preserves_accum() {
    assert_eq!(scenario(899, false, true), Duration::from_secs(1001));
}
#[test]
fn unavailable_or_stale_idle_does_not_reset_accum() {
    let t0 = Instant::now();
    for (idle, age) in [
        (Reading::Unavailable("idle unavailable"), 0),
        (Reading::Failed("idle failed"), 0),
        (Reading::Known(Duration::from_secs(900)), 6),
    ] {
        let mut c = Core::new(Config::default(), Stats::default(), t0);
        c.screen_accum = Duration::from_secs(1000);
        let now = t0 + Duration::from_secs(900);
        let mut snapshot = sample(now - Duration::from_secs(age), 900, true);
        snapshot.idle = idle;
        c.set_sensors(snapshot);
        assert!(c.tick(now, false).is_empty());
        assert_eq!(c.screen_accum, Duration::from_secs(1000));
    }
}
