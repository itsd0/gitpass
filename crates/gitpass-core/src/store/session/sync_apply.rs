//! [`Session`] methods for the shared sync engine (issue #452): building a
//! decrypted [`VaultSnapshot`](crate::sync_engine::VaultSnapshot) and applying
//! a [`SyncPlan`](crate::sync_engine::SyncPlan) to the local vault.
//!
//! Apply runs as a single manifest commit at the plan's epoch, so the vault
//! is never left half-merged in the manifest; callers should still take a
//! backup first (as `sync --merge` does), because item files are written
//! before the manifest is re-sealed. Plans are local-relative: applying a
//! plan computed from a different local epoch is refused.

use super::*;
use crate::sync_engine::{SyncAction, SyncPlan, VaultSnapshot};
use anyhow::{anyhow, bail, Result};
use std::collections::BTreeMap;
use std::fs;
use zeroize::Zeroize;

/// What [`Session::apply_sync_plan`] changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub upserted_items: usize,
    pub deleted_items: usize,
    pub vaults_created: usize,
    pub vaults_deleted: usize,
    pub new_epoch: u64,
}

impl Session {
    /// Decrypted snapshot of this vault for the sync engine: manifest, every
    /// item record and vault meta, raw vault-key blobs, attachment/share blobs,
    /// generator history, device registry and identity files. The snapshot is
    /// only as consistent as the vault on disk; callers unlock first (which
    /// runs the integrity + rollback checks).
    pub fn snapshot_sync(&self) -> Result<VaultSnapshot> {
        let manifest = self.load_manifest()?;
        let mut items = BTreeMap::new();
        let mut vault_meta = BTreeMap::new();
        let mut vault_key_blobs = BTreeMap::new();
        let mut vault_blobs = BTreeMap::new();
        for vid in manifest.vaults.keys() {
            vault_meta.insert(vid.clone(), self.vault_meta(vid)?);
            vault_key_blobs.insert(vid.clone(), fs::read(self.vdir(vid).join("vault.key.enc"))?);
            for iid in manifest.vaults[vid].items.keys() {
                let (_, rec) = self.read_record(vid, iid)?;
                items.insert((vid.clone(), iid.clone()), rec);
            }
            // Opaque blobs, copied verbatim: attachments and share envelopes.
            for sub in [FILES_DIR, "members"] {
                let dir = self.vdir(vid).join(sub);
                if let Ok(rd) = fs::read_dir(&dir) {
                    for e in rd.filter_map(|e| e.ok()) {
                        let p = e.path();
                        if p.extension().and_then(|s| s.to_str()) != Some("enc") {
                            continue;
                        }
                        let name = p.file_name().unwrap().to_string_lossy().to_string();
                        vault_blobs.insert((vid.clone(), format!("{sub}/{name}")), fs::read(&p)?);
                    }
                }
            }
        }
        let generator_history = self.read_generator_history().unwrap_or_default();
        let devices = crate::device::read_registry(&self.root, &self.account_key, &self.fmt.account_id)?;
        let identity =
            if self.root.join(IDENTITY_PUB_FILE).exists() && self.root.join(IDENTITY_ENC_FILE).exists() {
                Some((
                    fs::read(self.root.join(IDENTITY_PUB_FILE))?,
                    fs::read(self.root.join(IDENTITY_ENC_FILE))?,
                ))
            } else {
                None
            };
        Ok(VaultSnapshot {
            manifest,
            items,
            vault_meta,
            vault_key_blobs,
            vault_blobs,
            generator_history,
            devices,
            identity,
        })
    }

