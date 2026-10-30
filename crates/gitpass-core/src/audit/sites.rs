//! Offline 2FA and passkey site capability lookup.

use std::collections::BTreeSet;
use std::sync::OnceLock;
use super::url::host_of;

/// Documented MFA capabilities of a site (offline list).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SiteSupport { pub totp: bool, pub passkey: bool }

/// Bundled offline list of sites with documented TOTP and/or passkey support.
/// Source and licence are noted in the file; no network lookup happens at audit time.
const MFA_SITES_TSV: &str = include_str!("../../data/mfa-sites.tsv");

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

/// True when `host` equals `domain` or is a subdomain of it (`evil-domain.test` does not match).
pub(crate) fn domain_matches(host: &str, domain: &str) -> bool {
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

pub(crate) fn bundled_mfa_sites() -> &'static [(String, SiteSupport)] {
    static SITES: OnceLock<Vec<(String, SiteSupport)>> = OnceLock::new();
    SITES.get_or_init(|| parse_mfa_site_list(MFA_SITES_TSV))
}

/// The compiled-in offline site list (`crates/gitpass-core/data/mfa-sites.tsv`).
pub fn mfa_sites() -> &'static [(String, SiteSupport)] { bundled_mfa_sites() }

/// Look up a URL (or bare host) in the bundled offline list.
pub fn site_support(url: &str) -> Option<(&'static str, SiteSupport)> { lookup_mfa_site(bundled_mfa_sites(), url) }

pub(crate) fn has_passkey_for(passkey_sites: &BTreeSet<String>, host: &str, site: &str) -> bool {
    passkey_sites.iter().any(|rp_id| {
        domain_matches(host, rp_id)
            || domain_matches(rp_id, host)
            || domain_matches(rp_id, site)
            || domain_matches(site, rp_id)
    })
}
