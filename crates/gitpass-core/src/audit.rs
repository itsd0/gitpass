//! Security audit: weak, reused, old passwords; missing TOTP; offline 2FA/passkey availability;
//! optional HIBP Pwned Passwords check. Findings carry titles/ids only — never passwords.
use crate::merge::ItemRecord;
use crate::store::Item;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finding {
    Weak { reason: String },
    /// Password shared with `others` other items.
    Reused { group: usize, others: usize },
    Old { days: u64 },
    MissingTotp,
    /// `site` (from the bundled offline list) documents TOTP, but the item has none.
    TotpAvailable { site: String },
    /// `site` documents passkeys, but the item (and the vault) has none for it.
    PasskeyAvailable { site: String },
    Pwned { count: u64 },
    Expired { on: String },
    ExpiringSoon { on: String, days: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemFindings { pub vault: String, pub id: String, pub title: String, pub findings: Vec<Finding> }

#[derive(Clone, Debug)]
pub struct AuditOpts { pub max_age_days: u64, pub min_length: usize, pub min_entropy_bits: f64, pub now_secs: u64, pub expiring_days: u64 }

impl Default for AuditOpts {
    fn default() -> Self {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        AuditOpts { max_age_days: 365, min_length: 12, min_entropy_bits: 60.0, now_secs: now, expiring_days: 30 }
    }
}

#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditReport {
    pub items_checked: usize,
    pub weak: usize, pub reused: usize, pub reused_groups: usize, pub old: usize, pub missing_totp: usize, pub pwned: usize,
    pub totp_available: usize, pub passkey_available: usize,
    pub expired: usize, pub expiring_soon: usize,
    pub per_item: Vec<ItemFindings>,
}

const COMMON: &[&str] = &["password", "123456", "12345678", "qwerty", "letmein", "welcome", "admin", "iloveyou", "monkey", "dragon", "abc123", "111111", "passw0rd", "trustno1", "football", "baseball", "sunshine", "master"];

/// Documented MFA capabilities of a site (offline list).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SiteSupport { pub totp: bool, pub passkey: bool }

/// Bundled offline list of sites with documented TOTP and/or passkey support.
/// Source and licence are noted in the file; no network lookup happens at audit time.
const MFA_SITES_TSV: &str = include_str!("../data/mfa-sites.tsv");

/// Parse the bundled TSV format (`domain<TAB>totp,passkey`). Comments (`#`) and blank lines are
/// ignored, unknown capability words are ignored, and duplicate domains are merged. Exposed so
/// tests and future user-supplied lists can reuse the matcher.
pub fn parse_mfa_site_list(src: &str) -> Vec<(String, SiteSupport)> {
    let mut out: Vec<(String, SiteSupport)> = vec![];
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let Some((domain, caps)) = line.split_once('\t') else { continue };
        let domain = domain.trim().trim_matches('.').to_ascii_lowercase();
        let domain = domain.strip_prefix("www.").unwrap_or(&domain).to_string();
        let mut s = SiteSupport::default();
        for cap in caps.split(',') {
            match cap.trim() { "totp" => s.totp = true, "passkey" => s.passkey = true, _ => {} }
        }
        if !domain.is_empty() && (s.totp || s.passkey) { out.push((domain, s)); }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    let mut merged: Vec<(String, SiteSupport)> = Vec::with_capacity(out.len());
    for (domain, s) in out {
        let dup = merged.last().is_some_and(|(d, _)| *d == domain);
        if dup {
            let last = merged.last_mut().expect("checked above");
            last.1.totp |= s.totp;
            last.1.passkey |= s.passkey;
        } else {
            merged.push((domain, s));
        }
    }
    merged
}

/// Lowercased host of a URL (or bare host): scheme, userinfo, port, path and a leading `www.` are
/// dropped. Returns `None` for empty input or IPv6 literals, which the list does not carry.
fn host_of(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() { return None; }
    let rest = s.split_once("://").map_or(s, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("");
    let host = host.trim_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.starts_with('[') { return None; }
    Some(host.strip_prefix("www.").unwrap_or(&host).to_string())
}

/// True when `host` equals `domain` or is a subdomain of it (`evil-domain.test` does not match).
fn domain_matches(host: &str, domain: &str) -> bool {
    host == domain || host.strip_suffix(domain).is_some_and(|prefix| prefix.ends_with('.'))
}

/// Look up a URL (or bare host) in an offline site list. Returns the matched domain and its
/// capabilities. The most-specific (longest domain) match wins.
pub fn lookup_mfa_site<'a>(sites: &'a [(String, SiteSupport)], url: &str) -> Option<(&'a str, SiteSupport)> {
    let host = host_of(url)?;
    sites
        .iter()
        .filter(|(domain, _)| domain_matches(&host, domain))
        .max_by_key(|(domain, _)| (domain.len(), domain.as_str()))
        .map(|(domain, s)| (domain.as_str(), *s))
}

fn bundled_mfa_sites() -> &'static [(String, SiteSupport)] {
    static SITES: OnceLock<Vec<(String, SiteSupport)>> = OnceLock::new();
    SITES.get_or_init(|| parse_mfa_site_list(MFA_SITES_TSV))
}

