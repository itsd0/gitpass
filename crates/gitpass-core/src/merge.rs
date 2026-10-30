//! Format v3 field-level merge.
//!
//! Every logical item field carries its own logical clock (u64 milliseconds since the Unix epoch,
//! Lamport-bumped so a local edit always gets a clock strictly greater than the previous one for that
//! field). Clocks are stored next to the item JSON inside the same encrypted, padded payload
//! (see [`ItemRecord`]); nothing about them is visible outside the ciphertext.
//!
//! Clearing a field is modelled as a tombstone: the value becomes `None` and the field's clock is bumped,
//! so a clear competes with a concurrent edit by clock just like any other write.
use crate::item::{AttachmentMeta, Category, Field, FieldKind, HistoryEntry, Section, Snapshot};
use crate::store::Item;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const FIELDS: [&str; 6] = ["title", "username", "password", "url", "notes", "totp"];

/// Per-field logical clocks (ms). Not secret, but stored encrypted with the item.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Zeroize)]
pub struct FieldClocks {
    pub title: u64,
    pub username: u64,
    pub password: u64,
    pub url: u64,
    pub notes: u64,
    pub totp: u64,
}

impl FieldClocks {
    pub fn all(c: u64) -> Self { FieldClocks { title: c, username: c, password: c, url: c, notes: c, totp: c } }
    pub fn get(&self, f: &str) -> u64 {
        match f { "title" => self.title, "username" => self.username, "password" => self.password,
            "url" => self.url, "notes" => self.notes, "totp" => self.totp, _ => 0 }
    }
    pub fn set(&mut self, f: &str, v: u64) {
        match f { "title" => self.title = v, "username" => self.username = v, "password" => self.password = v,
            "url" => self.url = v, "notes" => self.notes = v, "totp" => self.totp = v, _ => {} }
    }
    pub fn max(&self) -> u64 { FIELDS.iter().map(|f| self.get(f)).max().unwrap_or(0) }
}

/// Encrypted item payload in format v3: the v2 item fields (flattened, so the JSON is a superset of v2)
/// plus `clocks`. A v2 payload (no `clocks`) deserializes with `clocks: None`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, Zeroize, ZeroizeOnDrop)]
pub struct ItemRecord {
    #[serde(flatten)]
    pub item: Item,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clocks: Option<FieldClocks>,
    // ---- format v5 (all optional: a v2-v4 payload deserializes with defaults = login item) ----
    #[zeroize(skip)]
    #[serde(default, skip_serializing_if = "is_login")]
    pub category: Category,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<Section>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub favorite: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub archived: bool,
    /// Soft-delete time (unix seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<u64>,
    /// Creation time (unix seconds). Absent for pre-v5 items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    /// v5 clocks keyed by `f:<field id>`, `s:<section id>`, `m:<meta key>` (ms, Lamport-bumped).
    #[zeroize(skip)]
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub xclocks: BTreeMap<String, u64>,
    /// Up to [`crate::item::HISTORY_LIMIT`] prior versions (newest last).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<HistoryEntry>,
    /// Findings ignored by the user in Watchtower (e.g. `["reused", "insecure_url"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_findings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentMeta>,
    /// Tombstones: ids of attachments removed from this item. Merged as a union and applied after the
    /// attachment union, so a concurrent edit on another device cannot resurrect a removed attachment
    /// (whose blob is gone). Omitted when empty, so older payloads stay byte-compatible.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_attachments: Vec<String>,
    /// Item ids of related items (one-way references within the same account).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_item_ids: Vec<String>,
    /// Prior passwords on a login item (newest first, capped at [`crate::item::PASSWORD_HISTORY_LIMIT`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub password_history: Vec<crate::item::PasswordHistoryEntry>,
}
fn is_login(c: &Category) -> bool { *c == Category::Login }
fn is_false(b: &bool) -> bool { !*b }

/// Synthesize clocks for a legacy (v2) item: every field gets `updated_at` (seconds) * 1000.
/// Unparseable `updated_at` yields clock 0 (any v3 edit wins).
pub fn legacy_clocks(item: &Item) -> FieldClocks {
    FieldClocks::all(item.updated_at.trim().parse::<u64>().map(|s| s.saturating_mul(1000)).unwrap_or(0))
}

