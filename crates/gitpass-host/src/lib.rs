//! gitpass-host: native messaging host for the gitpass browser extension.
//!
//! Keys never live in the browser: this process unlocks the vault with gitpass-core and keeps
//! the [`gitpass_core::Session`] in memory (with idle auto-lock). The extension only receives
//! item metadata, and a username/password only via `fill` after an origin check.
pub mod framing;
pub mod handler;
pub mod install;
pub mod origin;

/// Native messaging host name (must match `chrome.runtime.connectNative(...)`).
pub const HOST_NAME: &str = "com.gitpass.host";
