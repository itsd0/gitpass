//! Session methods (pure move from `store.rs`; see `session/mod.rs`).

use super::*;
use zeroize::Zeroize;
use crate::fsutil;
use crate::merge::{self, ItemRecord};
use anyhow::{anyhow, bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;

/// Targets without duplicates, first occurrence wins (keeps the caller's order).
fn dedup_targets(t: &[(String, String)]) -> Vec<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    t.iter().filter(|x| seen.insert((*x).clone())).cloned().collect()
}

impl Session {
    /// Add a v5 record (any category). Stamps legacy + v5 clocks and `created_at`; history starts empty.
    pub fn add_record(&self, vid: &str, mut rec: ItemRecord) -> Result<String> {
        let iid = uuid::Uuid::new_v4().to_string();
        let now = merge::now_ms();
        let now_sec = now / 1000;
        if rec.item.updated_at.is_empty() { rec.item.updated_at = now_sec.to_string(); }
        let up_sec = rec.item.updated_at.trim().parse::<u64>().unwrap_or(now_sec);
        rec.created_at.get_or_insert(up_sec);
        rec.clocks = Some(merge::stamp(None, &rec.item, now));
        merge::stamp_ext(None, &mut rec, now);
        rec.history.clear();
        if rec.password_history.len() > crate::item::PASSWORD_HISTORY_LIMIT {
            rec.password_history.truncate(crate::item::PASSWORD_HISTORY_LIMIT);
        }
        self.put_item(vid, &iid, 1, &rec)?;
        Ok(iid)
    }

    /// Overwrite item `iid` with `new` (local edit): changed fields get fresh clocks, and if the content
    /// (item fields / category / sections / tags) changed, the previous content is pushed onto the
    /// encrypted history (last [`crate::item::HISTORY_LIMIT`] kept).
    pub fn update_record(&self, vid: &str, iid: &str, new: ItemRecord) -> Result<()> {
        let (v, rec) = self.prepare_update(vid, iid, new)?;
        self.put_item(vid, iid, v, &rec)
    }

