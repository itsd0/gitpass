use super::super::display::*;
use super::super::dto::*;
use super::App;
use anyhow::{Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use gitpass_core::crypto::SecretKey;
use std::path::{Path};

impl App {
    // ---------------- sharing
    pub fn identity(&self) -> Result<Identity> {
        self.with(|u| { let (pk, fpr) = u.session.identity()?; Ok(Identity { public_key: B64.encode(pk), fingerprint: fpr }) })
    }
    /// Fingerprint of an arbitrary recipient public key (same encoding `share_vault` accepts), so the
    /// UI can show the exact value both parties compare out of band. Needs no unlocked session.
    pub fn fingerprint_of(&self, public_key: &str) -> Result<String> {
        Ok(gitpass_core::keys::fingerprint(&gitpass_core::keys::parse_pub(public_key.trim())?))
    }
    pub fn list_members(&self, vault_id: &str) -> Result<Vec<Member>> {
        let labels = self.read_local().member_labels;
        self.with(|u| Self::members_of(&u.session, vault_id, &labels))
    }
    pub fn share_vault(&self, vault_id: &str, public_key: &str, label: Option<String>) -> Result<Member> {
        let pk = gitpass_core::keys::parse_pub(public_key.trim())?;
        let fpr = self.write(|u| { u.session.share_vault(vault_id, &pk)?; Ok(gitpass_core::keys::fingerprint(&pk)) })?;
        let label = nonempty(label);
        if let Some(l) = &label { self.update_local(|f| { f.member_labels.insert(fpr.clone(), l.clone()); })?; }
        Ok(Member { fingerprint: fpr, label, is_self: false })
    }
    pub fn unshare_vault(&self, vault_id: &str, fingerprint: &str) -> Result<()> { self.write(|u| u.session.unshare_vault(vault_id, fingerprint).map(|_| ())) }
    pub fn accept_share(&self, path: &str) -> Result<VaultInfo> {
        let labels = self.read_local().member_labels;
        self.write(|u| { let vid = u.session.accept_share(Path::new(path))?; Self::vault_info(&u.session, &vid, &labels) })
    }

    // ---------------- devices
    pub fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        self.with(|u| {
            let cur = u.session.device_id.clone();
            Ok(u.session.devices()?.devices.into_iter().map(|d| DeviceInfo { current: d.id == cur, id: d.id, name: d.name, last_seen: d.last_seen_at, revoked: d.revoked }).collect())
        })
    }
    /// `rotateCredentials=false`: revocation marker only. `true`: real revocation (rotate every Vault Key
    /// and the Account Key, like `gitpass device revoke`). Master password / Secret Key are NOT changed
    /// here; use change_password / rotate_secret_key for that.
    pub fn revoke_device(&self, id: &str, rotate_credentials: bool) -> Result<()> {
        self.write(|u| {
            if rotate_credentials {
                let (pw, sk) = (u.password.clone(), SecretKey(u.secret.0));
                u.session.revoke_device_and_rotate(id, &pw, &sk).map(|_| ())
            } else { u.session.revoke_device(id).map(|_| ()) }
        })
    }

}
