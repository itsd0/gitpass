//! Format v3 field-level merge.
//!
//! Every logical item field carries its own logical clock (u64 milliseconds since the Unix epoch,
//! Lamport-bumped so a local edit always gets a clock strictly greater than the previous one for that
//! field). Clocks are stored next to the item JSON inside the same encrypted, padded payload
//! (see [`ItemRecord`]); nothing about them is visible outside the ciphertext.
//!
//! Clearing a field is modelled as a tombstone: the value becomes `None` and the field's clock is bumped,
//! so a clear competes with a concurrent edit by clock just like any other write.
use crate::store::Item;
use serde::{Deserialize, Serialize};
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
}

/// Synthesize clocks for a legacy (v2) item: every field gets `updated_at` (seconds) * 1000.
/// Unparseable `updated_at` yields clock 0 (any v3 edit wins).
pub fn legacy_clocks(item: &Item) -> FieldClocks {
    FieldClocks::all(item.updated_at.trim().parse::<u64>().map(|s| s.saturating_mul(1000)).unwrap_or(0))
}

impl ItemRecord {
    pub fn clocks_or_legacy(&self) -> FieldClocks { self.clocks.unwrap_or_else(|| legacy_clocks(&self.item)) }
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
}
impl MergeReport {
    pub fn has_conflicts(&self) -> bool { !self.conflicts.is_empty() }
    /// True if the merged result differs from local (i.e. a write is needed).
    pub fn changed_local(&self) -> bool { !self.from_remote.is_empty() }
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
    out.updated_at = (oc.max() / 1000).to_string();
    (ItemRecord { item: out, clocks: Some(oc) }, rep)
}

/// Convenience wrapper over plain [`Item`]s: clocks are synthesized from `updated_at` (legacy semantics),
/// which makes this whole-item last-write-wins with equal-timestamp conflicts reported per field.
pub fn merge_items(local: &Item, remote: &Item) -> (Item, MergeReport) {
    let (r, rep) = merge_records(&ItemRecord { item: local.clone(), clocks: None },
                                 &ItemRecord { item: remote.clone(), clocks: None });
    (r.item.clone(), rep)
}