/// The compiled-in offline site list (`crates/gitpass-core/data/mfa-sites.tsv`).
pub fn mfa_sites() -> &'static [(String, SiteSupport)] { bundled_mfa_sites() }

/// Look up a URL (or bare host) in the bundled offline list.
pub fn site_support(url: &str) -> Option<(&'static str, SiteSupport)> { lookup_mfa_site(bundled_mfa_sites(), url) }

fn has_passkey_for(passkey_sites: &BTreeSet<String>, host: &str, site: &str) -> bool {
    passkey_sites.iter().any(|rp_id| {
        domain_matches(host, rp_id)
            || domain_matches(rp_id, host)
            || domain_matches(rp_id, site)
            || domain_matches(site, rp_id)
    })
}

/// Naive entropy estimate: length * log2(pool size of the character classes used).
pub fn entropy_bits(pw: &str) -> f64 {
    let (mut lo, mut up, mut dg, mut sy, mut other) = (false, false, false, false, false);
    for c in pw.chars() {
        if c.is_ascii_lowercase() { lo = true } else if c.is_ascii_uppercase() { up = true }
        else if c.is_ascii_digit() { dg = true } else if c.is_ascii() { sy = true } else { other = true }
    }
    let pool = (lo as u32) * 26 + (up as u32) * 26 + (dg as u32) * 10 + (sy as u32) * 33 + (other as u32) * 100;
    if pool == 0 { return 0.0; }
    pw.chars().count() as f64 * (pool as f64).log2()
}

/// Returns a reason if the password is weak.
pub fn weakness(pw: &str, o: &AuditOpts) -> Option<String> {
    let lower = pw.to_lowercase();
    if COMMON.iter().any(|c| lower.contains(c)) { return Some("contains a very common password".into()); }
    let distinct = { let mut v: Vec<char> = pw.chars().collect(); v.sort(); v.dedup(); v.len() };
    if pw.chars().count() >= 4 && distinct <= 2 { return Some("too few distinct characters".into()); }
    let n = pw.chars().count();
    if n < o.min_length { return Some(format!("too short ({n} < {})", o.min_length)); }
    let e = entropy_bits(pw);
    if e < o.min_entropy_bits { return Some(format!("low estimated entropy (~{e:.0} bits)")); }
    None
}

/// Pluggable fetcher for `GET https://api.pwnedpasswords.com/range/{prefix}` (with `Add-Padding: true`).
/// Returns the response body. Tests inject a mock; the CLI injects a real HTTP client.
pub trait PwnedRangeClient { fn range(&self, prefix5: &str) -> Result<String>; }

pub const HIBP_RANGE_URL: &str = "https://api.pwnedpasswords.com/range/";

/// k-anonymity lookup: only the first 5 hex chars of SHA-1 leave the process. Padding rows (count 0) are ignored.
pub fn pwned_count(client: &dyn PwnedRangeClient, pw: &str, cache: &mut HashMap<String, String>) -> Result<u64> {
    let h: String = Sha1::digest(pw.as_bytes()).iter().map(|b| format!("{b:02X}")).collect();
    let (prefix, suffix) = h.split_at(5);
    if !cache.contains_key(prefix) { cache.insert(prefix.to_string(), client.range(prefix)?); }
    for line in cache[prefix].lines() {
        let Some((s, c)) = line.trim().split_once(':') else { continue };
        if s.eq_ignore_ascii_case(suffix) {
            let c: u64 = c.trim().parse().unwrap_or(0);
            return Ok(c);
        }
    }
    Ok(0)
}

/// Parse month_year (MM/YY, MM/YYYY, YYYY/MM, YYYY-MM) or date (YYYY-MM-DD, unix timestamp) to
/// unix seconds. For month_year, expiry is 23:59:59 UTC on the last day of that month.
/// For date without time, expiry is 23:59:59 UTC on that day.
/// Returns None on unparsable / out-of-range input instead of panicking.
pub fn parse_expiry(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if s.is_empty() { return None; }
    // 1. Check if it's all digits (unix epoch seconds)
    if s.len() >= 9 && s.len() <= 12 && s.chars().all(|c| c.is_ascii_digit()) {
        return s.parse::<u64>().ok();
    }
    // 2. Check month_year: MM/YY, MM/YYYY, MM-YY, MM-YYYY, YYYY/MM, YYYY-MM
    let parts: Vec<&str> = s.split(['/', '-']).map(str::trim).collect();
    if parts.len() == 2 {
        let (p1, p2) = (parts[0], parts[1]);
        let (y, m) = if let (Ok(m), Ok(mut y)) = (p1.parse::<i64>(), p2.parse::<i64>()) {
            if (1..=12).contains(&m) {
                if p2.len() == 2 && (0..100).contains(&y) { y += 2000; }
                (y, m)
            } else { (-1, -1) }
        } else if let (Ok(y), Ok(m)) = (p1.parse::<i64>(), p2.parse::<i64>()) {
            if p1.len() == 4 && (1..=12).contains(&m) { (y, m) } else { (-1, -1) }
        } else { (-1, -1) };

        if (1970..=9999).contains(&y) && (1..=12).contains(&m) {
            let (next_y, next_m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            let next_start = crate::import::iso_to_epoch(&format!("{next_y:04}-{next_m:02}-01"))?;
            return next_start.checked_sub(1);
        }
    }
    // 3. Check date: YYYY-MM-DD, YYYY/MM/DD, or ISO 8601 with time
    let norm = s.replace('/', "-");
    if norm.len() == 10 {
        let day_start = crate::import::iso_to_epoch(&norm)?;
        return Some(day_start + 86399);
    }
    if norm.len() >= 19 {
        return crate::import::iso_to_epoch(&norm);
    }
    None
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
    let passkey_domains: BTreeSet<String> = passkey_sites.iter().filter_map(|s| host_of(s)).collect();
    let mut r = AuditReport { items_checked: items.len(), ..Default::default() };
    // reuse groups: keyed by blake3(password); hashes live in memory only and are never printed.
    let mut groups: BTreeMap<[u8; 32], Vec<usize>> = BTreeMap::new();
    for (i, (_, _, rec)) in items.iter().enumerate() {
        if let Some(p) = rec.item.password.as_deref().filter(|p| !p.is_empty()) { groups.entry(*blake3::hash(p.as_bytes()).as_bytes()).or_default().push(i); }
    }
    let mut reuse: HashMap<usize, (usize, usize)> = HashMap::new();
    for (g, idxs) in groups.values().filter(|v| v.len() > 1).enumerate() {
        r.reused_groups += 1;
        for &i in idxs { reuse.insert(i, (g + 1, idxs.len() - 1)); }
    }
    let mut cache = HashMap::new();
    for (i, (vault, id, rec)) in items.iter().enumerate() {
        let it = &rec.item;
        let mut f = vec![];
        if let Some(p) = it.password.as_deref().filter(|p| !p.is_empty()) {
            if let Some(reason) = weakness(p, o) { f.push(Finding::Weak { reason }); r.weak += 1; }
            if let Some(&(group, others)) = reuse.get(&i) { f.push(Finding::Reused { group, others }); r.reused += 1; }
            if let Some(c) = hibp { let n = pwned_count(c, p, &mut cache)?; if n > 0 { f.push(Finding::Pwned { count: n }); r.pwned += 1; } }
            if let Ok(t) = it.updated_at.trim().parse::<u64>() {
                let days = o.now_secs.saturating_sub(t) / 86400;
                if days > o.max_age_days { f.push(Finding::Old { days }); r.old += 1; }
            }
        }
        let url = it.url.as_deref().filter(|u| !u.trim().is_empty());
        let no_totp = it.totp.as_deref().is_none_or(|t| t.trim().is_empty());
        let host = url.and_then(host_of);
        let support = host.as_deref().and_then(|h| lookup_mfa_site(sites, h));
        if url.is_some() && no_totp {
            let mut advised_totp = false;
            if let Some((site, s)) = support {
                if s.totp {
                    f.push(Finding::TotpAvailable { site: site.to_string() });
                    r.totp_available += 1;
                    advised_totp = true;
                }
            }
            if !advised_totp {
                f.push(Finding::MissingTotp);
                r.missing_totp += 1;
            }
        }
        if let (Some(host), Some((site, s))) = (host.as_deref(), support) {
            if s.passkey && !has_passkey_for(&passkey_domains, host, site) {
                f.push(Finding::PasskeyAvailable { site: site.to_string() }); r.passkey_available += 1;
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
                if let Some(exp_ts) = parse_expiry(val) {
                    if o.now_secs > exp_ts {
                        f.push(Finding::Expired { on: val.to_string() });
                        r.expired += 1;
                    } else {
                        let days = exp_ts.saturating_sub(o.now_secs) / 86400;
                        if days <= o.expiring_days {
                            f.push(Finding::ExpiringSoon { on: val.to_string(), days });
                            r.expiring_soon += 1;
                        }
                    }
                }
            }
        }

        if !f.is_empty() { r.per_item.push(ItemFindings { vault: vault.clone(), id: id.clone(), title: it.title.clone(), findings: f }); }
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
            Finding::Old { days } => write!(f, "password not changed in {days} days"),
            Finding::MissingTotp => write!(f, "has a URL but no TOTP configured"),
            Finding::TotpAvailable { site } => write!(f, "site supports TOTP 2FA ({site}) but this login has none"),
            Finding::PasskeyAvailable { site } => write!(f, "site supports passkeys ({site}) but this login has none"),
            Finding::Pwned { count } => write!(f, "password appears in {count} known breaches (HIBP)"),
            Finding::Expired { on } => write!(f, "expired on {on}"),
            Finding::ExpiringSoon { on, days } => write!(f, "expires in {days} days (on {on})"),
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
}
