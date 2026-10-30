//! Session methods (pure move from `store.rs`; see `session/mod.rs`).

use super::*;
use crate::fsutil::{self, atomic_write};
use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

impl Session {
    /// Re-wrap the Account Key under a new AUK (new salt, given password/secret/KDF). Crash-safe via
    /// format.json.new + keyset.enc.new, rolled forward on next unlock.
    pub fn rewrap_keyset(&mut self, password: &str, secret: &SecretKey, kdf: KdfParams) -> Result<()> {
        let _g = fsutil::lock(&self.root)?;
        let salt: [u8; 16] = random_bytes();
        let nf = Format { version: FORMAT_VERSION, kdf, salt: B64.encode(salt), account_id: self.fmt.account_id.clone() };
        let auk = derive_auk(password, secret, &salt, &nf.account_id, &nf.kdf)?;
        let env = wrap_key(&auk, &self.account_key, &keyset_aad(&nf))?;
        write_json(&self.root.join("format.json.new"), &nf)?;
        write_json(&self.root.join("keyset.enc.new"), &env)?;
        fsutil::rename(&self.root.join("keyset.enc.new"), &self.root.join("keyset.enc"))?;
        fsutil::rename(&self.root.join("format.json.new"), &self.root.join("format.json"))?;
        // Re-protect the local device key under the new AUK-derived key (best effort; local-only file).
        let new_kek = device_kek(&auk, &nf.account_id);
        if self.root.join(crate::device::LOCAL_DEVICE_KEY_FILE).exists() {
            let (kp, _) = crate::device::load_or_create_device_key(&self.root, &self.device_id, &self.device_kek)?;
            crate::device::store_device_key(&self.root, &self.device_id, &new_kek, &kp)?;
        }
        self.device_kek = new_kek;
        self.fmt = nf;
        self.commit_locked(|_| Ok(()))?;
        Ok(())
    }

    /// `gitpass passwd`
    pub fn change_password(&mut self, new_password: &str, secret: &SecretKey, kdf: KdfParams) -> Result<()> {
        self.ensure_not_revoked()?;
        self.rewrap_keyset(new_password, secret, kdf)
    }

    /// `gitpass rotate-secret-key`: returns the new Secret Key (print a new Emergency Kit!).
    pub fn rotate_secret_key(&mut self, password: &str, kdf: KdfParams) -> Result<SecretKey> {
        self.ensure_not_revoked()?;
        let sk = SecretKey::generate();
        self.rewrap_keyset(password, &sk, kdf)?;
        Ok(sk)
    }

    /// `gitpass rotate-account-key`: new Account Key; re-wrap every Vault Key, keyset.enc and the manifest.
    /// A backup is taken first; returns its path.
    pub fn rotate_account_key(&mut self, password: &str, secret: &SecretKey) -> Result<PathBuf> {
        let _g = fsutil::lock(&self.root)?;
        let backup = fsutil::backup(&self.root, "")?;
        let mut m = self.load_manifest()?;
        let new_ak = Key32::random();
        for (vid, ve) in m.vaults.iter_mut() {
            let vk = self.vault_key(vid)?;
            ve.key_hash = write_json(&self.vdir(vid).join("vault.key.enc"),
                &VaultKeyFile { key_version: ve.key_version, envelope: wrap_key(&new_ak, &vk, &vault_key_aad(vid, ve.key_version))? })?;
        }
        let reg = crate::device::read_registry(&self.root, &self.account_key, &self.fmt.account_id)?;
        if let Some(r) = &reg {
            m.devices_hash = Some(crate::device::write_registry(&self.root, &new_ak, &self.fmt.account_id, r)?);
            m.format = m.format.max(MANIFEST_FORMAT_V3);
        }
        if m.generator_history_hash.is_some() {
            let gh = crate::gen_history::read_history(&self.root, &self.account_key, &self.fmt.account_id)?;
            m.generator_history_hash = Some(crate::gen_history::write_history(&self.root, &new_ak, &self.fmt.account_id, &gh)?);
        }
        if m.identity.is_some() {
            let kp = self.load_identity()?;
            m.identity = Some(self.write_identity(&new_ak, &kp)?);
        }
        m.format = manifest_format_for(&m);
        let salt = B64.decode(&self.fmt.salt)?;
        let auk = derive_auk(password, secret, &salt, &self.fmt.account_id, &self.fmt.kdf)?;
        write_json(&self.root.join("keyset.enc"), &wrap_key(&auk, &new_ak, &keyset_aad(&self.fmt))?)?;
        self.account_key = new_ak;
        m.epoch += 1;
        m.keyset_hash = hash_file(&self.root.join("keyset.enc"))?;
        seal_manifest(&self.root, &self.account_key, &m)?;
        save_epoch(&self.root, &self.fmt.account_id, m.epoch)?;
        Ok(backup)
    }

