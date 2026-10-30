//! Shared unlock failure throttling: after 3 consecutive wrong password attempts per vault,
//! the next attempt is refused with a growing delay (doubling from 2 s up to 5 min cap).
//! Reset on successful unlock (or biometric unlock). Persists across restarts in `.gitpass-throttle.json`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const THROTTLE_FILE: &str = ".gitpass-throttle.json";
pub const FAILURE_THRESHOLD: u32 = 3;
pub const INITIAL_DELAY_SECS: u64 = 2;
pub const MAX_DELAY_SECS: u64 = 300; // 5 minutes

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ThrottleRecord {
    pub failures: u32,
    pub retry_after: u64, // Unix timestamp in seconds
    pub updated_at: u64,  // Unix timestamp in seconds
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThrottledError {
    pub seconds_remaining: u64,
}

impl std::fmt::Display for ThrottledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "throttled: try again in {} s", self.seconds_remaining)
    }
}

impl std::error::Error for ThrottledError {}

static MONOTONIC: OnceLock<Mutex<HashMap<PathBuf, Instant>>> = OnceLock::new();

fn monotonic_map() -> &'static Mutex<HashMap<PathBuf, Instant>> {
    MONOTONIC.get_or_init(|| Mutex::new(HashMap::new()))
}

fn canonical_path(root: &Path) -> PathBuf {
    fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

/// Calculate backoff delay in seconds based on consecutive failures.
/// 0 for < 3 failures; 2 s at 3 failures, doubling on each subsequent failure up to 300 s (5 min).
pub fn backoff_delay_secs(failures: u32) -> u64 {
    if failures < FAILURE_THRESHOLD {
        return 0;
    }
    let shift = (failures - FAILURE_THRESHOLD).min(30);
    let delay = INITIAL_DELAY_SECS.saturating_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX));
    delay.min(MAX_DELAY_SECS)
}

fn throttle_path(root: &Path) -> PathBuf {
    root.join(THROTTLE_FILE)
}

pub fn check_monotonic(root: &Path) -> Option<u64> {
    let key = canonical_path(root);
    let map = monotonic_map().lock().ok()?;
    let deadline = map.get(&key)?;
    let now = Instant::now();
    if now < *deadline {
        let d = deadline.duration_since(now);
        let secs = d.as_secs() + if d.subsec_nanos() > 0 { 1 } else { 0 };
        Some(secs.max(1))
    } else {
        None
    }
}

pub fn check_disk(root: &Path) -> Option<u64> {
    let p = throttle_path(root);
    let b = fs::read(&p).ok()?;
    let rec: ThrottleRecord = serde_json::from_slice(&b).ok()?;
    if rec.failures < FAILURE_THRESHOLD || rec.retry_after == 0 {
        return None;
    }
    let now_wall = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if now_wall < rec.retry_after {
        let diff = rec.retry_after - now_wall;
        // If clock rolled back before updated_at, guarantee at least the full backoff delay
        if now_wall < rec.updated_at {
            let delay = rec.retry_after.saturating_sub(rec.updated_at);
            Some(delay.max(diff).clamp(1, MAX_DELAY_SECS))
        } else {
            Some(diff.clamp(1, MAX_DELAY_SECS))
        }
    } else {
        None
    }
}

