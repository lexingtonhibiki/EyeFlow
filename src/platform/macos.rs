//! macOS sensor boundary. Idle uses public Quartz APIs, without an event tap.
//!
//! Fullscreen and screen-lock detection are intentionally unavailable: window bounds,
//! display sleep and fast user switching do not establish those states reliably.
//! The caller must surface these limitations. Strict mode pauses automatic
//! reminders; an explicit compatibility policy may accept unsupported capabilities.

use super::sensors::{Reading, SensorSnapshot};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct NativeSensors;

impl NativeSensors {
    pub fn new() -> Self {
        Self
    }

    pub fn sample(&mut self, now: Instant) -> SensorSnapshot {
        snapshot(now, native_idle())
    }
}

fn snapshot(now: Instant, idle: Reading<Duration>) -> SensorSnapshot {
    SensorSnapshot {
        sampled_at: now,
        idle,
        fullscreen: Reading::Unsupported("macOS fullscreen detection is not supported yet"),
        interruptible: Reading::Unsupported("macOS screen-lock detection is not supported yet"),
    }
}

fn idle_duration(seconds: f64) -> Reading<Duration> {
    // Do not saturate invalid native values to zero: that would falsely imply
    // recent user activity. This also handles NaN, infinities and overflow.
    match Duration::try_from_secs_f64(seconds) {
        Ok(duration) => Reading::Known(duration),
        Err(_) => Reading::Failed("Quartz returned an invalid idle duration"),
    }
}

#[cfg(target_os = "macos")]
fn native_idle() -> Reading<Duration> {
    use std::ffi::c_void;

    // CGEventSourceStateID is a signed 32-bit enum; CGEventType is uint32_t.
    const COMBINED_SESSION_STATE: i32 = 0;
    const ANY_INPUT_EVENT: u32 = u32::MAX;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGSessionCopyCurrentDictionary() -> *const c_void;
        fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(object: *const c_void);
    }

    // Apple documents NULL when the caller has no Quartz GUI session or the
    // window server is disabled. A non-NULL dictionary does NOT mean unlocked.
    // https://developer.apple.com/documentation/coregraphics/cgsessioncopycurrentdictionary()
    // SAFETY: The function takes no arguments. The Copy rule gives us one owned
    // reference; after a null check it is released exactly once, without dereference.
    let session = unsafe { CGSessionCopyCurrentDictionary() };
    if session.is_null() {
        return Reading::Unavailable("No macOS Quartz GUI session is available");
    }
    unsafe { CFRelease(session) };

    // This source covers events in the current user's login session. Do not
    // claim this API proves hardware activity or that permission was granted.
    // https://developer.apple.com/documentation/coregraphics/cgeventsourcestateid/combinedsessionstate
    // https://developer.apple.com/documentation/coregraphics/cgeventsource/secondssincelasteventtype(_:eventtype:)
    // SAFETY: Both arguments are public Quartz constants, and the API returns a
    // scalar with no pointer ownership or callback lifetime requirements.
    idle_duration(unsafe {
        CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT)
    })
}

#[cfg(not(target_os = "macos"))]
fn native_idle() -> Reading<Duration> {
    Reading::Unavailable("Quartz idle detection requires macOS")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_idle_preserves_fractional_seconds() {
        assert_eq!(idle_duration(0.0), Reading::Known(Duration::ZERO));
        assert_eq!(
            idle_duration(1.25),
            Reading::Known(Duration::from_millis(1250))
        );
        assert_eq!(
            idle_duration(86_400.0),
            Reading::Known(Duration::from_secs(86_400))
        );
    }

    #[test]
    fn invalid_native_values_are_not_fabricated_activity() {
        for seconds in [
            -1.0,
            -0.000_001,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            u64::MAX as f64,
        ] {
            assert!(
                matches!(idle_duration(seconds), Reading::Failed(_)),
                "{seconds}"
            );
        }
    }

    #[test]
    fn successful_idle_does_not_establish_safe_desktop_state() {
        let now = Instant::now();
        let result = snapshot(now, Reading::Known(Duration::from_secs(20)));
        assert_eq!(result.sampled_at, now);
        assert_eq!(result.idle, Reading::Known(Duration::from_secs(20)));
        assert!(matches!(result.fullscreen, Reading::Unsupported(_)));
        assert!(matches!(result.interruptible, Reading::Unsupported(_)));
        assert!(result.automatic_pause_reason(now).is_some());
    }

    #[test]
    fn unavailable_idle_is_preserved() {
        let now = Instant::now();
        let idle = Reading::Unavailable("No Quartz GUI session");
        assert_eq!(snapshot(now, idle).idle, idle);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_macos_never_claims_native_idle() {
        assert!(matches!(
            NativeSensors::new().sample(Instant::now()).idle,
            Reading::Unavailable(_)
        ));
    }
}
