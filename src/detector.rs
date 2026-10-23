//! Native detection entry points. OS adapters report unknown capabilities explicitly.

#[cfg(target_os = "windows")]
#[path = "platform/windows_detector.rs"]
mod native;
#[cfg(target_os = "windows")]
pub use native::{start_keyboard_hook, start_sensor_thread, stop_keyboard_hook};

#[cfg(not(target_os = "windows"))]
pub fn start_sensor_thread(tx: std::sync::mpsc::Sender<crate::event::Event>) {
    std::thread::Builder::new()
        .name("eyeflow-sensors".into())
        .spawn(move || {
            #[cfg(target_os = "linux")]
            let mut native = crate::platform::linux::NativeSensors::new();
            #[cfg(target_os = "macos")]
            let mut native = crate::platform::macos::NativeSensors::new();
            loop {
                let snapshot = native.sample(std::time::Instant::now());
                if tx.send(crate::event::Event::Sensors(snapshot)).is_err() {
                    break;
                }
                crate::event::wake_ui();
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        })
        .expect("could not start native sensor thread");
}

#[cfg(not(target_os = "windows"))]
pub fn start_keyboard_hook(_tx: std::sync::mpsc::Sender<crate::event::Event>) {
    // No global capture/permissions are requested merely to start the application.
    log::info!("Native typing-flow detection is unavailable on this platform");
}

#[cfg(not(target_os = "windows"))]
pub fn stop_keyboard_hook() {}
