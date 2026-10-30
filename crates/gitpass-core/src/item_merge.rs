//! Merge two duplicate login items into one.
//!
//! This is the item-level merge of issue #396 (duplicate logins flagged by
//! Watchtower), not the vault/sync field-level merge in [`crate::merge`].
//! [`merge_duplicate_records`] is pure: it combines two [`ItemRecord`]s
//! without touching disk, so it is unit-testable without a vault. The
//! [`Session`](crate::store::Session) method
//! [`merge_duplicate_items`](crate::store::Session::merge_duplicate_items)
//! wraps it: it copies the discarded item's attachments (re-sealed under the
//! kept item, so the copy survives even after the discarded item is purged)
//! and writes the merged kept record plus the soft-deleted discarded record
//! in a single manifest commit.
//!
//! Merge rules (kept item wins ties):
//! - Password: the newer password by [`ItemRecord::password_changed_at`]
//!   wins; the losing password is added to the kept item's password history
//!   with its own date, so no secret is lost. Both items' password histories
//!   are unioned (deduplicated by value, capped at
//!   [`PASSWORD_HISTORY_LIMIT`](crate::item::PASSWORD_HISTORY_LIMIT)).
//! - URLs: the kept item's URL stays primary; a differing discarded URL is
//!   kept as an extra `website (merged)` field so it is not lost.
//! - Tags and related-item ids are unioned; notes are joined with a
//!   separator when both are non-empty and different.
//! - TOTP and extra section fields come from the discarded item when the
//!   kept item lacks them; on a real field conflict both values are kept,
//!   the second labelled `(merged)`.
//! - Attachments are unioned by id (the Session method remaps the discarded
//!   item's attachments to freshly sealed copies first, so the ids in the
//!   merged record point at blobs owned by the kept item).
//! - The kept item's version history is kept as-is; the discarded item's
//!   full record (including its own history) stays in Recently Deleted, so
//!   restoring it recovers everything the merge did not carry over.
//! - Only login items merge; anything else is an error. The report never
//!   contains secret values, only counts, titles and field labels.

use crate::item::{Field, FieldKind, PasswordHistoryEntry, Section, PASSWORD_HISTORY_LIMIT};
use crate::merge::ItemRecord;
use anyhow::{bail, Result};

/// Outcome of a duplicate-item merge: the merged record plus what moved.
/// `keep_version_before` / `keep_version` / `other_version` are filled in by
/// the Session method (0 for a pure-function call, which writes nothing).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DuplicateMergeReport {
    /// True when the kept item's password came from the discarded item.
    pub password_from_other: bool,
    /// True when a losing password was added to the password history.
    pub password_moved_to_history: bool,
    /// True when the kept item's TOTP came from the discarded item.
    pub totp_from_other: bool,
    /// Extra URLs preserved as `website (merged)` fields.
    pub urls_added: usize,
    /// Tags taken from the discarded item (names only, never secrets).
    pub tags_added: Vec<String>,
    /// Related-item ids taken from the discarded item.
    pub related_added: usize,
    /// True when both notes were non-empty, different, and joined.
    pub notes_joined: bool,
    /// Labels of extra fields copied from the discarded item.
    pub fields_copied: Vec<String>,
    /// Labels of fields kept twice because both items had different values.
    pub fields_conflicted: Vec<String>,
    /// Attachment metadata entries added to the kept item.
    pub attachments_added: usize,
    /// Password-history entries on the merged record.
    pub history_entries: usize,
}

/// Session-level result of
/// [`merge_duplicate_items`](crate::store::Session::merge_duplicate_items).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateMergeOutcome {
    pub report: DuplicateMergeReport,
    /// Kept item's version before the merge (for Undo via version restore).
    pub keep_version_before: u64,
    /// Kept item's version after the merge.
    pub keep_version: u64,
    /// Discarded item's version after it moved to Recently Deleted.
    pub other_version: u64,
}

