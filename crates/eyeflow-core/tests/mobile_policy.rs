use eyeflow_core::mobile::{
    MobileConfig, MobileEngine, MobilePhase as Phase, MobileSnapshot, MobileState as State,
    MAX_ELAPSED_MS,
};

fn engine() -> MobileEngine {
    MobileEngine::new(MobileConfig {
        short_interval_min_ms: 1_200_000,
        short_interval_max_ms: 1_200_000,
        ..MobileConfig::default()
    })
    .unwrap()
}
fn advance(engine: &mut MobileEngine, seconds: u64, state: State) -> Phase {
    engine.advance(seconds * 1000, state, true).unwrap()
}
fn snapshot(engine: &MobileEngine) -> Vec<u8> {
    serde_json::to_vec(&engine.snapshot()).unwrap()
}
fn restored(engine: &MobileEngine) -> MobileEngine {
    MobileEngine::restore(serde_json::from_slice(&snapshot(engine)).unwrap()).unwrap()
}

#[test]
fn selected_a_to_b_accumulates_without_reset() {
    let mut e = engine();
    advance(&mut e, 600, State::Tracking); // App A.
    advance(&mut e, 600, State::Tracking); // App B, also selected.
    assert_eq!(e.short_debt_ms(), 1_200_000);
    assert_eq!(e.phase(), Phase::ShortDue);
    assert_eq!(e.stats().completed(), 0);
}

#[test]
fn unselected_unknown_and_stopped_pause_but_never_rest() {
    let mut e = engine();
    advance(&mut e, 1100, State::Tracking);
    let debt = e.short_debt_ms();
    let long = e.long_debt_ms();
    for state in [State::OutOfScope, State::Unknown, State::Stopped] {
        advance(&mut e, 86_400, state);
        assert_eq!(e.short_debt_ms(), debt);
        assert_eq!(e.long_debt_ms(), long);
        assert_eq!(e.remaining_ms(), 100_000);
        assert_eq!(e.stats().completed(), 0);
    }
    assert_eq!(advance(&mut e, 85, State::Tracking), Phase::ShortHeadsUp);
}

#[test]
fn head_up_countdown_uses_only_selected_exposure() {
    let mut e = engine();
    assert_eq!(advance(&mut e, 1185, State::Tracking), Phase::ShortHeadsUp);
    assert_eq!(e.remaining_ms(), 15_000);
    advance(&mut e, 300, State::OutOfScope);
    advance(&mut e, 300, State::Unknown);
    assert_eq!(e.remaining_ms(), 15_000);
    assert_eq!(advance(&mut e, 14, State::Tracking), Phase::ShortHeadsUp);
    assert_eq!(advance(&mut e, 1, State::Tracking), Phase::ShortDue);
}

#[test]
fn due_or_notification_elapsed_never_completes_rest() {
    let mut e = engine();
    assert_eq!(advance(&mut e, 1200, State::Tracking), Phase::ShortDue);
    advance(&mut e, 900, State::Tracking);
    assert_eq!(e.phase(), Phase::ShortDue);
    assert_eq!(e.stats().completed(), 0);
    assert_eq!(e.short_debt_ms(), 2_100_000);
    assert!(e.start_rest());
    advance(&mut e, 30, State::Tracking); // Notification/action is not rest proof.
    assert_eq!(e.phase(), Phase::ShortDue);
    assert_eq!(e.stats().completed(), 0);
}

#[test]
fn delivery_denial_preserves_overdue_reminder_and_identity() {
    let mut e = engine();
    e.advance(1_200_000, State::Tracking, false).unwrap();
    assert_eq!(e.phase(), Phase::ShortDue);
    assert!(!e.can_deliver());
    let id = e.reminder_id();
    e.advance(900_000, State::Tracking, false).unwrap();
    assert_eq!(e.reminder_id(), id);
    assert_eq!(e.stats().completed(), 0);
    e.advance(0, State::Tracking, true).unwrap();
    assert!(e.can_deliver());
    assert_eq!(e.phase(), Phase::ShortDue);
    advance(&mut e, 0, State::OutOfScope);
    assert!(!e.can_deliver());
}

#[test]
fn partial_lock_preserves_every_debt() {
    let mut e = engine();
    advance(&mut e, 1200, State::Tracking);
    advance(&mut e, 29, State::Locked);
    assert_eq!(e.short_debt_ms(), 1_200_000);
    assert_eq!(e.long_debt_ms(), 1_200_000);
    assert_eq!(e.stats().natural, 0);
    assert_eq!(e.phase(), Phase::ShortDue);
    advance(&mut e, 0, State::Tracking);
    assert_eq!(e.lock_ms(), 0);
    assert_eq!(e.phase(), Phase::ShortDue);
}

