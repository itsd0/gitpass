//! Security audit: weak, reused, old passwords; missing TOTP; offline 2FA/passkey availability;
//! optional HIBP Pwned Passwords check. Findings carry titles/ids only — never passwords.

pub mod dates;
pub mod hibp;
pub mod sites;
pub mod strength;
pub mod url;

mod scoring;

pub use dates::{parse_expiry, unused_threshold_days};
pub use hibp::{pwned_count, PwnedRangeClient, HIBP_RANGE_URL};
pub use sites::{lookup_mfa_site, mfa_sites, parse_mfa_site_list, site_support, SiteSupport};
pub use scoring::{is_score_password_item, score, SCORE_WEIGHT_BAD_PASSWORDS, SCORE_WEIGHT_MISSING_2FA, SCORE_WEIGHT_OLD_PASSWORDS, SCORE_WEIGHT_REUSED_OLD_PASSWORD};
pub use strength::{contains_personal_info, entropy_bits, pattern_estimate, weakness};
pub use url::{change_password_url, decode_punycode, extract_site_name, insecure_url_host};
pub(crate) use url::host_of;

use dates::{activity_ts, parse_expiry as dates_parse_expiry, unused_threshold_days as dates_unused_threshold_days};
use hibp::pwned_count as hibp_pwned_count;
use sites::{bundled_mfa_sites, has_passkey_for, lookup_mfa_site as sites_lookup_mfa_site};
use strength::{contains_personal_info as strength_contains_personal_info, weakness as strength_weakness};
use url::{change_password_url as url_change_password_url, host_of as url_host_of, insecure_url_host as url_insecure_url_host};

use crate::merge::ItemRecord;
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
mod tests;
