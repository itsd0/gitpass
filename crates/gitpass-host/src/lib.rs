//! gitpass-host: native messaging host for the gitpass browser extension.
//!
//! v0.6: when the gitpass desktop app is running, the host connects to its agent socket and proxies
//! every command to it (unlocking the app unlocks the extension); app lock/unlock events are
//! forwarded to the extension. When the app is not running, the host falls back to standalone mode:
//! it unlocks the vault with password + Secret Key and keeps the [`gitpass_core::Session`] in memory
//! only (with idle auto-lock). Keys never live in the browser.
pub mod agent;
pub mod bridge;
pub mod framing;
pub mod handler;
pub mod install;
pub mod origin;
pub mod passkey;

/// Native messaging host name (must match `chrome.runtime.connectNative(...)`).
pub const HOST_NAME: &str = "com.gitpass.host";
