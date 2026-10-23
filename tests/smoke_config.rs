use chrono::NaiveTime;
use eyeflow::{
    config::Config,
    core::{Core, Phase},
    stats::Stats,
};
use std::time::{Duration, Instant};

#[test]
fn native_smoke_profile_keeps_manual_rest_visible_at_any_hour() {
    let table: toml::Table = include_str!("../scripts/fixtures/desktop-smoke.toml")
        .parse()
        .unwrap();
    // This field marks the current format in Config::load, avoiding legacy migration.
    assert!(table.contains_key("short_break_min_secs"));
    let cfg: Config = table.try_into().unwrap();
    let cfg = cfg.sanitized();
    for hour in 0..24 {
        assert!(!cfg.in_quiet_hours(NaiveTime::from_hms_opt(hour, 28, 0).unwrap()));
    }
    let t0 = Instant::now();
    let quiet = cfg.in_quiet_hours(NaiveTime::from_hms_opt(7, 28, 0).unwrap());
    let mut core = Core::new(cfg, Stats::default(), t0);
    core.start_break_now(t0);
    core.tick(t0 + Duration::from_secs(1), quiet);
    assert!(matches!(core.phase, Phase::Break { .. }));
    // Product quiet-hours semantics remain unchanged, not bypassed by a test hook.
    core.tick(t0 + Duration::from_secs(2), true);
    assert!(matches!(core.phase, Phase::Idle));
}
