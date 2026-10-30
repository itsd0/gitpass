//! Session methods (pure move from `store.rs`; see `session/mod.rs`).

use super::*;
use zeroize::Zeroize;
use crate::fsutil::{self, atomic_write};
use crate::merge::ItemRecord;
use anyhow::{anyhow, bail, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

impl Session {
    pub fn load_manifest(&self) -> Result<Manifest> {
        let env: Envelope = read_json(&self.root.join("manifest.enc"))?;
        let pt = zeroize::Zeroizing::new(open(&manifest_key(&self.account_key), &env, &manifest_aad(&self.fmt.account_id))?);
        Ok(serde_json::from_slice(&pt)?)
    }

    pub fn epoch(&self) -> Result<u64> { Ok(self.load_manifest()?.epoch) }

    /// Apply `f` to the manifest under the repo lock, bump epoch, re-hash keyset, re-seal.
    pub(crate) fn commit<F: FnOnce(&mut Manifest) -> Result<()>>(&self, f: F) -> Result<u64> {
        let _g = fsutil::lock(&self.root)?;
        self.commit_locked(f)
    }
    pub(crate) fn commit_locked<F: FnOnce(&mut Manifest) -> Result<()>>(&self, f: F) -> Result<u64> {
        let mut m = self.load_manifest()?;
        f(&mut m)?;
        m.format = manifest_format_for(&m); // any write upgrades the manifest to v3 (v4 if identity/members exist)
        m.epoch += 1;
        m.keyset_hash = hash_file(&self.root.join("keyset.enc"))?;
        seal_manifest(&self.root, &self.account_key, &m)?;
        save_epoch(&self.root, &self.fmt.account_id, m.epoch)?;
        Ok(m.epoch)
    }

    /// Rebuild the manifest from whatever on-disk files authenticate correctly (used by --accept-rollback).
    pub(crate) fn reseal(&self, prev_epoch: u64) -> Result<()> {
        let _g = fsutil::lock(&self.root)?;
        let mut m = Manifest { format: FORMAT_VERSION, account_id: self.fmt.account_id.clone(), epoch: prev_epoch + 1,
            keyset_hash: hash_file(&self.root.join("keyset.enc"))?, vaults: BTreeMap::new(), devices_hash: None, identity: None, generator_history_hash: None };
        if self.load_identity().is_ok() {
            m.identity = Some(IdentityEntry { pub_hash: hash_file(&self.root.join(IDENTITY_PUB_FILE))?, enc_hash: hash_file(&self.root.join(IDENTITY_ENC_FILE))? });
        }
        if crate::device::read_registry(&self.root, &self.account_key, &self.fmt.account_id).ok().flatten().is_some() {
            m.devices_hash = Some(hash_file(&self.root.join(crate::device::DEVICES_FILE))?);
            m.format = MANIFEST_FORMAT_V3;
        }
        if self.root.join(crate::gen_history::GEN_HISTORY_FILE).exists()
            && crate::gen_history::read_history(&self.root, &self.account_key, &self.fmt.account_id).is_ok() {
            m.generator_history_hash = Some(hash_file(&self.root.join(crate::gen_history::GEN_HISTORY_FILE))?);
        }
        if let Ok(rd) = fs::read_dir(self.root.join("vaults")) {
            for e in rd.filter_map(|e| e.ok()) {
                let vid = e.file_name().to_string_lossy().to_string();
                if vid.starts_with('.') || !e.path().is_dir() { continue; }
                let d = e.path();
                let Ok(kf) = read_json::<VaultKeyFile>(&d.join("vault.key.enc")) else { continue };
                let Ok(vk) = unwrap_key(&self.account_key, &kf.envelope, &vault_key_aad(&vid, kf.key_version)) else { continue };
                let Ok(menv) = read_json::<Envelope>(&d.join("meta.enc")) else { continue };
                if open(&vk, &menv, &meta_aad(&vid)).is_err() { continue; }
                let mut ve = VaultEntry { key_version: kf.key_version, key_hash: hash_file(&d.join("vault.key.enc"))?,
                    meta_hash: hash_file(&d.join("meta.enc"))?, items: BTreeMap::new(), members: BTreeMap::new(), files: BTreeMap::new() };
                for fid in list_enc(&d.join(FILES_DIR)) {
                    let fp = d.join(FILES_DIR).join(format!("{fid}.enc"));
                    if let Ok(b) = read_json::<FileBlob>(&fp) {
                        if open(&vk, &b.envelope, &file_aad(&vid, &b.item_id, &fid)).is_ok() { ve.files.insert(fid, hash_file(&fp)?); }
                    }
                }
                // Member files are sealed to other people and cannot be authenticated by us; keep the
                // structurally valid ones that match this vault/key version.
                for fpr in list_enc(&d.join("members")) {
                    let mp = d.join("members").join(format!("{fpr}.enc"));
                    if let Ok(mf) = read_json::<MemberFile>(&mp) {
                        if mf.vault_id == vid && mf.key_version == kf.key_version && mf.recipient_fpr == fpr {
                            ve.members.insert(fpr, hash_file(&mp)?);
                        }
                    }
                }
                for ie in fs::read_dir(d.join("items")).into_iter().flatten().filter_map(|e| e.ok()) {
                    let p = ie.path();
                    if p.extension().and_then(|s| s.to_str()) != Some("enc") { continue; }
                    let iid = p.file_stem().unwrap().to_string_lossy().to_string();
                    let Ok(f) = read_json::<ItemFile>(&p) else { continue };
                    if open(&vk, &f.envelope, &item_aad(&vid, &iid, f.version)).is_ok() {
                        ve.items.insert(iid, ItemEntry { version: f.version, hash: hash_file(&p)? });
                    }
                }
                m.vaults.insert(vid, ve);
            }
        }
        m.format = manifest_format_for(&m).max(m.format);
        seal_manifest(&self.root, &self.account_key, &m)?;
        save_epoch(&self.root, &self.fmt.account_id, m.epoch)
    }

    pub(crate) fn vdir(&self, id: &str) -> PathBuf { self.root.join("vaults").join(id) }

    pub fn create_vault(&self, name: &str) -> Result<String> { self.create_vault_with(name, None) }

    pub fn create_vault_with(&self, name: &str, description: Option<String>) -> Result<String> {
        self.create_vault_full(name, description, None, None)
    }

    /// v5: create a vault with description, icon and color (all stored in the encrypted meta).
    pub fn create_vault_full(&self, name: &str, description: Option<String>, icon: Option<String>, color: Option<String>) -> Result<String> {
        self.ensure_not_revoked()?;
        if self.list_vaults()?.iter().any(|(_, m)| m.name == name) { bail!("vault '{name}' already exists"); }
        let id = uuid::Uuid::new_v4().to_string();
        self.commit(|m| {
            let vk = Key32::random();
            let d = self.vdir(&id);
            let key_hash = write_json(&d.join("vault.key.enc"),
                &VaultKeyFile { key_version: 1, envelope: wrap_key(&self.account_key, &vk, &vault_key_aad(&id, 1))? })?;
            let meta = VaultMeta { name: name.into(), description, icon: icon.clone(), color: color.clone() };
            let mut pt = serde_json::to_vec(&meta)?;
            let env = seal(&vk, &pt, &meta_aad(&id));
            pt.zeroize();
            let meta_hash = write_json(&d.join("meta.enc"), &env?)?;
            fs::create_dir_all(d.join("items"))?;
            atomic_write(&d.join("items/.keep"), b"")?;
            m.vaults.insert(id.clone(), VaultEntry { key_version: 1, key_hash, meta_hash, items: BTreeMap::new(), members: BTreeMap::new(), files: BTreeMap::new() });
            Ok(())
        })?;
        Ok(id)
    }

    /// Rename a vault and/or change its description, icon and color (re-seals the encrypted meta).
    pub fn update_vault_meta(&self, vid: &str, meta: VaultMeta) -> Result<()> {
        self.ensure_not_revoked()?;
        let name = meta.name.trim().to_string();
        if name.is_empty() { bail!("vault name is empty"); }
        if self.list_vaults()?.iter().any(|(id, m)| id != vid && m.name == name) { bail!("vault '{name}' already exists"); }
        let vk = self.vault_key(vid)?;
        let meta = VaultMeta { name, description: meta.description.clone(), icon: meta.icon.clone(), color: meta.color.clone() };
        self.commit(|m| {
            let ve = m.vaults.get_mut(vid).ok_or_else(|| anyhow!("no vault {vid}"))?;
            let mut pt = serde_json::to_vec(&meta)?;
            let env = seal(&vk, &pt, &meta_aad(vid));
            pt.zeroize();
            ve.meta_hash = write_json(&self.vdir(vid).join("meta.enc"), &env?)?;
            Ok(())
        })?;
        Ok(())
    }

    /// Permanently delete a vault: its items, attachments, history, key and member (share) envelopes.
    /// Not recoverable except from git history / backups. The last remaining vault can't be deleted.
    pub fn delete_vault(&self, vid: &str) -> Result<()> {
        self.ensure_not_revoked()?;
        let vaults = self.list_vaults()?;
        if !vaults.iter().any(|(id, _)| id == vid) { bail!("no vault {vid}"); }
        if vaults.len() <= 1 { bail!("can't delete the last vault"); }
        let d = self.vdir(vid);
        self.commit(|m| {
            m.vaults.remove(vid);
            if d.exists() { fs::remove_dir_all(&d)?; }
            fsutil::fsync_dir(&self.root.join("vaults"));
            Ok(())
        })?;
        Ok(())
    }

    pub fn list_vaults(&self) -> Result<Vec<(String, VaultMeta)>> {
        let m = self.load_manifest()?;
        let mut out = vec![];
        for vid in m.vaults.keys() { out.push((vid.clone(), self.vault_meta(vid)?)); }
        out.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        Ok(out)
    }

    pub fn vault_meta(&self, vid: &str) -> Result<VaultMeta> {
        let vk = self.vault_key(vid)?;
        let env: Envelope = read_json(&self.vdir(vid).join("meta.enc"))?;
        let pt = zeroize::Zeroizing::new(open(&vk, &env, &meta_aad(vid))?);
        Ok(serde_json::from_slice(&pt)?)
    }

    /// Resolve a vault by name or id.
    pub fn resolve_vault(&self, q: &str) -> Result<String> {
        self.list_vaults()?
            .into_iter()
            .find(|(id, m)| m.name == q || id == q)
            .map(|(id, _)| id.clone())
            .ok_or_else(|| anyhow!("no vault named '{q}'"))
    }

    pub(crate) fn vault_key(&self, vid: &str) -> Result<Key32> {
        let f: VaultKeyFile = read_json(&self.vdir(vid).join("vault.key.enc"))?;
        unwrap_key(&self.account_key, &f.envelope, &vault_key_aad(vid, f.key_version))
    }

    pub(crate) fn write_item_in(dir: &Path, vk: &Key32, vid: &str, iid: &str, version: u64, rec: &ItemRecord) -> Result<String> {
        let mut pt = serde_json::to_vec(rec)?;
        let mut padded = pad(&pt);
        let env = seal(vk, &padded, &item_aad(vid, iid, version));
        pt.zeroize();
        padded.zeroize();
        write_json(&dir.join("items").join(format!("{iid}.enc")), &ItemFile { version, envelope: env? })
    }

    pub(crate) fn put_item(&self, vid: &str, iid: &str, version: u64, item: &ItemRecord) -> Result<()> {
        self.ensure_not_revoked()?;
        let vk = self.vault_key(vid)?;
        let vid_s = vid.to_string();
        self.commit(|m| {
            let ve = m.vaults.get_mut(&vid_s).ok_or_else(|| anyhow!("vault {vid_s} not in manifest"))?;
            let hash = Self::write_item_in(&self.vdir(vid), &vk, vid, iid, version, item)?;
            ve.items.insert(iid.to_string(), ItemEntry { version, hash });
            Ok(())
        })?;
        Ok(())
    }
}