#[test]
fn lock_thirty_seconds_resets_short_once_but_not_long() {
    let mut e = engine();
    advance(&mut e, 1200, State::Tracking);
    advance(&mut e, 30, State::Locked);
    assert_eq!(e.short_debt_ms(), 0);
    assert_eq!(e.long_debt_ms(), 1_200_000);
    assert_eq!(e.stats().natural, 1);
    advance(&mut e, 869, State::Locked);
    assert_eq!(e.stats().natural, 1);
    assert_eq!(e.stats().natural_long, 0);
    assert_eq!(e.long_debt_ms(), 1_200_000);
    assert_eq!(e.lock_ms(), 899_000);
}

#[test]
fn lock_fifteen_minutes_clears_long_once_without_double_counting() {
    let mut e = engine();
    advance(&mut e, 7200, State::Tracking);
    assert_eq!(e.phase(), Phase::LongDue);
    assert!(e.start_rest());
    advance(&mut e, 30, State::Locked);
    assert_eq!(e.long_debt_ms(), 7_200_000);
    assert_eq!(e.phase(), Phase::LongRest);
    assert_eq!(e.remaining_ms(), 870_000);
    advance(&mut e, 870, State::Locked);
    assert_eq!(e.short_debt_ms(), 0);
    assert_eq!(e.long_debt_ms(), 0);
    assert_eq!(e.phase(), Phase::Idle);
    assert_eq!(e.stats().natural, 1);
    assert_eq!(e.stats().natural_long, 1);
    assert_eq!(e.stats().guided_long, 0);
    assert_eq!(e.stats().completed(), 1);
    advance(&mut e, 900, State::Locked);
    assert_eq!(e.stats().completed(), 1);
}

#[test]
fn short_natural_rest_does_not_consume_long_pending() {
    let mut e = engine();
    advance(&mut e, 7200, State::Tracking);
    let id = e.reminder_id();
    advance(&mut e, 30, State::Locked);
    assert_eq!(e.phase(), Phase::LongDue);
    assert_eq!(e.reminder_id(), id);
    assert_eq!(e.long_debt_ms(), 7_200_000);
    advance(&mut e, 0, State::Tracking);
    assert!(e.can_deliver());
    assert_eq!(e.phase(), Phase::LongDue);
}

#[test]
fn interrupted_lock_chains_do_not_add_together() {
    for interruption in [
        State::Tracking,
        State::OutOfScope,
        State::Unknown,
        State::Stopped,
    ] {
        let mut e = engine();
        advance(&mut e, 7200, State::Tracking);
        advance(&mut e, 20, State::Locked);
        advance(&mut e, 0, interruption);
        advance(&mut e, 20, State::Locked);
        assert_eq!(e.stats().natural, 0, "{interruption:?}");
        assert_eq!(e.short_debt_ms(), 7_200_000);
        advance(&mut e, 10, State::Locked);
        assert_eq!(e.stats().natural, 1);
        advance(&mut e, 500, State::Locked);
        advance(&mut e, 1, interruption);
        advance(&mut e, 400, State::Locked);
        assert!(e.long_debt_ms() >= 7_200_000, "{interruption:?}");
        assert_eq!(e.stats().natural_long, 0);
    }
}

#[test]
fn long_single_proven_interval_equals_chunked_intervals() {
    let mut one = engine();
    let mut chunks = engine();
    advance(&mut one, 7200, State::Tracking);
    for _ in 0..7200 {
        advance(&mut chunks, 1, State::Tracking);
    }
    advance(&mut one, 900, State::Locked);
    for _ in 0..900 {
        advance(&mut chunks, 1, State::Locked);
    }
    assert_eq!(snapshot(&one), snapshot(&chunks));
}

#[test]
fn voluntary_rest_requires_explicit_start_and_own_app_participation() {
    let mut e = engine();
    advance(&mut e, 1200, State::Tracking);
    advance(&mut e, 900, State::VoluntaryRest);
    assert_eq!(e.stats().completed(), 0);
    assert_eq!(e.phase(), Phase::ShortDue);
    assert!(e.start_rest());
    advance(&mut e, 29, State::VoluntaryRest);
    assert_eq!(e.phase(), Phase::ShortRest);
    assert_eq!(e.remaining_ms(), 1000);
    advance(&mut e, 1, State::VoluntaryRest);
    assert_eq!(e.stats().guided_short, 1);
    assert_eq!(e.stats().natural, 0);
    assert_eq!(e.short_debt_ms(), 0);
    assert_eq!(e.long_debt_ms(), 1_200_000);
    advance(&mut e, 900, State::VoluntaryRest);
    assert_eq!(e.stats().completed(), 1);
}