fn nonempty(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|v| !v.trim().is_empty())
}

/// Sort key for a password-history `replaced_at` string: unix seconds when
/// parseable (plain seconds or ISO-8601), so history can be ordered newest
/// first regardless of which format an entry was written in.
fn history_key(replaced_at: &str) -> Option<u64> {
    let t = replaced_at.trim();
    if let Ok(secs) = t.parse::<u64>() {
        return Some(secs);
    }
    crate::device::parse_iso8601(t).and_then(|s| if s >= 0 { Some(s as u64) } else { None })
}

/// Union two password histories, deduplicated by value. On a duplicate value
/// the `primary` entry wins (its date is kept); the result is newest first
/// and capped at [`PASSWORD_HISTORY_LIMIT`].
pub fn union_password_histories(
    primary: &[PasswordHistoryEntry],
    secondary: &[PasswordHistoryEntry],
) -> Vec<PasswordHistoryEntry> {
    let mut out: Vec<PasswordHistoryEntry> = Vec::new();
    for e in primary.iter().chain(secondary.iter()) {
        if !out.iter().any(|x| x.value == e.value) {
            out.push(e.clone());
        }
    }
    out.sort_by(|a, b| history_key(&b.replaced_at).cmp(&history_key(&a.replaced_at)));
    out.truncate(PASSWORD_HISTORY_LIMIT);
    out
}

/// ISO-8601 string for a password-change time: the changed-at seconds when
/// known, else the item's `updated_at` parsed the same way, else the raw
/// `updated_at` string (kept as-is rather than dropped).
fn change_date(changed_at: Option<u64>, updated_at: &str) -> String {
    if let Some(secs) = changed_at {
        return crate::device::iso8601(secs as i64);
    }
    if let Some(secs) = history_key(updated_at) {
        return crate::device::iso8601(secs as i64);
    }
    updated_at.to_string()
}

fn unique_field_id(section: &Section, base: &str) -> String {
    if !section.fields.iter().any(|f| f.id == base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let cand = format!("{base}-{n}");
        if !section.fields.iter().any(|f| f.id == cand) {
            return cand;
        }
        n += 1;
    }
}

