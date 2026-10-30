//! Have I Been Pwned (HIBP) k-anonymity client and lookup.

use anyhow::Result;
use sha1::{Digest, Sha1};
use std::collections::HashMap;

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
