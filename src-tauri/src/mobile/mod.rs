//! Mobile-facing modules.
//!
//! The [`pairing`] submodule is always compiled when `relay-client` is on
//! (desktop test harness included) because its handshake exercises the
//! shared crypto + dispatcher code paths. The actual Tauri mobile app
//! entry point in [`app`] is gated to `mobile-runtime` so desktop builds
//! don't try to link in iOS/Android plugins.

pub mod pairing;

#[cfg(all(feature = "tauri-runtime", any(target_os = "ios", target_os = "android")))]
mod app;

#[cfg(all(feature = "tauri-runtime", any(target_os = "ios", target_os = "android")))]
pub use app::run;
