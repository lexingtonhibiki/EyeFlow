//! Portable desktop adapter plus a distinct conservative mobile interval engine.
//! Serialize access to each opaque handle; never mix the two host contracts.
pub mod config;
#[path = "../../../src/core.rs"]
pub mod core;
pub mod mobile;
mod mobile_ffi;
pub mod stats;
use config::Config;
use core::{Core, CoreSnapshot, Phase, Sensors};
use stats::Stats;
use std::time::{Duration, Instant};

pub struct EyeFlow {
    core: Core,
    epoch: Instant,
    now: Instant,
    elapsed_ms: u64,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedState {
    version: u32,
    config: Config,
    stats: Stats,
    scheduler: CoreSnapshot,
}

#[no_mangle]
pub extern "C" fn eyeflow_create() -> *mut EyeFlow {
    let now = Instant::now();
    Box::into_raw(Box::new(EyeFlow {
        core: Core::new(Config::default(), Stats::default(), now),
        epoch: now,
        now,
        elapsed_ms: 0,
    }))
}
/// # Safety
/// A non-null handle must be a live handle from create, destroyed exactly once.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_destroy(handle: *mut EyeFlow) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}
/// # Safety
/// Handle must be null or live and exclusively accessible for this call.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_command(handle: *mut EyeFlow, command: u32) {
    let Some(h) = handle.as_mut() else { return };
    match command {
        1 => h.core.start_break_now(h.now),
        2 => {
            h.core.postpone(h.now);
        }
        3 => h.core.skip(h.now),
        4 => h.core.pause_for(h.now, Duration::from_secs(3600)),
        5 => h.core.resume(),
        _ => (),
    }
}
/// # Safety
/// Handle must be null or live and exclusively accessible for this call.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_tick(
    handle: *mut EyeFlow,
    elapsed_ms: u64,
    idle_secs: u64,
    focus: u8,
) -> i32 {
    let Some(h) = handle.as_mut() else { return -1 };
    // A century is ample for a process and keeps Instant arithmetic bounded.
    if elapsed_ms < h.elapsed_ms || elapsed_ms > 100 * 366 * 86400 * 1000 {
        return -1;
    }
    let Some(now) = h.epoch.checked_add(Duration::from_millis(elapsed_ms)) else {
        return -1;
    };
    h.now = now;
    h.elapsed_ms = elapsed_ms;
    h.core
        .stats
        .roll_day(&chrono::Local::now().format("%Y-%m-%d").to_string());
    h.core.set_sensors(Sensors {
        idle_secs,
        fullscreen: false,
        interruptible: true,
    });
    h.core.tick(now, focus != 0);
    match h.core.phase {
        Phase::Idle => 0,
        Phase::HeadsUp { .. } => 1,
        Phase::Break { .. } => 2,
    }
}
/// # Safety
/// Handle must be null or live without concurrent mutation.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_remaining_seconds(handle: *const EyeFlow) -> u64 {
    let Some(h) = handle.as_ref() else { return 0 };
    let d = match h.core.phase {
        Phase::Idle => h.core.next_break_in(h.now),
        Phase::HeadsUp { ends, .. } | Phase::Break { ends, .. } => {
            ends.saturating_duration_since(h.now)
        }
    };
    d.as_secs() + u64::from(d.subsec_nanos() != 0)
}
/// # Safety
/// Handle must be null or live without concurrent mutation.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_completed_count(handle: *const EyeFlow) -> u32 {
    handle
        .as_ref()
        .map_or(0, |h| h.core.stats.completed_today())
}
/// # Safety
/// Handle must be null or live without concurrent mutation.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_postponed_count(handle: *const EyeFlow) -> u32 {
    handle.as_ref().map_or(0, |h| h.core.stats.postponed)
}
/// # Safety
/// Live handle; a non-null buffer must be writable for capacity bytes and not alias handle.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_export_state(
    handle: *const EyeFlow,
    buffer: *mut u8,
    capacity: usize,
) -> usize {
    let Some(h) = handle.as_ref() else { return 0 };
    let saved = SavedState {
        version: 1,
        config: h.core.cfg.clone(),
        stats: h.core.stats.clone(),
        scheduler: h.core.snapshot(h.now),
    };
    let Ok(bytes) = serde_json::to_vec(&saved) else {
        return 0;
    };
    if !buffer.is_null() && capacity >= bytes.len() {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
    }
    bytes.len()
}
/// # Safety
/// Live exclusively-accessible handle; buffer must be readable for length bytes and not alias handle.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_import_state(
    handle: *mut EyeFlow,
    buffer: *const u8,
    length: usize,
) -> i32 {
    let Some(h) = handle.as_mut() else { return -1 };
    if buffer.is_null() || length == 0 || length > 4 * 1024 * 1024 {
        return -1;
    }
    let Ok(mut saved) =
        serde_json::from_slice::<SavedState>(std::slice::from_raw_parts(buffer, length))
    else {
        return -1;
    };
    if saved.version != 1 {
        return -1;
    }
    // Reject counters that could overflow the unchanged scheduler on the next event.
    let count = u64::from(saved.stats.completed_short)
        + u64::from(saved.stats.completed_long)
        + u64::from(saved.stats.completed_natural);
    if count >= u64::from(u32::MAX)
        || saved.stats.skipped == u32::MAX
        || saved.stats.postponed == u32::MAX
        || saved.stats.streak_days == u32::MAX
    {
        return -1;
    }
    saved
        .stats
        .roll_day(&chrono::Local::now().format("%Y-%m-%d").to_string());
    let now = Instant::now();
    let Some(core) = Core::restore(saved.config.sanitized(), saved.stats, saved.scheduler, now)
    else {
        return -1;
    };
    h.core = core;
    h.epoch = now;
    h.now = now;
    h.elapsed_ms = 0;
    0
}
/// # Safety
/// Handle must be null or live and exclusively accessible for this call.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_configure(
    handle: *mut EyeFlow,
    interval_secs: u64,
    break_secs: u64,
    long_after_secs: u64,
    long_break_secs: u64,
) -> i32 {
    let Some(h) = handle.as_mut() else { return -1 };
    let mut cfg = h.core.cfg.clone();
    cfg.short_break_min_secs = interval_secs;
    cfg.short_break_max_secs = interval_secs;
    cfg.short_break_secs = break_secs;
    cfg.long_break_enabled = long_after_secs != 0;
    cfg.long_break_after_secs = long_after_secs;
    cfg.long_break_secs = long_break_secs;
    h.core.apply_config(cfg.sanitized(), h.now);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abi_cycle_and_bad_inputs() {
        unsafe {
            let h = eyeflow_create();
            assert_eq!(eyeflow_configure(h, 60, 30, 0, 900), 0);
            assert_eq!(eyeflow_tick(h, 45000, 0, 0), 1);
            assert_eq!(eyeflow_tick(h, 44000, 0, 0), -1);
            assert_eq!(eyeflow_tick(h, u64::MAX, 0, 0), -1);
            eyeflow_command(h, 1);
            assert_eq!(eyeflow_remaining_seconds(h), 30);
            assert_eq!(eyeflow_tick(h, 75000, 0, 0), 0);
            assert_eq!(eyeflow_completed_count(h), 1);
            eyeflow_destroy(h);
            eyeflow_destroy(std::ptr::null_mut());
            assert_eq!(eyeflow_tick(std::ptr::null_mut(), 0, 0, 0), -1);
        }
    }
    #[test]
    fn restore_preserves_postponement_and_remaining_active_time() {
        unsafe {
            let h = eyeflow_create();
            eyeflow_configure(h, 60, 30, 0, 900);
            eyeflow_tick(h, 45000, 0, 0);
            eyeflow_command(h, 2);
            let n = eyeflow_export_state(h, std::ptr::null_mut(), 0);
            let mut bytes = vec![0; n];
            assert_eq!(eyeflow_export_state(h, bytes.as_mut_ptr(), n), n);
            let restored = eyeflow_create();
            assert_eq!(eyeflow_import_state(restored, bytes.as_ptr(), n), 0);
            assert_eq!(eyeflow_remaining_seconds(restored), 300);
            assert_eq!(eyeflow_postponed_count(restored), 1);
            assert_eq!(eyeflow_tick(restored, 285000, 0, 0), 1);
            eyeflow_command(restored, 2); // Already postponed this round: unchanged.
            assert_eq!(eyeflow_postponed_count(restored), 1);
            assert_eq!(eyeflow_tick(restored, 285000, 0, 0), 1);
            eyeflow_destroy(h);
            eyeflow_destroy(restored);
        }
    }
    #[test]
    fn lock_freezes_time_and_restore_preserves_break() {
        unsafe {
            let h = eyeflow_create();
            eyeflow_command(h, 1);
            eyeflow_tick(h, 10000, 0, 0);
            let n = eyeflow_export_state(h, std::ptr::null_mut(), 0);
            let mut bytes = vec![0; n];
            eyeflow_export_state(h, bytes.as_mut_ptr(), n);
            let restored = eyeflow_create();
            assert_eq!(eyeflow_import_state(restored, bytes.as_ptr(), n), 0);
            assert_eq!(eyeflow_remaining_seconds(restored), 20);
            assert_eq!(eyeflow_tick(restored, 0, 0, 0), 2);
            assert_eq!(eyeflow_remaining_seconds(restored), 20);
            assert_eq!(eyeflow_tick(restored, 20000, 0, 0), 0);
            assert_eq!(eyeflow_completed_count(restored), 1);
            eyeflow_destroy(h);
            eyeflow_destroy(restored);
        }
    }
    #[test]
    fn invalid_import_is_atomic_and_small_export_does_not_write() {
        unsafe {
            let h = eyeflow_create();
            eyeflow_command(h, 1);
            let before = eyeflow_remaining_seconds(h);
            let mut tiny = [42u8; 1];
            assert!(eyeflow_export_state(h, tiny.as_mut_ptr(), tiny.len()) > 1);
            assert_eq!(tiny, [42]);
            assert_eq!(eyeflow_import_state(h, b"bad".as_ptr(), 3), -1);
            assert_eq!(eyeflow_remaining_seconds(h), before);
            let n = eyeflow_export_state(h, std::ptr::null_mut(), 0);
            let mut bytes = vec![0; n];
            eyeflow_export_state(h, bytes.as_mut_ptr(), n);
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["scheduler"]["phase_remaining_ms"] = serde_json::json!(u64::MAX);
            let corrupt = serde_json::to_vec(&value).unwrap();
            assert_eq!(eyeflow_import_state(h, corrupt.as_ptr(), corrupt.len()), -1);
            assert_eq!(eyeflow_remaining_seconds(h), before);
            eyeflow_destroy(h);
        }
    }
}
