//! Password strength, entropy estimation, and personal info detection for security audits.

use super::url::{decode_punycode, extract_site_name};
use super::AuditOpts;

const COMMON: &[&str] = &[
    "password", "123456", "12345678", "qwerty", "letmein", "welcome", "admin", "iloveyou", "monkey",
    "dragon", "abc123", "111111", "passw0rd", "trustno1", "football", "baseball", "sunshine", "master",
];

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
