use super::super::dto::*;
use super::sync::git;
use super::App;
use anyhow::{anyhow, bail, Result};
use gitpass_core::crypto::SecretKey;
use zeroize::Zeroizing;

impl App {
    // ---------------- settings / onboarding
    pub fn get_settings(&self) -> Settings { self.read_local().settings.unwrap_or_default() }
    pub fn set_settings(&self, s: Settings) -> Result<Settings> {
        let was = self.get_settings().biometric_unlock;
        if s.biometric_unlock && !was {
            if !self.biometric.supported() { bail!("biometric unlock is not supported yet on this device"); }
            let (root, pw, sk) = self.with(|u| Ok((u.session.root.display().to_string(), u.password.clone(), Zeroizing::new(u.secret.encode()))))
                .map_err(|_| anyhow!("unlock the vault before enabling biometric unlock"))?;
            self.biometric.store(&root, &pw, &sk)?;
        } else if !s.biometric_unlock && was {
            let root = self.status()?.vault_path;
            self.biometric.delete(&root)?;
        }
        let was_icons = self.get_settings().show_website_icons;
        if was_icons && !s.show_website_icons {
            let cache_dir = self.favicon_cache_dir.lock().unwrap().clone().unwrap_or_else(crate::favicon::default_favicon_cache_dir);
            let _ = crate::favicon::clear_favicon_cache(&cache_dir);
        }
        if s.auto_lock_minutes > 24 * 60 { bail!("autoLockMinutes must be <= 1440"); }
        if s.clipboard_clear_secs > 3600 { bail!("clipboardClearSecs must be <= 3600"); }
        if ![0, 6, 12, 24].contains(&s.watchtower_unused_months) { bail!("watchtowerUnusedMonths must be 0 (off), 6, 12 or 24"); }
        self.update_local(|l| l.settings = Some(s.clone()))?;
        self.state.set_auto_lock_minutes(s.auto_lock_minutes);
        Ok(s)
    }
    pub fn onboarding(&self) -> Onboarding { self.read_local().onboarding }
    pub fn set_onboarding(&self, p: OnboardingPatch) -> Result<Onboarding> {
        self.update_local(|l| {
            let o = &mut l.onboarding;
            if let Some(v) = p.extension_installed { o.extension_installed = v; }
            if let Some(v) = p.emergency_kit_saved { o.emergency_kit_saved = v; }
            if let Some(v) = p.repo_connected { o.repo_connected = v; }
            if let Some(v) = p.imported { o.imported = v; }
            if let Some(v) = p.dismissed { o.dismissed = v; }
            o.clone()
        })
    }

    // ---------------- credentials
    pub fn change_password(&self, current: &str, next: &str) -> Result<()> {
        if next.chars().count() < 8 { bail!("new master password must be at least 8 characters"); }
        let kdf = self.unlock_opts.lock().map_err(|_| anyhow!("poisoned"))?.target_kdf.clone();
        self.write(|u| {
            if !ct_eq(current.as_bytes(), u.password.as_bytes()) { bail!("current master password is incorrect"); }
            let sk = SecretKey(u.secret.0);
            let k = u.session.fmt.kdf.max_with(&kdf);
            u.session.change_password(next, &sk, k)?;
            u.password = Zeroizing::new(next.to_string());
            Ok(())
        })
    }
    pub(crate) fn kit_text(account: &str, vault: &str, secret: Option<&str>) -> String {
        let mut s = format!("gitpass Emergency Kit\n=====================\nAccount ID: {account}\nVault folder: {vault}\nCreated: {}\n\n",
            gitpass_core::device::now_iso8601());
        match secret {
            Some(k) => s.push_str(&format!("Secret Key: {k}\n\nStore this kit offline. Anyone with it AND your master password can open your vault.\n")),
            None => s.push_str("Secret Key: (not shown — find it on your saved Emergency Kit or rotate it in Settings to print a new one)\n"),
        }
        s.push_str("Master password: write it down by hand: ______________________\n");
        s
    }
    pub fn rotate_secret_key(&self) -> Result<RotateResult> {
        let kdf = self.unlock_opts.lock().map_err(|_| anyhow!("poisoned"))?.target_kdf.clone();
        let kit = self.write(|u| {
            let k = u.session.fmt.kdf.max_with(&kdf);
            let pw = u.password.clone();
            let sk = u.session.rotate_secret_key(&pw, k)?;
            let enc = Zeroizing::new(sk.encode());
            u.secret = sk;
            Ok((u.session.fmt.account_id.clone(), enc.clone(), Zeroizing::new(Self::kit_text(&u.session.fmt.account_id, &u.session.root.display().to_string(), Some(&enc)))))
        })?;
        let (acc, enc, kit) = kit;
        *self.fresh_secret.lock().map_err(|_| anyhow!("poisoned"))? = Some((acc, enc));
        *self.fresh_kit.lock().map_err(|_| anyhow!("poisoned"))? = Some(kit.clone());
        Ok(RotateResult { emergency_kit: kit.to_string() })
    }
    /// Structured kit. The Secret Key is included only (a) right after create_account / rotate_secret_key
    /// in this session, or (b) when `password` is given and matches the master password of the unlocked
    /// session (the key is then taken from the unlocked session, where it already lives in memory).
    pub fn emergency_kit_data(&self, password: Option<&str>) -> Result<EmergencyKitData> {
        let fresh = self.fresh_secret.lock().map_err(|_| anyhow!("poisoned"))?.clone();
        let (mut d, root) = self.with(|u| {
            let acc = u.session.fmt.account_id.clone();
            let mut secret = fresh.filter(|(a, _)| *a == acc).map(|(_, k)| k.to_string());
            if let Some(pw) = password {
                if !ct_eq(pw.as_bytes(), u.password.as_bytes()) { bail!("wrong master password"); }
                secret = Some(u.secret.encode().to_string());
            }
            let device_name = u.session.devices().ok().and_then(|r| r.devices.into_iter().find(|d| d.id == u.session.device_id).map(|d| d.name))
                .or_else(|| u.device_name.clone());
            Ok((EmergencyKitData { account_id: acc, vault_path: u.session.root.display().to_string(), remote: None, device_name,
                created_at: gitpass_core::device::now_iso8601(), secret_key_available: true, secret_key: secret }, u.session.root.clone()))
        })?;
        d.remote = git(&root, &["remote", "get-url", "origin"]).filter(|s| !s.is_empty());
        Ok(d)
    }
    /// Kit text; contains the Secret Key only right after rotate_secret_key (until lock).
    pub fn emergency_kit(&self) -> Result<String> {
        if let Some(k) = self.fresh_kit.lock().map_err(|_| anyhow!("poisoned"))?.as_ref() { return Ok(k.to_string()); }
        self.with(|u| Ok(Self::kit_text(&u.session.fmt.account_id, &u.session.root.display().to_string(), None)))
    }

}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
