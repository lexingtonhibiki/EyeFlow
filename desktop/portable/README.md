# Portable desktop session preview

Run `cargo run --manifest-path desktop/portable/Cargo.toml` with Rust and native desktop graphics libraries installed. Linux needs X11/Wayland development packages; macOS needs Xcode command-line tools. Windows's established tray app remains at the repository root.

This separate host reuses the same Rust scheduler on Windows, macOS and Linux. It persists remaining active-time schedule and local statistics under the OS app-data directory. A session begins paused on launch. The app counts time only while its window is foreground; background/minimized/inactive time is conservatively excluded. Manual focus mode defers reminders. This is an explicitly limited foreground preview, not yet a replacement for Windows global keyboard/idle/full-screen sensing, tray behavior or OS lock integration. Builds on other OSes require their toolchains; a Linux cargo check is not macOS/Windows runtime validation.
