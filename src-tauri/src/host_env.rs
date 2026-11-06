// Moved to launcher-core (phase 2, no behaviour change); re-export keeps every
// `crate::host_env::…` call site unchanged.
// Only native_update (a Linux-only module) uses this re-export; gating it
// here keeps clippy -D warnings green on Windows/macOS without allow.
#[cfg(target_os = "linux")]
pub use launcher_core::host_env::*;