impl ItemRecord {
    pub fn clocks_or_legacy(&self) -> FieldClocks { self.clocks.unwrap_or_else(|| legacy_clocks(&self.item)) }
    pub fn new(item: Item) -> Self { let mut r = ItemRecord::default(); r.item = item; r }
    pub fn with_clocks(item: Item, clocks: Option<FieldClocks>) -> Self { let mut r = ItemRecord::new(item); r.clocks = clocks; r }
    pub fn is_finding_ignored(&self, kind: &str) -> bool {
        self.ignored_findings.iter().any(|k| {
            k == kind
                || (kind == "missing_totp" && k == "totp_available")
                || (kind == "totp_available" && k == "missing_totp")
                || (kind == "pwned" && k == "breached")
                || (kind == "breached" && k == "pwned")
        })
    }
    pub fn ignore_finding(&mut self, kind: &str) -> bool {
        if !self.is_finding_ignored(kind) { self.ignored_findings.push(kind.to_string()); true } else { false }
    }
    pub fn unignore_finding(&mut self, kind: &str) -> bool {
        let len_before = self.ignored_findings.len();
        self.ignored_findings.retain(|k| {
            k != kind && !(kind == "missing_totp" && k == "totp_available") && !(kind == "totp_available" && k == "missing_totp") && !(kind == "pwned" && k == "breached") && !(kind == "breached" && k == "pwned")
        });
        self.ignored_findings.len() < len_before
    }

    /// Content snapshot (for history).
    pub fn snapshot(&self) -> Snapshot {
        Snapshot { item: self.item.clone(), category: self.category, sections: self.sections.clone(), tags: self.tags.clone(), related_item_ids: self.related_item_ids.clone() }
    }
    pub fn from_snapshot(s: &Snapshot) -> Self {
        let mut r = ItemRecord::new(s.item.clone()); r.category = s.category; r.sections = s.sections.clone(); r.tags = s.tags.clone(); r.related_item_ids = s.related_item_ids.clone(); r
    }

    /// Sections as shown to a user: for login items a synthesized `login` section (username, password,
    /// totp from the legacy fields) followed by the stored sections; tombstoned fields and empty sections
    /// are dropped.
    pub fn view_sections(&self) -> Vec<Section> {
        let mut out = vec![];
        if self.category == Category::Login {
            let mut f = vec![];
            let mut push = |id: &str, label: &str, kind: FieldKind, v: &Option<String>| {
                if let Some(v) = v.as_ref().filter(|v| !v.is_empty()) { f.push(Field { id: id.into(), label: label.into(), kind, value: Some(v.clone()), removed: false }); }
            };
            push(crate::item::LOGIN_USERNAME, "username", FieldKind::Text, &self.item.username);
            push(crate::item::LOGIN_PASSWORD, "password", FieldKind::Concealed, &self.item.password);
            push(crate::item::LOGIN_TOTP, "one-time password", FieldKind::Totp, &self.item.totp);
            if !f.is_empty() { out.push(Section { id: "login".into(), title: None, fields: f }); }
        }
        for s in &self.sections {
            let fields: Vec<Field> = s.fields.iter().filter(|f| !f.removed).cloned().collect();
            if !fields.is_empty() || s.title.is_some() { out.push(Section { id: s.id.clone(), title: s.title.clone(), fields }); }
        }
        out
    }

    /// Find a visible field by id (including the synthesized login fields).
    pub fn find_field(&self, id: &str) -> Option<Field> {
        self.view_sections().into_iter().flat_map(|s| s.fields).find(|f| f.id == id)
    }

