use super::super::display::*;
use super::super::dto::*;
use super::App;
use anyhow::{anyhow, bail, Result};
use gitpass_core::store::{Session, UnlockOpts};
use std::collections::{BTreeMap};
use std::path::{PathBuf};
use zeroize::Zeroizing;

impl App {
    // ---------------- session
    pub fn status(&self) -> Result<UnlockState> {
        self.state.check_idle();
        let g = self.state.inner.lock().map_err(|_| anyhow!("state poisoned"))?;
        Ok(match &g.unlocked {
            Some(u) => UnlockState { unlocked: true, vault_path: u.session.root.display().to_string(), account: Some(u.session.fmt.account_id.clone()), vault_exists: true,
                biometric_available: false, throttled_secs: None },
            None => {
                let vp = self.read_local().last_vault_path.unwrap_or_else(|| crate::default_vault_dir().display().to_string());
                let p = PathBuf::from(&vp);
                let exists = p.join("format.json").exists();
                let bio = exists && self.biometric.supported() && self.get_settings().biometric_unlock;
                let throttled_secs = if exists {
                    gitpass_core::unlock_throttle::seconds_remaining(&p)
                } else {
                    None
                };
                UnlockState { unlocked: false, vault_path: vp, account: None, vault_exists: exists, biometric_available: bio, throttled_secs }
            }
        })
    }
    pub fn unlock(&self, vault_path: &str, password: &str, secret_key: Option<&str>) -> Result<UnlockState> {
        self.unlock_internal(vault_path, password, secret_key, false)
    }

