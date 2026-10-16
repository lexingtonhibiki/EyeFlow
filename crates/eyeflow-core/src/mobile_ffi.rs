//! The mobile C API intentionally uses an opaque type distinct from EyeFlow.
use crate::mobile::{seconds_ceil, MobileConfig, MobileEngine, MobileSnapshot, MobileState};

pub struct EyeFlowMobile {
    engine: MobileEngine,
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[no_mangle]
pub extern "C" fn eyeflow_mobile_create() -> *mut EyeFlowMobile {
    let mut engine = MobileEngine::default();
    engine.roll_day(&today());
    Box::into_raw(Box::new(EyeFlowMobile { engine }))
}

/// # Safety
/// A non-null handle must be a live mobile handle, destroyed exactly once.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_mobile_destroy(handle: *mut EyeFlowMobile) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// # Safety
/// Handle must be null or live and exclusively accessible for this call.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_mobile_advance(
    handle: *mut EyeFlowMobile,
    delta_ms: u64,
    state: u32,
    delivery_allowed: u8,
) -> i32 {
    let Some(h) = handle.as_mut() else { return -1 };
    let Ok(state) = MobileState::try_from(state) else {
        return -1;
    };
    if delta_ms > crate::mobile::MAX_ELAPSED_MS {
        return -1;
    }
    h.engine.roll_day(&today());
    h.engine
        .advance(delta_ms, state, delivery_allowed != 0)
        .map_or(-1, |phase| phase as i32)
}

/// # Safety
/// Handle must be null or live and exclusively accessible for this call.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_mobile_command(handle: *mut EyeFlowMobile, command: u32) -> i32 {
    let Some(h) = handle.as_mut() else { return -1 };
    h.engine.roll_day(&today());
    if h.engine.command(command) {
        0
    } else {
        -1
    }
}

macro_rules! query {
    ($name:ident, $result:ty, $default:expr, $h:ident, $expr:expr) => {
        /// # Safety
        /// Handle must be null or live without concurrent mutation.
        #[no_mangle]
        pub unsafe extern "C" fn $name(handle: *const EyeFlowMobile) -> $result {
            let Some($h) = handle.as_ref() else {
                return $default;
            };
            $expr
        }
    };
}
query!(eyeflow_mobile_phase, i32, -1, h, h.engine.phase() as i32);
query!(eyeflow_mobile_state, i32, -1, h, h.engine.state() as i32);
query!(
    eyeflow_mobile_remaining_seconds,
    u64,
    0,
    h,
    seconds_ceil(h.engine.remaining_ms())
);
query!(
    eyeflow_mobile_short_debt_seconds,
    u64,
    0,
    h,
    h.engine.short_debt_ms() / 1000
);
query!(
    eyeflow_mobile_long_debt_seconds,
    u64,
    0,
    h,
    h.engine.long_debt_ms() / 1000
);
query!(
    eyeflow_mobile_lock_seconds,
    u64,
    0,
    h,
    h.engine.lock_ms() / 1000
);
query!(
    eyeflow_mobile_paused_seconds,
    u64,
    0,
    h,
    seconds_ceil(h.engine.pause_remaining_ms())
);
query!(
    eyeflow_mobile_reminder_id,
    u64,
    0,
    h,
    h.engine.reminder_id()
);
query!(
    eyeflow_mobile_can_deliver,
    u8,
    0,
    h,
    u8::from(h.engine.can_deliver())
);
query!(
    eyeflow_mobile_can_postpone,
    u8,
    0,
    h,
    u8::from(h.engine.can_postpone())
);
query!(
    eyeflow_mobile_enabled,
    u8,
    0,
    h,
    u8::from(h.engine.enabled())
);
query!(
    eyeflow_mobile_completed_count,
    u32,
    0,
    h,
    h.engine.stats().completed()
);
query!(
    eyeflow_mobile_postponed_count,
    u32,
    0,
    h,
    h.engine.stats().postponed
);
query!(
    eyeflow_mobile_skipped_count,
    u32,
    0,
    h,
    h.engine.stats().skipped
);
query!(
    eyeflow_mobile_natural_count,
    u32,
    0,
    h,
    h.engine.stats().natural
);
query!(
    eyeflow_mobile_natural_long_count,
    u32,
    0,
    h,
    h.engine.stats().natural_long
);
query!(
    eyeflow_mobile_guided_short_count,
    u32,
    0,
    h,
    h.engine.stats().guided_short
);
query!(
    eyeflow_mobile_guided_long_count,
    u32,
    0,
    h,
    h.engine.stats().guided_long
);

