//! Encrypted, synced generator history (vault root `generator_history.enc`).
//!
//! Stores the newest 100 generated passwords, dropping entries older than 90 days.
//! Sealed with an XChaCha20-Poly1305 envelope under an HKDF32 key derived from
//! the Account Key with AAD `"{account_id}|generator-history|1"`.
//! The blake3 hash is authenticated in the manifest (`generator_history_hash`).

use crate::crypto::{hkdf32, open, seal, Envelope, Key32};
use crate::store::{pad, read_json, unpad, write_json};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const GEN_HISTORY_FILE: &str = "generator_history.enc";
pub const GEN_HISTORY_FORMAT: u32 = 1;
pub const MAX_HISTORY_ENTRIES: usize = 100;
pub const MAX_HISTORY_AGE_SECS: u64 = 90 * 86400; // 90 days

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct GeneratorHistoryEntry {
    #[zeroize(skip)]
    pub id: String,
    pub value: String,
    #[zeroize(skip)]
    pub created_at: u64,
    #[zeroize(skip)]
    pub source: String,
    #[zeroize(skip)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GeneratorHistory {
    pub format: u32,
    pub entries: Vec<GeneratorHistoryEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleared_at: Option<u64>,
}

impl Default for GeneratorHistory {
    fn default() -> Self {
        GeneratorHistory {
            format: GEN_HISTORY_FORMAT,
            entries: Vec::new(),
            cleared_at: None,
        }
    }
}

impl GeneratorHistory {
    pub fn prune(&mut self, now: u64) {
        let cutoff = now.saturating_sub(MAX_HISTORY_AGE_SECS);
        self.entries.retain(|e| e.created_at >= cutoff);
        if let Some(c) = self.cleared_at {
            self.entries.retain(|e| e.created_at > c);
        }
        if self.entries.len() > MAX_HISTORY_ENTRIES {
            self.entries.truncate(MAX_HISTORY_ENTRIES);
        }
    }

    pub fn add_entry(&mut self, value: String, source: &str, host: Option<String>, now: u64) -> GeneratorHistoryEntry {
        let entry = GeneratorHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            value,
            created_at: now,
            source: source.to_string(),
            host,
        };
        self.entries.insert(0, entry.clone());
        self.prune(now);
        entry
    }

    pub fn delete_entry(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() < before
    }

    pub fn clear(&mut self, now: u64) {
        self.entries.clear();
        self.cleared_at = Some(now);
    }

    pub fn merge(&self, remote: &GeneratorHistory, now: u64) -> GeneratorHistory {
        let max_cleared = match (self.cleared_at, remote.cleared_at) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let cutoff = now.saturating_sub(MAX_HISTORY_AGE_SECS);

        let mut seen_ids = HashSet::new();
        let mut seen_tuples = HashSet::new();
        let mut combined = Vec::new();

        for e in self.entries.iter().chain(remote.entries.iter()) {
            if e.created_at < cutoff {
                continue;
            }
            if let Some(c) = max_cleared {
                if e.created_at <= c {
                    continue;
                }
            }
            if !seen_ids.insert(e.id.clone()) {
                continue;
            }
            let key = (e.value.clone(), e.created_at, e.source.clone(), e.host.clone());
            if !seen_tuples.insert(key) {
                continue;
            }
            combined.push(e.clone());
        }

        combined.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        if combined.len() > MAX_HISTORY_ENTRIES {
            combined.truncate(MAX_HISTORY_ENTRIES);
        }

        GeneratorHistory {
            format: GEN_HISTORY_FORMAT,
            entries: combined,
            cleared_at: max_cleared,
        }
    }
}

fn gen_history_key(account_key: &Key32) -> Key32 {
    hkdf32(&account_key.0, None, b"gitpass/generator-history/v1")
}

fn gen_history_aad(account_id: &str) -> Vec<u8> {
    format!("{account_id}|generator-history|1").into_bytes()
}

pub fn seal_history(account_key: &Key32, account_id: &str, h: &GeneratorHistory) -> Result<Envelope> {
    let pt = pad(&serde_json::to_vec(h)?);
    seal(&gen_history_key(account_key), &pt, &gen_history_aad(account_id))
}

pub fn write_history(root: &Path, account_key: &Key32, account_id: &str, h: &GeneratorHistory) -> Result<String> {
    let env = seal_history(account_key, account_id, h)?;
    write_json(&root.join(GEN_HISTORY_FILE), &env)
}

pub fn read_history(root: &Path, account_key: &Key32, account_id: &str) -> Result<GeneratorHistory> {
    let p = root.join(GEN_HISTORY_FILE);
    if !p.exists() {
        return Ok(GeneratorHistory::default());
    }
    let env: Envelope = read_json(&p)?;
    let pt = unpad(&open(&gen_history_key(account_key), &env, &gen_history_aad(account_id))?)?;
    let h: GeneratorHistory = serde_json::from_slice(&pt)?;
    if h.format > GEN_HISTORY_FORMAT {
        bail!("generator history format {} is newer than supported ({GEN_HISTORY_FORMAT})", h.format);
    }
    Ok(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_cap_at_100() {
        let mut gh = GeneratorHistory::default();
        let base_time = 1_000_000u64;
        for i in 0..120 {
            gh.add_entry(format!("pw-{i}"), "desktop", None, base_time + i);
        }
        assert_eq!(gh.entries.len(), 100);
        // Newest should be first (pw-119)
        assert_eq!(gh.entries[0].value, "pw-119");
        // Oldest retained should be pw-20
        assert_eq!(gh.entries[99].value, "pw-20");
    }

    #[test]
    fn test_90_day_expiry() {
        let mut gh = GeneratorHistory::default();
        let now = 100 * 86400u64; // day 100
        // Added on day 5 (95 days ago -> expired)
        gh.add_entry("old-pw".into(), "cli", None, 5 * 86400);
        // Added on day 15 (85 days ago -> alive)
        gh.add_entry("new-pw".into(), "cli", None, 15 * 86400);

        gh.prune(now);
        assert_eq!(gh.entries.len(), 1);
        assert_eq!(gh.entries[0].value, "new-pw");
    }

    #[test]
    fn test_clear_and_merge_two_devices() {
        let now = 1000u64;
        let mut dev_a = GeneratorHistory::default();
        dev_a.add_entry("dev-a-1".into(), "desktop", Some("github.com".into()), 100);
        dev_a.add_entry("dev-a-2".into(), "extension", Some("gitlab.com".into()), 200);

        let mut dev_b = GeneratorHistory::default();
        dev_b.add_entry("dev-b-1".into(), "cli", None, 150);
        dev_b.add_entry("dev-b-2".into(), "desktop", None, 250);

        let merged = dev_a.merge(&dev_b, now);
        assert_eq!(merged.entries.len(), 4);
        assert_eq!(merged.entries[0].value, "dev-b-2"); // 250
        assert_eq!(merged.entries[1].value, "dev-a-2"); // 200
        assert_eq!(merged.entries[2].value, "dev-b-1"); // 150
        assert_eq!(merged.entries[3].value, "dev-a-1"); // 100

        // Now dev_a clears history at t=180
        dev_a.clear(180);
        assert!(dev_a.entries.is_empty());

        // Merge with dev_b: entries <= 180 (dev-a-1, dev-b-1) must be dropped!
        let merged_after_clear = dev_a.merge(&dev_b, now);
        assert_eq!(merged_after_clear.entries.len(), 1);
        assert_eq!(merged_after_clear.entries[0].value, "dev-b-2"); // 250 > 180
    }
}