    /// Apply a plan computed by [`crate::sync_engine::plan_sync`] from this
    /// vault's current state. Refuses stale plans (local epoch moved on) and
    /// plans for another account. A no-change plan is a no-op: the epoch is
    /// not bumped. On success the manifest epoch is exactly the plan's
    /// `new_epoch`, strictly above both sides' epochs.
    pub fn apply_sync_plan(&self, plan: &SyncPlan) -> Result<ApplyReport> {
        self.ensure_not_revoked()?;
        if plan.account_id != self.fmt.account_id {
            bail!("sync plan is for account {}, not {}", plan.account_id, self.fmt.account_id);
        }
        let cur = self.load_manifest()?;
        if cur.epoch != plan.local_epoch {
            bail!(
                "stale sync plan: local epoch is now {}, plan was computed from {}",
                cur.epoch, plan.local_epoch
            );
        }
        let mut rep = ApplyReport { new_epoch: cur.epoch, ..Default::default() };
        if !plan.local_changes {
            return Ok(rep);
        }
        // Attachment ids of items about to be deleted (read before the commit;
        // best-effort — a missing record just means no attachments to clean).
        let mut doomed_attachments: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for a in &plan.actions {
            if let SyncAction::DeleteItem { vault_id, item_id } = a {
                let fids = self
                    .read_record(vault_id, item_id)
                    .map(|(_, r)| r.attachments.iter().map(|a| a.id.clone()).collect())
                    .unwrap_or_default();
                doomed_attachments.insert((vault_id.clone(), item_id.clone()), fids);
            }
        }
        for a in &plan.actions {
            match a {
                SyncAction::UpsertItem { .. } => rep.upserted_items += 1,
                SyncAction::DeleteItem { .. } => rep.deleted_items += 1,
                SyncAction::CreateVault { .. } => rep.vaults_created += 1,
                SyncAction::DeleteVault { .. } => rep.vaults_deleted += 1,
                _ => {}
            }
        }
        let actions = plan.actions.clone();
        let gh = plan.generator_history.clone();
        let dev = plan.devices.clone();
        let epoch = self.commit_at_epoch(plan.new_epoch, |m| {
            // Phase 1: vault deletes (never the last vault).
            for a in &actions {
                if let SyncAction::DeleteVault { vault_id } = a {
                    if m.vaults.len() <= 1 {
                        bail!("sync plan would delete the last vault {vault_id}; refusing");
                    }
                    let d = self.vdir(vault_id);
                    if d.exists() {
                        fs::remove_dir_all(&d)?;
                    }
                    fsutil::fsync_dir(&self.root.join("vaults"));
                    m.vaults.remove(vault_id);
                }
            }
            // Phase 2: vault creates (key blob verbatim, meta + items sealed
            // under that key, item versions restart at 1 on this replica).
            for a in &actions {
                if let SyncAction::CreateVault { vault_id, meta, key_blob, items, blobs } = a {
                    if m.vaults.contains_key(vault_id) {
                        bail!("sync plan creates vault {vault_id} that already exists");
                    }
                    let d = self.vdir(vault_id);
                    fs::create_dir_all(d.join("items"))?;
                    atomic_write(&d.join("vault.key.enc"), key_blob)?;
                    let kf: VaultKeyFile = serde_json::from_slice(key_blob)?;
                    let vk = unwrap_key(&self.account_key, &kf.envelope, &vault_key_aad(vault_id, kf.key_version))?;
                    let mut pt = serde_json::to_vec(meta)?;
                    let meta_hash = write_json(&d.join("meta.enc"), &seal(&vk, &pt, &meta_aad(vault_id))?)?;
                    pt.zeroize();
                    atomic_write(&d.join("items/.keep"), b"")?;
                    let mut ve = VaultEntry {
                        key_version: kf.key_version,
                        key_hash: hash_bytes(key_blob),
                        meta_hash,
                        items: BTreeMap::new(),
                        members: BTreeMap::new(),
                        files: BTreeMap::new(),
                    };
                    for (iid, rec) in items {
                        let h = Self::write_item_in(&d, &vk, vault_id, iid, 1, rec)?;
                        ve.items.insert(iid.clone(), ItemEntry { version: 1, hash: h });
                    }
                    for (path, bytes) in blobs {
                        write_vault_blob(&d, &mut ve, path, bytes)?;
                    }
                    m.vaults.insert(vault_id.clone(), ve);
                }
            }
            // Phase 3: vault-key adoptions (clean pull only, decided by the
            // planner). Meta/items/blobs are re-sealed by later phases.
            for a in &actions {
                if let SyncAction::SetVaultKey { vault_id, key_blob } = a {
                    let ve = m.vaults.get_mut(vault_id).ok_or_else(|| anyhow!("no vault {vault_id}"))?;
                    let kf: VaultKeyFile = serde_json::from_slice(key_blob)?;
                    atomic_write(&self.vdir(vault_id).join("vault.key.enc"), key_blob)?;
                    ve.key_version = kf.key_version;
                    ve.key_hash = hash_bytes(key_blob);
                }
            }
            // Phase 4: vault meta (re-sealed under the current vault key).
            for a in &actions {
                if let SyncAction::UpdateVaultMeta { vault_id, meta } = a {
                    let vk = self.vault_key(vault_id)?;
                    let mut pt = serde_json::to_vec(meta)?;
                    let ve = m.vaults.get_mut(vault_id).ok_or_else(|| anyhow!("no vault {vault_id}"))?;
                    ve.meta_hash =
                        write_json(&self.vdir(vault_id).join("meta.enc"), &seal(&vk, &pt, &meta_aad(vault_id))?)?;
                    pt.zeroize();
                }
            }
            // Phase 5: items.
            for a in &actions {
                match a {
                    SyncAction::UpsertItem { vault_id, item_id, record } => {
                        let vk = self.vault_key(vault_id)?;
                        let ve = m.vaults.get_mut(vault_id).ok_or_else(|| anyhow!("no vault {vault_id}"))?;
                        let v = ve.items.get(item_id).map(|e| e.version + 1).unwrap_or(1);
                        let h = Self::write_item_in(&self.vdir(vault_id), &vk, vault_id, item_id, v, record)?;
                        ve.items.insert(item_id.clone(), ItemEntry { version: v, hash: h });
                    }
                    SyncAction::DeleteItem { vault_id, item_id } => {
                        let ve = m.vaults.get_mut(vault_id).ok_or_else(|| anyhow!("no vault {vault_id}"))?;
                        let p = self.vdir(vault_id).join("items").join(format!("{item_id}.enc"));
                        if p.exists() {
                            fs::remove_file(&p)?;
                            fsutil::fsync_dir(p.parent().unwrap());
                        }
                        if let Some(fids) = doomed_attachments.get(&(vault_id.clone(), item_id.clone())) {
                            for f in fids {
                                let fp = self.vdir(vault_id).join(FILES_DIR).join(format!("{f}.enc"));
                                if fp.exists() {
                                    fs::remove_file(&fp)?;
                                }
                                ve.files.remove(f);
                            }
                        }
                        ve.items.remove(item_id);
                    }
                    _ => {}
                }
            }
            // Phase 6: blobs.
            for a in &actions {
                match a {
                    SyncAction::UpsertBlob { vault_id, path, bytes } => {
                        let d = self.vdir(vault_id);
                        let ve = m.vaults.get_mut(vault_id).ok_or_else(|| anyhow!("no vault {vault_id}"))?;
                        write_vault_blob(&d, ve, path, bytes)?;
                    }
                    SyncAction::DeleteBlob { vault_id, path } => {
                        let p = self.vdir(vault_id).join(path);
                        if p.exists() {
                            fs::remove_file(&p)?;
                        }
                        let ve = m.vaults.get_mut(vault_id).ok_or_else(|| anyhow!("no vault {vault_id}"))?;
                        remove_vault_blob(ve, path);
                    }
                    _ => {}
                }
            }
            // Phase 7: identity.
            for a in &actions {
                if let SyncAction::SetIdentity { pub_blob, enc_blob } = a {
                    atomic_write(&self.root.join(IDENTITY_PUB_FILE), pub_blob)?;
                    atomic_write(&self.root.join(IDENTITY_ENC_FILE), enc_blob)?;
                    m.identity = Some(crate::store::sharing::IdentityEntry {
                        pub_hash: hash_bytes(pub_blob),
                        enc_hash: hash_bytes(enc_blob),
                    });
                }
            }
            // Phase 8: account-level state.
            if let Some(g) = &gh {
                m.generator_history_hash =
                    Some(crate::gen_history::write_history(&self.root, &self.account_key, &self.fmt.account_id, g)?);
            }
            if let Some(r) = &dev {
                m.devices_hash =
                    Some(crate::device::write_registry(&self.root, &self.account_key, &self.fmt.account_id, r)?);
            }
            Ok(())
        })?;
        rep.new_epoch = epoch;
        Ok(rep)
    }
}

