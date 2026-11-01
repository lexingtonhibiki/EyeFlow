//! Native platform boundaries and their portable data contracts.
pub mod sensors;

#[cfg(any(target_os = "macos", test))]
pub mod macos;

#[cfg(all(target_os = "linux", feature = "desktop"))]
pub mod linux;

#[cfg(any(target_os = "windows", test))]
pub mod windows_policy;
