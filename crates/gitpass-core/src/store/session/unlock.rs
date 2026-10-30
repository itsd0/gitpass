//! Session methods (pure move from `store.rs`; see `session/mod.rs`).

use super::*;
use crate::fsutil;
use anyhow::{bail, Context, Result};
use std::path::Path;

impl Session {
    pub fn unlock(root: &Path, password: &str, secret: &SecretKey) -> Result<Self> {
        Self::unlock_with(root, password, secret, &UnlockOpts::default())
    }

    pub fn unlock_with(root: &Path, password: &str, secret: &SecretKey, opts: &UnlockOpts) -> Result<Self> {
        Self::unlock_device(root, password, secret, opts, None)
    }

    /// Unlock and register/refresh this device in the registry (`device_name` None => env/hostname).
    pub fn unlock_device(root: &Path, password: &str, secret: &SecretKey, opts: &UnlockOpts, device_name: Option<&str>) -> Result<Self> {
        Self::unlock_device_inner(root, password, secret, opts, device_name, true, false)
    }
    /// Unlock via biometric authentication: bypasses password failure throttling, but resets it on success.
    pub fn unlock_device_biometric(root: &Path, password: &str, secret: &SecretKey, opts: &UnlockOpts, device_name: Option<&str>) -> Result<Self> {
        Self::unlock_device_inner(root, password, secret, opts, device_name, true, true)
    }
    /// Read-only unlock for `gitpass doctor`: opens the keyset and loads the Account Key without touching the
    /// device registry, resealing the manifest, finishing a pending rewrap or upgrading the KDF. The only file
    /// it may write is the unlock throttle state (`.gitpass-throttle.json`): failures are still counted so
    /// brute-force throttling applies, and a success resets the counter.
    pub fn unlock_readonly(root: &Path, password: &str, secret: &SecretKey) -> Result<Self> {
        crate::unlock_throttle::check_throttle(root)?;
        let fmt = read_format(root)?;
        if fmt.version < MIN_READ_FORMAT { bail!("vault is format v{}; run `gitpass migrate` to upgrade to v{FORMAT_VERSION}", fmt.version); }
        let (auk, account_key) = match open_keyset_auk(root, &fmt, &root.join("keyset.enc"), password, secret) {
            Ok(k) => {
                let _ = crate::unlock_throttle::record_success(root);
                k
            }
            Err(e) => {
                let _ = crate::unlock_throttle::record_failure(root);
                return Err(e);
            }
        };
        let device_kek = device_kek(&auk, &fmt.account_id);
        drop(auk);
        let device_id = crate::device::local_device_id(root).unwrap_or_default();
        Ok(Session { root: root.to_path_buf(), fmt, account_key, kdf_upgraded: false, accepted_problems: vec![], device_id, device_revoked: false, device_kek })
    }
    /// Verify-only re-unlock used after a sync pull: same integrity/rollback checks, but never touches the
    /// device registry or upgrades the KDF, so the working tree stays exactly as committed/pulled.
    pub fn reopen_verify(root: &Path, password: &str, secret: &SecretKey, accept_rollback: bool) -> Result<Self> {
        let fmt = read_format(root)?;
        let opts = UnlockOpts { accept_rollback, target_kdf: fmt.kdf.clone() };
        Self::unlock_device_inner(root, password, secret, &opts, None, false, false)
    }
    fn unlock_device_inner(root: &Path, password: &str, secret: &SecretKey, opts: &UnlockOpts, device_name: Option<&str>, touch: bool, is_biometric: bool) -> Result<Self> {
        if !is_biometric {
            crate::unlock_throttle::check_throttle(root)?;
        }
        let recovered = {
            let _g = fsutil::lock(root)?;
            finish_pending_rewrap(root, password, secret)?
        };
        let fmt = read_format(root)?;
        if fmt.version < MIN_READ_FORMAT { bail!("vault is format v{}; run `gitpass migrate` to upgrade to v{FORMAT_VERSION}", fmt.version); }
        let (auk, account_key) = match open_keyset_auk(root, &fmt, &root.join("keyset.enc"), password, secret) {
            Ok(k) => {
                let _ = crate::unlock_throttle::record_success(root);
                k
            }
            Err(e) => {
                if !is_biometric {
                    let _ = crate::unlock_throttle::record_failure(root);
                }
                return Err(e);
            }
        };
        let device_kek = device_kek(&auk, &fmt.account_id);
        drop(auk);
        let device_id = crate::device::local_device_id(root)?;
        let mut s = Session { root: root.to_path_buf(), fmt, account_key, kdf_upgraded: false, accepted_problems: vec![], device_id, device_revoked: false, device_kek };
        if recovered { s.commit(|_| Ok(()))?; } // re-auth keyset hash after roll-forward
        let m = s.load_manifest().context("manifest.enc missing or failed authentication")?;
        if m.account_id != s.fmt.account_id || !(MIN_READ_FORMAT..=FORMAT_VERSION).contains(&m.format) { bail!("manifest does not belong to this account/format"); }
        let mut problems = verify(root, &m);
        let last = last_epoch(root, &s.fmt.account_id);
        if let Some(l) = last {
            if m.epoch < l { problems.push(format!("manifest epoch {} is older than last seen epoch {l} on this device (rollback?)", m.epoch)); }
        }
        if !problems.is_empty() {
            if !opts.accept_rollback {
                bail!("integrity check FAILED, refusing to continue (use --accept-rollback to override):\n  - {}", problems.join("\n  - "));
            }
            s.reseal(m.epoch.max(last.unwrap_or(0)))?;
            s.accepted_problems = problems;
        } else {
            save_epoch(root, &s.fmt.account_id, m.epoch)?;
        }
        if s.fmt.kdf.below(&opts.target_kdf) {
            let k = s.fmt.kdf.max_with(&opts.target_kdf);
            s.rewrap_keyset(password, secret, k)?;
            s.kdf_upgraded = true;
        }
        if touch {
            let name = device_name.map(str::to_string).unwrap_or_else(crate::device::default_device_name);
            s.touch_device(&name)?;
        } else if let Some(d) = s.devices()?.get(&s.device_id) {
            if d.revoked { s.device_revoked = true; }
        }
        // v5: purge soft-deleted items older than 30 days (best effort; never blocks unlock). Skipped on verify-only reopen.
        if touch && !s.device_revoked { let _ = s.purge_expired(now_secs()); }
        // Roll forward an interrupted `device revoke` (marker + key rotation) from this device.
        if let Ok(p) = read_json::<RevokePending>(&root.join(REVOKE_PENDING_FILE)) {
            if touch && !s.device_revoked && p.device_id != s.device_id {
                s.revoke_device_and_rotate(&p.device_id, password, secret)?;
            }
        }
        Ok(s)
    }
}