/// Check remaining throttled seconds (combining monotonic timer and persisted disk record).
pub fn seconds_remaining(root: &Path) -> Option<u64> {
    let m = check_monotonic(root);
    let d = check_disk(root);
    match (m, d) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Verify if the vault is currently throttled. Returns `Err(ThrottledError)` with remaining seconds if so.
pub fn check_throttle(root: &Path) -> Result<(), ThrottledError> {
    if let Some(secs) = seconds_remaining(root) {
        if secs > 0 {
            return Err(ThrottledError { seconds_remaining: secs });
        }
    }
    Ok(())
}

/// Record a failed master-password attempt for the vault at `root`.
/// Returns the resulting backoff delay in seconds (0 if < 3 failures).
pub fn record_failure(root: &Path) -> Result<u64> {
    use fs2::FileExt;
    let _ = fs::create_dir_all(root);
    let dir = fs::File::open(root).with_context(|| format!("opening {}", root.display()))?;
    dir.lock_exclusive().context("locking vault directory for throttle")?;
    let p = throttle_path(root);
    let cur: ThrottleRecord = match fs::read(&p) {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
        Err(_) => ThrottleRecord::default(),
    };
    let now_wall = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let failures = cur.failures.saturating_add(1);
    let delay = backoff_delay_secs(failures);
    let retry_after = if delay > 0 {
        now_wall.saturating_add(delay)
    } else {
        0
    };
    let next = ThrottleRecord {
        failures,
        retry_after,
        updated_at: now_wall,
    };
    let bytes = serde_json::to_vec_pretty(&next)?;
    crate::fsutil::atomic_write(&p, &bytes)?;
    let _ = FileExt::unlock(&dir);

    if delay > 0 {
        let key = canonical_path(root);
        let deadline = Instant::now() + Duration::from_secs(delay);
        if let Ok(mut map) = monotonic_map().lock() {
            map.insert(key, deadline);
        }
    }
    Ok(delay)
}

/// Record a successful unlock for the vault at `root`, clearing failure counts and delays.
pub fn record_success(root: &Path) -> Result<()> {
    use fs2::FileExt;
    let key = canonical_path(root);
    if let Ok(mut map) = monotonic_map().lock() {
        map.remove(&key);
    }
    let p = throttle_path(root);
    if p.exists() {
        if let Ok(dir) = fs::File::open(root) {
            let _ = dir.lock_exclusive();
            let rec = ThrottleRecord::default();
            if let Ok(bytes) = serde_json::to_vec_pretty(&rec) {
                let _ = crate::fsutil::atomic_write(&p, &bytes);
            }
            let _ = FileExt::unlock(&dir);
        }
    }
    Ok(())
}

/// Read the current persisted record on disk (for testing/inspection).
pub fn read_record(root: &Path) -> Option<ThrottleRecord> {
    let p = throttle_path(root);
    let b = fs::read(p).ok()?;
    serde_json::from_slice(&b).ok()
}

/// Clear in-memory monotonic throttle state (useful in test setups).
pub fn reset_memory() {
    if let Ok(mut map) = monotonic_map().lock() {
        map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_delay_calculation() {
        assert_eq!(backoff_delay_secs(0), 0);
        assert_eq!(backoff_delay_secs(1), 0);
        assert_eq!(backoff_delay_secs(2), 0);
        assert_eq!(backoff_delay_secs(3), 2);
        assert_eq!(backoff_delay_secs(4), 4);
        assert_eq!(backoff_delay_secs(5), 8);
        assert_eq!(backoff_delay_secs(6), 16);
        assert_eq!(backoff_delay_secs(7), 32);
        assert_eq!(backoff_delay_secs(8), 64);
        assert_eq!(backoff_delay_secs(9), 128);
        assert_eq!(backoff_delay_secs(10), 256);
        assert_eq!(backoff_delay_secs(11), 300);
        assert_eq!(backoff_delay_secs(12), 300);
        assert_eq!(backoff_delay_secs(100), 300);
    }

    #[test]
    fn test_failure_counting_and_reset_on_success() {
        reset_memory();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // 1st failure: delay 0, not throttled
        assert_eq!(record_failure(root).unwrap(), 0);
        assert!(check_throttle(root).is_ok());

        // 2nd failure: delay 0, not throttled
        assert_eq!(record_failure(root).unwrap(), 0);
        assert!(check_throttle(root).is_ok());

        // 3rd failure: delay 2 s, throttled
        assert_eq!(record_failure(root).unwrap(), 2);
        let err = check_throttle(root).unwrap_err();
        assert!((1..=2).contains(&err.seconds_remaining));
        assert!(err.to_string().starts_with("throttled: try again in "));

        // Success resets failure count and throttle
        record_success(root).unwrap();
        assert!(check_throttle(root).is_ok());
        let rec = read_record(root).unwrap();
        assert_eq!(rec.failures, 0);
        assert_eq!(rec.retry_after, 0);
    }

    #[test]
    fn test_persistence_across_restart() {
        reset_memory();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Trigger 3 failures
        record_failure(root).unwrap();
        record_failure(root).unwrap();
        record_failure(root).unwrap();

        // Simulate app restart by clearing in-memory state
        reset_memory();

        // Disk state should still report throttled
        let remaining = seconds_remaining(root);
        assert!(remaining.is_some());
        assert!(remaining.unwrap() >= 1);
        let err = check_throttle(root).unwrap_err();
        assert_eq!(err.seconds_remaining, remaining.unwrap());
    }

    #[test]
    fn test_clock_rollback_protection() {
        reset_memory();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Simulate a failure recorded with updated_at in the future relative to current clock (clock rolled back)
        let now_wall = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let p = throttle_path(root);
        let rec = ThrottleRecord {
            failures: 3,
            retry_after: now_wall + 102,
            updated_at: now_wall + 100,
        };
        fs::write(&p, serde_json::to_vec(&rec).unwrap()).unwrap();

        // check_disk detects that now_wall < updated_at <= retry_after and enforces delay
        let rem = check_disk(root);
        assert!(rem.is_some());
        assert!(rem.unwrap() >= 2);
    }
}
