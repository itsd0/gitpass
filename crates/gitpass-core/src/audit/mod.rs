//! Security audit: weak, reused, old passwords; missing TOTP; offline 2FA/passkey availability;
//! optional HIBP Pwned Passwords check. Findings carry titles/ids only — never passwords.

pub mod dates;
pub mod hibp;
pub mod sites;
pub mod strength;
pub mod url;

pub use dates::{parse_expiry, unused_threshold_days};
pub use hibp::{pwned_count, PwnedRangeClient, HIBP_RANGE_URL};
pub use sites::{lookup_mfa_site, mfa_sites, parse_mfa_site_list, site_support, SiteSupport};
pub use strength::{contains_personal_info, entropy_bits, weakness};
pub use url::{change_password_url, decode_punycode, extract_site_name, insecure_url_host};
pub(crate) use url::host_of;

use dates::{activity_ts, parse_expiry as dates_parse_expiry, unused_threshold_days as dates_unused_threshold_days};
use hibp::pwned_count as hibp_pwned_count;
use sites::{bundled_mfa_sites, has_passkey_for, lookup_mfa_site as sites_lookup_mfa_site};
use strength::{contains_personal_info as strength_contains_personal_info, weakness as strength_weakness};
use url::{change_password_url as url_change_password_url, host_of as url_host_of, insecure_url_host as url_insecure_url_host};

use crate::merge::ItemRecord;
use crate::store::Item;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finding {
    Weak { reason: String },
    /// Password shared with `others` other items.
    Reused { group: usize, others: usize },
    /// The current password matches a retired entry in a password history: `other_title` is the
    /// title of the item whose history held it (the item itself when `same_item`), and
    /// `retired_on` is that entry's `replaced_at` date. The retired value itself is never stored here.
    ReusedOldPassword { other_title: String, retired_on: String, same_item: bool },
    Old { days: u64 },
    MissingTotp,
    /// `site` (from the bundled offline list) documents TOTP, but the item has none.
    TotpAvailable { site: String },
    /// `site` documents passkeys, but the item (and the vault) has none for it.
    PasskeyAvailable { site: String },
    Pwned { count: u64 },
    Expired { on: String },
    ExpiringSoon { on: String, days: u64 },
    /// Saved with an unencrypted HTTP URL (public host only; no secrets/paths/queries).
    InsecureUrl { url_host: String },
    Duplicate { group: String },
    WeakSshKey { bits: usize },
    /// A live Login whose last use on this device (or last edit/creation when it was never used)
    /// is older than the unused threshold. Housekeeping advice: it does not affect the score.
    Unused { days: u64 },
    /// The login's own "change password every `every_days` days" reminder is due: the password is
    /// `days` days old (measured from [`ItemRecord::password_changed_at`], not the last edit).
    RotationDue { every_days: u32, days: u64 },
    /// The rotation deadline is `in_days` days away (within [`crate::merge::ROTATION_SOON_DAYS`]).
    RotationSoon { every_days: u32, in_days: u64 },
}

/// Every [`Finding::kind`] string, in declaration order (docs and `gitpass audit --help` list these).
pub const FINDING_KINDS: &[&str] = &["weak", "reused", "reused_old_password", "old", "missing_totp", "totp_available", "passkey_available", "pwned", "expired", "expiring_soon", "insecure_url", "duplicate", "weak_ssh_key", "unused", "rotation_due", "rotation_soon"];