    /// Replace the content with user-edited `sections`. For login items, fields with ids
    /// `username`/`password`/`totp` are stored in the legacy fields; everything else goes to `sections`.
    /// Fields that existed before but are absent now become tombstones.
    pub fn apply_sections(&mut self, input: &[Section]) {
        let login = self.category == Category::Login;
        if login { self.item.username = None; self.item.password = None; self.item.totp = None; }
        let old_ids: Vec<(String, String)> = self.sections.iter().flat_map(|s| s.fields.iter().filter(|f| !f.removed).map(move |f| (s.id.clone(), f.id.clone()))).collect();
        let mut new_sections: Vec<Section> = vec![];
        let mut seen = std::collections::HashSet::new();
        for s in input {
            let mut ns = Section { id: s.id.clone(), title: s.title.clone(), fields: vec![] };
            for f in &s.fields {
                let v = f.value.clone();
                if login && (f.id == crate::item::LOGIN_USERNAME || f.id == crate::item::LOGIN_PASSWORD || f.id == crate::item::LOGIN_TOTP) {
                    let v = v.filter(|x| !x.is_empty());
                    match f.id.as_str() { "username" => self.item.username = v, "password" => self.item.password = v, _ => self.item.totp = v }
                    continue;
                }
                seen.insert(f.id.clone());
                ns.fields.push(Field { id: f.id.clone(), label: f.label.clone(), kind: f.kind, value: v, removed: false });
            }
            if !(login && s.id == "login" && ns.fields.is_empty()) { new_sections.push(ns); }
        }
        for (sid, fid) in old_ids {
            if seen.contains(&fid) { continue; }
            let tomb = Field { id: fid, label: String::new(), kind: FieldKind::Text, value: None, removed: true };
            match new_sections.iter_mut().find(|s| s.id == sid) {
                Some(s) => s.fields.push(tomb),
                None => new_sections.push(Section { id: sid, title: None, fields: vec![tomb] }),
            }
        }
        // keep old tombstones too, so they keep propagating
        for s in &self.sections { for f in s.fields.iter().filter(|f| f.removed) {
            if new_sections.iter().any(|ns| ns.fields.iter().any(|x| x.id == f.id)) { continue; }
            match new_sections.iter_mut().find(|ns| ns.id == s.id) {
                Some(ns) => ns.fields.push(f.clone()),
                None => new_sections.push(Section { id: s.id.clone(), title: None, fields: vec![f.clone()] }),
            }
        } }
        self.sections = new_sections;
    }

    /// Timestamp (unix seconds) when the password was last changed.
    /// Computed from the newest history entry or edit where the password value differs,
    /// or the item's creation time if it has never changed. When the history was truncated
    /// (see [`crate::item::HISTORY_LIMIT`]) and every retained entry has the current password,
    /// the oldest retained entry's `updated_at` is used instead of the creation time.
    pub fn password_changed_at(&self) -> Option<u64> {
        let cur_pw = record_password(self)?;
        let n = self.history.len();
        for i in (0..n).rev() {
            let hist_pw = snapshot_password(&self.history[i].snapshot);
            if hist_pw != Some(cur_pw) {
                let ts_str = if i + 1 < n {
                    &self.history[i + 1].updated_at
                } else {
                    &self.item.updated_at
                };
                return parse_ts(ts_str);
            }
        }
        // Every retained entry has the current password. If the history was truncated (capped at
        // HISTORY_LIMIT, or the oldest retained entry is not the item's first version), the real
        // change may have happened after creation but before the oldest retained entry, so the
        // oldest retained entry's timestamp is the safest bound (avoids false "Old" findings).
        // Only fall back to the creation time when the history is complete.
        if let Some(first) = self.history.first() {
            let truncated = n >= crate::item::HISTORY_LIMIT || first.version > 1;
            if truncated {
                if let Some(t) = parse_ts(&first.updated_at) { return Some(t); }
            }
        }
        let creation = match (self.created_at, parse_ts(&self.item.updated_at)) { (Some(c), Some(u)) => Some(c.min(u)), (Some(c), None) => Some(c), (None, u) => u };
        creation.or_else(|| self.history.first().and_then(|h| parse_ts(&h.updated_at))).or_else(|| parse_ts(&self.item.updated_at))
    }
}

fn record_password(r: &ItemRecord) -> Option<&str> {
    if let Some(p) = r.item.password.as_deref().filter(|p| !p.is_empty()) {
        return Some(p);
    }
    r.sections.iter().flat_map(|s| &s.fields).find(|f| !f.removed && f.kind == FieldKind::Concealed && (f.id == "password" || f.label.eq_ignore_ascii_case("password"))).and_then(|f| f.value.as_deref()).filter(|p| !p.is_empty())
}