    /// `gitpass vault rotate`: new Vault Key (key_version+1); re-encrypt meta + all items into a temp dir,
    /// then swap directories.
    pub fn rotate_vault(&self, vid: &str) -> Result<()> { self.rotate_vault_excluding(vid, None) }

    /// Rotate the Vault Key; every remaining member gets the NEW key re-sealed to their identity key,
    /// `exclude` (a member fingerprint) is dropped. Crash-safety as for `rotate_vault` (temp dir + swap).
    pub(crate) fn rotate_vault_excluding(&self, vid: &str, exclude: Option<&str>) -> Result<()> {
        let _g = fsutil::lock(&self.root)?;
        let mut m = self.load_manifest()?;
        let ve = m.vaults.get(vid).cloned().ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
        let meta = self.vault_meta(vid)?;
        let mut items = vec![];
        for (iid, ie) in &ve.items { let (v, it) = self.read_record(vid, iid)?; items.push((iid.clone(), v.max(ie.version), it)); }
        let vk = Key32::random();
        let kv = ve.key_version + 1;
        let vaults = self.root.join("vaults");
        let tmp = vaults.join(format!(".rotate-{vid}"));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("items"))?;
        let mut ne = VaultEntry { key_version: kv, items: BTreeMap::new(), ..Default::default() };
        ne.key_hash = write_json(&tmp.join("vault.key.enc"),
            &VaultKeyFile { key_version: kv, envelope: wrap_key(&self.account_key, &vk, &vault_key_aad(vid, kv))? })?;
        let mpt = zeroize::Zeroizing::new(serde_json::to_vec(&meta)?);
        ne.meta_hash = write_json(&tmp.join("meta.enc"), &seal(&vk, &mpt, &meta_aad(vid))?)?;
        atomic_write(&tmp.join("items/.keep"), b"")?;
        if !ve.files.is_empty() {
            let old_vk = self.vault_key(vid)?;
            fs::create_dir_all(tmp.join(FILES_DIR))?;
            for fid in ve.files.keys() {
                let b: FileBlob = read_json(&self.vdir(vid).join(FILES_DIR).join(format!("{fid}.enc")))?;
                let pt = zeroize::Zeroizing::new(open(&old_vk, &b.envelope, &file_aad(vid, &b.item_id, fid))?);
                let nb = FileBlob { format: 5, item_id: b.item_id.clone(), envelope: seal(&vk, &pt, &file_aad(vid, &b.item_id, fid))? };
                ne.files.insert(fid.clone(), write_json(&tmp.join(FILES_DIR).join(format!("{fid}.enc")), &nb)?);
            }
        }
        for (iid, v, it) in &items {
            let h = Self::write_item_in(&tmp, &vk, vid, iid, v + 1, it)?;
            ne.items.insert(iid.clone(), ItemEntry { version: v + 1, hash: h });
        }
        if !ve.members.is_empty() {
            let me = self.load_identity()?;
            for fpr in ve.members.keys() {
                if Some(fpr.as_str()) == exclude { continue; }
                let old: MemberFile = read_json(&self.vdir(vid).join("members").join(format!("{fpr}.enc")))?;
                let rpub = crate::keys::parse_pub(&old.recipient_pub)?;
                if crate::keys::fingerprint(&rpub) != *fpr { bail!("member file {fpr} has a mismatched public key"); }
                let h = Self::write_member(&tmp, vid, kv, &vk, &rpub, &me)?;
                ne.members.insert(fpr.clone(), h);
            }
        }
        let old = vaults.join(format!(".old-{vid}"));
        let _ = fs::remove_dir_all(&old);
        fsutil::rename(&self.vdir(vid), &old)?;
        fsutil::rename(&tmp, &self.vdir(vid))?;
        m.vaults.insert(vid.to_string(), ne);
        m.format = manifest_format_for(&m);
        m.epoch += 1;
        m.keyset_hash = hash_file(&self.root.join("keyset.enc"))?;
        seal_manifest(&self.root, &self.account_key, &m)?;
        save_epoch(&self.root, &self.fmt.account_id, m.epoch)?;
        fs::remove_dir_all(&old)?;
        Ok(())
    }
}
