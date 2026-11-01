# Portable manual-break preview

Run `cargo run --manifest-path desktop/portable/Cargo.toml` with Rust and native desktop graphics libraries installed. Linux needs X11/Wayland development packages; macOS needs Xcode command-line tools. The established native desktop application remains at the repository root.

This separate host reuses the Rust scheduler on Windows, macOS and Linux, but **automatic reminders are paused**: it does not provide native idle, fullscreen or unlocked-session evidence. Foreground-window focus is not proof of an unlocked device. The UI shows the pause reason instead of presenting a frozen automatic schedule as an active countdown. It does not enable compatibility mode or invent healthy sensor readings.

Use **Rest now** for an explicit manual break. The session begins paused on launch; while running, the manual countdown advances only when the window is reported focused and not minimized, excluding update gaps of five seconds or more. Pause session freezes that local clock. Do not disturb cancels an active break. Manual state and statistics persist under the OS app-data directory; unknown time does not become screen use or natural-rest credit. Postponing preserves state but does not re-enable automatic reminders.

This is not a replacement for the native desktop app's sensing, tray behavior or OS lock integration. Builds on other OSes require their toolchains; a Linux cargo check is not macOS/Windows runtime validation.
