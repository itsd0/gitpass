//! In-memory activity log for the desktop SSH agent (last 50 sign requests).
//!
//! Only metadata is kept: when, which key, which algorithm was requested, the result and the
//! client (pid/name from the socket peer). The signed data and the key material never enter the
//! log; nothing is written to disk or to the vault, and [`SshActivityLog::clear`] runs on lock.
use gitpass_core::device::iso8601;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_ACTIVITY: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SshActivityResult { Signed, Denied, Refused }

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshActivityEntry {
    pub time: String,
    pub key_title: String,
    pub fingerprint: String,
    pub key_type: String,
    /// Requested agent algorithm: `ed25519`, `ecdsa`, `rsa-sha2-256`, `rsa-sha2-512`, `ssh-rsa` (SHA-1) or `unknown`.
    pub algorithm: String,
    pub result: SshActivityResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}

#[derive(Default)]
pub struct SshActivityLog { entries: Mutex<VecDeque<SshActivityEntry>> }

impl SshActivityLog {
    /// Push an entry (newest first) and drop anything past the last [`MAX_ACTIVITY`] requests.
    pub fn record(&self, entry: SshActivityEntry) {
        let Ok(mut e) = self.entries.lock() else { return };
        e.push_front(entry);
        e.truncate(MAX_ACTIVITY);
    }
    /// Newest first.
    pub fn entries(&self) -> Vec<SshActivityEntry> {
        self.entries.lock().map(|e| e.iter().cloned().collect()).unwrap_or_default()
    }
    pub fn clear(&self) {
        if let Ok(mut e) = self.entries.lock() { e.clear(); }
    }
}

pub fn now_iso() -> String {
    iso8601(SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str) -> SshActivityEntry {
        SshActivityEntry { time: now_iso(), key_title: title.into(), fingerprint: format!("SHA256:{title}"), key_type: "ssh-ed25519".into(),
            algorithm: "ed25519".into(), result: SshActivityResult::Signed, client: None, pid: None }
    }

    #[test]
    fn caps_at_50_newest_first() {
        let log = SshActivityLog::default();
        for i in 0..55 { log.record(entry(&format!("k{i}"))); }
        let e = log.entries();
        assert_eq!(e.len(), MAX_ACTIVITY);
        assert_eq!(e[0].key_title, "k54");
        assert_eq!(e[MAX_ACTIVITY - 1].key_title, "k5");
    }

    #[test]
    fn clear_empties_the_log() {
        let log = SshActivityLog::default();
        log.record(entry("k"));
        assert_eq!(log.entries().len(), 1);
        log.clear();
        assert!(log.entries().is_empty());
    }

    #[test]
    fn serialises_camel_case_and_lowercase_result() {
        let v = serde_json::to_value(entry("k")).unwrap();
        assert_eq!(v["keyTitle"], "k");
        assert_eq!(v["result"], "signed");
        assert!(v.get("client").is_none() && v.get("pid").is_none());
    }
}
