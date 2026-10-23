//! GUI-independent shared scheduling, configuration, statistics and translations.
//!
//! Test without native desktop dependencies with
//! `cargo test --lib --no-default-features`. This is not a headless desktop app:
//! the executable still requires native platform integration. Configuration and
//! statistics retain their existing filesystem persistence APIs.

pub mod config;
pub mod core;
pub mod duration_format;
pub mod i18n;
pub mod stats;
pub mod tr;

pub mod platform;