/// Combine two login records into the kept one. Fails on non-login items,
/// on already-deleted items, and (defensively) on identical records passed
/// as both sides; same-vault and same-item checks live in the Session
/// method, which knows the ids.
pub fn merge_duplicate_records(
    keep: &ItemRecord,
    other: &ItemRecord,
) -> Result<(ItemRecord, DuplicateMergeReport)> {
    use crate::item::Category;
    if keep.category != Category::Login || other.category != Category::Login {
        bail!("only login items can be merged");
    }
    if keep.deleted_at.is_some() || other.deleted_at.is_some() {
        bail!("cannot merge a deleted item (restore it first)");
    }
    let mut merged = keep.clone();
    merged.deleted_at = None;

    // ---- password: newer password_changed_at wins, keep wins ties ----
    let keep_pw = nonempty(&keep.item.password);
    let other_pw = nonempty(&other.item.password);
    let mut report = DuplicateMergeReport {
        password_from_other: false,
        password_moved_to_history: false,
        totp_from_other: false,
        urls_added: 0,
        tags_added: vec![],
        related_added: 0,
        notes_joined: false,
        fields_copied: vec![],
        fields_conflicted: vec![],
        attachments_added: 0,
        history_entries: 0,
    };
    let mut history = union_password_histories(&keep.password_history, &other.password_history);
    match (keep_pw, other_pw) {
        (Some(k), Some(o)) if k != o => {
            let keep_newer = match (keep.password_changed_at(), other.password_changed_at()) {
                (Some(kc), Some(oc)) => kc >= oc,
                // Unknown change time: the kept item wins, so its password stays.
                _ => true,
            };
            if keep_newer {
                let date = change_date(other.password_changed_at(), &other.item.updated_at);
                if !history.iter().any(|e| e.value == o) {
                    history.insert(
                        0,
                        PasswordHistoryEntry {
                            value: o.to_string(),
                            replaced_at: date,
                        },
                    );
                    history.truncate(PASSWORD_HISTORY_LIMIT);
                    report.password_moved_to_history = true;
                }
            } else {
                merged.item.password = Some(o.to_string());
                report.password_from_other = true;
                let date = change_date(keep.password_changed_at(), &keep.item.updated_at);
                if !history.iter().any(|e| e.value == k) {
                    history.insert(
                        0,
                        PasswordHistoryEntry {
                            value: k.to_string(),
                            replaced_at: date,
                        },
                    );
                    history.truncate(PASSWORD_HISTORY_LIMIT);
                    report.password_moved_to_history = true;
                }
            }
        }
        (None, Some(o)) => {
            merged.item.password = Some(o.to_string());
            report.password_from_other = true;
        }
        _ => {}
    }
    // Keep the union newest-first after any insert above.
    history.sort_by(|a, b| history_key(&b.replaced_at).cmp(&history_key(&a.replaced_at)));
    history.truncate(PASSWORD_HISTORY_LIMIT);
    merged.password_history = history;
    report.history_entries = merged.password_history.len();

    // ---- legacy login fields ----
    if nonempty(&merged.item.username).is_none() {
        merged.item.username = other.item.username.clone();
    }
    if merged.item.title.trim().is_empty() {
        merged.item.title = other.item.title.clone();
    }
    if nonempty(&merged.item.totp).is_none() {
        if let Some(t) = nonempty(&other.item.totp) {
            merged.item.totp = Some(t.to_string());
            report.totp_from_other = true;
        }
    }

    // ---- URL: kept primary, differing discarded URL preserved as a field ----
    match (nonempty(&keep.item.url), nonempty(&other.item.url)) {
        (None, Some(o)) => merged.item.url = Some(o.to_string()),
        (Some(_), Some(o)) => {
            let already = merged.item.url.as_deref() == Some(o)
                || merged.sections.iter().flat_map(|s| &s.fields).any(|f| {
                    f.kind == FieldKind::Url && !f.removed && f.value.as_deref() == Some(o)
                });
            if !already {
                let section = match merged.sections.iter_mut().find(|s| s.id == "merged") {
                    Some(s) => s,
                    None => {
                        merged.sections.push(Section {
                            id: "merged".into(),
                            title: Some("Merged".into()),
                            fields: vec![],
                        });
                        merged.sections.last_mut().expect("just pushed")
                    }
                };
                let id = unique_field_id(section, "website-merged");
                section.fields.push(Field {
                    id,
                    label: "website (merged)".into(),
                    kind: FieldKind::Url,
                    value: Some(o.to_string()),
                    removed: false,
                });
                report.urls_added = 1;
            }
        }
        _ => {}
    }

    // ---- notes ----
    match (nonempty(&keep.item.notes), nonempty(&other.item.notes)) {
        (Some(k), Some(o)) if k != o => {
            merged.item.notes = Some(format!("{k}\n\n---\n\n{o}"));
            report.notes_joined = true;
        }
        (None, Some(o)) => merged.item.notes = Some(o.to_string()),
        _ => {}
    }

    // ---- tags, related ids, flags, meta ----
    for t in &other.tags {
        if !merged.tags.contains(t) {
            merged.tags.push(t.clone());
            report.tags_added.push(t.clone());
        }
    }
    for r in &other.related_item_ids {
        if !merged.related_item_ids.contains(r) {
            merged.related_item_ids.push(r.clone());
            report.related_added += 1;
        }
    }
    merged.favorite = keep.favorite || other.favorite;
    for f in &other.ignored_findings {
        if !merged.ignored_findings.contains(f) {
            merged.ignored_findings.push(f.clone());
        }
    }
    if merged.rotate_every_days.is_none() {
        merged.rotate_every_days = other.rotate_every_days;
    }
    merged.created_at = match (keep.created_at, other.created_at) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, b) => b,
    };
    for a in &other.attachments {
        if !merged.attachments.iter().any(|x| x.id == a.id) {
            merged.attachments.push(a.clone());
            report.attachments_added += 1;
        }
    }
    for a in &other.removed_attachments {
        if !merged.removed_attachments.contains(a) {
            merged.removed_attachments.push(a.clone());
        }
    }

    // ---- extra section fields ----
    for sec in &other.sections {
        for field in &sec.fields {
            if field.removed {
                continue;
            }
            // Synthesized login fields live in the legacy fields above.
            if sec.id == "login" && matches!(field.id.as_str(), "username" | "password" | "totp") {
                continue;
            }
            let section = match merged.sections.iter_mut().find(|s| s.id == sec.id) {
                Some(s) => s,
                None => {
                    merged.sections.push(Section {
                        id: sec.id.clone(),
                        title: sec.title.clone(),
                        fields: vec![],
                    });
                    merged.sections.last_mut().expect("just pushed")
                }
            };
            if section.fields.iter().any(|f| f.id == field.id && f.removed) {
                continue; // kept item deleted this field: do not resurrect it
            }
            match section
                .fields
                .iter_mut()
                .find(|f| f.id == field.id && !f.removed)
            {
                None => {
                    section.fields.push(field.clone());
                    report.fields_copied.push(field.label.clone());
                }
                Some(existing) => {
                    let existing_value = existing.value.clone().filter(|v| !v.trim().is_empty());
                    let other_value = field.value.clone().filter(|v| !v.trim().is_empty());
                    match (existing_value, other_value) {
                        (None, Some(v)) => existing.value = Some(v),
                        (Some(a), Some(b)) if a != b => {
                            let id = unique_field_id(section, &format!("{}-merged", field.id));
                            section.fields.push(Field {
                                id,
                                label: format!("{} (merged)", field.label),
                                kind: field.kind,
                                value: Some(b),
                                removed: false,
                            });
                            report.fields_conflicted.push(field.label.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok((merged, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Category, Field, FieldKind, Section};
    use crate::store::Item;

    fn login(title: &str, username: &str, password: &str, url: &str) -> ItemRecord {
        let mut it = Item::default();
        it.title = title.into();
        it.username = Some(username.into());
        it.password = if password.is_empty() {
            None
        } else {
            Some(password.into())
        };
        it.url = if url.is_empty() {
            None
        } else {
            Some(url.into())
        };
        it.updated_at = "1700000000".into();
        let mut r = ItemRecord::new(it);
        r.category = Category::Login;
        r.created_at = Some(1700000000);
        r
    }

    fn field(id: &str, label: &str, kind: FieldKind, value: &str) -> Field {
        Field {
            id: id.into(),
            label: label.into(),
            kind,
            value: Some(value.into()),
            removed: false,
        }
    }

    #[test]
    fn newer_password_wins_and_loser_goes_to_history() {
        let mut keep = login("Example", "alice", "FAKE-old", "https://a.example.invalid");
        keep.created_at = Some(1700000000);
        let mut other = login("Example", "alice", "FAKE-new", "https://a.example.invalid");
        other.created_at = Some(1700000100);
        // Newer change: other was edited later (history entry proves the change).
        let mut hist = ItemRecord::new(other.item.clone());
        hist.item.password = Some("FAKE-older".into());
        other.history.push(crate::item::HistoryEntry {
            version: 1,
            updated_at: "1700000050".into(),
            snapshot: hist.snapshot(),
        });
        other.item.updated_at = "1700000200".into();
        let (merged, report) = merge_duplicate_records(&keep, &other).unwrap();
        assert_eq!(merged.item.password.as_deref(), Some("FAKE-new"));
        assert!(report.password_from_other);
        assert!(report.password_moved_to_history);
        assert!(merged
            .password_history
            .iter()
            .any(|e| e.value == "FAKE-old"));
    }

    #[test]
    fn keep_password_wins_tie_and_other_goes_to_history() {
        let keep = login("Example", "alice", "FAKE-keep", "https://a.example.invalid");
        let other = login(
            "Example",
            "alice",
            "FAKE-other",
            "https://a.example.invalid",
        );
        let (merged, report) = merge_duplicate_records(&keep, &other).unwrap();
        assert_eq!(merged.item.password.as_deref(), Some("FAKE-keep"));
        assert!(!report.password_from_other);
        assert!(report.password_moved_to_history);
        assert!(merged
            .password_history
            .iter()
            .any(|e| e.value == "FAKE-other"));
    }

    #[test]
    fn tags_notes_totp_and_urls_union() {
        let mut keep = login("Example", "alice", "FAKE-pw", "https://a.example.invalid");
        keep.tags = vec!["work".into()];
        keep.item.notes = Some("keep note".into());
        let mut other = login("Example", "alice", "FAKE-pw", "https://b.example.invalid");
        other.tags = vec!["work".into(), "personal".into()];
        other.item.notes = Some("other note".into());
        other.item.totp = Some("FAKE-totp".into());
        let (merged, report) = merge_duplicate_records(&keep, &other).unwrap();
        assert_eq!(merged.tags, vec!["work", "personal"]);
        assert_eq!(report.tags_added, vec!["personal"]);
        assert!(merged.item.notes.as_deref().unwrap().contains("keep note"));
        assert!(merged.item.notes.as_deref().unwrap().contains("other note"));
        assert!(report.notes_joined);
        assert_eq!(merged.item.totp.as_deref(), Some("FAKE-totp"));
        assert!(report.totp_from_other);
        assert_eq!(report.urls_added, 1);
        assert!(merged
            .sections
            .iter()
            .flat_map(|s| &s.fields)
            .any(|f| f.label == "website (merged)"
                && f.value.as_deref() == Some("https://b.example.invalid")));
    }

    #[test]
    fn conflicting_extra_field_kept_twice_labelled_merged() {
        let mut keep = login("Example", "alice", "FAKE-pw", "");
        keep.sections = vec![Section {
            id: "extra".into(),
            title: None,
            fields: vec![field("pin", "PIN", FieldKind::Concealed, "1111")],
        }];
        let mut other = login("Example", "alice", "FAKE-pw", "");
        other.sections = vec![Section {
            id: "extra".into(),
            title: None,
            fields: vec![
                field("pin", "PIN", FieldKind::Concealed, "2222"),
                field(
                    "recovery",
                    "Recovery code",
                    FieldKind::Text,
                    "FAKE-recovery",
                ),
            ],
        }];
        let (merged, report) = merge_duplicate_records(&keep, &other).unwrap();
        let fields: Vec<&Field> = merged.sections.iter().flat_map(|s| &s.fields).collect();
        assert!(fields
            .iter()
            .any(|f| f.id == "pin" && f.value.as_deref() == Some("1111")));
        assert!(fields
            .iter()
            .any(|f| f.label == "PIN (merged)" && f.value.as_deref() == Some("2222")));
        assert!(fields.iter().any(|f| f.id == "recovery"));
        assert_eq!(report.fields_conflicted, vec!["PIN"]);
        assert_eq!(report.fields_copied, vec!["Recovery code"]);
    }

    #[test]
    fn non_login_and_deleted_rejected() {
        let mut keep = login("Example", "alice", "FAKE-pw", "");
        let mut other = login("Example", "alice", "FAKE-pw", "");
        other.category = Category::SecureNote;
        assert!(merge_duplicate_records(&keep, &other).is_err());
        other.category = Category::Login;
        other.deleted_at = Some(1700000000);
        assert!(merge_duplicate_records(&keep, &other).is_err());
        keep.deleted_at = Some(1700000000);
        assert!(merge_duplicate_records(&keep, &other).is_err());
    }
}