fn snapshot_password(s: &Snapshot) -> Option<&str> {
    if let Some(p) = s.item.password.as_deref().filter(|p| !p.is_empty()) {
        return Some(p);
    }
    s.sections.iter().flat_map(|s| &s.fields).find(|f| !f.removed && f.kind == FieldKind::Concealed && (f.id == "password" || f.label.eq_ignore_ascii_case("password"))).and_then(|f| f.value.as_deref()).filter(|p| !p.is_empty())
}

fn parse_ts(s: &str) -> Option<u64> {
    let trimmed = s.trim();
    if let Ok(secs) = trimmed.parse::<u64>() {
        return Some(secs);
    }
    crate::device::parse_iso8601(trimmed).and_then(|s| if s >= 0 { Some(s as u64) } else { None })
}

fn field_map(r: &ItemRecord) -> BTreeMap<String, (String, Field)> {
    r.sections.iter().flat_map(|s| s.fields.iter().map(move |f| (f.id.clone(), (s.id.clone(), f.clone())))).collect()
}
fn meta_values(r: &ItemRecord) -> [(&'static str, String); 7] {
    [("m:category", r.category.as_str().into()), ("m:tags", r.tags.join("\u{1}")), ("m:favorite", r.favorite.to_string()),
     ("m:archived", r.archived.to_string()), ("m:deleted", r.deleted_at.map(|d| d.to_string()).unwrap_or_default()),
     ("m:ignored", r.ignored_findings.join("\u{1}")),
     ("m:related", r.related_item_ids.join("\u{1}"))]
}
fn section_titles(r: &ItemRecord) -> BTreeMap<String, Option<String>> { r.sections.iter().map(|s| (s.id.clone(), s.title.clone())).collect() }

/// Stamp v5 clocks on `new`: every field / section title / meta key whose value differs from `old`
/// gets `max(now, old_clock+1)`. With no `old` everything gets `now`.
pub fn stamp_ext(old: Option<&ItemRecord>, new: &mut ItemRecord, now: u64) {
    let mut c = old.map(|o| o.xclocks.clone()).unwrap_or_default();
    let bump = |c: &mut BTreeMap<String, u64>, k: String| { let v = now.max(c.get(&k).copied().unwrap_or(0) + 1); c.insert(k, v); };
    let (of, os, om) = match old { Some(o) => (field_map(o), section_titles(o), Some(meta_values(o))), None => (BTreeMap::new(), BTreeMap::new(), None) };
    for (id, (_, f)) in field_map(new) { if of.get(&id).map(|(_, x)| x) != Some(&f) { bump(&mut c, format!("f:{id}")); } }
    for (id, t) in section_titles(new) { if os.get(&id) != Some(&t) { bump(&mut c, format!("s:{id}")); } }
    let nm = meta_values(new);
    for (i, (k, v)) in nm.iter().enumerate() {
        // new items: default-valued meta keeps clock 0 (keeps v4-sized payloads small; any edit wins)
        let changed = match om.as_ref() { Some(m) => &m[i].1 != v, None => *v != meta_values(&ItemRecord::default())[i].1 };
        if changed { bump(&mut c, k.to_string()); }
    }
    new.xclocks = c;
}

/// Merge the v5 extension (sections by field id, meta, history, attachments) of `remote` into `out`
/// (which starts as a copy of local). Returns (keys taken from remote, conflicting keys).
fn merge_ext(local: &ItemRecord, remote: &ItemRecord, out: &mut ItemRecord) -> (Vec<String>, Vec<String>) {
    let (mut from_r, mut conf) = (vec![], vec![]);
    let lc = |k: &str| local.xclocks.get(k).copied().unwrap_or(0);
    let rc = |k: &str| remote.xclocks.get(k).copied().unwrap_or(0);
    // fields
    let (lf, rf) = (field_map(local), field_map(remote));
    for (id, (rsid, rfld)) in &rf {
        let k = format!("f:{id}");
        match lf.get(id) {
            None => {
                match out.sections.iter_mut().find(|s| &s.id == rsid) {
                    Some(s) => s.fields.push(rfld.clone()),
                    None => {
                        let title = remote.sections.iter().find(|s| &s.id == rsid).and_then(|s| s.title.clone());
                        out.sections.push(Section { id: rsid.clone(), title, fields: vec![rfld.clone()] });
                    }
                }
                out.xclocks.insert(k.clone(), rc(&k));
                from_r.push(k);
            }
            Some((_, lfld)) => {
                if rc(&k) > lc(&k) {
                    if lfld != rfld {
                        for s in out.sections.iter_mut() { for f in s.fields.iter_mut() { if &f.id == id { *f = rfld.clone(); } } }
                        from_r.push(k.clone());
                    }
                    out.xclocks.insert(k.clone(), rc(&k));
                } else if rc(&k) == lc(&k) && lfld != rfld { conf.push(k); }
            }
        }
    }
    // section titles
    for s in &remote.sections {
        let k = format!("s:{}", s.id);
        if rc(&k) > lc(&k) {
            if let Some(os) = out.sections.iter_mut().find(|x| x.id == s.id) { if os.title != s.title { os.title = s.title.clone(); from_r.push(k.clone()); } }
            out.xclocks.insert(k.clone(), rc(&k));
        }
    }
    // meta
    let (lm, rm) = (meta_values(local), meta_values(remote));
    for i in 0..lm.len() {
        let k = lm[i].0;
        if rc(k) > lc(k) {
            if lm[i].1 != rm[i].1 {
                match k {
                    "m:category" => out.category = remote.category,
                    "m:tags" => out.tags = remote.tags.clone(),
                    "m:favorite" => out.favorite = remote.favorite,
                    "m:archived" => out.archived = remote.archived,
                    "m:ignored" => out.ignored_findings = remote.ignored_findings.clone(),
                    "m:related" => out.related_item_ids = remote.related_item_ids.clone(),
                    _ => out.deleted_at = remote.deleted_at,
                }
                from_r.push(k.to_string());
            }
            out.xclocks.insert(k.to_string(), rc(k));
        } else if rc(k) == lc(k) && lm[i].1 != rm[i].1 && (rc(k) != 0) { conf.push(k.to_string()); }
    }
    // attachment tombstones: union (applied after the attachment union below)
    for id in &remote.removed_attachments {
        if !out.removed_attachments.contains(id) { out.removed_attachments.push(id.clone()); from_r.push(format!("a-:{id}")); }
    }
    // attachments: union by id, minus anything removed on either side
    for a in &remote.attachments {
        if out.removed_attachments.contains(&a.id) { continue; }
        if !out.attachments.iter().any(|x| x.id == a.id) { out.attachments.push(a.clone()); from_r.push(format!("a:{}", a.id)); }
    }
    let removed = out.removed_attachments.clone();
    out.attachments.retain(|a| !removed.contains(&a.id));
    // history: union by (version, updated_at), keep newest HISTORY_LIMIT
    for h in &remote.history {
        if !out.history.iter().any(|x| x.version == h.version && x.updated_at == h.updated_at) { out.history.push(h.clone()); }
    }
    out.history.sort_by_key(|h| h.version);
    let n = out.history.len();
    if n > crate::item::HISTORY_LIMIT { out.history.drain(..n - crate::item::HISTORY_LIMIT); }
    // password_history: union by (value, replaced_at), keep newest PASSWORD_HISTORY_LIMIT
    for ph in &remote.password_history {
        if !out.password_history.iter().any(|x| x.value == ph.value && x.replaced_at == ph.replaced_at) {
            out.password_history.push(ph.clone());
        }
    }
    out.password_history.sort_by(|a, b| b.replaced_at.cmp(&a.replaced_at));
    if out.password_history.len() > crate::item::PASSWORD_HISTORY_LIMIT {
        out.password_history.truncate(crate::item::PASSWORD_HISTORY_LIMIT);
    }
    out.created_at = match (local.created_at, remote.created_at) { (Some(a), Some(b)) => Some(a.min(b)), (a, b) => a.or(b) };
    (from_r, conf)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Returns field value as a comparable Option<&str> (title is always Some).
pub fn field<'a>(it: &'a Item, f: &str) -> Option<&'a str> {
    match f {
        "title" => Some(it.title.as_str()),
        "username" => it.username.as_deref(), "password" => it.password.as_deref(),
        "url" => it.url.as_deref(), "notes" => it.notes.as_deref(), "totp" => it.totp.as_deref(),
        _ => None,
    }
}
fn copy_field(dst: &mut Item, src: &Item, f: &str) {
    match f {
        "title" => dst.title = src.title.clone(),
        "username" => dst.username = src.username.clone(), "password" => dst.password = src.password.clone(),
        "url" => dst.url = src.url.clone(), "notes" => dst.notes = src.notes.clone(), "totp" => dst.totp = src.totp.clone(),
        _ => {}
    }
}