#[test]
fn interrupted_guided_session_never_completes_from_background_time() {
    for interruption in [
        State::Tracking,
        State::OutOfScope,
        State::Unknown,
        State::Stopped,
    ] {
        let mut e = engine();
        advance(&mut e, 1200, State::Tracking);
        e.start_rest();
        advance(&mut e, 29, State::VoluntaryRest);
        advance(&mut e, 0, interruption);
        advance(&mut e, 30, State::VoluntaryRest);
        assert_eq!(e.stats().completed(), 0, "{interruption:?}");
        assert_eq!(e.phase(), Phase::ShortDue);
    }
}

#[test]
fn guided_and_locked_partial_evidence_is_not_combined() {
    let mut e = engine();
    advance(&mut e, 1200, State::Tracking);
    e.start_rest();
    advance(&mut e, 20, State::VoluntaryRest);
    advance(&mut e, 20, State::Locked);
    assert_eq!(e.stats().completed(), 0);
    advance(&mut e, 10, State::VoluntaryRest);
    assert_eq!(e.stats().completed(), 0);
    assert_eq!(e.remaining_ms(), 20_000);
}

#[test]
fn guided_long_requires_full_duration() {
    let mut e = engine();
    advance(&mut e, 7200, State::Tracking);
    e.start_rest();
    advance(&mut e, 30, State::VoluntaryRest);
    assert_eq!(e.long_debt_ms(), 7_200_000);
    assert_eq!(e.stats().completed(), 0);
    advance(&mut e, 870, State::VoluntaryRest);
    assert_eq!(e.stats().guided_long, 1);
    assert_eq!(e.long_debt_ms(), 0);
    assert_eq!(e.short_debt_ms(), 0);
}

#[test]
fn postpone_five_selected_minutes_only_once_and_across_restore() {
    let mut e = engine();
    advance(&mut e, 1185, State::Tracking);
    let old_id = e.reminder_id();
    assert!(e.postpone());
    assert_eq!(e.stats().postponed, 1);
    assert_eq!(e.remaining_ms(), 300_000);
    assert!(!e.postpone());
    advance(&mut e, 3600, State::OutOfScope);
    let mut e = restored(&e);
    assert_eq!(e.remaining_ms(), 300_000);
    advance(&mut e, 285, State::Tracking);
    assert_eq!(e.phase(), Phase::ShortHeadsUp);
    assert_ne!(e.reminder_id(), old_id);
    assert!(!e.postpone());
    assert_eq!(e.stats().postponed, 1);
}

#[test]
fn long_preempts_short_and_preserves_obligation_when_postponed_before_due() {
    let mut e = engine();
    advance(&mut e, 7170, State::Tracking);
    assert_eq!(e.phase(), Phase::LongHeadsUp);
    assert_eq!(e.remaining_ms(), 30_000);
    assert!(e.postpone());
    assert_eq!(e.phase(), Phase::Idle);
    assert_eq!(e.remaining_ms(), 300_000);
    advance(&mut e, 270, State::Tracking);
    assert_eq!(e.phase(), Phase::LongHeadsUp);
    assert!(!e.postpone());
    advance(&mut e, 30, State::Tracking);
    assert_eq!(e.phase(), Phase::LongDue);
}

#[test]
fn skipping_long_retries_after_ten_selected_minutes_without_resetting_debt() {
    let mut e = engine();
    advance(&mut e, 7170, State::Tracking);
    assert!(e.skip());
    assert_eq!(e.phase(), Phase::Idle);
    assert_eq!(e.remaining_ms(), 600_000);
    assert_eq!(e.long_debt_ms(), 7_170_000);
    advance(&mut e, 900, State::Unknown);
    assert_eq!(e.remaining_ms(), 600_000);
    advance(&mut e, 570, State::Tracking);
    assert_eq!(e.phase(), Phase::LongHeadsUp);
    assert!(e.can_postpone());
    advance(&mut e, 30, State::Tracking);
    assert_eq!(e.phase(), Phase::LongDue);
    assert_eq!(e.stats().skipped, 1);
    assert_eq!(e.stats().completed(), 0);
}

#[test]
fn skipping_short_reschedules_without_forgiving_exposure() {
    let mut e = engine();
    advance(&mut e, 1200, State::Tracking);
    assert!(e.skip());
    assert_eq!(e.remaining_ms(), 1_200_000);
    assert_eq!(e.short_debt_ms(), 1_200_000);
    assert_eq!(e.long_debt_ms(), 1_200_000);
    assert_eq!(e.stats().completed(), 0);
}

#[test]
fn pause_expires_on_elapsed_time_without_hiding_or_erasing_debt() {
    let mut e = engine();
    advance(&mut e, 1200, State::Tracking);
    assert!(e.command(4));
    assert!(!e.can_deliver());
    advance(&mut e, 100, State::Tracking);
    assert_eq!(e.short_debt_ms(), 1_300_000);
    assert_eq!(e.phase(), Phase::ShortDue);
    advance(&mut e, 3500, State::Unknown);
    assert_eq!(e.pause_remaining_ms(), 0);
    assert_eq!(e.stats().completed(), 0);
    advance(&mut e, 0, State::Tracking);
    assert!(e.can_deliver());
}