/// # Safety
/// Live handle; buffer must be writable for capacity bytes and not alias handle.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_mobile_export_state(
    handle: *const EyeFlowMobile,
    buffer: *mut u8,
    capacity: usize,
) -> usize {
    let Some(h) = handle.as_ref() else { return 0 };
    let Ok(bytes) = serde_json::to_vec(&h.engine.snapshot()) else {
        return 0;
    };
    if !buffer.is_null() && capacity >= bytes.len() {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
    }
    bytes.len()
}

/// # Safety
/// Live exclusively-accessible handle; buffer must be readable for length bytes
/// and must not alias handle. A failed import leaves the handle unchanged.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_mobile_import_state(
    handle: *mut EyeFlowMobile,
    buffer: *const u8,
    length: usize,
) -> i32 {
    let Some(h) = handle.as_mut() else { return -1 };
    if buffer.is_null() || length == 0 || length > 1024 * 1024 {
        return -1;
    }
    let Ok(saved) =
        serde_json::from_slice::<MobileSnapshot>(std::slice::from_raw_parts(buffer, length))
    else {
        return -1;
    };
    let Some(mut engine) = MobileEngine::restore(saved) else {
        return -1;
    };
    engine.roll_day(&today());
    h.engine = engine;
    0
}

/// Configure a fixed schedule, chiefly for deterministic tests and demo mode.
/// # Safety
/// Handle must be null or live and exclusively accessible for this call.
#[no_mangle]
pub unsafe extern "C" fn eyeflow_mobile_configure(
    handle: *mut EyeFlowMobile,
    interval_secs: u64,
    break_secs: u64,
    long_after_secs: u64,
    long_break_secs: u64,
) -> i32 {
    let Some(h) = handle.as_mut() else { return -1 };
    let Some(interval_ms) = interval_secs.checked_mul(1000) else {
        return -1;
    };
    let Some(break_ms) = break_secs.checked_mul(1000) else {
        return -1;
    };
    let Some(long_after_ms) = long_after_secs.checked_mul(1000) else {
        return -1;
    };
    let Some(long_break_ms) = long_break_secs.checked_mul(1000) else {
        return -1;
    };
    let config = MobileConfig {
        short_interval_min_ms: interval_ms,
        short_interval_max_ms: interval_ms,
        short_rest_ms: break_ms,
        long_after_ms,
        long_rest_ms: long_break_ms,
        short_heads_up_ms: h.engine.config().short_heads_up_ms.min(interval_ms),
        long_heads_up_ms: h.engine.config().long_heads_up_ms.min(long_after_ms),
        ..h.engine.config().clone()
    };
    if h.engine.configure(config) {
        0
    } else {
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn snapshot(h: *const EyeFlowMobile) -> Vec<u8> {
        let n = eyeflow_mobile_export_state(h, std::ptr::null_mut(), 0);
        let mut bytes = vec![0; n];
        assert_eq!(eyeflow_mobile_export_state(h, bytes.as_mut_ptr(), n), n);
        bytes
    }

    #[test]
    fn mobile_abi_validates_inputs_and_small_buffers_atomically() {
        unsafe {
            let h = eyeflow_mobile_create();
            assert_eq!(eyeflow_mobile_configure(h, 60, 30, 7200, 900), 0);
            assert_eq!(eyeflow_mobile_advance(h, 60_000, 0, 0), 2);
            let before = snapshot(h);
            assert_eq!(eyeflow_mobile_advance(h, 1, 99, 1), -1);
            assert_eq!(eyeflow_mobile_advance(h, u64::MAX, 0, 1), -1);
            assert_eq!(eyeflow_mobile_configure(h, u64::MAX, 30, 7200, 900), -1);
            assert_eq!(eyeflow_mobile_configure(h, 0, 30, 7200, 900), -1);
            assert_eq!(snapshot(h), before);
            let mut tiny = [42u8; 1];
            assert_eq!(
                eyeflow_mobile_export_state(h, tiny.as_mut_ptr(), 1),
                before.len()
            );
            assert_eq!(tiny, [42]);
            assert_eq!(eyeflow_mobile_import_state(h, b"invalid".as_ptr(), 7), -1);
            assert_eq!(snapshot(h), before);
            let mut corrupt: serde_json::Value = serde_json::from_slice(&before).unwrap();
            corrupt["pause_remaining_ms"] = serde_json::json!(u64::MAX);
            let corrupt = serde_json::to_vec(&corrupt).unwrap();
            assert_eq!(
                eyeflow_mobile_import_state(h, corrupt.as_ptr(), corrupt.len()),
                -1
            );
            assert_eq!(snapshot(h), before);
            assert_eq!(eyeflow_mobile_phase(std::ptr::null()), -1);
            assert_eq!(eyeflow_mobile_advance(std::ptr::null_mut(), 0, 0, 1), -1);
            assert_eq!(eyeflow_mobile_command(std::ptr::null_mut(), 1), -1);
            eyeflow_mobile_destroy(h);
            eyeflow_mobile_destroy(std::ptr::null_mut());
        }
    }

    #[test]
    fn mobile_abi_exports_restores_and_preserves_delivery_obligation() {
        unsafe {
            let h = eyeflow_mobile_create();
            assert_eq!(eyeflow_mobile_configure(h, 60, 30, 0, 900), 0);
            assert_eq!(eyeflow_mobile_advance(h, 45_000, 0, 0), 1);
            assert_eq!(eyeflow_mobile_can_deliver(h), 0);
            assert_eq!(eyeflow_mobile_command(h, 2), 0);
            let bytes = snapshot(h);
            let restored = eyeflow_mobile_create();
            assert_eq!(
                eyeflow_mobile_import_state(restored, bytes.as_ptr(), bytes.len()),
                0
            );
            assert_eq!(eyeflow_mobile_state(restored), 3);
            assert_eq!(eyeflow_mobile_remaining_seconds(restored), 300);
            assert_eq!(eyeflow_mobile_postponed_count(restored), 1);
            assert_eq!(eyeflow_mobile_advance(restored, 300_000, 0, 0), 2);
            assert_eq!(eyeflow_mobile_command(restored, 2), -1);
            assert_eq!(eyeflow_mobile_advance(restored, 0, 0, 1), 2);
            assert_eq!(eyeflow_mobile_can_deliver(restored), 1);
            assert_eq!(eyeflow_mobile_command(restored, 1), 0);
            assert_eq!(eyeflow_mobile_advance(restored, 30_000, 0, 1), 2);
            assert_eq!(eyeflow_mobile_completed_count(restored), 0);
            assert_eq!(eyeflow_mobile_advance(restored, 30_000, 2, 1), 0);
            assert_eq!(eyeflow_mobile_completed_count(restored), 1);
            assert_eq!(eyeflow_mobile_natural_count(restored), 1);
            eyeflow_mobile_destroy(h);
            eyeflow_mobile_destroy(restored);
        }
    }

    #[test]
    fn legacy_and_mobile_checkpoint_formats_are_not_interchangeable() {
        unsafe {
            let legacy = crate::eyeflow_create();
            let n = crate::eyeflow_export_state(legacy, std::ptr::null_mut(), 0);
            let mut bytes = vec![0; n];
            crate::eyeflow_export_state(legacy, bytes.as_mut_ptr(), n);
            let mobile = eyeflow_mobile_create();
            assert_eq!(eyeflow_mobile_import_state(mobile, bytes.as_ptr(), n), -1);
            let mobile_bytes = snapshot(mobile);
            assert_eq!(
                crate::eyeflow_import_state(legacy, mobile_bytes.as_ptr(), mobile_bytes.len()),
                -1
            );
            eyeflow_mobile_destroy(mobile);
            crate::eyeflow_destroy(legacy);
        }
    }
}
