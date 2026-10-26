//! Security audit: weak, reused, old passwords; missing TOTP; optional HIBP Pwned Passwords check.
//! Findings carry titles/ids only — never passwords.
use crate::store::Item;
use anyhow::{bail, Result};
use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Finding {
    Weak { reason: String },
    /// Password shared with `others` other items.
    Reused { group: usize, others: usize },
    Old { days: u64 },
    MissingTotp,
    Pwned { count: u64 },
}

#[derive(Clone, Debug)]
pub struct ItemFindings { pub vault: String, pub id: String, pub title: String, pub findings: Vec<Finding> }

#[derive(Clone, Debug)]
pub struct AuditOpts { pub max_age_days: u64, pub min_length: usize, pub min_entropy_bits: f64, pub now_secs: u64 }

impl Default for AuditOpts {
    fn default() -> Self {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        AuditOpts { max_age_days: 365, min_length: 12, min_entropy_bits: 60.0, now_secs: now }
    }
}

#[derive(Default, Debug)]
pub struct AuditReport {
    pub items_checked: usize,
    pub weak: usize, pub reused: usize, pub reused_groups: usize, pub old: usize, pub missing_totp: usize, pub pwned: usize,
    pub per_item: Vec<ItemFindings>,
}

const COMMON: &[&str] = &["password", "123456", "12345678", "qwerty", "letmein", "welcome", "admin", "iloveyou", "monkey", "dragon", "abc123", "111111", "passw0rd", "trustno1", "football", "baseball", "sunshine", "master"];

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

/// Audit `(vault, id, item)` triples.
pub fn audit(items: &[(String, String, Item)], o: &AuditOpts, hibp: Option<&dyn PwnedRangeClient>) -> Result<AuditReport> {
    if o.min_length == 0 { bail!("min_length must be > 0"); }
    let mut r = AuditReport { items_checked: items.len(), ..Default::default() };
    // reuse groups: keyed by blake3(password); hashes live in memory only and are never printed.
    let mut groups: BTreeMap<[u8; 32], Vec<usize>> = BTreeMap::new();
    for (i, (_, _, it)) in items.iter().enumerate() {
        if let Some(p) = it.password.as_deref().filter(|p| !p.is_empty()) { groups.entry(*blake3::hash(p.as_bytes()).as_bytes()).or_default().push(i); }
    }
    let mut reuse: HashMap<usize, (usize, usize)> = HashMap::new();
    for (g, idxs) in groups.values().filter(|v| v.len() > 1).enumerate() {
        r.reused_groups += 1;
        for &i in idxs { reuse.insert(i, (g + 1, idxs.len() - 1)); }
    }
    let mut cache = HashMap::new();
    for (i, (vault, id, it)) in items.iter().enumerate() {
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
        if it.url.as_deref().is_some_and(|u| !u.trim().is_empty()) && it.totp.as_deref().is_none_or(|t| t.trim().is_empty()) {
            f.push(Finding::MissingTotp); r.missing_totp += 1;
        }
        if !f.is_empty() { r.per_item.push(ItemFindings { vault: vault.clone(), id: id.clone(), title: it.title.clone(), findings: f }); }
    }
    Ok(r)
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Finding::Weak { reason } => write!(f, "weak password: {reason}"),
            Finding::Reused { group, others } => write!(f, "reused password (group #{group}, shared with {others} other item(s))"),
            Finding::Old { days } => write!(f, "password not changed in {days} days"),
            Finding::MissingTotp => write!(f, "has a URL but no TOTP configured"),
            Finding::Pwned { count } => write!(f, "password appears in {count} known breaches (HIBP)"),
        }
    }
}
