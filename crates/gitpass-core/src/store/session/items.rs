//! Session methods (pure move from `store.rs`; see `session/mod.rs`).

use super::*;
use crate::fsutil;
use crate::merge::{self, ItemRecord, MergeReport};
use anyhow::{anyhow, bail, Result};
use std::fs;

impl Session {
    pub fn add_item(&self, vid: &str, item: &Item) -> Result<String> {
        self.add_record(vid, ItemRecord::new(item.clone()))
    }

    pub fn read_item(&self, vid: &str, iid: &str) -> Result<(u64, Item)> {
        let (v, r) = self.read_record(vid, iid)?;
        Ok((v, r.item.clone()))
    }

    /// Read the full v3 record. v2 payloads have `clocks: None`; use [`ItemRecord::clocks_or_legacy`].
    pub fn read_record(&self, vid: &str, iid: &str) -> Result<(u64, ItemRecord)> {
        let f: ItemFile = read_json(&self.vdir(vid).join("items").join(format!("{iid}.enc")))?;
        let vk = self.vault_key(vid)?;
        let padded = zeroize::Zeroizing::new(open(&vk, &f.envelope, &item_aad(vid, iid, f.version))?);
        let pt = zeroize::Zeroizing::new(unpad(&padded)?);
        let rec: ItemRecord = serde_json::from_slice(&pt)?;
        Ok((f.version, rec))
    }

    /// Write `item`, stamping a fresh clock on every field whose value changed (clearing a field to `None`
    /// is a tombstone write and also bumps its clock). Note: this overwrites whatever is on disk; when
    /// applying an item from another device use [`Session::merge_item`] instead.
    pub fn update_item(&self, vid: &str, iid: &str, item: &Item) -> Result<()> {
        let (_, old) = self.read_record(vid, iid)?;
        let mut rec = old.clone();
        rec.item = item.clone();
        self.update_record(vid, iid, rec)
    }

    /// Field-merge `remote` into the on-disk item `iid` and write the result (version+1, manifest re-sealed)
    /// if anything changed. Equal-clock conflicts keep the local value and are reported.
    pub fn merge_item(&self, vid: &str, iid: &str, remote: &ItemRecord) -> Result<MergeReport> {
        let (v, local) = self.read_record(vid, iid)?;
        let (merged, rep) = merge::merge_records(&local, remote);
        if rep.changed_local() || local.clocks.is_none() {
            self.put_item(vid, iid, v + 1, &merged)?;
        }
        Ok(rep)
    }

    /// Insert a record verbatim under a given id (used by `sync --merge` to re-apply local-only items).
    pub fn put_record(&self, vid: &str, iid: &str, rec: &ItemRecord) -> Result<()> {
        let v = self.read_record(vid, iid).map(|(v, _)| v + 1).unwrap_or(1);
        self.put_item(vid, iid, v, rec)
    }

    /// Decrypt a manifest.enc blob (e.g. from `git show <rev>:manifest.enc`) with this account's key.
    pub fn open_manifest_bytes(&self, bytes: &[u8]) -> Result<Manifest> {
        let env: Envelope = serde_json::from_slice(bytes)?;
        let pt = zeroize::Zeroizing::new(open(&manifest_key(&self.account_key), &env, &manifest_aad(&self.fmt.account_id))?);
        Ok(serde_json::from_slice(&pt)?)
    }

    pub fn remove_item(&self, vid: &str, iid: &str) -> Result<()> {
        self.ensure_not_revoked()?;
        let p = self.vdir(vid).join("items").join(format!("{iid}.enc"));
        if !p.exists() { bail!("no item {iid}"); }
        let fids: Vec<String> = self.read_record(vid, iid).map(|(_, r)| r.attachments.iter().map(|a| a.id.clone()).collect()).unwrap_or_default();
        self.commit(|m| {
            fs::remove_file(&p)?;
            fsutil::fsync_dir(p.parent().unwrap());
            if let Some(ve) = m.vaults.get_mut(vid) {
                ve.items.remove(iid);
                for f in &fids {
                    let fp = self.vdir(vid).join(FILES_DIR).join(format!("{f}.enc"));
                    if fp.exists() { fs::remove_file(&fp)?; }
                    ve.files.remove(f);
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    pub fn list_items(&self, vid: &str) -> Result<Vec<(String, Item)>> {
        let m = self.load_manifest()?;
        let ve = m.vaults.get(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
        let mut out = vec![];
        for iid in ve.items.keys() {
            let (_, item) = self.read_item(vid, iid)?;
            out.push((iid.clone(), item));
        }
        out.sort_by(|a, b| a.1.title.to_lowercase().cmp(&b.1.title.to_lowercase()));
        Ok(out)
    }

    /// Find item(s) by id, id prefix, or case-insensitive title substring.
    pub fn find_items(&self, vid: &str, q: &str) -> Result<Vec<(String, Item)>> {
        let ql = q.to_lowercase();
        let all = self.list_items(vid)?;
        if let Some(exact) = all.iter().find(|(id, _)| id == q) { return Ok(vec![exact.clone()]); }
        Ok(all.into_iter().filter(|(id, it)| id.starts_with(q) || it.title.to_lowercase().contains(&ql)).collect())
    }

    pub fn resolve_item(&self, vid: &str, q: &str) -> Result<String> {
        let m = self.find_items(vid, q)?;
        match m.len() {
            0 => bail!("no item matches '{q}'"),
            1 => Ok(m[0].0.clone()),
            n => bail!("{n} items match '{q}'; use the item id"),
        }
    }

    // ---------- key lifecycle ----------
}
