//! Explicit native acceptance probe, never run on a developer's desktop by default.
#![cfg(all(target_os = "linux", feature = "desktop"))]

use eyeflow::platform::linux::NativeSensors;
use eyeflow::platform::sensors::{Reading, SensorSnapshot};
use std::process::Command;
use std::time::{Duration, Instant};

fn fresh_sample_after(sensors: &mut NativeSensors, after: Instant) -> SensorSnapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let now = Instant::now();
        let sample = sensors.sample(now);
        if sample.sampled_at >= after
            && sample.is_fresh(now)
            && matches!(sample.idle, Reading::Known(_))
        {
            return sample;
        }
        assert!(
            now < deadline,
            "No fresh native X11 idle reading: {sample:?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn idle(sample: SensorSnapshot) -> Duration {
    match sample.idle {
        Reading::Known(idle) => idle,
        other => panic!("Expected real X11 idle, got {other:?}"),
    }
}

fn move_pointer(x: &str, y: &str) {
    assert!(Command::new("xdotool")
        .args(["mousemove", x, y])
        .status()
        .expect("isolated native probe requires xdotool")
        .success());
}

#[test]
#[ignore = "Requires an explicitly isolated Xvfb display and xdotool; run in native smoke CI"]
fn isolated_x11_idle_grows_and_resets_without_fabricating_lock_state() {
    assert_eq!(
        std::env::var("EYEFLOW_ISOLATED_X11_TEST").as_deref(),
        Ok("1"),
        "Refusing input injection without the isolated CI display marker"
    );
    assert_eq!(std::env::var("XDG_SESSION_TYPE").as_deref(), Ok("x11"));
    assert!(std::env::var("WAYLAND_DISPLAY")
        .unwrap_or_default()
        .is_empty());
    assert!(std::env::var("DISPLAY").is_ok());
    let mut sensors = NativeSensors::new();
    move_pointer("10", "10");
    let first = fresh_sample_after(&mut sensors, Instant::now());
    std::thread::sleep(Duration::from_millis(1300));
    let later = fresh_sample_after(&mut sensors, Instant::now());
    assert!(
        idle(later) >= idle(first) + Duration::from_millis(900),
        "Idle did not grow with real elapsed input inactivity: {first:?} -> {later:?}"
    );
    // A loaded runner can delay a background round trip. Retry input a bounded
    // number of times, but never weaken the measured idle-reset requirement.
    let mut reset = later;
    for x in ["30", "40", "50"] {
        move_pointer(x, "30");
        reset = fresh_sample_after(&mut sensors, Instant::now());
        if idle(reset) < Duration::from_secs(1) {
            break;
        }
    }
    assert!(
        idle(reset) < Duration::from_secs(1),
        "Input did not reset idle after three bounded attempts: {reset:?}"
    );
    assert!(
        !matches!(reset.interruptible, Reading::Known(true)),
        "Bare Xvfb without an authenticated desktop session cannot establish unlocked: {reset:?}"
    );
    assert!(
        reset.automatic_pause_reason(Instant::now()).is_some(),
        "An unknown bare-Xvfb session must not enable automatic reminders"
    );
}