    /// The `update_record` logic without writing: returns (new version, finished record).
    fn prepare_update(&self, vid: &str, iid: &str, mut new: ItemRecord) -> Result<(u64, ItemRecord)> {
        let (v, old) = self.read_record(vid, iid)?;
        let now = merge::now_ms();
        let mut cmp_new = new.snapshot(); cmp_new.item.updated_at.clear();
        let mut cmp_old = old.snapshot(); cmp_old.item.updated_at.clear();
        new.history = old.history.clone();
        if cmp_new != cmp_old || old.attachments != new.attachments {
            new.history.push(crate::item::HistoryEntry { version: v, updated_at: old.item.updated_at.clone(), snapshot: old.snapshot() });
            let n = new.history.len();
            if n > crate::item::HISTORY_LIMIT { new.history.drain(..n - crate::item::HISTORY_LIMIT); }
            new.item.updated_at = (now / 1000).to_string();
        } else {
            new.item.updated_at = old.item.updated_at.clone();
        }
        new.created_at = old.created_at.or(new.created_at);
        if new.removed_attachments.is_empty() { new.removed_attachments = old.removed_attachments.clone(); }
        if new.attachments.is_empty() {
            new.attachments = old.attachments.iter().filter(|a| !new.removed_attachments.contains(&a.id)).cloned().collect();
        }
        // Login password history: if the password changed, push old password onto history (newest first, capped at 20)
        if old.category == crate::item::Category::Login && old.item.password != new.item.password {
            if let Some(old_pw) = old.item.password.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
                new.password_history = old.password_history.clone();
                new.password_history.insert(0, crate::item::PasswordHistoryEntry {
                    value: old_pw.to_string(),
                    replaced_at: crate::device::iso8601((now / 1000) as i64),
                });
                if new.password_history.len() > crate::item::PASSWORD_HISTORY_LIMIT {
                    new.password_history.truncate(crate::item::PASSWORD_HISTORY_LIMIT);
                }
            }
        }
        new.clocks = Some(merge::stamp(Some((&old.item, old.clocks_or_legacy())), &new.item, now));
        merge::stamp_ext(Some(&old), &mut new, now);
        Ok((v + 1, new))
    }

    /// Write several items (any vaults) under ONE manifest commit: one epoch bump / re-seal / git change.
    fn put_items(&self, writes: &[(String, String, u64, ItemRecord)]) -> Result<()> {
        if writes.is_empty() { return Ok(()); }
        self.ensure_not_revoked()?;
        let mut keys: BTreeMap<String, Key32> = BTreeMap::new();
        for (vid, ..) in writes { if !keys.contains_key(vid) { keys.insert(vid.clone(), self.vault_key(vid)?); } }
        self.commit(|m| {
            for (vid, iid, version, rec) in writes {
                let ve = m.vaults.get_mut(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
                let hash = Self::write_item_in(&self.vdir(vid), &keys[vid], vid, iid, *version, rec)?;
                ve.items.insert(iid.clone(), ItemEntry { version: *version, hash });
            }
            Ok(())
        })?;
        Ok(())
    }

    /// Bulk edit: apply `f` to every (vault, item) in `targets` and write them all in one commit.
    /// Items whose record does not change are skipped. Returns how many were written.
    /// Fails before writing anything if any target cannot be read.
    pub fn mutate_records<F: FnMut(&mut ItemRecord)>(&self, targets: &[(String, String)], mut f: F) -> Result<usize> {
        let mut writes = Vec::with_capacity(targets.len());
        for (vid, iid) in dedup_targets(targets) {
            let (_, old) = self.read_record(&vid, &iid)?;
            let mut r = old.clone();
            f(&mut r);
            if serde_json::to_vec(&r)? == serde_json::to_vec(&old)? { continue; }
            let (v, rec) = self.prepare_update(&vid, &iid, r)?;
            writes.push((vid, iid, v, rec));
        }
        self.put_items(&writes)?;
        Ok(writes.len())
    }

    /// Permanently remove several items (and their attachment files) in one commit.
    pub fn remove_items(&self, targets: &[(String, String)]) -> Result<usize> {
        self.ensure_not_revoked()?;
        let targets = dedup_targets(targets);
        let mut plan = vec![];
        for (vid, iid) in &targets {
            let p = self.vdir(vid).join("items").join(format!("{iid}.enc"));
            if !p.exists() { bail!("no item {iid}"); }
            let fids: Vec<String> = self.read_record(vid, iid).map(|(_, r)| r.attachments.iter().map(|a| a.id.clone()).collect()).unwrap_or_default();
            plan.push((vid.clone(), iid.clone(), p, fids));
        }
        if plan.is_empty() { return Ok(0); }
        self.commit(|m| {
            for (vid, iid, p, fids) in &plan {
                fs::remove_file(p)?;
                if let Some(ve) = m.vaults.get_mut(vid) {
                    ve.items.remove(iid);
                    for f in fids {
                        let fp = self.vdir(vid).join(FILES_DIR).join(format!("{f}.enc"));
                        if fp.exists() { fs::remove_file(&fp)?; }
                        ve.files.remove(f);
                    }
                }
            }
            for (vid, ..) in &plan { fsutil::fsync_dir(&self.vdir(vid).join("items")); }
            Ok(())
        })?;
        Ok(plan.len())
    }

    fn copy_items_impl(&self, targets: &[(String, String)], to_vid: &str, preserve_history: bool) -> Result<Vec<String>> {
        self.vault_key(to_vid)?; // target must exist
        let now = merge::now_ms();
        let mut writes = vec![];
        let mut atts = vec![];
        for (vid, iid) in dedup_targets(targets) {
            let (_, r) = self.read_record(&vid, &iid)?;
            let mut n = ItemRecord::from_snapshot(&r.snapshot());
            n.favorite = r.favorite;
            n.ignored_findings = r.ignored_findings.clone();
            n.rotate_every_days = r.rotate_every_days;
            n.archived = r.archived;
            if n.item.updated_at.is_empty() { n.item.updated_at = (now / 1000).to_string(); }
            if preserve_history {
                n.created_at = r.created_at.or(Some(now / 1000));
                n.history = r.history.clone();
            } else {
                n.created_at = Some(now / 1000);
                n.history.clear();
            }
            n.clocks = Some(merge::stamp(None, &n.item, now));
            merge::stamp_ext(None, &mut n, now);
            let nid = uuid::Uuid::new_v4().to_string();
            if !r.attachments.is_empty() { atts.push((vid.clone(), iid.clone(), nid.clone(), r.attachments.clone())); }
            writes.push((to_vid.to_string(), nid, 1, n));
        }
        self.put_items(&writes)?;
        for (vid, iid, nid, list) in atts {
            for a in &list {
                let bytes = zeroize::Zeroizing::new(self.read_attachment(&vid, &iid, &a.id)?);
                self.add_attachment(to_vid, &nid, &a.name, &a.mime, &bytes)?;
            }
            if preserve_history {
                let (_, r_orig) = self.read_record(&vid, &iid)?;
                let (v, mut rec) = self.read_record(to_vid, &nid)?;
                rec.history = r_orig.history.clone();
                rec.created_at = r_orig.created_at;
                self.put_item(to_vid, &nid, v, &rec)?;
            } else {
                let (v, mut rec) = self.read_record(to_vid, &nid)?;
                rec.history.clear();
                self.put_item(to_vid, &nid, v, &rec)?;
            }
        }
        Ok(writes.into_iter().map(|w| w.1).collect())
    }

    /// Copy several items to `to_vid`: all records land in one commit; attachments (if any) are then
    /// re-encrypted per file. Returns the new ids in input order. History starts empty and `created_at` is reset.
    pub fn copy_items_to(&self, targets: &[(String, String)], to_vid: &str) -> Result<Vec<String>> {
        self.copy_items_impl(targets, to_vid, false)
    }

    /// Move several items to `to_vid` (items already there are left alone): copy (one commit) then
    /// remove the originals (one commit). Returns (old vault, old id, new id) for each moved item.
    /// Preserves version history, `created_at`, tags, and attachments.
    pub fn move_items(&self, targets: &[(String, String)], to_vid: &str) -> Result<Vec<(String, String, String)>> {
        let movers: Vec<(String, String)> = dedup_targets(targets).into_iter().filter(|(v, _)| v != to_vid).collect();
        if movers.is_empty() { return Ok(vec![]); }
        let nids = self.copy_items_impl(&movers, to_vid, true)?;
        self.remove_items(&movers)?;
        Ok(movers.into_iter().zip(nids).map(|((v, i), n)| (v, i, n)).collect())
    }

    fn mutate_record<F: FnOnce(&mut ItemRecord)>(&self, vid: &str, iid: &str, f: F) -> Result<()> {
        let (_, mut r) = self.read_record(vid, iid)?;
        f(&mut r);
        self.update_record(vid, iid, r)
    }

    /// Locate which vault contains `iid`. Returns None if not found in any vault.
    pub fn find_item_vault(&self, iid: &str) -> Result<Option<String>> {
        let m = self.load_manifest()?;
        for (vid, ve) in &m.vaults {
            if ve.items.contains_key(iid) {
                return Ok(Some(vid.clone()));
            }
        }
        Ok(None)
    }

    /// Read an item record by its ID across any vault in the account.
    pub fn get_record_by_id(&self, iid: &str) -> Result<Option<(String, u64, ItemRecord)>> {
        if let Some(vid) = self.find_item_vault(iid)? {
            let (v, r) = self.read_record(&vid, iid)?;
            return Ok(Some((vid, v, r)));
        }
        Ok(None)
    }

    /// Link a related item by id. Refuses to link an item to itself. Idempotent.
    pub fn add_related_item(&self, vid: &str, iid: &str, related_id: &str) -> Result<()> {
        if iid == related_id {
            anyhow::bail!("cannot link an item to itself");
        }
        self.mutate_record(vid, iid, |r| {
            if !r.related_item_ids.iter().any(|x| x == related_id) {
                r.related_item_ids.push(related_id.to_string());
            }
        })
    }

    /// Remove a related item reference. Idempotent.
    pub fn remove_related_item(&self, vid: &str, iid: &str, related_id: &str) -> Result<()> {
        self.mutate_record(vid, iid, |r| {
            r.related_item_ids.retain(|x| x != related_id);
        })
    }

    pub fn set_favorite(&self, vid: &str, iid: &str, fav: bool) -> Result<()> { self.mutate_record(vid, iid, |r| r.favorite = fav) }
    /// Set (or clear with `None`) the per-login "change password every N days" reminder. Only the
    /// values in [`merge::ROTATION_DAYS`] are accepted.
    pub fn set_rotation(&self, vid: &str, iid: &str, days: Option<u32>) -> Result<()> {
        if let Some(d) = days {
            if !merge::ROTATION_DAYS.contains(&d) { bail!("rotation must be one of 30, 90, 180 or 365 days"); }
        }
        self.mutate_record(vid, iid, |r| r.rotate_every_days = days)
    }
    pub fn set_archived(&self, vid: &str, iid: &str, a: bool) -> Result<()> { self.mutate_record(vid, iid, |r| r.archived = a) }
    pub fn ignore_finding(&self, vid: &str, iid: &str, kind: &str) -> Result<()> { self.mutate_record(vid, iid, |r| { r.ignore_finding(kind); }) }
    pub fn unignore_finding(&self, vid: &str, iid: &str, kind: &str) -> Result<()> { self.mutate_record(vid, iid, |r| { r.unignore_finding(kind); }) }
    /// Soft delete: the item stays (encrypted) with `deleted_at` set and is purged after 30 days.
    pub fn soft_delete(&self, vid: &str, iid: &str) -> Result<()> { let t = now_secs(); self.mutate_record(vid, iid, |r| r.deleted_at = Some(t)) }
    pub fn restore(&self, vid: &str, iid: &str) -> Result<()> { self.mutate_record(vid, iid, |r| r.deleted_at = None) }
    /// Permanently remove an item and its attachment files.
    pub fn purge_item(&self, vid: &str, iid: &str) -> Result<()> { self.remove_item(vid, iid) }

    /// Purge every item soft-deleted more than 30 days before `now` (unix secs). Returns purged ids.
    pub fn purge_expired(&self, now: u64) -> Result<Vec<(String, String)>> {
        let m = self.load_manifest()?;
        let mut out = vec![];
        for (vid, ve) in &m.vaults {
            for iid in ve.items.keys() {
                let Ok((_, r)) = self.read_record(vid, iid) else { continue };
                if let Some(d) = r.deleted_at { if now.saturating_sub(d) >= crate::item::PURGE_AFTER_SECS { out.push((vid.clone(), iid.clone())); } }
            }
        }
        for (v, i) in &out { self.remove_item(v, i)?; }
        Ok(out)
    }

    /// All records of a vault (including archived / deleted ones).
    pub fn list_records(&self, vid: &str) -> Result<Vec<(String, u64, ItemRecord)>> {
        let m = self.load_manifest()?;
        let ve = m.vaults.get(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
        let mut out = vec![];
        for iid in ve.items.keys() { let (v, r) = self.read_record(vid, iid)?; out.push((iid.clone(), v, r)); }
        Ok(out)
    }

    /// A prior version's content (from history), or the current one if `version` is current.
    pub fn item_version(&self, vid: &str, iid: &str, version: u64) -> Result<ItemRecord> {
        let (v, r) = self.read_record(vid, iid)?;
        if v == version { return Ok(r); }
        let h = r.history.iter().find(|h| h.version == version).ok_or_else(|| anyhow!("no version {version} in item history"))?;
        let mut out = ItemRecord::from_snapshot(&h.snapshot);
        out.created_at = r.created_at;
        Ok(out)
    }

    /// Restore an item to a previous version from its history.
    /// Writes a new version whose content equals the chosen snapshot.
    /// History is append-only: the pre-restore state is pushed to history, and no versions are deleted.
    /// Errors cleanly for an unknown version or a deleted item.
    pub fn restore_version(&self, vid: &str, iid: &str, version: u64) -> Result<u64> {
        let (v, old) = self.read_record(vid, iid)?;
        if old.deleted_at.is_some() {
            bail!("cannot restore version of deleted item");
        }
        let h = old.history.iter().find(|h| h.version == version).ok_or_else(|| anyhow!("no version {version} in item history"))?;
        let snapshot = h.snapshot.clone();
        let now = merge::now_ms();
        let mut new = ItemRecord::from_snapshot(&snapshot);
        new.favorite = old.favorite;
        new.archived = old.archived;
        new.ignored_findings = old.ignored_findings.clone();
        new.rotate_every_days = old.rotate_every_days;
        new.attachments = old.attachments.clone();
        // Keep attachment tombstones so a merge with a stale copy can't resurrect a removed file.
        new.removed_attachments = old.removed_attachments.clone();
        new.created_at = old.created_at;
        new.history = old.history.clone();
        new.history.push(crate::item::HistoryEntry {
            version: v,
            updated_at: old.item.updated_at.clone(),
            snapshot: old.snapshot(),
        });
        let n = new.history.len();
        if n > crate::item::HISTORY_LIMIT {
            new.history.drain(..n - crate::item::HISTORY_LIMIT);
        }
        new.item.updated_at = (now / 1000).to_string();
        new.clocks = Some(merge::stamp(Some((&old.item, old.clocks_or_legacy())), &new.item, now));
        merge::stamp_ext(Some(&old), &mut new, now);
        let new_v = v + 1;
        self.put_item(vid, iid, new_v, &new)?;
        Ok(new_v)
    }

    /// Copy an item (content, tags, attachments) to a new id in `to_vid` (title unchanged).
    /// Version history starts empty and `created_at` is reset.
    pub fn copy_item_to(&self, vid: &str, iid: &str, to_vid: &str) -> Result<String> {
        let nids = self.copy_items_impl(&[(vid.to_string(), iid.to_string())], to_vid, false)?;
        nids.into_iter().next().ok_or_else(|| anyhow!("copy item failed"))
    }

    /// Move an item to another vault (re-encrypted under the target Vault Key). Returns the new id.
    /// Preserves version history, `created_at`, tags, and attachments.
    pub fn move_item(&self, vid: &str, iid: &str, to_vid: &str) -> Result<String> {
        if vid == to_vid { return Ok(iid.to_string()); }
        let moved = self.move_items(&[(vid.to_string(), iid.to_string())], to_vid)?;
        let nid = moved.into_iter().next().map(|(_, _, n)| n).ok_or_else(|| anyhow!("failed to move item"))?;
        Ok(nid)
    }

    /// Encrypt and store an attachment for item `iid` (size cap [`crate::item::MAX_ATTACHMENT_BYTES`]).
    /// The file is padded, sealed under the Vault Key with AAD binding vault/item/file id, and its hash
    /// is recorded in the manifest.
    pub fn add_attachment(&self, vid: &str, iid: &str, name: &str, mime: &str, data: &[u8]) -> Result<crate::item::AttachmentMeta> {
        self.ensure_not_revoked()?;
        if data.len() > crate::item::MAX_ATTACHMENT_BYTES {
            bail!("attachment too large ({} bytes exceeds {} MiB limit)", data.len(), crate::item::MAX_ATTACHMENT_BYTES / (1024 * 1024));
        }
        let (_, mut rec) = self.read_record(vid, iid)?;
        let vk = self.vault_key(vid)?;
        let fid = uuid::Uuid::new_v4().to_string();
        let mut padded = pad(data);
        let env = seal(&vk, &padded, &file_aad(vid, iid, &fid));
        padded.zeroize();
        let blob = FileBlob { format: 5, item_id: iid.into(), envelope: env? };
        let meta = crate::item::AttachmentMeta { id: fid.clone(), name: name.into(), size: data.len() as u64, mime: mime.into() };
        let d = self.vdir(vid);
        fs::create_dir_all(d.join(FILES_DIR))?;
        self.commit(|m| {
            let ve = m.vaults.get_mut(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
            let h = write_json(&d.join(FILES_DIR).join(format!("{fid}.enc")), &blob)?;
            ve.files.insert(fid.clone(), h);
            Ok(())
        })?;
        rec.attachments.push(meta.clone());
        self.update_record(vid, iid, rec)?;
        Ok(meta)
    }

    /// Decrypt an attachment (checks the manifest hash and the AEAD binding).
    pub fn read_attachment(&self, vid: &str, iid: &str, fid: &str) -> Result<Vec<u8>> {
        let (_, rec) = self.read_record(vid, iid)?;
        if !rec.attachments.iter().any(|a| a.id == fid) { bail!("item has no attachment {fid}"); }
        let m = self.load_manifest()?;
        let want = m.vaults.get(vid).and_then(|v| v.files.get(fid)).ok_or_else(|| anyhow!("attachment {fid} not in manifest"))?;
        let p = self.vdir(vid).join(FILES_DIR).join(format!("{fid}.enc"));
        let bytes = fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
        if &hash_bytes(&bytes) != want { bail!("attachment {fid} does not match manifest hash (tampered?)"); }
        let b: FileBlob = serde_json::from_slice(&bytes)?;
        if b.item_id != iid { bail!("attachment {fid} belongs to another item"); }
        let vk = self.vault_key(vid)?;
        let padded = zeroize::Zeroizing::new(open(&vk, &b.envelope, &file_aad(vid, iid, fid)).context("attachment failed authentication")?);
        unpad(&padded)
    }

    /// Remove an attachment from item `iid`: deletes the encrypted blob from `files/<fid>.enc`,
    /// removes the file from the manifest, removes the attachment metadata, and records the prior state
    /// in the item's history.
    pub fn remove_attachment(&self, vid: &str, iid: &str, fid: &str) -> Result<()> {
        self.ensure_not_revoked()?;
        let (v, old) = self.read_record(vid, iid)?;
        let idx = old.attachments.iter().position(|a| a.id == fid).ok_or_else(|| anyhow!("item has no attachment {fid}"))?;
        let now = merge::now_ms();
        let mut rec = old.clone();
        rec.attachments.remove(idx);
        if !rec.removed_attachments.iter().any(|x| x == fid) { rec.removed_attachments.push(fid.to_string()); }
        rec.history.push(crate::item::HistoryEntry {
            version: v,
            updated_at: old.item.updated_at.clone(),
            snapshot: old.snapshot(),
        });
        let n = rec.history.len();
        if n > crate::item::HISTORY_LIMIT { rec.history.drain(..n - crate::item::HISTORY_LIMIT); }
        rec.item.updated_at = (now / 1000).to_string();
        rec.clocks = Some(merge::stamp(Some((&old.item, old.clocks_or_legacy())), &rec.item, now));
        merge::stamp_ext(Some(&old), &mut rec, now);
        let vk = self.vault_key(vid)?;
        let d = self.vdir(vid);
        let fp = d.join(FILES_DIR).join(format!("{fid}.enc"));
        // One manifest commit: the new record (without the attachment, with its tombstone), the blob
        // deletion and the manifest `files` entry all land together, like `remove_item`.
        self.commit(|m| {
            let ve = m.vaults.get_mut(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
            let hash = Self::write_item_in(&d, &vk, vid, iid, v + 1, &rec)?;
            ve.items.insert(iid.to_string(), ItemEntry { version: v + 1, hash });
            if fp.exists() {
                fs::remove_file(&fp)?;
                if let Some(parent) = fp.parent() { fsutil::fsync_dir(parent); }
            }
            ve.files.remove(fid);
            Ok(())
        })?;
        Ok(())
    }

    // ---------- generator history ----------

    pub fn read_generator_history(&self) -> Result<crate::gen_history::GeneratorHistory> {
        let now = now_secs();
        let mut gh = crate::gen_history::read_history(&self.root, &self.account_key, &self.fmt.account_id)?;
        gh.prune(now);
        Ok(gh)
    }

    pub fn write_generator_history(&self, gh: &crate::gen_history::GeneratorHistory) -> Result<String> {
        let mut hash = String::new();
        self.commit(|m| {
            let h = crate::gen_history::write_history(&self.root, &self.account_key, &self.fmt.account_id, gh)?;
            m.generator_history_hash = Some(h.clone());
            hash = h;
            Ok(())
        })?;
        Ok(hash)
    }

    pub fn record_generated_password(&self, value: &str, source: &str, host: Option<String>) -> Result<crate::gen_history::GeneratorHistoryEntry> {
        self.ensure_not_revoked()?;
        let mut gh = self.read_generator_history()?;
        let now = now_secs();
        let entry = gh.add_entry(value.to_string(), source, host, now);
        self.write_generator_history(&gh)?;
        Ok(entry)
    }

    pub fn clear_generator_history(&self) -> Result<()> {
        self.ensure_not_revoked()?;
        let mut gh = self.read_generator_history()?;
        let now = now_secs();
        gh.clear(now);
        self.write_generator_history(&gh)?;
        Ok(())
    }

    pub fn delete_generator_history_entry(&self, id: &str) -> Result<bool> {
        self.ensure_not_revoked()?;
        let mut gh = self.read_generator_history()?;
        let now = now_secs();
        let removed = gh.delete_entry(id, now);
        if removed {
            self.write_generator_history(&gh)?;
        }
        Ok(removed)
    }
}
