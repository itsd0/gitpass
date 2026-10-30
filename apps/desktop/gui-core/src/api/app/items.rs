use super::super::display::*;
use super::super::dto::*;
use super::App;
use anyhow::{anyhow, bail, Context, Result};
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{Item, Session};
use gitpass_core::{item, totp};
use gitpass_pasteboard::PasteboardMarkers;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{PathBuf};
use std::sync::atomic::{Ordering};
use zeroize::Zeroizing;

impl App {
    // ---------------- items
    pub(crate) fn all_records(s: &Session) -> Result<Vec<(String, String, ItemRecord)>> {
        let mut out = vec![];
        for (vid, _) in s.list_vaults()? { for (iid, _, r) in s.list_records(&vid)? { out.push((vid.clone(), iid, r)); } }
        Ok(out)
    }
    pub fn list_items(&self, filter: &Filter) -> Result<Vec<ItemSummary>> {
        self.with(|u| {
            let all = Self::all_records(&u.session)?;
            let q_str = filter.query.as_deref().unwrap_or("").trim();
            let parsed_q = (!q_str.is_empty()).then(|| gitpass_core::search::parse_query(q_str));
            let vnames: BTreeMap<String, String> = u.session.list_vaults()?.into_iter().map(|(id, m)| (id, m.name.clone())).collect();
            let usage = u.session.all_last_used();
            let mut v: Vec<(ItemSummary, u64)> = all.iter().filter(|(vid, _, r)| {
                let live = r.deleted_at.is_none();
                let keep = match filter.kind {
                    FilterKind::All => live && !r.archived,
                    FilterKind::Favorites => live && !r.archived && r.favorite,
                    FilterKind::Archive => live && r.archived,
                    FilterKind::Deleted => !live,
                    FilterKind::Vault => live && !r.archived && Some(vid) == filter.vault_id.as_ref(),
                    FilterKind::Tag => live && !r.archived && filter.tag.as_ref().is_some_and(|t| r.tags.contains(t)),
                    FilterKind::Category => live && !r.archived && Some(r.category) == filter.category,
                };
                // The item-type picker narrows every view (Recently deleted, Archive,
                // Favorites, a vault, a tag), not only the Category sidebar entry.
                if !keep || filter.category.is_some_and(|c| r.category != c) {
                    return false;
                }
                if let Some(pq) = &parsed_q {
                    let vn = vnames.get(vid.as_str()).map(|s| s.as_str());
                    let cand = gitpass_core::search::CandidateInfo::from_record(r, vn);
                    if !pq.matches(&cand) { return false; }
                }
                true
            }).map(|(vid, iid, r)| (summary(vid, iid, r, usage.get(iid).copied()), r.created_at.unwrap_or(0)))
              .filter(|(s, _)| {
                  if let Some(pq) = &parsed_q {
                      if pq.terms.is_empty() { return true; }
                      let q = pq.terms.to_lowercase();
                      [s.title.as_str(), s.subtitle.as_str(), s.url.as_deref().unwrap_or("")].iter().any(|x| x.to_lowercase().contains(&q))
                          || s.tags.iter().any(|t| t.to_lowercase().contains(&q))
                  } else {
                      true
                  }
              })
              .collect();
            match filter.sort.unwrap_or(Sort::Updated) {
                Sort::Title => v.sort_by(|a, b| a.0.title.to_lowercase().cmp(&b.0.title.to_lowercase())),
                Sort::Updated => v.sort_by(|a, b| b.0.updated_at.cmp(&a.0.updated_at).then_with(|| a.0.title.cmp(&b.0.title))),
                Sort::Created => v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.created_at.cmp(&a.0.created_at))),
                Sort::Recent => v.sort_by(|a, b| {
                    let a_u = a.0.last_used.unwrap_or(0);
                    let b_u = b.0.last_used.unwrap_or(0);
                    b_u.cmp(&a_u).then_with(|| a.0.title.to_lowercase().cmp(&b.0.title.to_lowercase()))
                }),
            }
            Ok(v.into_iter().map(|x| x.0).collect())
        })
    }
    pub fn get_item(&self, vault_id: &str, item_id: &str) -> Result<ItemDetail> {
        self.with(|u| {
            let (v, r) = u.session.read_record(vault_id, item_id)?;
            let findings = item_findings(&u.session, &self.get_settings(), vault_id, item_id)?;
            let last_used = u.session.last_used(item_id);
            Ok(detail(vault_id, item_id, v, &r, findings, last_used))
        })
    }
    /// The ONLY call that returns a stored secret value.
    pub fn reveal_field(&self, vault_id: &str, item_id: &str, field_id: &str) -> Result<Zeroizing<String>> {
        self.reveal_field_version(vault_id, item_id, field_id, None)
    }
    pub fn reveal_field_version(&self, vault_id: &str, item_id: &str, field_id: &str, version: Option<u64>) -> Result<Zeroizing<String>> {
        self.with(|u| {
            let r = match version {
                Some(v) => u.session.item_version(vault_id, item_id, v)?,
                None => u.session.read_record(vault_id, item_id)?.1,
            };
            let f = r.find_field(field_id).or_else(|| {
                if version.is_some() {
                    u.session.read_record(vault_id, item_id).ok().and_then(|(_, cur)| cur.find_field(field_id))
                } else {
                    None
                }
            }).ok_or_else(|| anyhow!("item has no field '{field_id}'"))?;
            let _ = u.session.touch_item_used(item_id);
            Ok(Zeroizing::new(f.value.clone().unwrap_or_default()))
        })
    }
    /// Copy a field value (TOTP fields: the current code) to the clipboard; cleared after
    /// `clipboardClearSecs` only if the clipboard still holds this value and nothing newer was copied.
    /// Secret fields carry the macOS concealed/transient markers; plain fields do not.
    pub fn copy_field(&self, vault_id: &str, item_id: &str, field_id: &str) -> Result<()> {
        let (v, markers) = self.with(|u| {
            let (_, r) = u.session.read_record(vault_id, item_id)?;
            let f = r.find_field(field_id).ok_or_else(|| anyhow!("item has no field '{field_id}'"))?;
            let markers = is_secret_copy(r.category, &f).then_some(PasteboardMarkers::SECRET);
            let raw = f.value.clone().filter(|v| !v.is_empty()).ok_or_else(|| anyhow!("field '{field_id}' is empty"))?;
            let _ = u.session.touch_item_used(item_id);
            Ok((Zeroizing::new(if f.kind == FieldKind::Totp { totp::code_now(&totp::parse(&raw)?).0 } else { raw }), markers))
        })?;
        match markers {
            Some(m) => self.clipboard.set_marked(&v, m)?,
            None => self.clipboard.set_text(&v)?,
        }
        if let Ok(mut lc) = self.last_copied.lock() {
            *lc = Some(Zeroizing::new(v.to_string()));
        }
        let gen = self.clip_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let secs = self.get_settings().clipboard_clear_secs;
        if secs > 0 {
            let (cb, g, lc) = (self.clipboard.clone(), self.clip_gen.clone(), self.last_copied.clone());
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(secs));
                if g.load(Ordering::SeqCst) == gen && cb.get_text().as_deref() == Some(v.as_str()) {
                    let _ = cb.set_text("");
                    if let Ok(mut l) = lc.lock() {
                        if l.as_ref().map(|s| s.as_str()) == Some(v.as_str()) { *l = None; }
                    }
                }
            });
        }
        Ok(())
    }
    /// Copy a password/secret to the clipboard; cleared after `clipboardClearSecs` only if the clipboard still holds this value and nothing newer was copied.
    pub fn copy_password(&self, password: &str) -> Result<()> {
        let v = Zeroizing::new(password.to_string());
        self.clipboard.set_text(&v)?;
        if let Ok(mut lc) = self.last_copied.lock() {
            *lc = Some(Zeroizing::new(v.to_string()));
        }
        let gen = self.clip_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let secs = self.get_settings().clipboard_clear_secs;
        if secs > 0 {
            let (cb, g, lc) = (self.clipboard.clone(), self.clip_gen.clone(), self.last_copied.clone());
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(secs));
                if g.load(Ordering::SeqCst) == gen && cb.get_text().as_deref() == Some(v.as_str()) {
                    let _ = cb.set_text("");
                    if let Ok(mut l) = lc.lock() {
                        if l.as_ref().map(|s| s.as_str()) == Some(v.as_str()) { *l = None; }
                    }
                }
            });
        }
        Ok(())
    }
    pub fn totp_now(&self, vault_id: &str, item_id: &str) -> Result<TotpNow> {
        self.with(|u| {
            let (_, r) = u.session.read_record(vault_id, item_id)?;
            let f = r.view_sections().into_iter().flat_map(|s| s.fields).find(|f| f.kind == FieldKind::Totp && f.value.is_some()).ok_or_else(|| anyhow!("item has no TOTP"))?;
            let spec = totp::parse(f.value.as_deref().unwrap())?;
            let (code, remaining) = totp::code_now(&spec);
            Ok(TotpNow { code, remaining, period: spec.period })
        })
    }

    /// The UI sends `id: ""` for newly added sections/fields; assign fresh random ids here (before
    /// validation) so they are unique and never collide with legacy login ids.
    pub(crate) fn assign_ids(input: &ItemInput) -> ItemInput {
        use rand::Rng;
        let fresh = |p: &str| { let n: u64 = rand::rngs::OsRng.gen(); format!("{p}_{n:016x}") };
        let mut out = input.clone();
        for s in &mut out.sections {
            if s.id.trim().is_empty() { s.id = fresh("s"); }
            for f in &mut s.fields { if f.id.trim().is_empty() { f.id = fresh("f"); } }
        }
        out
    }
    pub(crate) fn validate(input: &ItemInput) -> Result<()> {
        if input.title.trim().is_empty() { bail!("title is required"); }
        if input.category == Category::Other { bail!("invalid category"); }
        let mut ids = BTreeSet::new();
        for s in &input.sections { for f in &s.fields {
            if f.id.trim().is_empty() { bail!("field id is empty"); }
            if !ids.insert(f.id.clone()) { bail!("duplicate field id '{}'", f.id); }
            if f.kind == FieldKind::Totp { if let Some(v) = f.value.as_deref().filter(|v| !v.is_empty()) { totp::parse(v)?; } }
        } }
        Ok(())
    }
    /// Build a record from input. Secret fields sent back as `null` keep their stored value.
    pub(crate) fn apply_input(r: &mut ItemRecord, input: &ItemInput) {
        let old = r.clone();
        r.category = input.category;
        r.item.title = input.title.trim().into();
        r.item.notes = nonempty(input.notes.clone());
        r.item.url = nonempty(input.url.clone());
        if let Some(t) = &input.tags { let mut t: Vec<String> = t.iter().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(); t.dedup(); r.tags = t; }
        if let Some(f) = input.favorite { r.favorite = f; }
        let secs: Vec<item::Section> = input.sections.iter().map(|s| item::Section { id: s.id.clone(), title: s.title.clone(), fields: s.fields.iter().map(|f| {
            let value = match &f.value { None if f.kind.is_secret() => old.find_field(&f.id).and_then(|o| o.value), v => v.clone() };
            item::Field { id: f.id.clone(), label: f.label.clone(), kind: f.kind, value, removed: false }
        }).collect() }).collect();
        r.apply_sections(&secs);
    }
    pub fn create_item(&self, input: &ItemInput) -> Result<ItemSummary> {
        let input = &Self::assign_ids(input);
        Self::validate(input)?;
        self.write(|u| {
            let mut r = ItemRecord::new(Item::default());
            Self::apply_input(&mut r, input);
            let iid = u.session.add_record(&input.vault_id, r)?;
            let (_, r) = u.session.read_record(&input.vault_id, &iid)?;
            Ok(summary(&input.vault_id, &iid, &r, None))
        })
    }
    pub fn update_item(&self, item_id: &str, input: &ItemInput) -> Result<ItemSummary> {
        let input = &Self::assign_ids(input);
        Self::validate(input)?;
        self.write(|u| {
            let (_, mut r) = u.session.read_record(&input.vault_id, item_id)?;
            Self::apply_input(&mut r, input);
            u.session.update_record(&input.vault_id, item_id, r)?;
            let (_, r) = u.session.read_record(&input.vault_id, item_id)?;
            Ok(summary(&input.vault_id, item_id, &r, u.session.last_used(item_id)))
        })
    }
    pub fn set_favorite(&self, vault_id: &str, item_id: &str, favorite: bool) -> Result<()> { self.write(|u| u.session.set_favorite(vault_id, item_id, favorite)) }
    pub fn archive_item(&self, vault_id: &str, item_id: &str, archived: bool) -> Result<()> { self.write(|u| u.session.set_archived(vault_id, item_id, archived)) }
    pub fn delete_item(&self, vault_id: &str, item_id: &str) -> Result<()> { self.write(|u| u.session.soft_delete(vault_id, item_id)) }
    pub fn restore_item(&self, vault_id: &str, item_id: &str) -> Result<()> { self.write(|u| u.session.restore(vault_id, item_id)) }
    pub fn purge_item(&self, vault_id: &str, item_id: &str) -> Result<()> { self.write(|u| u.session.purge_item(vault_id, item_id)) }
    pub fn ignore_finding(&self, vault_id: &str, item_id: &str, kind: &str) -> Result<()> { self.write(|u| u.session.ignore_finding(vault_id, item_id, kind)) }
    pub fn unignore_finding(&self, vault_id: &str, item_id: &str, kind: &str) -> Result<()> { self.write(|u| u.session.unignore_finding(vault_id, item_id, kind)) }
    /// Permanently delete every item in Recently Deleted (all vaults) in one commit.
    /// Returns how many items were purged; an empty trash is a no-op.
    pub fn empty_trash(&self) -> Result<usize> {
        self.write(|u| {
            let targets: Vec<(String, String)> = Self::all_records(&u.session)?.into_iter()
                .filter(|(_, _, r)| r.deleted_at.is_some())
                .map(|(v, i, _)| (v, i)).collect();
            if targets.is_empty() { return Ok(0); }
            u.session.remove_items(&targets)
        })
    }
    /// Bulk action on many items: one vault write (single manifest commit) for every action except
    /// move (copy commit + remove commit) and items with attachments on move/copy (one extra commit per file).
    pub fn bulk_items(&self, req: &BulkRequest) -> Result<BulkResult> {
        if req.items.is_empty() { return Ok(BulkResult { count: 0, items: vec![] }); }
        if req.items.len() > BULK_MAX { bail!("too many items selected (max {BULK_MAX})"); }
        let t: Vec<(String, String)> = req.items.iter().map(|r| (r.vault_id.clone(), r.item_id.clone())).collect();
        let tag = || -> Result<String> {
            let x = req.tag.as_deref().map(str::trim).unwrap_or_default();
            if x.is_empty() { bail!("tag is required"); }
            if x.chars().count() > 64 { bail!("tag is too long"); }
            Ok(x.to_string())
        };
        let same = |count: usize| BulkResult { count, items: req.items.clone() };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        self.write(|u| {
            let s = &u.session;
            Ok(match req.action.as_str() {
                "trash" => same(s.mutate_records(&t, |r| { if r.deleted_at.is_none() { r.deleted_at = Some(now); } })?),
                "restore" => same(s.mutate_records(&t, |r| { r.deleted_at = None; })?),
                "archive" => same(s.mutate_records(&t, |r| r.archived = true)?),
                "unarchive" => same(s.mutate_records(&t, |r| r.archived = false)?),
                "favorite" => same(s.mutate_records(&t, |r| r.favorite = true)?),
                "unfavorite" => same(s.mutate_records(&t, |r| r.favorite = false)?),
                "add_tag" => { let g = tag()?; same(s.mutate_records(&t, |r| if !r.tags.contains(&g) { r.tags.push(g.clone()); })?) }
                "remove_tag" => { let g = tag()?; same(s.mutate_records(&t, |r| r.tags.retain(|x| x != &g))?) }
                "purge" => BulkResult { count: s.remove_items(&t)?, items: vec![] },
                "move" | "copy" => {
                    let to = req.to_vault_id.as_deref().filter(|x| !x.is_empty()).ok_or_else(|| anyhow!("toVaultId is required"))?;
                    if !s.list_vaults()?.iter().any(|(v, _)| v == to) { bail!("no vault {to}"); }
                    if req.action == "copy" {
                        let n = s.copy_items_to(&t, to)?;
                        BulkResult { count: n.len(), items: n.into_iter().map(|i| ItemRef { vault_id: to.to_string(), item_id: i }).collect() }
                    } else {
                        let n = s.move_items(&t, to)?;
                        BulkResult { count: n.len(), items: n.into_iter().map(|(_, _, i)| ItemRef { vault_id: to.to_string(), item_id: i }).collect() }
                    }
                }
                other => bail!("unknown bulk action '{other}'"),
            })
        })
    }
    pub fn move_item(&self, vault_id: &str, item_id: &str, to_vault_id: &str) -> Result<ItemSummary> {
        self.write(|u| {
            let nid = u.session.move_item(vault_id, item_id, to_vault_id)?;
            let (_, r) = u.session.read_record(to_vault_id, &nid)?;
            Ok(summary(to_vault_id, &nid, &r, u.session.last_used(&nid)))
        })
    }
    pub fn duplicate_item(&self, vault_id: &str, item_id: &str) -> Result<ItemSummary> {
        self.write(|u| {
            let nid = u.session.copy_item_to(vault_id, item_id, vault_id)?;
            let (_, mut r) = u.session.read_record(vault_id, &nid)?;
            r.item.title = format!("{} copy", r.item.title);
            r.favorite = false;
            u.session.update_record(vault_id, &nid, r)?;
            let (_, r) = u.session.read_record(vault_id, &nid)?;
            Ok(summary(vault_id, &nid, &r, None))
        })
    }
    pub fn item_version(&self, vault_id: &str, item_id: &str, version: u64) -> Result<ItemDetail> {
        self.with(|u| {
            let r = u.session.item_version(vault_id, item_id, version)?;
            let (_, cur) = u.session.read_record(vault_id, item_id)?;
            let last_used = u.session.last_used(item_id); let mut d = detail(vault_id, item_id, version, &r, Vec::new(), last_used);
            d.history.clear();
            d.attachments.clear();
            if let Some(h) = cur.history.iter().find(|h| h.version == version) { d.summary.updated_at = ts(&h.updated_at); }
            annotate_diff(&mut d, &r, &cur);
            Ok(d)
        })
    }
    pub fn restore_item_version(&self, vault_id: &str, item_id: &str, version: u64) -> Result<ItemDetail> {
        self.write(|u| {
            let new_v = u.session.restore_version(vault_id, item_id, version)?;
            let (_, r) = u.session.read_record(vault_id, item_id)?;
            let findings = item_findings(&u.session, &self.get_settings(), vault_id, item_id)?;
            let last_used = u.session.last_used(item_id);
            Ok(detail(vault_id, item_id, new_v, &r, findings, last_used))
        })
    }
    pub fn add_attachment(&self, vault_id: &str, item_id: &str, path: &str) -> Result<Attachment> {
        let p = PathBuf::from(path);
        let meta = std::fs::metadata(&p).with_context(|| format!("cannot read {path}"))?;
        if meta.len() as usize > item::MAX_ATTACHMENT_BYTES { bail!("attachment too large (limit {} MiB)", item::MAX_ATTACHMENT_BYTES / 1024 / 1024); }
        let data = Zeroizing::new(std::fs::read(&p)?);
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
        self.write(|u| {
            let a = u.session.add_attachment(vault_id, item_id, &name, mime_for(&name), &data)?;
            Ok(Attachment { id: a.id.clone(), name: a.name.clone(), size: a.size, mime: a.mime.clone() })
        })
    }
    pub fn save_attachment(&self, vault_id: &str, item_id: &str, attachment_id: &str, dest_path: &str) -> Result<()> {
        let data = self.with(|u| Ok(Zeroizing::new(u.session.read_attachment(vault_id, item_id, attachment_id)?)))?;
        std::fs::write(dest_path, &*data).with_context(|| format!("cannot write {dest_path}"))
    }
    pub fn remove_attachment(&self, vault_id: &str, item_id: &str, attachment_id: &str) -> Result<()> {
        self.write(|u| u.session.remove_attachment(vault_id, item_id, attachment_id))
    }
    pub fn list_tags(&self) -> Result<Vec<String>> {
        self.with(|u| Ok(Self::all_records(&u.session)?.iter().filter(|(_, _, r)| r.deleted_at.is_none())
            .flat_map(|(_, _, r)| r.tags.clone()).collect::<BTreeSet<_>>().into_iter().collect()))
    }
    pub fn rename_tag(&self, old: &str, new: &str) -> Result<BulkResult> {
        let old = old.trim();
        let new = new.trim();
        if old.is_empty() { bail!("tag name cannot be empty"); }
        if new.is_empty() { bail!("new tag name cannot be empty"); }
        if old == new { return Ok(BulkResult { count: 0, items: vec![] }); }
        self.write(|u| {
            let records = Self::all_records(&u.session)?;
            let targets: Vec<(String, String)> = records.into_iter()
                .filter(|(_, _, r)| r.tags.iter().any(|t| t == old))
                .map(|(v, i, _)| (v, i))
                .collect();
            if targets.is_empty() {
                return Ok(BulkResult { count: 0, items: vec![] });
            }
            let count = u.session.mutate_records(&targets, |r| {
                let mut new_tags = Vec::with_capacity(r.tags.len());
                for t in &r.tags {
                    let target = if t == old { new } else { t };
                    if !new_tags.iter().any(|x| x == target) {
                        new_tags.push(target.to_string());
                    }
                }
                r.tags = new_tags;
            })?;
            let items = targets.into_iter().map(|(vault_id, item_id)| ItemRef { vault_id, item_id }).collect();
            Ok(BulkResult { count, items })
        })
    }
    pub fn delete_tag(&self, name: &str) -> Result<BulkResult> {
        let name = name.trim();
        if name.is_empty() { bail!("tag name cannot be empty"); }
        self.write(|u| {
            let records = Self::all_records(&u.session)?;
            let targets: Vec<(String, String)> = records.into_iter()
                .filter(|(_, _, r)| r.tags.iter().any(|t| t == name))
                .map(|(v, i, _)| (v, i))
                .collect();
            if targets.is_empty() {
                return Ok(BulkResult { count: 0, items: vec![] });
            }
            let count = u.session.mutate_records(&targets, |r| {
                r.tags.retain(|t| t != name);
            })?;
            let items = targets.into_iter().map(|(vault_id, item_id)| ItemRef { vault_id, item_id }).collect();
            Ok(BulkResult { count, items })
        })
    }

}
