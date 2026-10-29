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
    /// Saved with an unencrypted HTTP URL (public host only; no secrets/paths/queries).
    InsecureUrl { url_host: String },
    Duplicate { group: String },
    WeakSshKey { bits: usize },
}

/// Every [`Finding::kind`] string, in declaration order (docs and `gitpass audit --help` list these).
pub const FINDING_KINDS: &[&str] = &["weak", "reused", "old", "missing_totp", "totp_available", "passkey_available", "pwned", "expired", "expiring_soon", "insecure_url", "duplicate", "weak_ssh_key"];

impl Finding {
    pub fn kind(&self) -> &'static str {
        match self {
            Finding::Weak { .. } => "weak",
            Finding::Reused { .. } => "reused",
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
pub struct AuditOpts { pub max_age_days: u64, pub min_length: usize, pub min_entropy_bits: f64, pub now_secs: u64, pub expiring_days: u64, pub include_ignored: bool }

impl Default for AuditOpts {
    fn default() -> Self {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        AuditOpts { max_age_days: 365, min_length: 12, min_entropy_bits: 60.0, now_secs: now, expiring_days: 30, include_ignored: false }
    }
}

#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditReport {
    pub items_checked: usize,
    pub weak: usize, pub reused: usize, pub reused_groups: usize, pub duplicates: usize, pub old: usize, pub missing_totp: usize, pub pwned: usize,
    pub totp_available: usize, pub passkey_available: usize,
    pub expired: usize, pub expiring_soon: usize,
    pub insecure_urls: usize,
    pub weak_ssh_keys: usize,
    pub ignored: usize,
    pub per_item: Vec<ItemFindings>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_per_item: Vec<ItemFindings>,
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
pub(crate) fn host_of(input: &str) -> Option<String> {
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

/// Extracts the host if `url` uses plain unencrypted HTTP and points to a public host.
/// Returns None for HTTPS, non-HTTP schemes, localhost, 127.0.0.0/8, ::1, *.local, or RFC1918 private IP ranges.
/// Strips userinfo/password, port, path, query, and fragment so no secrets can leak into findings.
pub fn insecure_url_host(url: &str) -> Option<String> {
    let s = url.trim();
    if s.is_empty() { return None; }
    let rest = if s.len() >= 7 && s[..7].eq_ignore_ascii_case("http://") {
        &s[7..]
    } else {
        return None;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    let host = if host_port.starts_with('[') {
        let end = host_port.find(']')?;
        &host_port[1..end]
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    let host = host.trim_matches('.').to_ascii_lowercase();
    if host.is_empty() { return None; }

    if host == "localhost" || host.ends_with(".localhost") || host == "local" || host.ends_with(".local") {
        return None;
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(v4) => {
                if v4.is_loopback() { return None; }
                let octets = v4.octets();
                let is_rfc1918 = octets[0] == 10
                    || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                    || (octets[0] == 192 && octets[1] == 168);
                if is_rfc1918 { return None; }
            }
            std::net::IpAddr::V6(v6) => {
                if v6.is_loopback() { return None; }
            }
        }
    }

    Some(host)
}

/// The site's standard change-password page (W3C "well-known change password URL" / RFC 8615):
/// `https://<host>/.well-known/change-password`.
///
/// Only http(s) URLs with a plain DNS host are accepted. The subdomain and an explicit port are
/// kept; path, query, fragment and any userinfo are dropped; `http://` is upgraded to https; a
/// trailing dot on the host is removed. Returns `None` for empty or non-http(s) input, IP literals
/// (v4/v6), `localhost`/`*.localhost`/`local`/`*.local`, and malformed hosts.
pub fn change_password_url(item_url: &str) -> Option<String> {
    let s = item_url.trim();
    let rest = if s.get(..8).is_some_and(|p| p.eq_ignore_ascii_case("https://")) {
        &s[8..]
    } else if s.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("http://")) {
        &s[7..]
    } else {
        return None;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    if host_port.starts_with('[') { return None; } // IPv6 literal: no standard change-password page
    let (host, port) = host_port.split_once(':').map_or((host_port, None), |(h, p)| (h, Some(p)));
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() { return None; }
    // Browsers also accept shorthand IPv4 (`127.1`, `2130706433`): an all-numeric host is an IP.
    if host.split('.').all(|label| label.bytes().all(|b| b.is_ascii_digit())) { return None; }
    if host == "localhost" || host.ends_with(".localhost") || host == "local" || host.ends_with(".local") { return None; }
    if !valid_dns_name(&host) { return None; }
    let port = match port {
        Some(p) => {
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) { return None; }
            let n: u32 = p.parse().ok()?;
            if !(1..=65535).contains(&n) { return None; }
            format!(":{n}")
        }
        None => String::new(),
    };
    Some(format!("https://{host}{port}/.well-known/change-password"))
}

/// Dot-separated non-empty DNS labels (ASCII letters/digits, `-`, `_`; no leading/trailing `-`).
fn valid_dns_name(host: &str) -> bool {
    host.split('.').all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
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


/// Normalize a string for personal-info substring matching:
/// lowercases, strips common separators, and applies leetspeak substitutions:
/// 0->o, 1->i, l->i, 3->e, 4/@->a, 5/$->s.
fn normalize_for_match(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        for lc in c.to_lowercase() {
            match lc {
                '0' => out.push('o'),
                '1' | 'l' => out.push('i'),
                '3' => out.push('e'),
                '4' | '@' => out.push('a'),
                '5' | '$' => out.push('s'),
                c if c.is_whitespace() || (c.is_ascii_punctuation() && c != '@' && c != '$') || c == '—' || c == '–' || c == '·' => {
                    // separator: ignore
                }
                other => out.push(other),
            }
        }
    }
    out
}

fn matches_personal_info(pw: &str, target: &str) -> bool {
    let t_clean = target.trim();
    if t_clean.chars().count() < 4 {
        return false;
    }
    let norm_target = normalize_for_match(t_clean);
    if norm_target.chars().count() < 4 {
        return false;
    }
    let norm_pw = normalize_for_match(pw);
    norm_pw.contains(&norm_target)
}

/// Extracts the registrable site name from a URL or host.
/// E.g. "https://login.github.com" -> Some("github")
///      "https://github.com" -> Some("github")
///      "sub.example.co.uk" -> Some("example")
///      "amazon.co.jp" -> Some("amazon")
///      "münchen.de" -> Some("münchen")
///      "xn--mnchen-3ya.de" -> Some("xn--mnchen-3ya")
/// Returns None if no host is found or if the host is an IP address.
pub fn extract_site_name(url_or_host: &str) -> Option<String> {
    let host = host_of(url_or_host)?;
    if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    let labels: Vec<&str> = host.split('.').filter(|s| !s.is_empty()).collect();
    if labels.is_empty() {
        return None;
    }
    if labels.len() == 1 {
        return Some(labels[0].to_string());
    }
    if labels.len() >= 3 {
        let last = labels[labels.len() - 1];
        let second_last = labels[labels.len() - 2];
        const CC_SLDS: &[&str] = &[
            "co", "com", "org", "net", "edu", "gov", "ac", "ne", "or", "gen", "nom", "ind",
        ];
        const MULTI_PSL: &[&str] = &[
            "github.io", "gitlab.io", "pages.dev", "vercel.app", "herokuapp.com", "azurewebsites.net", "cloudfront.net",
        ];
        let is_cc_sld = last.len() == 2 && last.chars().all(|c| c.is_ascii_alphabetic()) && CC_SLDS.contains(&second_last);
        let pair = format!("{second_last}.{last}");
        if is_cc_sld || MULTI_PSL.contains(&pair.as_str()) {
            return Some(labels[labels.len() - 3].to_string());
        }
    }
    Some(labels[labels.len() - 2].to_string())
}

/// Pure RFC 3492 Punycode decoder for IDN labels without external dependencies.
/// Returns decoded Unicode string if `encoded` is valid punycode, or None.
pub fn decode_punycode(encoded: &str) -> Option<String> {
    let input = encoded.strip_prefix("xn--").unwrap_or(encoded);
    const BASE: u32 = 36;
    const TMIN: u32 = 1;
    const TMAX: u32 = 26;
    const SKEW: u32 = 38;
    const DAMP: u32 = 700;
    const INITIAL_BIAS: u32 = 72;
    const INITIAL_N: u32 = 128;

    fn adapt(mut delta: u32, numpoints: u32, firsttime: bool) -> u32 {
        delta = if firsttime { delta / DAMP } else { delta / 2 };
        delta += delta / numpoints;
        let mut k = 0;
        while delta > ((BASE - TMIN) * TMAX) / 2 {
            delta /= BASE - TMIN;
            k += BASE;
        }
        k + (((BASE - TMIN + 1) * delta) / (delta + SKEW))
    }

    let mut n = INITIAL_N;
    let mut i = 0u32;
    let mut bias = INITIAL_BIAS;
    let mut output = Vec::new();

    let delim = input.rfind('-');
    let (basic, mut pos) = if let Some(d) = delim {
        (&input[..d], d + 1)
    } else {
        ("", 0)
    };
    for c in basic.chars() {
        output.push(c);
    }

    let chars: Vec<char> = input.chars().collect();
    while pos < chars.len() {
        let oldi = i;
        let mut w = 1u32;
        let mut k = BASE;
        loop {
            if pos >= chars.len() { return None; }
            let c = chars[pos];
            pos += 1;
            let digit = match c {
                'a'..='z' => (c as u32) - ('a' as u32),
                'A'..='Z' => (c as u32) - ('A' as u32),
                '0'..='9' => (c as u32) - ('0' as u32) + 26,
                _ => return None,
            };
            i = i.checked_add(digit.checked_mul(w)?)?;
            let t = if k <= bias {
                TMIN
            } else if k >= bias + TMAX {
                TMAX
            } else {
                k - bias
            };
            if digit < t { break; }
            w = w.checked_mul(BASE - t)?;
            k += BASE;
        }
        let out_len = (output.len() + 1) as u32;
        bias = adapt(i - oldi, out_len, oldi == 0);
        n = n.checked_add(i / out_len)?;
        i %= out_len;
        let ch = char::from_u32(n)?;
        output.insert(i as usize, ch);
        i += 1;
    }

    Some(output.into_iter().collect())
}

/// Returns Some("contains the username") or Some("contains the site name") if the password
/// case-insensitively, and ignoring common separators and leetspeak substitutions,
/// contains:
/// - the username or the local part of an email username (when that part is at least 4 characters), or
/// - the registrable site name from the item URL (when at least 4 characters).
pub fn contains_personal_info(pw: &str, username: Option<&str>, url_host: Option<&str>) -> Option<String> {
    let pw = pw.trim();
    if pw.is_empty() { return None; }

    // 1. Check username
    if let Some(u) = username.map(str::trim).filter(|u| !u.is_empty()) {
        let mut candidates = Vec::new();
        if let Some((local, _)) = u.split_once('@') {
            let local = local.trim();
            if local.chars().count() >= 4 {
                candidates.push(local);
            }
        }
        if u.chars().count() >= 4 {
            candidates.push(u);
        }
        for cand in candidates {
            if matches_personal_info(pw, cand) {
                return Some("contains the username".to_string());
            }
        }
    }

    // 2. Check site name
    if let Some(url) = url_host.map(str::trim).filter(|u| !u.is_empty()) {
        if let Some(site) = extract_site_name(url) {
            let mut candidates = Vec::new();
            if site.starts_with("xn--") {
                if let Some(decoded) = decode_punycode(&site) {
                    if decoded.chars().count() >= 4 {
                        candidates.push(decoded);
                    }
                }
                if site.chars().count() >= 4 {
                    candidates.push(site);
                }
            } else if site.chars().count() >= 4 {
                candidates.push(site);
            }
            for cand in candidates {
                if matches_personal_info(pw, &cand) {
                    return Some("contains the site name".to_string());
                }
            }
        }
    }

    None
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
        let Some(site) = rec.item.url.as_deref().and_then(host_of) else {
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
            let weak_reason = contains_personal_info(p, user, url)
                .or_else(|| weakness(p, o));
            if let Some(reason) = weak_reason { record_finding(Finding::Weak { reason }, &mut r.weak); }
            if let Some(&(group, others)) = reuse.get(&i) { record_finding(Finding::Reused { group, others }, &mut r.reused); }
            if let Some(c) = hibp { let n = pwned_count(c, p, &mut cache)?; if n > 0 { record_finding(Finding::Pwned { count: n }, &mut r.pwned); } }
            if let Some(t) = rec.password_changed_at() {
                let days = o.now_secs.saturating_sub(t) / 86400;
                if days > o.max_age_days { record_finding(Finding::Old { days }, &mut r.old); }
            }
        }
        let no_totp = it.totp.as_deref().is_none_or(|t| t.trim().is_empty());
        let host = url.and_then(host_of);
        let support = host.as_deref().and_then(|h| lookup_mfa_site(sites, h));
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
            if let Some(url_host) = insecure_url_host(u) {
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
                if let Some(exp_ts) = parse_expiry(val) {
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

        let change_pw_url = url.and_then(change_password_url);
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

    #[test]
    fn test_insecure_url_host_matching() {
        // Public HTTP hosts are flagged
        assert_eq!(insecure_url_host("http://example.com"), Some("example.com".into()));
        assert_eq!(insecure_url_host("http://public.example.com/login?foo=bar"), Some("public.example.com".into()));
        assert_eq!(insecure_url_host("http://example.com:8080/path"), Some("example.com".into()));
        assert_eq!(insecure_url_host("http://172.32.0.1"), Some("172.32.0.1".into()));
        assert_eq!(insecure_url_host("http://8.8.8.8:80/dns"), Some("8.8.8.8".into()));

        // User credentials in URL: only host is returned, NO secrets leaked
        assert_eq!(insecure_url_host("http://admin:secret_pass123@public.site.test:8080/dashboard?token=xyz#hash"), Some("public.site.test".into()));

        // HTTPS is not flagged
        assert_eq!(insecure_url_host("https://example.com"), None);
        assert_eq!(insecure_url_host("https://public.example.com/path"), None);
        assert_eq!(insecure_url_host("https://localhost:8080"), None);

        // localhost and *.localhost are not flagged
        assert_eq!(insecure_url_host("http://localhost"), None);
        assert_eq!(insecure_url_host("http://localhost:3000/app"), None);
        assert_eq!(insecure_url_host("http://dev.localhost:8080"), None);

        // Loopback 127.0.0.0/8 and ::1 are not flagged
        assert_eq!(insecure_url_host("http://127.0.0.1"), None);
        assert_eq!(insecure_url_host("http://127.0.0.5:8080/test"), None);
        assert_eq!(insecure_url_host("http://127.255.255.255"), None);
        assert_eq!(insecure_url_host("http://[::1]"), None);
        assert_eq!(insecure_url_host("http://[::1]:8080/"), None);

        // *.local and local are not flagged
        assert_eq!(insecure_url_host("http://local"), None);
        assert_eq!(insecure_url_host("http://my-nas.local"), None);
        assert_eq!(insecure_url_host("http://printer.office.local:9100"), None);

        // RFC1918 private IPv4 ranges are not flagged
        assert_eq!(insecure_url_host("http://10.0.0.1"), None);
        assert_eq!(insecure_url_host("http://10.255.1.2:8080"), None);
        assert_eq!(insecure_url_host("http://172.16.0.1"), None);
        assert_eq!(insecure_url_host("http://172.24.10.20:8000"), None);
        assert_eq!(insecure_url_host("http://172.31.255.255"), None);
        assert_eq!(insecure_url_host("http://192.168.0.1"), None);
        assert_eq!(insecure_url_host("http://192.168.1.254:8443"), None);

        // Empty, non-http, or malformed
        assert_eq!(insecure_url_host(""), None);
        assert_eq!(insecure_url_host("ftp://ftp.example.com"), None);
        assert_eq!(insecure_url_host("not-a-url"), None);
    }

    #[test]
    fn finding_kinds_lists_every_variant() {
        let all = [
            Finding::Weak { reason: String::new() }, Finding::Reused { group: 1, others: 1 }, Finding::Old { days: 1 },
            Finding::MissingTotp, Finding::TotpAvailable { site: String::new() }, Finding::PasskeyAvailable { site: String::new() },
            Finding::Pwned { count: 1 }, Finding::Expired { on: String::new() }, Finding::ExpiringSoon { on: String::new(), days: 1 },
            Finding::InsecureUrl { url_host: String::new() }, Finding::Duplicate { group: String::new() }, Finding::WeakSshKey { bits: 2048 },
        ];
        let kinds: Vec<&str> = all.iter().map(Finding::kind).collect();
        assert_eq!(kinds, FINDING_KINDS);
    }

    #[test]
    fn test_change_password_url() {
        let well_known = "/.well-known/change-password";
        // Plain https host, subdomain kept, path/query/fragment dropped.
        assert_eq!(change_password_url("https://example.com"), Some(format!("https://example.com{well_known}")));
        assert_eq!(
            change_password_url("https://app.example.com/login?next=%2F#top"),
            Some(format!("https://app.example.com{well_known}"))
        );
        // Explicit port kept (and normalized).
        assert_eq!(
            change_password_url("https://example.com:8443/login"),
            Some(format!("https://example.com:8443{well_known}"))
        );
        assert_eq!(change_password_url("http://example.com:080"), Some(format!("https://example.com:80{well_known}")));
        // http is upgraded, scheme/host case and a trailing dot are normalized.
        assert_eq!(change_password_url("http://Example.COM"), Some(format!("https://example.com{well_known}")));
        assert_eq!(change_password_url("HTTPS://example.com."), Some(format!("https://example.com{well_known}")));
        // Userinfo never reaches the output.
        assert_eq!(
            change_password_url("https://user:secret@example.com/account"),
            Some(format!("https://example.com{well_known}"))
        );

        // IP literals (v4 and v6), local names, non-http schemes, bare hosts and empties.
        for bad in [
            "", "   ", "example.com", "not-a-url", "file:///etc/passwd", "ftp://example.com",
            "http://127.0.0.1", "http://127.0.0.5:8080", "http://8.8.8.8", "http://127.1", "http://2130706433", "https://[::1]", "https://[2001:db8::1]:8443",
            "http://localhost", "http://localhost:3000", "https://app.localhost", "http://local", "http://nas.local",
            "https://exa mple.com", "https://example.com:0", "https://example.com:", "https://example.com:notaport",
            "https://example.com:70000", "https://-bad.example.com", "https://a..example.com", "https://example.com\\evil",
        ] {
            assert_eq!(change_password_url(bad), None, "expected None for {bad:?}");
        }
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
    fn test_contains_personal_info_username() {
        // Direct match
        assert_eq!(
            contains_personal_info("github-alice-2024!", Some("alice"), None),
            Some("contains the username".into())
        );

        // Leetspeak substitutions: 4->a, 1->i, 3->e
        assert_eq!(
            contains_personal_info("4l1c3-2024!", Some("alice"), None),
            Some("contains the username".into())
        );

        // Leetspeak @->a, 1->i
        assert_eq!(
            contains_personal_info("@l1ce", Some("alice"), None),
            Some("contains the username".into())
        );

        // Separators: a-l-i-c-e, a_l_1_c_e
        assert_eq!(
            contains_personal_info("a-l-i-c-e", Some("alice"), None),
            Some("contains the username".into())
        );
        assert_eq!(
            contains_personal_info("a_l_1_c_e", Some("alice"), None),
            Some("contains the username".into())
        );
        assert_eq!(
            contains_personal_info("al1ce", Some("alice"), None),
            Some("contains the username".into())
        );
        assert_eq!(
            contains_personal_info("a1ice", Some("alice"), None),
            Some("contains the username".into())
        );
        assert_eq!(
            contains_personal_info("a11ce", Some("alice"), None),
            Some("contains the username".into())
        );

        // Leetspeak $->s, 5->s
        assert_eq!(
            contains_personal_info("$m1th", Some("smith"), None),
            Some("contains the username".into())
        );
        assert_eq!(
            contains_personal_info("5m1th", Some("smith"), None),
            Some("contains the username".into())
        );

        // Email username: local part >= 4 chars
        assert_eq!(
            contains_personal_info("alice2024!", Some("alice@example.com"), None),
            Some("contains the username".into())
        );
        assert_eq!(
            contains_personal_info("alicesmith2024!", Some("alice.smith@example.com"), None),
            Some("contains the username".into())
        );
        assert_eq!(
            contains_personal_info("alice-smith-2024!", Some("alice.smith@example.com"), None),
            Some("contains the username".into())
        );
    }

    #[test]
    fn test_contains_personal_info_site_name() {
        // Direct and leetspeak matches on registrable site name
        assert_eq!(
            contains_personal_info("G1thub#99", None, Some("https://login.github.com")),
            Some("contains the site name".into())
        );
        assert_eq!(
            contains_personal_info("github-2024-strong!", None, Some("https://github.com")),
            Some("contains the site name".into())
        );
        assert_eq!(
            contains_personal_info("Example-Pass-2024", None, Some("https://sub.portal.example.co.uk")),
            Some("contains the site name".into())
        );
        assert_eq!(
            contains_personal_info("Amazon2024!", None, Some("https://amazon.co.jp")),
            Some("contains the site name".into())
        );
        assert_eq!(
            contains_personal_info("g00g1e-pass", None, Some("https://google.com")),
            Some("contains the site name".into())
        );
        assert_eq!(
            contains_personal_info("goog1e-pass", None, Some("https://google.com")),
            Some("contains the site name".into())
        );

        // IDN / punycode hosts
        assert_eq!(
            contains_personal_info("München2024!", None, Some("https://login.münchen.de")),
            Some("contains the site name".into())
        );
        assert_eq!(
            contains_personal_info("xn--mnchen-3ya-pass!", None, Some("https://xn--mnchen-3ya.de")),
            Some("contains the site name".into())
        );
        assert_eq!(
            contains_personal_info("München2024!", None, Some("https://xn--mnchen-3ya.de")),
            Some("contains the site name".into())
        );
    }

    #[test]
    fn test_contains_personal_info_near_misses() {
        // Short username < 4 chars: ignored
        assert_eq!(contains_personal_info("bobby123!", Some("bob"), None), None);
        assert_eq!(contains_personal_info("amysecret!", Some("amy"), None), None);
        assert_eq!(contains_personal_info("daniel2024!", Some("dan@example.com"), None), None);

        // Short site name < 4 chars: ignored
        assert_eq!(contains_personal_info("x-factor-2024!", None, Some("https://x.com")), None);
        assert_eq!(contains_personal_info("ticket-master!", None, Some("https://t.co")), None);

        // Unrelated words
        assert_eq!(
            contains_personal_info("correct-horse-battery-staple", Some("alice"), Some("https://github.com")),
            None
        );
        assert_eq!(
            contains_personal_info("Xq7#vR2!mK9@pL4$wZ", Some("alice"), Some("https://github.com")),
            None
        );
        assert_eq!(
            contains_personal_info("gitlab-charlie-2024!", Some("alice"), Some("https://github.com")),
            None
        );

        // Near misses
        assert_eq!(contains_personal_info("alicia-2024!", Some("alice"), None), None);
        assert_eq!(contains_personal_info("alec-2024!", Some("alice"), None), None);
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
            snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![] },
        });
        rec.item.notes = Some("updated note".into());
        rec.item.updated_at = "2000".into();
        assert_eq!(rec.password_changed_at(), Some(1000));

        // 3. Edit password to secretB at t=3000: returns 3000
        rec.history.push(HistoryEntry {
            version: 2,
            updated_at: "2000".into(),
            snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![] },
        });
        rec.item.password = Some("secretB".into());
        rec.item.updated_at = "3000".into();
        assert_eq!(rec.password_changed_at(), Some(3000));

        // 4. Edit tags at t=4000 (password secretB unchanged): still returns 3000
        rec.history.push(HistoryEntry {
            version: 3,
            updated_at: "3000".into(),
            snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![] },
        });
        rec.tags = vec!["tag1".into()];
        rec.item.updated_at = "4000".into();
        assert_eq!(rec.password_changed_at(), Some(3000));

        // 5. Item without password: returns None
        rec.item.password = None;
        assert_eq!(rec.password_changed_at(), None);
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
            snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![] },
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
            snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![] },
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
                snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: rec.sections.clone(), tags: rec.tags.clone() },
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
            snapshot: Snapshot { item: fresh.item.clone(), category: fresh.category, sections: vec![], tags: vec![] },
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
}