/// Compute clocks for a new write: fields whose value changed vs `old` get `max(now, old_clock+1)`;
/// unchanged fields keep their clock. With no `old` (new item) every field gets `now`.
pub fn stamp(old: Option<(&Item, FieldClocks)>, new: &Item, now: u64) -> FieldClocks {
    match old {
        None => FieldClocks::all(now),
        Some((o, oc)) => {
            let mut c = oc;
            for f in FIELDS {
                if field(o, f) != field(new, f) { c.set(f, now.max(oc.get(f) + 1)); }
            }
            c
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldOutcome { Same, LocalWins, RemoteWins, Conflict }

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Fields taken from remote (remote clock was higher).
    pub from_remote: Vec<&'static str>,
    /// Fields kept from local because the local clock was higher.
    pub from_local: Vec<&'static str>,
    /// Equal clocks but different values; local value kept.
    pub conflicts: Vec<&'static str>,
    /// v5 keys (`f:<field id>`, `s:<section>`, `m:<meta>`, `a:<attachment>`) taken from remote.
    pub ext_from_remote: Vec<String>,
    /// v5 keys with equal clocks but different values (local kept).
    pub ext_conflicts: Vec<String>,
}
impl MergeReport {
    pub fn has_conflicts(&self) -> bool { !self.conflicts.is_empty() || !self.ext_conflicts.is_empty() }
    /// True if the merged result differs from local (i.e. a write is needed).
    pub fn changed_local(&self) -> bool { !self.from_remote.is_empty() || !self.ext_from_remote.is_empty() }
}

/// Field-level merge. Per field: higher clock wins; equal clock + equal value = same;
/// equal clock + different value = conflict (local kept, recorded). `updated_at` of the result is the max
/// field clock, in seconds (same unit as v2 `updated_at`).
pub fn merge_records(local: &ItemRecord, remote: &ItemRecord) -> (ItemRecord, MergeReport) {
    let (lc, rc) = (local.clocks_or_legacy(), remote.clocks_or_legacy());
    let mut out = local.item.clone();
    let mut oc = lc;
    let mut rep = MergeReport::default();
    for f in FIELDS {
        let (l, r) = (lc.get(f), rc.get(f));
        let same = field(&local.item, f) == field(&remote.item, f);
        let o = if r > l { FieldOutcome::RemoteWins } else if l > r { FieldOutcome::LocalWins }
            else if same { FieldOutcome::Same } else { FieldOutcome::Conflict };
        match o {
            FieldOutcome::RemoteWins => { copy_field(&mut out, &remote.item, f); oc.set(f, r); if !same { rep.from_remote.push(f); } }
            FieldOutcome::LocalWins => { if !same { rep.from_local.push(f); } }
            FieldOutcome::Conflict => rep.conflicts.push(f),
            FieldOutcome::Same => {}
        }
    }
    let ext_max = local.xclocks.values().chain(remote.xclocks.values()).copied().max().unwrap_or(0);
    out.updated_at = (oc.max().max(ext_max) / 1000).to_string();
    let mut res = local.clone();
    res.item = out;
    res.clocks = Some(oc);
    let (fr, cf) = merge_ext(local, remote, &mut res);
    rep.ext_from_remote = fr;
    rep.ext_conflicts = cf;
    (res, rep)
}

/// Convenience wrapper over plain [`Item`]s: clocks are synthesized from `updated_at` (legacy semantics),
/// which makes this whole-item last-write-wins with equal-timestamp conflicts reported per field.
pub fn merge_items(local: &Item, remote: &Item) -> (Item, MergeReport) {
    let (r, rep) = merge_records(&ItemRecord::new(local.clone()),
                                 &ItemRecord::new(remote.clone()));
    (r.item.clone(), rep)
}