/// Write one opaque vault blob (`files/<fid>.enc` or `members/<fpr>.enc`)
/// verbatim and record its hash in the manifest entry. Anything else is
/// refused: remote paths are untrusted, and only these two subtrees hold
/// opaque blobs.
fn write_vault_blob(d: &std::path::Path, ve: &mut VaultEntry, path: &str, bytes: &[u8]) -> Result<()> {
    let (sub, name) = match path.split_once('/') {
        Some((sub, name)) if (sub == FILES_DIR || sub == "members") => (sub, name),
        _ => bail!("refusing vault blob outside files/ or members/: {path}"),
    };
    if name.is_empty() || !name.ends_with(".enc") || name.contains('/') || name.contains("..") {
        bail!("refusing malformed vault blob path {path}");
    }
    fs::create_dir_all(d.join(sub))?;
    atomic_write(&d.join(path), bytes)?;
    let hash = hash_bytes(bytes);
    let id = name.trim_end_matches(".enc").to_string();
    if sub == FILES_DIR {
        ve.files.insert(id, hash);
    } else {
        ve.members.insert(id, hash);
    }
    Ok(())
}

fn remove_vault_blob(ve: &mut VaultEntry, path: &str) {
    if let Some((sub, name)) = path.split_once('/') {
        let id = name.trim_end_matches(".enc").to_string();
        if sub == FILES_DIR {
            ve.files.remove(&id);
        } else if sub == "members" {
            ve.members.remove(&id);
        }
    }
}
