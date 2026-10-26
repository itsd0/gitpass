//! Windows agent — STUB.
//!
//! On macOS/Linux the desktop app serves the unlocked session to `gitpass-host` over a UNIX socket
//! (`agent.rs`). The Windows transport is planned as a named pipe (`\\.\pipe\gitpass-agent`, see
//! `gitpass_host::agent::WINDOWS_PIPE_NAME`) restricted to the current user's SID, with the same
//! pairing-token `hello`. It is NOT implemented: `Agent::start` returns an error, the app keeps
//! working, and the browser extension uses the host's standalone (password + Secret Key) mode.
use crate::api::App;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const PIPE_NAME: &str = r"\\.\pipe\gitpass-agent";

/// UI side effects the Tauri shell provides (same trait as the UNIX agent).
pub trait AgentUi: Send + Sync {
    fn request_unlock(&self, reason: &str);
    fn open_item(&self, vault_id: Option<&str>, item_id: Option<&str>);
    /// Ask the user to allow a sensitive action (SSH signature, passkey). Default: deny.
    fn approve(&self, _title: &str, _message: &str) -> bool { false }
}
pub struct NoUi;
impl AgentUi for NoUi { fn request_unlock(&self, _: &str) {} fn open_item(&self, _: Option<&str>, _: Option<&str>) {} }

/// Directory for agent state (token). Unused while the pipe server is a stub.
pub fn default_dir() -> PathBuf { dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("gitpass") }

pub struct Agent;
impl Agent {
    pub fn start(_app: Arc<App>, _ui: Arc<dyn AgentUi>, _dir: &Path) -> Result<Arc<Agent>> {
        bail!("extension agent (named pipe {PIPE_NAME}) is not implemented on Windows yet")
    }
}