impl Finding {
    pub fn kind(&self) -> &'static str {
        match self {
            Finding::Weak { .. } => "weak",
            Finding::Reused { .. } => "reused",
            Finding::ReusedOldPassword { .. } => "reused_old_password",
            Finding::Old { .. } => "old",
            Finding::MissingTotp => "missing_totp",
            Finding::TotpAvailable { .. } => "totp_available",
            Finding::PasskeyAvailable { .. } => "passkey_available",
            Finding::Pwned { .. } => "pwned",
            Finding::Expired { .. } => "expired",
            Finding::ExpiringSoon { .. } => "expiring_soon",
            Finding::InsecureUrl { .. } => "insecure_url",
            Finding::Duplicate { .. } => "duplicate",
            Finding::WeakSshKey { .. } => "weak_ssh_key",
            Finding::Unused { .. } => "unused",
            Finding::RotationDue { .. } => "rotation_due",
            Finding::RotationSoon { .. } => "rotation_soon",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemFindings {
    pub vault: String, pub id: String, pub title: String,
    /// `https://<host>/.well-known/change-password` for the item's URL (the same URL applies to
    /// every finding in the item). Absent when the item has no usable URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_password_url: Option<String>,
    pub findings: Vec<Finding>,
}

#[derive(Clone, Debug)]
pub struct AuditOpts {
    pub max_age_days: u64, pub min_length: usize, pub min_entropy_bits: f64, pub now_secs: u64, pub expiring_days: u64, pub include_ignored: bool,
    /// Months without use after which a live Login is reported `unused` (see
    /// [`unused_threshold_days`]); `None` turns the check off.
    pub unused_months: Option<u64>,
    /// Per-item last-used timestamps (unix seconds) from local state. A missing entry means the item
    /// was never used on this device, so its last edit/creation time is used instead.
    pub last_used: BTreeMap<String, u64>,
}

impl Default for AuditOpts {
    fn default() -> Self {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        AuditOpts { max_age_days: 365, min_length: 12, min_entropy_bits: 60.0, now_secs: now, expiring_days: 30, include_ignored: false,
            unused_months: Some(12), last_used: BTreeMap::new() }
    }
}

#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditReport {
    pub items_checked: usize,
    pub weak: usize, pub reused: usize, pub reused_groups: usize,
    #[serde(default)]
    pub reused_old_password: usize,
    pub duplicates: usize, pub old: usize, pub missing_totp: usize, pub pwned: usize,
    pub totp_available: usize, pub passkey_available: usize,
    pub expired: usize, pub expiring_soon: usize,
    pub insecure_urls: usize,
    pub weak_ssh_keys: usize,
    pub unused: usize,
    #[serde(default)]
    pub rotation_due: usize,
    #[serde(default)]
    pub rotation_soon: usize,
    pub ignored: usize,
    pub per_item: Vec<ItemFindings>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_per_item: Vec<ItemFindings>,
}

/// Audit `(vault, id, record)` triples with the bundled site list and no passkey context.
pub fn audit(items: &[(String, String, ItemRecord)], o: &AuditOpts, hibp: Option<&dyn PwnedRangeClient>) -> Result<AuditReport> {
    audit_with_sites(items, &BTreeSet::new(), bundled_mfa_sites(), o, hibp)
}

/// Audit with the vault's existing passkey `rp_id`s: a login on a passkey-capable site is not
/// flagged when a stored passkey already covers the host.
pub fn audit_with_passkeys(items: &[(String, String, ItemRecord)], passkey_sites: &BTreeSet<String>, o: &AuditOpts, hibp: Option<&dyn PwnedRangeClient>) -> Result<AuditReport> {
    audit_with_sites(items, passkey_sites, bundled_mfa_sites(), o, hibp)
}

/// Audit with an explicit offline site list (the bundled list via [`audit`] / [`audit_with_passkeys`]).
pub fn audit_with_sites(items: &[(String, String, ItemRecord)], passkey_sites: &BTreeSet<String>, sites: &[(String, SiteSupport)], o: &AuditOpts, hibp: Option<&dyn PwnedRangeClient>) -> Result<AuditReport> {
    if o.min_length == 0 { bail!("min_length must be > 0"); }
    let passkey_domains: BTreeSet<String> = passkey_sites.iter().filter_map(|s| url_host_of(s)).collect();
    let mut r = AuditReport { items_checked: items.len(), ..Default::default() };
    fn auditable_password(rec: &ItemRecord) -> Option<&str> {
        if rec.category != crate::item::Category::Login && rec.category != crate::item::Category::Server {
            return None;
        }
        if let Some(p) = rec.item.password.as_deref().filter(|p| !p.is_empty()) {
            return Some(p);
        }
        for s in &rec.sections {
            for f in &s.fields {
                if !f.removed && f.kind == crate::item::FieldKind::Concealed && (f.id == "password" || f.label.eq_ignore_ascii_case("password")) {
                    if let Some(p) = f.value.as_deref().filter(|p| !p.is_empty()) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    fn auditable_username(rec: &ItemRecord) -> Option<&str> {
        if let Some(u) = rec.item.username.as_deref().filter(|u| !u.is_empty()) {
            return Some(u);
        }
        for s in &rec.sections {
            for f in &s.fields {
                if !f.removed && (f.id == "username" || f.label.eq_ignore_ascii_case("username")) {
                    if let Some(u) = f.value.as_deref().filter(|u| !u.is_empty()) {
                        return Some(u);
                    }
                }
            }
        }
        None
    }

    fn auditable_url(rec: &ItemRecord) -> Option<&str> {
        if let Some(u) = rec.item.url.as_deref().filter(|u| !u.trim().is_empty()) {
            return Some(u);
        }
        for s in &rec.sections {
            for f in &s.fields {
                if !f.removed && (f.id == "url" || f.id == "website" || f.id == "hostname" || f.label.eq_ignore_ascii_case("url") || f.label.eq_ignore_ascii_case("hostname")) {
                    if let Some(u) = f.value.as_deref().filter(|u| !u.trim().is_empty()) {
                        return Some(u);
                    }
                }
            }
        }
        None
    }

    // reuse groups: keyed by blake3(password); hashes live in memory only and are never printed.
    let mut groups: BTreeMap<[u8; 32], Vec<usize>> = BTreeMap::new();
    for (i, (_, _, rec)) in items.iter().enumerate() {
        if let Some(p) = auditable_password(rec) { groups.entry(*blake3::hash(p.as_bytes()).as_bytes()).or_default().push(i); }
    }
    let mut reuse: HashMap<usize, (usize, usize)> = HashMap::new();
    for (g, idxs) in groups.values().filter(|v| v.len() > 1).enumerate() {
        r.reused_groups += 1;
        for &i in idxs { reuse.insert(i, (g + 1, idxs.len() - 1)); }
    }
    // duplicate groups: login items in the same vault scope that share the same normalized site and username.
    // Archived and recently deleted items are ignored. Items with an empty username are not grouped.
    let mut dup_map: BTreeMap<(String, String, String), Vec<usize>> = BTreeMap::new();
    for (i, (vault, _, rec)) in items.iter().enumerate() {
        if rec.archived || rec.deleted_at.is_some() || rec.category != crate::item::Category::Login {
            continue;
        }
        let Some(user) = rec.item.username.as_deref().map(str::trim).filter(|u| !u.is_empty()) else {
            continue;
        };
        let Some(site) = rec.item.url.as_deref().and_then(url_host_of) else {
            continue;
        };
        dup_map.entry((vault.clone(), site, user.to_lowercase())).or_default().push(i);
    }
    let mut dup_findings: HashMap<usize, String> = HashMap::new();
    for ((_, site, norm_user), idxs) in dup_map.into_iter().filter(|(_, v)| v.len() > 1) {
        let group = format!("{site}/{norm_user}");
        for i in idxs {
            dup_findings.insert(i, group.clone());
        }
    }
    // Retired-password index: password value -> (source item index, history entry), in
    // audit order. Values are compared in memory only; they are never copied into the
    // report — only the source item's title and the entry's retirement date travel with
    // the finding. Blank/whitespace-only history entries are skipped.
    let mut retired: HashMap<&str, Vec<(usize, &crate::item::PasswordHistoryEntry)>> = HashMap::new();
    for (j, (_, _, rec)) in items.iter().enumerate() {
        for entry in &rec.password_history {
            if entry.value.trim().is_empty() { continue; }
            retired.entry(entry.value.as_str()).or_default().push((j, entry));
        }
    }
    let mut cache = HashMap::new();
    for (i, (vault, id, rec)) in items.iter().enumerate() {
        let it = &rec.item;
        let mut f: Vec<Finding> = vec![];
        let mut f_ignored: Vec<Finding> = vec![];
        let mut record_finding = |finding: Finding, r_count: &mut usize| {
            let k = finding.kind();
            if rec.is_finding_ignored(k) {
                r.ignored += 1;
                f_ignored.push(finding);
            } else {
                *r_count += 1;
                f.push(finding);
            }
        };
        if let Some(group) = dup_findings.get(&i) {
            record_finding(Finding::Duplicate { group: group.clone() }, &mut r.duplicates);
        }
        let url = auditable_url(rec);
        if let Some(p) = auditable_password(rec) {
            let user = auditable_username(rec);
            let weak_reason = strength_contains_personal_info(p, user, url)
                .or_else(|| strength_weakness(p, o));
            if let Some(reason) = weak_reason { record_finding(Finding::Weak { reason }, &mut r.weak); }
            if let Some(&(group, others)) = reuse.get(&i) { record_finding(Finding::Reused { group, others }, &mut r.reused); }
            if let Some(candidates) = retired.get(p) {
                // Cross-item match first (most informative), then the item's own history.
                let chosen = candidates.iter().find(|(j, _)| *j != i).or_else(|| candidates.iter().find(|(j, _)| *j == i));
                if let Some((j, entry)) = chosen {
                    record_finding(Finding::ReusedOldPassword { other_title: items[*j].2.item.title.clone(), retired_on: entry.replaced_at.clone(), same_item: *j == i }, &mut r.reused_old_password);
                }
            }
            if let Some(c) = hibp { let n = hibp_pwned_count(c, p, &mut cache)?; if n > 0 { record_finding(Finding::Pwned { count: n }, &mut r.pwned); } }
            if let Some(t) = rec.password_changed_at() {
                let days = o.now_secs.saturating_sub(t) / 86400;
                if days > o.max_age_days { record_finding(Finding::Old { days }, &mut r.old); }
                match rec.rotation_status(o.now_secs) {
                    Some(crate::merge::RotationStatus::Due { every_days, days }) => record_finding(Finding::RotationDue { every_days, days }, &mut r.rotation_due),
                    Some(crate::merge::RotationStatus::Soon { every_days, in_days }) => record_finding(Finding::RotationSoon { every_days, in_days }, &mut r.rotation_soon),
                    None => {}
                }
            }
        }
        let no_totp = it.totp.as_deref().is_none_or(|t| t.trim().is_empty());
        let host = url.and_then(url_host_of);
        let support = host.as_deref().and_then(|h| sites_lookup_mfa_site(sites, h));
        if rec.category == crate::item::Category::Login && url.is_some() && no_totp {
            let mut advised_totp = false;
            if let Some((site, s)) = support {
                if s.totp {
                    record_finding(Finding::TotpAvailable { site: site.to_string() }, &mut r.totp_available);
                    advised_totp = true;
                }
            }
            if !advised_totp {
                record_finding(Finding::MissingTotp, &mut r.missing_totp);
            }
        }
        if rec.category == crate::item::Category::Login {
            if let (Some(host), Some((site, s))) = (host.as_deref(), support) {
                if s.passkey && !has_passkey_for(&passkey_domains, host, site) {
                    record_finding(Finding::PasskeyAvailable { site: site.to_string() }, &mut r.passkey_available);
                }
            }
        }
        if let Some(u) = url {
            if let Some(url_host) = url_insecure_url_host(u) {
                record_finding(Finding::InsecureUrl { url_host }, &mut r.insecure_urls);
            }
        }

        // Expiry checks: credit cards have 'expiry', API credentials have 'expires'
        for s in rec.view_sections() {
            for fl in s.fields {
                if fl.removed { continue; }
                let id_lower = fl.id.to_lowercase();
                let label_lower = fl.label.to_lowercase();
                let is_expiry = id_lower == "expiry" || id_lower == "expires"
                    || id_lower.contains("expir") || label_lower.contains("expir");
                if !is_expiry { continue; }
                let Some(val) = fl.value.as_deref().map(str::trim).filter(|v| !v.is_empty()) else { continue; };
                if let Some(exp_ts) = dates_parse_expiry(val) {
                    if o.now_secs > exp_ts {
                        record_finding(Finding::Expired { on: val.to_string() }, &mut r.expired);
                    } else {
                        let days = exp_ts.saturating_sub(o.now_secs) / 86400;
                        if days <= o.expiring_days {
                            record_finding(Finding::ExpiringSoon { on: val.to_string(), days }, &mut r.expiring_soon);
                        }
                    }
                }
            }
        }

        if rec.category == crate::item::Category::SshKey && !rec.archived && rec.deleted_at.is_none() {
            let bits = if let Ok(ssh_key) = crate::ssh::SshKey::from_record(rec) {
                if ssh_key.key.algorithm().is_rsa() {
                    ssh_key.bit_length()
                } else {
                    None
                }
            } else if let Some(pub_line) = rec.find_field(crate::ssh::F_PUBLIC).and_then(|f| f.value) {
                if let Ok(info) = crate::ssh::public_info(&pub_line) {
                    if info.key_type == "ssh-rsa" {
                        info.bit_length
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(bits) = bits {
                if bits < 3072 {
                    record_finding(Finding::WeakSshKey { bits }, &mut r.weak_ssh_keys);
                }
            }
        }

        if let Some(months) = o.unused_months.filter(|m| *m > 0) {
            if rec.category == crate::item::Category::Login && !rec.archived && rec.deleted_at.is_none() {
                let last_activity = o.last_used.get(id).copied()
                    .or_else(|| activity_ts(&rec.item.updated_at))
                    .or(rec.created_at);
                if let Some(t) = last_activity {
                    let days = o.now_secs.saturating_sub(t) / 86400;
                    if days > dates_unused_threshold_days(months) {
                        record_finding(Finding::Unused { days }, &mut r.unused);
                    }
                }
            }
        }

        let change_pw_url = url.and_then(url_change_password_url);
        if !f.is_empty() { r.per_item.push(ItemFindings { vault: vault.clone(), id: id.clone(), title: it.title.clone(), change_password_url: change_pw_url.clone(), findings: f }); }
        if !f_ignored.is_empty() { r.ignored_per_item.push(ItemFindings { vault: vault.clone(), id: id.clone(), title: it.title.clone(), change_password_url: change_pw_url, findings: f_ignored }); }
    }
    Ok(r)
}

/// Convenience wrapper when caller has `Item` instead of `ItemRecord`.
pub fn audit_items(items: &[(String, String, Item)], o: &AuditOpts, hibp: Option<&dyn PwnedRangeClient>) -> Result<AuditReport> {
    let recs: Vec<(String, String, ItemRecord)> = items.iter().map(|(v, id, it)| (v.clone(), id.clone(), ItemRecord::new(it.clone()))).collect();
    audit(&recs, o, hibp)
}

/// Convenience wrapper when caller has `Item` instead of `ItemRecord`.
pub fn audit_items_with_passkeys(items: &[(String, String, Item)], passkey_sites: &BTreeSet<String>, o: &AuditOpts, hibp: Option<&dyn PwnedRangeClient>) -> Result<AuditReport> {
    let recs: Vec<(String, String, ItemRecord)> = items.iter().map(|(v, id, it)| (v.clone(), id.clone(), ItemRecord::new(it.clone()))).collect();
    audit_with_passkeys(&recs, passkey_sites, o, hibp)
}

/// Convenience wrapper when caller has `Item` instead of `ItemRecord`.
pub fn audit_items_with_sites(items: &[(String, String, Item)], passkey_sites: &BTreeSet<String>, sites: &[(String, SiteSupport)], o: &AuditOpts, hibp: Option<&dyn PwnedRangeClient>) -> Result<AuditReport> {
    let recs: Vec<(String, String, ItemRecord)> = items.iter().map(|(v, id, it)| (v.clone(), id.clone(), ItemRecord::new(it.clone()))).collect();
    audit_with_sites(&recs, passkey_sites, sites, o, hibp)
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Finding::Weak { reason } => write!(f, "weak password: {reason}"),
            Finding::Reused { group, others } => write!(f, "reused password (group #{group}, shared with {others} other item(s))"),
            Finding::ReusedOldPassword { other_title, retired_on, same_item } => if *same_item {
                write!(f, "reuses its own retired password (retired {retired_on})")
            } else {
                write!(f, "reuses a retired password previously used by {other_title} (retired {retired_on})")
            },
            Finding::Old { days } => write!(f, "password not changed in {days} days"),
            Finding::MissingTotp => write!(f, "has a URL but no TOTP configured"),
            Finding::TotpAvailable { site } => write!(f, "site supports TOTP 2FA ({site}) but this login has none"),
            Finding::PasskeyAvailable { site } => write!(f, "site supports passkeys ({site}) but this login has none"),
            Finding::Pwned { count } => write!(f, "password appears in {count} known breaches (HIBP)"),
            Finding::Expired { on } => write!(f, "expired on {on}"),
            Finding::ExpiringSoon { on, days } => write!(f, "expires in {days} days (on {on})"),
            Finding::InsecureUrl { url_host } => write!(f, "saved with unencrypted HTTP URL ({url_host})"),
            Finding::Duplicate { group } => write!(f, "duplicate login for {group}"),
            Finding::WeakSshKey { bits } => write!(f, "weak SSH key: {bits}-bit RSA (recommend 3072+ bits or Ed25519)"),
            Finding::Unused { days } => write!(f, "not used in {days} days"),
            Finding::RotationDue { every_days, days } => write!(f, "due for rotation: password is {days} days old (reminder every {every_days} days)"),
            Finding::RotationSoon { every_days, in_days } => write!(f, "rotate in {in_days} days (reminder every {every_days} days)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Category, Field, FieldKind, Section};

    fn card_item(title: &str, expiry: &str) -> ItemRecord {
        let mut it = Item::default();
        it.title = title.into();
        let mut rec = ItemRecord::new(it);
        rec.category = Category::CreditCard;
        rec.sections = vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![Field {
                id: "expiry".into(),
                label: "expiry date".into(),
                kind: FieldKind::MonthYear,
                value: Some(expiry.into()),
                removed: false,
            }],
        }];
        rec
    }

    fn api_cred_item(title: &str, expires: &str) -> ItemRecord {
        let mut it = Item::default();
        it.title = title.into();
        let mut rec = ItemRecord::new(it);
        rec.category = Category::ApiCredential;
        rec.sections = vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![Field {
                id: "expires".into(),
                label: "expires".into(),
                kind: FieldKind::Date,
                value: Some(expires.into()),
                removed: false,
            }],
        }];
        rec
    }

    #[test]
    fn test_unused_login_threshold_and_fallbacks() {
        use std::collections::BTreeMap;
        // 2027-01-15T00:00:00Z
        let now_secs: u64 = 1_800_000_000;
        let day = 86400u64;
        let login = |title: &str| {
            let mut it = Item::default();
            it.title = title.into();
            let mut rec = ItemRecord::new(it);
            rec.category = Category::Login;
            rec
        };
        let mut rows: Vec<(String, String, ItemRecord)> = vec![];
        let mut push = |id: &str, rec: ItemRecord| rows.push(("v".into(), id.into(), rec));

        // Recently edited (fallback when never used): not flagged.
        let mut recent = login("Recent");
        recent.item.updated_at = (now_secs - 10 * day).to_string();
        push("recent", recent);

        // Never used, no edit time, recently created: not flagged.
        let mut created = login("New");
        created.created_at = Some(now_secs - 10 * day);
        push("created", created);

        // No timestamp at all: old age is unknown, so not flagged.
        push("unknown", login("Unknown"));

        // Old edit time and no use recorded: flagged with the days since that edit.
        let mut old = login("Old");
        old.item.updated_at = (now_secs - 400 * day).to_string();
        push("old", old);

        // ISO 8601 `updated_at` (older items) is parsed too.
        let mut iso = login("Old ISO");
        iso.item.updated_at = crate::device::iso8601((now_secs - 400 * day) as i64);
        push("iso", iso);

        // Used recently even though edited long ago: not flagged.
        let mut used = login("Used");
        used.item.updated_at = (now_secs - 400 * day).to_string();
        push("used", used);

        // Last use is old even though the item was edited recently: flagged.
        let mut stale = login("Stale");
        stale.item.updated_at = (now_secs - 5 * day).to_string();
        push("stale", stale);

        // Exactly at the threshold is kept; one day past it is flagged.
        let mut boundary = login("Boundary");
        boundary.item.updated_at = (now_secs - 365 * day).to_string();
        push("boundary", boundary);
        let mut beyond = login("Beyond");
        beyond.item.updated_at = (now_secs - 366 * day).to_string();
        push("beyond", beyond);

        // Archived, deleted and non-login items are never flagged.
        let mut archived = login("Archived");
        archived.item.updated_at = (now_secs - 400 * day).to_string();
        archived.archived = true;
        push("archived", archived);
        let mut deleted = login("Deleted");
        deleted.item.updated_at = (now_secs - 400 * day).to_string();
        deleted.deleted_at = Some(now_secs - 400 * day);
        push("deleted", deleted);
        let mut card = card_item("Old card", "12/30");
        card.item.updated_at = (now_secs - 400 * day).to_string();
        push("card", card);

        let mut last_used = BTreeMap::new();
        last_used.insert("used".to_string(), now_secs - 5 * day);
        last_used.insert("stale".to_string(), now_secs - 400 * day);
        let opts = AuditOpts { now_secs, unused_months: Some(12), last_used, ..Default::default() };
        let report = audit(&rows, &opts, None).unwrap();

        let find = |id: &str| report.per_item.iter().find(|i| i.id == id).map(|i| i.findings.clone()).unwrap_or_default();
        assert!(find("recent").is_empty());
        assert!(find("created").is_empty());
        assert!(find("unknown").is_empty());
        assert_eq!(find("old"), vec![Finding::Unused { days: 400 }]);
        assert_eq!(find("iso"), vec![Finding::Unused { days: 400 }]);
        assert!(find("used").is_empty());
        assert_eq!(find("stale"), vec![Finding::Unused { days: 400 }]);
        assert!(find("boundary").is_empty());
        assert_eq!(find("beyond"), vec![Finding::Unused { days: 366 }]);
        assert!(find("archived").is_empty() && find("deleted").is_empty() && find("card").is_empty());
        assert_eq!(report.unused, 4);

        // 24 months is looser, 6 months is stricter.
        let looser = AuditOpts { now_secs, unused_months: Some(24), ..Default::default() };
        assert_eq!(audit(&rows, &looser, None).unwrap().unused, 0);
        let stricter = AuditOpts { now_secs, unused_months: Some(6), ..Default::default() };
        // 6 months = 182 days also catches the item exactly 365 days old, plus the four 12-month ones.
        assert_eq!(audit(&rows, &stricter, None).unwrap().unused, 5);
        assert_eq!(dates_unused_threshold_days(6), 182);
        assert_eq!(dates_unused_threshold_days(24), 730);

        // Off (None or 0 months) disables the check.
        for off in [None, Some(0)] {
            let opts = AuditOpts { now_secs, unused_months: off, ..Default::default() };
            let r = audit(&rows, &opts, None).unwrap();
            assert_eq!(r.unused, 0);
            assert!(!r.per_item.iter().flat_map(|i| &i.findings).any(|f| matches!(f, Finding::Unused { .. })));
        }
    }

    #[test]
    fn test_expiry_audit_cases() {
        // 2026-05-15 12:00:00 UTC
        let now_secs: u64 = 1778846400;
        let opts = AuditOpts { now_secs, expiring_days: 30, ..Default::default() };

        let items = vec![
            // 1. Expired card (past month: 04/26 ended 2026-04-30 23:59:59)
            ("v".into(), "card_exp".into(), card_item("Expired Card", "04/26")),
            // 2. Card expiring this month (05/26 ends 2026-05-31 23:59:59, 16 days left)
            ("v".into(), "card_soon".into(), card_item("Card Expiring Soon", "05/26")),
            // 3. Card expiring in > 30 days (07/26 ends 2026-07-31 23:59:59)
            ("v".into(), "card_far".into(), card_item("Card Far Future", "07/26")),
            // 4. API credential expires in the past (2026-05-10 23:59:59)
            ("v".into(), "api_exp".into(), api_cred_item("Expired API", "2026-05-10")),
            // 5. API credential expires within the window (2026-05-25 23:59:59, 10 days left)
            ("v".into(), "api_soon".into(), api_cred_item("API Expiring Soon", "2026-05-25")),
            // 6. Unparsable value (no finding, no panic)
            ("v".into(), "card_bad".into(), card_item("Garbage Card", "invalid-expiry")),
            ("v".into(), "api_bad".into(), api_cred_item("Garbage API", "not-a-date")),
        ];

        let report = audit(&items, &opts, None).unwrap();
        assert_eq!(report.expired, 2);
        assert_eq!(report.expiring_soon, 2);

        let find = |id: &str| report.per_item.iter().find(|i| i.id == id).map(|i| i.findings.clone()).unwrap_or_default();

        assert_eq!(find("card_exp"), vec![Finding::Expired { on: "04/26".into() }]);
        assert_eq!(find("card_soon"), vec![Finding::ExpiringSoon { on: "05/26".into(), days: 16 }]);
        assert!(find("card_far").is_empty());
        assert_eq!(find("api_exp"), vec![Finding::Expired { on: "2026-05-10".into() }]);
        assert_eq!(find("api_soon"), vec![Finding::ExpiringSoon { on: "2026-05-25".into(), days: 10 }]);
        assert!(find("card_bad").is_empty());
        assert!(find("api_bad").is_empty());
    }

    #[test]
    fn finding_kinds_lists_every_variant() {
        let all = [
            Finding::Weak { reason: String::new() }, Finding::Reused { group: 1, others: 1 }, Finding::ReusedOldPassword { other_title: String::new(), retired_on: String::new(), same_item: false }, Finding::Old { days: 1 },
            Finding::MissingTotp, Finding::TotpAvailable { site: String::new() }, Finding::PasskeyAvailable { site: String::new() },
            Finding::Pwned { count: 1 }, Finding::Expired { on: String::new() }, Finding::ExpiringSoon { on: String::new(), days: 1 },
            Finding::InsecureUrl { url_host: String::new() }, Finding::Duplicate { group: String::new() }, Finding::WeakSshKey { bits: 2048 },
            Finding::Unused { days: 400 }, Finding::RotationDue { every_days: 90, days: 100 }, Finding::RotationSoon { every_days: 90, in_days: 5 },
        ];
        let kinds: Vec<&str> = all.iter().map(Finding::kind).collect();
        assert_eq!(kinds, FINDING_KINDS);
    }

    #[test]
    fn test_audit_includes_change_password_url_per_item() {
        let mut it = Item::default();
        it.title = "Forum".into();
        it.url = Some("http://forum.example.com/login?token=secret#frag".into());
        it.password = Some("SuperSecurePassword123!".into());
        let mut rec = ItemRecord::new(it);
        rec.ignore_finding("missing_totp");
        let items = vec![("v".into(), "forum".into(), rec)];

        let report = audit(&items, &AuditOpts::default(), None).unwrap();
        // insecure_url is active; missing_totp is ignored. Both carry the same derived URL.
        let active = report.per_item.iter().find(|i| i.id == "forum").unwrap();
        assert_eq!(
            active.change_password_url.as_deref(),
            Some("https://forum.example.com/.well-known/change-password")
        );
        assert!(!active.change_password_url.as_deref().unwrap().contains("secret"));
        let ignored = report.ignored_per_item.iter().find(|i| i.id == "forum").unwrap();
        assert_eq!(ignored.change_password_url, active.change_password_url);

        // No URL -> no change-password URL in JSON.
        let mut no_url = Item::default();
        no_url.title = "No URL".into();
        no_url.password = Some("123456".into());
        let report = audit(&[("v".into(), "none".into(), ItemRecord::new(no_url))], &AuditOpts::default(), None).unwrap();
        assert_eq!(report.per_item.iter().find(|i| i.id == "none").unwrap().change_password_url, None);
    }

    #[test]
    fn test_audit_insecure_urls() {
        let mut login1 = Item::default();
        login1.title = "Public Insecure Login".into();
        login1.url = Some("http://user:secret@insecure.example.com:8080/login?token=sensitive".into());
        login1.password = Some("SuperSecurePassword123!".into());

        let mut login2 = Item::default();
        login2.title = "Secure HTTPS Login".into();
        login2.url = Some("https://secure.example.com/login".into());
        login2.password = Some("SuperSecurePassword123!".into());

        let mut login3 = Item::default();
        login3.title = "Local Router".into();
        login3.url = Some("http://192.168.1.1/admin".into());
        login3.password = Some("SuperSecurePassword123!".into());

        let mut login4 = Item::default();
        login4.title = "Localhost Dev".into();
        login4.url = Some("http://localhost:3000".into());
        login4.password = Some("SuperSecurePassword123!".into());

        let mut login5 = Item::default();
        login5.title = "Local NAS".into();
        login5.url = Some("http://nas.local:8080".into());
        login5.password = Some("SuperSecurePassword123!".into());

        let mut login6 = Item::default();
        login6.title = "Private 10.x".into();
        login6.url = Some("http://10.1.2.3/".into());
        login6.password = Some("SuperSecurePassword123!".into());

        let items = vec![
            ("v".into(), "i1".into(), ItemRecord::new(login1)),
            ("v".into(), "i2".into(), ItemRecord::new(login2)),
            ("v".into(), "i3".into(), ItemRecord::new(login3)),
            ("v".into(), "i4".into(), ItemRecord::new(login4)),
            ("v".into(), "i5".into(), ItemRecord::new(login5)),
            ("v".into(), "i6".into(), ItemRecord::new(login6)),
        ];

        let opts = AuditOpts::default();
        let report = audit(&items, &opts, None).unwrap();
        assert_eq!(report.insecure_urls, 1);

        let f1 = report.per_item.iter().find(|i| i.id == "i1").unwrap();
        let insecure_finding = f1.findings.iter().find(|f| matches!(f, Finding::InsecureUrl { .. })).unwrap();
        assert_eq!(insecure_finding, &Finding::InsecureUrl { url_host: "insecure.example.com".into() });

        // Ensure display format contains no secrets (no username, password, port, query)
        let disp = format!("{insecure_finding}");
        assert_eq!(disp, "saved with unencrypted HTTP URL (insecure.example.com)");
        assert!(!disp.contains("secret"));
        assert!(!disp.contains("token"));
        assert!(!disp.contains("8080"));

        for safe_id in ["i2", "i3", "i4", "i5", "i6"] {
            if let Some(entry) = report.per_item.iter().find(|i| i.id == safe_id) {
                assert!(!entry.findings.iter().any(|f| matches!(f, Finding::InsecureUrl { .. })));
            }
        }
    }

    #[test]
    fn test_ignored_findings_audit() {
        // Password is long enough and high entropy, but shared -> reused, not weak
        let shared_pw = "Correct-Horse-Battery-Staple-2026!";
        let mut item1 = Item::default();
        item1.title = "Item 1".into();
        item1.password = Some(shared_pw.into()); // reused
        item1.url = Some("http://example.com".into()); // insecure url, missing totp

        let mut item2 = Item::default();
        item2.title = "Item 2".into();
        item2.password = Some(shared_pw.into()); // reused

        let mut item3 = Item::default();
        item3.title = "Item 3 (weak)".into();
        item3.password = Some("123456".into()); // weak password

        let mut rec1 = ItemRecord::new(item1);
        rec1.ignore_finding("reused");
        rec1.ignore_finding("insecure_url");

        let rec2 = ItemRecord::new(item2);

        let mut rec3 = ItemRecord::new(item3);
        rec3.ignore_finding("weak");

        let items = vec![
            ("v".into(), "id1".into(), rec1),
            ("v".into(), "id2".into(), rec2),
            ("v".into(), "id3".into(), rec3),
        ];

        let opts = AuditOpts::default();
        let report = audit(&items, &opts, None).unwrap();

        // item1 had: reused (ignored), insecure_url (ignored), missing_totp (active)
        // item2 had: reused (active)
        // item3 had: weak (ignored)
        // Total ignored: 3 (reused on id1, insecure_url on id1, weak on id3)
        assert_eq!(report.weak, 0);
        assert_eq!(report.insecure_urls, 0);
        assert_eq!(report.ignored, 3);
        assert_eq!(report.reused, 1); // Only item2 has active reused finding

        let active1 = report.per_item.iter().find(|i| i.id == "id1").unwrap();
        assert!(!active1.findings.iter().any(|f| f.kind() == "reused"));
        assert!(!active1.findings.iter().any(|f| f.kind() == "insecure_url"));
        assert!(active1.findings.iter().any(|f| f.kind() == "missing_totp"));

        let ignored1 = report.ignored_per_item.iter().find(|i| i.id == "id1").unwrap();
        assert_eq!(ignored1.findings.len(), 2);
        assert!(ignored1.findings.iter().any(|f| f.kind() == "reused"));
        assert!(ignored1.findings.iter().any(|f| f.kind() == "insecure_url"));

        let ignored3 = report.ignored_per_item.iter().find(|i| i.id == "id3").unwrap();
        assert_eq!(ignored3.findings.len(), 1);
        assert!(ignored3.findings.iter().any(|f| f.kind() == "weak"));
    }

    #[test]
    fn test_ignored_findings_roundtrip_and_backward_compatibility() {
        // 1. Backward compatibility: older JSON without ignored_findings
        let legacy_json = r#"{"title":"Old Item","updated_at":"1600000000"}"#;
        let rec: ItemRecord = serde_json::from_str(legacy_json).unwrap();
        assert!(rec.ignored_findings.is_empty());
        assert!(!rec.is_finding_ignored("weak"));

        // Reserializing empty ignored_findings does not add "ignored_findings" key
        let reserialized = serde_json::to_string(&rec).unwrap();
        assert!(!reserialized.contains("ignored_findings"));

        // 2. Round-trip with ignored findings
        let mut rec_with_ignored = rec.clone();
        rec_with_ignored.ignore_finding("reused");
        rec_with_ignored.ignore_finding("insecure_url");
        assert!(rec_with_ignored.is_finding_ignored("reused"));
        assert!(rec_with_ignored.is_finding_ignored("insecure_url"));
        assert!(!rec_with_ignored.is_finding_ignored("weak"));

        let json = serde_json::to_string(&rec_with_ignored).unwrap();
        assert!(json.contains("ignored_findings"));

        let parsed: ItemRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.ignored_findings, vec!["reused", "insecure_url"]);

        // 3. Unignore
        let mut rec_unignore = parsed;
        assert!(rec_unignore.unignore_finding("reused"));
        assert!(!rec_unignore.is_finding_ignored("reused"));
        assert!(rec_unignore.is_finding_ignored("insecure_url"));
        assert_eq!(rec_unignore.ignored_findings, vec!["insecure_url"]);
    }

    fn login_rec(title: &str, username: Option<&str>, url: Option<&str>, pw: Option<&str>) -> ItemRecord {
        let mut it = Item::default();
        it.title = title.into();
        it.username = username.map(|u| u.into());
        it.url = url.map(|u| u.into());
        it.password = pw.map(|p| p.into());
        let mut rec = ItemRecord::new(it);
        rec.category = Category::Login;
        rec
    }

    #[test]
    fn test_duplicate_logins_detection() {
        let strong_pw = "Kj9#mQ2$vL5!wP8*zR";
        let mut rec_archived = login_rec("Archived Site", Some("archived_user"), Some("https://archive-test.com/"), Some(strong_pw));
        rec_archived.archived = true;

        let mut rec_deleted = login_rec("Deleted Site", Some("deleted_user"), Some("https://delete-test.com/"), Some(strong_pw));
        rec_deleted.deleted_at = Some(1778846400);

        let items = vec![
            // 1. Same host+username in different case / scheme / www. / port / path are grouped:
            ("v".into(), "dup1".into(), login_rec("Alpha 1", Some("Alice"), Some("https://www.EXAMPLE.com:8443/login?redirect=1"), Some(strong_pw))),
            ("v".into(), "dup2".into(), login_rec("Alpha 2", Some(" alice "), Some("http://example.com/dashboard"), Some(strong_pw))),
            // 2. Different username -> not grouped
            ("v".into(), "diff_user".into(), login_rec("Alpha Bob", Some("bob"), Some("https://example.com/"), Some(strong_pw))),
            // 3. Different host -> not grouped
            ("v".into(), "diff_host".into(), login_rec("Other Site", Some("alice"), Some("https://other.com/"), Some(strong_pw))),
            // 4. Empty username -> not grouped
            ("v".into(), "empty_user1".into(), login_rec("Empty 1", Some("   "), Some("https://empty-test.com/"), Some(strong_pw))),
            ("v".into(), "empty_user2".into(), login_rec("Empty 2", Some(""), Some("https://empty-test.com/"), Some(strong_pw))),
            ("v".into(), "none_user".into(), login_rec("None User", None, Some("https://empty-test.com/"), Some(strong_pw))),
            // 5. Archived items are excluded (and do not cause single active item to be flagged)
            ("v".into(), "archive_active".into(), login_rec("Archive Active", Some("archived_user"), Some("https://archive-test.com/"), Some(strong_pw))),
            ("v".into(), "archive_inactive".into(), rec_archived),
            // 6. Deleted items are excluded
            ("v".into(), "delete_active".into(), login_rec("Delete Active", Some("deleted_user"), Some("https://delete-test.com/"), Some(strong_pw))),
            ("v".into(), "delete_inactive".into(), rec_deleted),
            // 7. Same host+user in different vault scopes are not grouped
            ("vault_other".into(), "other_vault_dup".into(), login_rec("Alpha in other vault", Some("alice"), Some("https://example.com/"), Some(strong_pw))),
        ];

        let opts = AuditOpts::default();
        let report = audit(&items, &opts, None).unwrap();
        assert_eq!(report.duplicates, 2);

        let find = |id: &str| report.per_item.iter().find(|i| i.id == id).map(|i| i.findings.clone()).unwrap_or_default();

        let dup1_f = find("dup1");
        let expected_finding = Finding::Duplicate { group: "example.com/alice".into() };
        assert!(dup1_f.contains(&expected_finding));

        let dup2_f = find("dup2");
        assert!(dup2_f.contains(&expected_finding));

        // Finding text contains no password, path, or query
        let text = expected_finding.to_string();
        assert_eq!(text, "duplicate login for example.com/alice");
        assert!(!text.contains(strong_pw));
        assert!(!text.contains("/login"));
        assert!(!text.contains("redirect=1"));
        assert!(!text.contains("8443"));

        // Others are NOT flagged as duplicate
        for id in ["diff_user", "diff_host", "empty_user1", "empty_user2", "none_user", "archive_active", "archive_inactive", "delete_active", "delete_inactive", "other_vault_dup"] {
            assert!(find(id).iter().all(|f| !matches!(f, Finding::Duplicate { .. })), "{id} should not have Duplicate finding");
        }
    }

    #[test]
    fn test_audit_flags_personal_info_weakness() {
        let mut it_user = Item::default();
        it_user.title = "User Password".into();
        it_user.username = Some("alice".into());
        it_user.url = Some("https://gitlab.com".into());
        it_user.password = Some("github-alice-2024!".into());

        let mut it_site = Item::default();
        it_site.title = "Site Password".into();
        it_site.username = Some("charlie".into());
        it_site.url = Some("https://login.github.com".into());
        it_site.password = Some("G1thub#99-Secure!".into());

        let mut it_ok = Item::default();
        it_ok.title = "OK Item".into();
        it_ok.username = Some("bob".into());
        it_ok.url = Some("https://example.com".into());
        it_ok.password = Some("Xq7#vR2!mK9@pL4$wZ".into());

        let items = vec![
            ("v".into(), "user_item".into(), ItemRecord::new(it_user)),
            ("v".into(), "site_item".into(), ItemRecord::new(it_site)),
            ("v".into(), "ok_item".into(), ItemRecord::new(it_ok)),
        ];

        let report = audit(&items, &AuditOpts::default(), None).unwrap();
        assert_eq!(report.weak, 2);

        let find = |id: &str| report.per_item.iter().find(|i| i.id == id).map(|i| i.findings.clone()).unwrap_or_default();
        let u_findings = find("user_item");
        assert!(u_findings.iter().any(|f| matches!(f, Finding::Weak { reason } if reason == "contains the username")));

        let s_findings = find("site_item");
        assert!(s_findings.iter().any(|f| matches!(f, Finding::Weak { reason } if reason == "contains the site name")));

        let ok_findings = find("ok_item");
        assert!(!ok_findings.iter().any(|f| matches!(f, Finding::Weak { .. })));
    }

    #[test]
    fn test_password_changed_at_derivation() {
        use crate::item::{HistoryEntry, Snapshot};

        // 1. New item without history: uses created_at
        let mut it = Item::default();
        it.title = "Login".into();
        it.password = Some("secretA".into());
        it.updated_at = "1000".into();
        let mut rec = ItemRecord::new(it);
        rec.created_at = Some(1000);
        assert_eq!(rec.password_changed_at(), Some(1000));

        // 2. Edit note at t=2000 (password unchanged): still returns 1000
        rec.history.push(HistoryEntry {
            version: 1,
            updated_at: "1000".into(),
            snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
        });
        rec.item.notes = Some("updated note".into());
        rec.item.updated_at = "2000".into();
        assert_eq!(rec.password_changed_at(), Some(1000));

        // 3. Edit password to secretB at t=3000: returns 3000
        rec.history.push(HistoryEntry {
            version: 2,
            updated_at: "2000".into(),
            snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
        });
        rec.item.password = Some("secretB".into());
        rec.item.updated_at = "3000".into();
        assert_eq!(rec.password_changed_at(), Some(3000));

        // 4. Edit tags at t=4000 (password secretB unchanged): still returns 3000
        rec.history.push(HistoryEntry {
            version: 3,
            updated_at: "3000".into(),
            snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
        });
        rec.tags = vec!["tag1".into()];
        rec.item.updated_at = "4000".into();
        assert_eq!(rec.password_changed_at(), Some(3000));

        // 5. Item without password: returns None
        rec.item.password = None;
        assert_eq!(rec.password_changed_at(), None);
    }

    #[test]
    fn test_rotation_reminder_due_soon_not_due_and_unset() {
        let now_secs = 2_000_000_000u64;
        let day = 86400u64;
        let mk = |changed_days_ago: u64, every: Option<u32>| {
            let mut it = Item::default();
            it.title = "Bank".into();
            it.password = Some("FAKE-Rotation-Pass-123!".into());
            let t = now_secs - changed_days_ago * day;
            it.updated_at = t.to_string();
            let mut rec = ItemRecord::new(it);
            rec.created_at = Some(t);
            rec.rotate_every_days = every;
            rec
        };
        let opts = AuditOpts { now_secs, max_age_days: 3650, ..Default::default() };
        let run = |rec: ItemRecord| audit(&[("v".into(), "i".into(), rec)], &opts, None).unwrap();

        // due: 95 days old with a 90-day reminder, and exactly on the deadline
        let r = run(mk(95, Some(90)));
        assert_eq!((r.rotation_due, r.rotation_soon), (1, 0));
        assert!(r.per_item[0].findings.contains(&Finding::RotationDue { every_days: 90, days: 95 }));
        assert_eq!(run(mk(90, Some(90))).rotation_due, 1);

        // soon: 80 days old => 10 days left; the 14-day window edge is included, 15 days is not
        let r = run(mk(80, Some(90)));
        assert_eq!((r.rotation_due, r.rotation_soon), (0, 1));
        assert!(r.per_item[0].findings.contains(&Finding::RotationSoon { every_days: 90, in_days: 10 }));
        assert_eq!(run(mk(76, Some(90))).rotation_soon, 1);
        assert_eq!(run(mk(75, Some(90))).rotation_soon, 0);

        // not due, and unset (the default): no rotation findings
        let r = run(mk(10, Some(365)));
        assert_eq!((r.rotation_due, r.rotation_soon), (0, 0));
        let r = run(mk(500, None));
        assert_eq!((r.rotation_due, r.rotation_soon), (0, 0));

        // archived logins are skipped; ignoring the finding moves it to the ignored list
        let mut archived = mk(95, Some(90));
        archived.archived = true;
        assert_eq!(run(archived).rotation_due, 0);
        let mut ignored = mk(95, Some(90));
        ignored.ignore_finding("rotation_due");
        let r = run(ignored);
        assert_eq!((r.rotation_due, r.ignored), (0, 1));

        // measured from the password change, not the last edit: a note edit today keeps it due
        let mut edited = mk(95, Some(90));
        edited.history.push(crate::item::HistoryEntry {
            version: 1,
            updated_at: edited.item.updated_at.clone(),
            snapshot: crate::item::Snapshot { item: edited.item.clone(), category: edited.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
        });
        edited.item.notes = Some("edited today".into());
        edited.item.updated_at = now_secs.to_string();
        assert_eq!(run(edited).rotation_due, 1);
    }

    #[test]
    fn test_rotation_field_roundtrip_and_backward_compat() {
        let legacy: ItemRecord = serde_json::from_str(r#"{"title":"Old Item","updated_at":"1600000000"}"#).unwrap();
        assert_eq!(legacy.rotate_every_days, None);
        assert!(!serde_json::to_string(&legacy).unwrap().contains("rotate_every_days"));
        let mut rec = legacy.clone();
        rec.rotate_every_days = Some(90);
        let back: ItemRecord = serde_json::from_str(&serde_json::to_string(&rec).unwrap()).unwrap();
        assert_eq!(back.rotate_every_days, Some(90));
    }

    #[test]
    fn test_old_password_audit_uses_password_change_time() {
        use crate::item::{HistoryEntry, Snapshot};

        let now_secs = 2_000_000_000u64;
        let day = 86400u64;
        let created = now_secs - 400 * day; // 400 days old

        let mut it = Item::default();
        it.title = "Old Login".into();
        it.password = Some("SecureLongPassword123!".into());
        it.updated_at = created.to_string();
        let mut rec = ItemRecord::new(it);
        rec.created_at = Some(created);

        let opts = AuditOpts { now_secs, max_age_days: 365, ..Default::default() };

        // Fresh audit: 400 days > 365 => flagged as Old
        let report = audit(&[("v".into(), "item1".into(), rec.clone())], &opts, None).unwrap();
        assert_eq!(report.old, 1);
        let findings = &report.per_item[0].findings;
        assert!(findings.iter().any(|f| matches!(f, Finding::Old { days } if *days >= 400)));

        // Simulate editing a note today (now_secs - day). Password did not change.
        rec.history.push(HistoryEntry {
            version: 1,
            updated_at: created.to_string(),
            snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
        });
        rec.item.notes = Some("new note added today".into());
        rec.item.updated_at = (now_secs - day).to_string();

        // Crucial test: editing notes does NOT reset password age!
        let report2 = audit(&[("v".into(), "item1".into(), rec.clone())], &opts, None).unwrap();
        assert_eq!(report2.old, 1, "editing notes must not reset old password age");

        // Now change password to a new password 10 days ago
        rec.history.push(HistoryEntry {
            version: 2,
            updated_at: rec.item.updated_at.clone(),
            snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
        });
        rec.item.password = Some("BrandNewPassword123!".into());
        rec.item.updated_at = (now_secs - 10 * day).to_string();

        // Changing password resets age: 10 days < 365 => no Old finding
        let report3 = audit(&[("v".into(), "item1".into(), rec)], &opts, None).unwrap();
        assert_eq!(report3.old, 0, "changing password must reset old password age");
    }

    #[test]
    fn test_truncated_history_does_not_report_false_old() {
        use crate::item::{HistoryEntry, HISTORY_LIMIT, Snapshot};

        let now_secs = 2_000_000_000u64;
        let day = 86400u64;
        let created = now_secs - 400 * day; // item created 400 days ago

        let mut it = Item::default();
        it.title = "Busy Login".into();
        it.password = Some("OriginalPassword123!".into());
        it.updated_at = created.to_string();
        let mut rec = ItemRecord::new(it);
        rec.created_at = Some(created);
        let mut version = 1u64;

        // Push the current content to history (like Session::prepare_update), capped at HISTORY_LIMIT.
        let push = |rec: &mut ItemRecord, version: &mut u64| {
            rec.history.push(HistoryEntry {
                version: *version,
                updated_at: rec.item.updated_at.clone(),
                snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: rec.sections.clone(), tags: rec.tags.clone(), related_item_ids: vec![] },
            });
            let n = rec.history.len();
            if n > HISTORY_LIMIT { rec.history.drain(..n - HISTORY_LIMIT); }
            *version += 1;
        };

        // Password changed 100 days ago.
        push(&mut rec, &mut version);
        rec.item.password = Some("ChangedPassword456!".into());
        rec.item.updated_at = (now_secs - 100 * day).to_string();

        // Followed by more than HISTORY_LIMIT metadata-only edits (notes), one per day.
        for k in 0..(HISTORY_LIMIT as u64 + 5) {
            push(&mut rec, &mut version);
            rec.item.notes = Some(format!("note {k}"));
            rec.item.updated_at = (now_secs - (90 - k) * day).to_string();
        }

        // The password-change entry was drained: every retained entry has the current password.
        assert_eq!(rec.history.len(), HISTORY_LIMIT);
        assert!(rec.history.first().unwrap().version > 1);
        assert!(rec.history.iter().all(|h| h.snapshot.item.password.as_deref() == Some("ChangedPassword456!")));

        // Truncated history: fall back to the oldest retained entry, not created_at.
        let oldest: u64 = rec.history.first().unwrap().updated_at.parse().unwrap();
        assert_eq!(rec.password_changed_at(), Some(oldest));
        assert!(oldest > created);

        let opts = AuditOpts { now_secs, max_age_days: 365, ..Default::default() };
        let report = audit(&[("v".into(), "item1".into(), rec.clone())], &opts, None).unwrap();
        assert_eq!(report.old, 0, "truncated history must not produce a false Old finding");
        assert!(!report.per_item.iter().flat_map(|i| &i.findings).any(|f| matches!(f, Finding::Old { .. })));

        // Complete history (never truncated, starts at version 1): created_at fallback still applies.
        let mut fresh = rec.clone();
        fresh.history.clear();
        fresh.item.password = Some("ChangedPassword456!".into());
        fresh.history.push(HistoryEntry {
            version: 1,
            updated_at: created.to_string(),
            snapshot: Snapshot { item: fresh.item.clone(), category: fresh.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
        });
        assert_eq!(fresh.password_changed_at(), Some(created));
    }

    #[test]
    fn test_watchtower_server_passwords_and_bank_account_exclusion() {
        let now_secs = 1700000000;
        let day = 86400;

        // 1. Server item with weak password
        let mut srv_weak = ItemRecord::new(Item {
            title: "Weak Srv".into(),
            username: Some("root".into()),
            password: None,
            url: Some("https://srv1.example.invalid".into()),
            notes: None,
            totp: None,
            updated_at: now_secs.to_string(),
        });
        srv_weak.category = Category::Server;
        srv_weak.sections = vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![
                Field { id: "password".into(), label: "password".into(), kind: FieldKind::Concealed, value: Some("123456".into()), removed: false },
            ],
        }];

        // 2. Server item sharing password with Login item
        let shared_pw = "SharedComplexPass123!@#";
        let mut srv_reused = ItemRecord::new(Item {
            title: "Reused Srv".into(),
            username: Some("admin".into()),
            password: None,
            url: Some("https://srv2.example.invalid".into()),
            notes: None,
            totp: None,
            updated_at: now_secs.to_string(),
        });
        srv_reused.category = Category::Server;
        srv_reused.sections = vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![
                Field { id: "password".into(), label: "password".into(), kind: FieldKind::Concealed, value: Some(shared_pw.into()), removed: false },
            ],
        }];

        let login_reused = ItemRecord::new(Item {
            title: "Login Site".into(),
            username: Some("user".into()),
            password: Some(shared_pw.into()),
            url: Some("https://login.example.invalid".into()),
            notes: None,
            totp: Some("JBSWY3DPEHPK3PXP".into()),
            updated_at: now_secs.to_string(),
        });

        // 3. Server item with old password
        let mut srv_old = ItemRecord::new(Item {
            title: "Old Srv".into(),
            username: Some("ubuntu".into()),
            password: None,
            url: Some("https://srv3.example.invalid".into()),
            notes: None,
            totp: None,
            updated_at: (now_secs - 400 * day).to_string(),
        });
        srv_old.category = Category::Server;
        srv_old.created_at = Some(now_secs - 400 * day);
        srv_old.sections = vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![
                Field { id: "password".into(), label: "password".into(), kind: FieldKind::Concealed, value: Some("UniqueOldPass987$%^".into()), removed: false },
            ],
        }];

        // 4. Bank account with account number and PIN (even if PIN is "1234" which would be weak if checked as a password)
        let mut bank_item = ItemRecord::new(Item {
            title: "Bank Account".into(),
            username: None,
            password: None,
            url: None,
            notes: None,
            totp: None,
            updated_at: (now_secs - 400 * day).to_string(),
        });
        bank_item.category = Category::BankAccount;
        bank_item.created_at = Some(now_secs - 400 * day);
        bank_item.sections = vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![
                Field { id: "bank_name".into(), label: "bank name".into(), kind: FieldKind::Text, value: Some("Chase".into()), removed: false },
                Field { id: "account_number".into(), label: "account number".into(), kind: FieldKind::Concealed, value: Some("1234567890".into()), removed: false },
                Field { id: "pin".into(), label: "PIN".into(), kind: FieldKind::Concealed, value: Some("1234".into()), removed: false },
            ],
        }];

        let items = vec![
            ("v".into(), "srv_weak".into(), srv_weak),
            ("v".into(), "srv_reused".into(), srv_reused),
            ("v".into(), "login_reused".into(), login_reused),
            ("v".into(), "srv_old".into(), srv_old),
            ("v".into(), "bank_item".into(), bank_item),
        ];

        let opts = AuditOpts { now_secs, max_age_days: 365, ..Default::default() };
        let report = audit(&items, &opts, None).unwrap();

        // Server weak password detected
        assert_eq!(report.weak, 1);
        let weak_item = report.per_item.iter().find(|i| i.id == "srv_weak").unwrap();
        assert!(weak_item.findings.iter().any(|f| matches!(f, Finding::Weak { .. })));

        // Server and Login password reuse detected
        assert_eq!(report.reused, 2);
        let srv_reused_f = report.per_item.iter().find(|i| i.id == "srv_reused").unwrap();
        assert!(srv_reused_f.findings.iter().any(|f| matches!(f, Finding::Reused { .. })));
        let login_reused_f = report.per_item.iter().find(|i| i.id == "login_reused").unwrap();
        assert!(login_reused_f.findings.iter().any(|f| matches!(f, Finding::Reused { .. })));

        // Server old password detected
        assert_eq!(report.old, 1);
        let srv_old_f = report.per_item.iter().find(|i| i.id == "srv_old").unwrap();
        assert!(srv_old_f.findings.iter().any(|f| matches!(f, Finding::Old { .. })));

        // Bank account has NO findings: PIN and account number are never treated as passwords
        assert!(!report.per_item.iter().any(|i| i.id == "bank_item"), "Bank account number and PIN must never produce findings");
    }

    #[test]
    fn test_weak_ssh_key_audit_and_ignore() {
        use crate::ssh::SshKey;
        use ssh_key::private::{KeypairData, RsaKeypair};
        use ssh_key::LineEnding;

        // 1. 2048-bit RSA key: weak (< 3072)
        let rsa2048 = RsaKeypair::random(&mut rand::rngs::OsRng, 2048).unwrap();
        let k2048 = SshKey::import_openssh(&ssh_key::PrivateKey::new(KeypairData::Rsa(rsa2048), "test2048").unwrap().to_openssh(LineEnding::LF).unwrap(), None).unwrap();
        let rec2048 = k2048.to_record("RSA 2048 Key").unwrap();

        // 2. 3072-bit RSA key: not weak (>= 3072)
        let rsa3072 = RsaKeypair::random(&mut rand::rngs::OsRng, 3072).unwrap();
        let k3072 = SshKey::import_openssh(&ssh_key::PrivateKey::new(KeypairData::Rsa(rsa3072), "test3072").unwrap().to_openssh(LineEnding::LF).unwrap(), None).unwrap();
        let rec3072 = k3072.to_record("RSA 3072 Key").unwrap();

        // 3. Ed25519 key: not weak
        let ed = SshKey::generate_ed25519("ed@test").unwrap();
        let rec_ed = ed.to_record("Ed25519 Key").unwrap();

        let items = vec![
            ("v".into(), "id1".into(), rec2048.clone()),
            ("v".into(), "id2".into(), rec3072),
            ("v".into(), "id3".into(), rec_ed),
        ];

        let rep = audit(&items, &AuditOpts::default(), None).unwrap();
        assert_eq!(rep.weak_ssh_keys, 1);
        let id1_f = rep.per_item.iter().find(|i| i.id == "id1").unwrap();
        assert!(id1_f.findings.contains(&Finding::WeakSshKey { bits: 2048 }));
        assert!(!rep.per_item.iter().any(|i| i.id == "id2"));
        assert!(!rep.per_item.iter().any(|i| i.id == "id3"));

        // Ignored weak_ssh_key
        let mut rec_ignored = rec2048;
        rec_ignored.ignore_finding("weak_ssh_key");
        let rep_ign = audit(&[("v".into(), "id1".into(), rec_ignored)], &AuditOpts::default(), None).unwrap();
        assert_eq!(rep_ign.weak_ssh_keys, 0);
        assert_eq!(rep_ign.ignored, 1);
        assert_eq!(rep_ign.ignored_per_item.len(), 1);
        assert!(rep_ign.ignored_per_item[0].findings.contains(&Finding::WeakSshKey { bits: 2048 }));
    }

    #[test]
    fn test_watchtower_own_password_history_not_flagged_as_reused() {
        let mut it = Item::default();
        it.title = "Single Item".into();
        it.username = Some("alice".into());
        it.password = Some("Complex-P@ssword-1234!".into());
        it.url = Some("https://example.com".into());
        it.updated_at = "1700000000".into();
        let mut login = ItemRecord::new(it);
        login.password_history.push(crate::item::PasswordHistoryEntry {
            value: "Complex-P@ssword-1234!".into(),
            replaced_at: "2024-01-01T00:00:00Z".into(),
        });
        login.password_history.push(crate::item::PasswordHistoryEntry {
            value: "Older-Complex-P@ss-5678!".into(),
            replaced_at: "2023-01-01T00:00:00Z".into(),
        });

        let items = vec![("vault1".into(), "item1".into(), login)];
        let report = audit(&items, &AuditOpts::default(), None).unwrap();
        assert_eq!(report.reused, 0);
        assert_eq!(report.reused_groups, 0);
        let item1 = report.per_item.iter().find(|i| i.id == "item1");
        if let Some(i) = item1 {
            assert!(!i.findings.iter().any(|f| matches!(f, Finding::Reused { .. })));
        }
    }

    #[test]
    fn test_reused_old_password_audit() {
        use crate::item::PasswordHistoryEntry;
        let strong = "Kj9#mQ2$vL5!wP8*zR";
        let retired_pw = "Retired-Old-Pass-987!";
        let hist = |value: &str, at: &str| PasswordHistoryEntry { value: value.into(), replaced_at: at.into() };
        // A: current password equals B's retired password (cross-item).
        let a = login_rec("Site A", Some("alice"), None, Some(retired_pw));
        // B: retired the password, now uses a different strong one.
        let mut b = login_rec("Site B", Some("bob"), None, Some(strong));
        b.password_history = vec![hist(retired_pw, "2025-06-01T00:00:00Z"), hist("   ", "2025-01-01T00:00:00Z"), hist("", "2024-01-01T00:00:00Z")];
        // C: current password equals its own retired password (same-item).
        let mut c = login_rec("Site C", Some("carol"), None, Some("Own-Retired-111!"));
        c.password_history = vec![hist("Own-Retired-111!", "2025-03-01T00:00:00Z")];
        // D: clean login, plus an unmatched retired value that must never leak into the report.
        let mut d = login_rec("Site D", Some("dan"), None, Some(strong));
        d.password_history = vec![hist("Never-Leaked-Retired-555!", "2025-02-01T00:00:00Z")];
        let items = vec![
            ("v".into(), "a".into(), a),
            ("v".into(), "b".into(), b),
            ("v".into(), "c".into(), c),
            ("v".into(), "d".into(), d),
        ];
        let report = audit(&items, &AuditOpts::default(), None).unwrap();
        assert_eq!(report.reused_old_password, 2);
        let find = |id: &str| report.per_item.iter().find(|i| i.id == id).map(|i| i.findings.clone()).unwrap_or_default();
        assert_eq!(find("a").iter().filter(|f| matches!(f, Finding::ReusedOldPassword { .. })).count(), 1);
        assert!(find("a").contains(&Finding::ReusedOldPassword { other_title: "Site B".into(), retired_on: "2025-06-01T00:00:00Z".into(), same_item: false }));
        assert!(find("c").contains(&Finding::ReusedOldPassword { other_title: "Site C".into(), retired_on: "2025-03-01T00:00:00Z".into(), same_item: true }));
        assert!(!find("d").iter().any(|f| matches!(f, Finding::ReusedOldPassword { .. })));
        // The finding carries the source title and retirement date, never the retired value.
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains(retired_pw));
        assert!(!json.contains("Own-Retired-111!"));
        assert!(!json.contains("Never-Leaked-Retired-555!"));
        assert!(json.contains("Site B") && json.contains("2025-06-01T00:00:00Z"));
        // Display text is equally value-free.
        let disp = format!("{}", Finding::ReusedOldPassword { other_title: "Site B".into(), retired_on: "2025-06-01T00:00:00Z".into(), same_item: false });
        assert_eq!(disp, "reuses a retired password previously used by Site B (retired 2025-06-01T00:00:00Z)");
        // Ignoring the kind moves it to the ignored list, like the other kinds.
        let mut a_ignored = login_rec("Site A", Some("alice"), None, Some(retired_pw));
        a_ignored.ignore_finding("reused_old_password");
        let mut b2 = login_rec("Site B", Some("bob"), None, Some(strong));
        b2.password_history = vec![hist(retired_pw, "2025-06-01T00:00:00Z")];
        let r2 = audit(&[("v".into(), "a".into(), a_ignored), ("v".into(), "b".into(), b2)], &AuditOpts::default(), None).unwrap();
        assert_eq!((r2.reused_old_password, r2.ignored), (0, 1));
        assert!(r2.ignored_per_item.iter().any(|i| i.findings.iter().any(|f| matches!(f, Finding::ReusedOldPassword { .. }))));
    }
}