#[test]
fn stop_and_resume_preserve_debt_and_pending_across_restart() {
    let mut e = engine();
    advance(&mut e, 1200, State::Tracking);
    e.command(6);
    assert!(!e.enabled());
    advance(&mut e, 900, State::Tracking);
    advance(&mut e, 900, State::Locked);
    assert_eq!(e.short_debt_ms(), 1_200_000);
    assert_eq!(e.stats().completed(), 0);
    let mut e = restored(&e);
    assert!(!e.enabled());
    assert_eq!(e.state(), State::Stopped);
    e.command(5);
    assert_eq!(e.state(), State::Unknown);
    advance(&mut e, 0, State::Tracking);
    assert_eq!(e.phase(), Phase::ShortDue);
    assert!(e.can_deliver());
}

#[test]
fn restore_never_credits_missing_or_pre_restart_partial_rest() {
    let mut e = engine();
    advance(&mut e, 1200, State::Tracking);
    advance(&mut e, 29, State::Locked);
    let mut e = restored(&e);
    assert_eq!(e.lock_ms(), 0);
    assert_eq!(e.state(), State::Unknown);
    advance(&mut e, 1, State::Locked);
    assert_eq!(e.stats().completed(), 0);
    advance(&mut e, 0, State::Tracking);
    e.start_rest();
    advance(&mut e, 29, State::VoluntaryRest);
    let mut e = restored(&e);
    assert_eq!(e.phase(), Phase::ShortDue);
    advance(&mut e, 900, State::VoluntaryRest);
    assert_eq!(e.stats().completed(), 0);
    advance(&mut e, 900, State::Unknown);
    assert_eq!(e.long_debt_ms(), 1_200_000);
}

#[test]
fn invalid_intervals_config_and_snapshots_rejected_without_mutation() {
    let mut e = engine();
    let before = snapshot(&e);
    assert!(e
        .advance(MAX_ELAPSED_MS + 1, State::Tracking, true)
        .is_err());
    assert_eq!(snapshot(&e), before);
    let invalid = MobileConfig {
        short_rest_ms: 0,
        ..MobileConfig::default()
    };
    assert!(!e.configure(invalid));
    assert_eq!(snapshot(&e), before);
    for field in [
        "version",
        "short_debt_ms",
        "long_debt_ms",
        "short_due_in_ms",
        "long_due_in_ms",
        "pause_remaining_ms",
        "lock_ms",
    ] {
        let mut value: serde_json::Value = serde_json::from_slice(&before).unwrap();
        value[field] = serde_json::json!(u64::MAX);
        let result = serde_json::from_value::<MobileSnapshot>(value)
            .ok()
            .and_then(MobileEngine::restore);
        assert!(result.is_none(), "{field}");
    }
}

#[test]
fn saturating_counters_and_extreme_supported_duration_do_not_wrap() {
    let mut e = engine();
    advance(&mut e, 1200, State::Tracking);
    let mut value: serde_json::Value = serde_json::from_slice(&snapshot(&e)).unwrap();
    value["stats"]["natural"] = serde_json::json!(u32::MAX);
    value["stats"]["natural_long"] = serde_json::json!(u32::MAX);
    let mut e = MobileEngine::restore(serde_json::from_value(value).unwrap()).unwrap();
    e.advance(MAX_ELAPSED_MS, State::Locked, true).unwrap();
    assert_eq!(e.stats().completed(), u32::MAX);
    assert_eq!(e.stats().natural_long, u32::MAX);
    assert_eq!(e.long_debt_ms(), 0);
    assert_eq!(e.short_debt_ms(), 0);
}

#[test]
fn day_roll_changes_only_counters_not_pending_or_exposure() {
    let mut e = engine();
    e.roll_day("2026-10-14");
    advance(&mut e, 1200, State::Tracking);
    e.postpone();
    e.roll_day("2026-10-15");
    assert_eq!(e.stats().postponed, 0);
    assert_eq!(e.short_debt_ms(), 1_200_000);
    assert_eq!(e.remaining_ms(), 300_000);
    advance(&mut e, 300, State::Tracking);
    assert!(!e.postpone());
}

#[test]
fn default_short_schedule_stays_within_original_bounds() {
    let config = MobileConfig::default();
    assert_eq!(config.short_rest_ms, 30_000);
    assert_eq!(config.long_after_ms, 7_200_000);
    assert_eq!(config.long_rest_ms, 900_000);
    for _ in 0..200 {
        let e = MobileEngine::default();
        assert!((900_000..=1_500_000).contains(&e.remaining_ms()));
    }
}