    pub(crate) fn unlock_internal(&self, vault_path: &str, password: &str, secret_key: Option<&str>, is_biometric: bool) -> Result<UnlockState> {
        let dir = PathBuf::from(if vault_path.trim().is_empty() { crate::default_vault_dir().display().to_string() } else { vault_path.trim().to_string() });
        if password.is_empty() { bail!("master password is empty"); }
        if !dir.join("format.json").exists() { bail!("{} is not a gitpass vault (no format.json)", dir.display()); }
        if !is_biometric {
            gitpass_core::unlock_throttle::check_throttle(&dir)?;
        }
        let sk = match secret_key.filter(|k| !k.trim().is_empty()) {
            Some(k) => crate::parse_secret_key(k)?,
            None => crate::read_secret_key_file(&dir.join("secret.key"))?,
        };
        let opts = { let o = self.unlock_opts.lock().map_err(|_| anyhow!("poisoned"))?; UnlockOpts { accept_rollback: o.accept_rollback, target_kdf: o.target_kdf.clone() } };
        if is_biometric {
            self.state.unlock_biometric_with_opts(&dir, password, sk, None, &opts)?;
        } else {
            self.state.unlock_with_opts(&dir, password, sk, None, &opts)?;
        }
        let _ = self.update_local(|l| l.last_vault_path = Some(dir.display().to_string()));
        self.ensure_default_vault();
        self.status()
    }
    /// `gitpass init` creates no vault; give a fresh account a "Personal" vault so the sidebar is not empty.
    pub(crate) fn ensure_default_vault(&self) {
        let _ = self.write(|u| { if u.session.list_vaults()?.is_empty() { u.session.create_vault_full("Personal", None, None, None)?; } Ok(()) });
    }
    /// Create a new account/vault at `vault_path` (like `gitpass init`) and unlock it.
    /// Returns the Secret Key ONCE for the user to save; it is written to disk only if `save_secret_key`.
    pub fn create_account(&self, vault_path: &str, password: &str, save_secret_key: bool) -> Result<CreateAccountResult> {
        let dir = PathBuf::from(if vault_path.trim().is_empty() { crate::default_vault_dir().display().to_string() } else { vault_path.trim().to_string() });
        if password.chars().count() < 8 { bail!("master password must be at least 8 characters"); }
        if dir.join("format.json").exists() { bail!("{} already contains a gitpass vault", dir.display()); }
        let sk = gitpass_core::store::init(&dir, password, gitpass_core::crypto::KdfParams::default())?;
        let enc = Zeroizing::new(sk.encode());
        if save_secret_key {
            gitpass_core::fsutil::atomic_write(&dir.join("secret.key"), format!("{}\n", enc.as_str()).as_bytes())?;
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(dir.join("secret.key"), std::fs::Permissions::from_mode(0o600))?; }
        }
        self.unlock(&dir.display().to_string(), password, Some(&enc))?;
        if let Ok(acc) = self.with(|u| Ok(u.session.fmt.account_id.clone())) {
            *self.fresh_secret.lock().map_err(|_| anyhow!("poisoned"))? = Some((acc, enc.clone()));
        }
        Ok(CreateAccountResult { vault_path: dir.display().to_string(), secret_key: enc.to_string() })
    }
    /// Unlock with credentials released by the platform biometric store (Touch ID on macOS).
    pub fn unlock_biometric(&self, vault_path: &str) -> Result<UnlockState> {
        if !self.get_settings().biometric_unlock { bail!("biometric unlock is not enabled"); }
        let vp = if vault_path.trim().is_empty() { self.status()?.vault_path } else { vault_path.trim().to_string() };
        let (pw, sk) = self.biometric.load(&vp, "unlock gitpass")?;
        self.unlock_internal(&vp, &pw, Some(&sk), true)
    }
    pub fn lock(&self) {
        self.clear_clipboard_if_ours();
        self.state.lock();
        self.ssh_activity.clear();
        if let Ok(mut k) = self.fresh_kit.lock() { *k = None; }
        if let Ok(mut k) = self.fresh_secret.lock() { *k = None; }
    }

    /// Handle a system event (sleep, screen lock, fast user switch).
    /// Returns true if the vault was unlocked and locked as a result of the event, false if ignored or already locked.
    pub fn handle_system_lock_event(&self, trigger: LockTrigger) -> bool {
        let s = self.get_settings();
        let should_lock = match trigger {
            LockTrigger::Sleep => s.lock_on_sleep,
            LockTrigger::ScreenLock => s.lock_on_screen_lock,
            LockTrigger::UserSwitch => s.lock_on_user_switch,
        };
        if should_lock && self.state.is_unlocked() {
            self.lock();
            true
        } else {
            false
        }
    }

    /// Clears the clipboard immediately if it still holds the text copied by [`App::copy_field`].
    pub fn clear_clipboard_if_ours(&self) {
        if let Ok(mut lc) = self.last_copied.lock() {
            if let Some(token) = lc.take() {
                if self.clipboard.get_text().as_deref() == Some(token.as_str()) {
                    let _ = self.clipboard.set_text("");
                }
            }
        }
    }

    // ---------------- vaults
    pub(crate) fn members_of(s: &Session, vid: &str, labels: &BTreeMap<String, String>) -> Result<Vec<Member>> {
        let mut out = vec![];
        let ms = s.members(vid)?;
        if !ms.is_empty() {
            if let Some((_, fpr)) = s.identity_if_exists()? { out.push(Member { label: labels.get(&fpr).cloned(), fingerprint: fpr, is_self: true }); }
        }
        for (fpr, _) in ms { out.push(Member { label: labels.get(&fpr).cloned(), fingerprint: fpr, is_self: false }); }
        Ok(out)
    }
    pub(crate) fn vault_info(s: &Session, vid: &str, labels: &BTreeMap<String, String>) -> Result<VaultInfo> {
        let m = s.vault_meta(vid)?;
        let count = s.list_records(vid)?.iter().filter(|(_, _, r)| r.deleted_at.is_none()).count();
        let members = Self::members_of(s, vid, labels)?;
        Ok(VaultInfo { id: vid.into(), name: m.name.clone(), description: m.description.clone(), item_count: count, shared: !members.is_empty(),
            members, icon: m.icon.clone(), color: m.color.clone() })
    }
    pub fn list_vaults(&self) -> Result<Vec<VaultInfo>> {
        let labels = self.read_local().member_labels;
        self.with(|u| u.session.list_vaults()?.iter().map(|(id, _)| Self::vault_info(&u.session, id, &labels)).collect())
    }
    pub fn create_vault(&self, name: &str, description: Option<String>, icon: Option<String>, color: Option<String>) -> Result<VaultInfo> {
        if name.trim().is_empty() { bail!("vault name is empty"); }
        let labels = self.read_local().member_labels;
        self.write(|u| {
            let id = u.session.create_vault_full(name.trim(), nonempty(description), nonempty(icon), nonempty(color))?;
            Self::vault_info(&u.session, &id, &labels)
        })
    }

    /// Rename / re-describe / re-style a vault. `None` description/icon/color clears them.
    pub fn update_vault(&self, vault_id: &str, name: &str, description: Option<String>, icon: Option<String>, color: Option<String>) -> Result<VaultInfo> {
        let labels = self.read_local().member_labels;
        self.write(|u| {
            u.session.update_vault_meta(vault_id, gitpass_core::store::VaultMeta { name: name.trim().into(), description: nonempty(description), icon: nonempty(icon), color: nonempty(color) })?;
            Self::vault_info(&u.session, vault_id, &labels)
        })
    }
    /// Permanently delete a vault. `confirm_name` must equal the vault's current name (typed by the user).
    pub fn delete_vault(&self, vault_id: &str, confirm_name: &str) -> Result<()> {
        self.write(|u| {
            let name = u.session.vault_meta(vault_id)?.name.clone();
            if confirm_name != name { bail!("confirmation name does not match"); }
            u.session.delete_vault(vault_id)
        })
    }

}
