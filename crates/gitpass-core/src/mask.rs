//! Secret masking helpers: produces consistent hints for API keys and other secrets.
//!
//! Formats:
//! - 20+ chars:  `abcd••••••••wxyz` (first 4, 8 bullets, last 4)
//! - 12–19 chars: `abcd••••••••`     (first 4, 8 bullets)
//! - 6–11 chars:  `ab••••••••`       (first 2, 8 bullets)
//! - < 6 chars:   `••••••••`         (8 bullets only)
//!
//! Handles UTF-8 characters safely (multibyte characters are counted as single code points).
//! Never reveals most of the secret.

use crate::item::Category;
use crate::merge::ItemRecord;

pub const BULLETS: &str = "••••••••";

/// Computes the masked hint for a secret string.
pub fn secret_hint(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    if n >= 20 {
        let prefix: String = chars[..4].iter().collect();
        let suffix: String = chars[n - 4..].iter().collect();
        format!("{prefix}{BULLETS}{suffix}")
    } else if n >= 12 {
        let prefix: String = chars[..4].iter().collect();
        format!("{prefix}{BULLETS}")
    } else if n >= 6 {
        let prefix: String = chars[..2].iter().collect();
        format!("{prefix}{BULLETS}")
    } else {
        BULLETS.to_string()
    }
}

/// Extracts the secret credential string for an API Credential item if present.
pub fn api_credential_secret(r: &ItemRecord) -> Option<&str> {
    if r.category != Category::ApiCredential {
        return None;
    }
    // Check preferred field IDs first ("credential", "key", "api_key", "secret")
    for id in ["credential", "key", "api_key", "secret"] {
        for s in &r.sections {
            for f in &s.fields {
                if !f.removed && f.kind.is_secret() && f.id.eq_ignore_ascii_case(id) {
                    if let Some(v) = f.value.as_deref().filter(|v| !v.is_empty()) {
                        return Some(v);
                    }
                }
            }
        }
    }
    // Fall back to any non-removed concealed field with a non-empty value
    for s in &r.sections {
        for f in &s.fields {
            if !f.removed && f.kind.is_secret() {
                if let Some(v) = f.value.as_deref().filter(|v| !v.is_empty()) {
                    return Some(v);
                }
            }
        }
    }
    // Fall back to item password if present
    r.item.password.as_deref().filter(|p| !p.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Field, FieldKind, Section};
    use crate::store::Item;

    #[test]
    fn test_secret_hint_boundaries() {
        // Boundary < 6 chars: bullets only
        assert_eq!(secret_hint(""), BULLETS);
        assert_eq!(secret_hint("a"), BULLETS);
        assert_eq!(secret_hint("12345"), BULLETS);

        // Boundary 6..=11 chars: 2 prefix + bullets
        assert_eq!(secret_hint("123456"), format!("12{BULLETS}"));
        assert_eq!(secret_hint("abcdefg"), format!("ab{BULLETS}"));
        assert_eq!(secret_hint("12345678901"), format!("12{BULLETS}"));

        // Boundary 12..=19 chars: 4 prefix + bullets
        assert_eq!(secret_hint("123456789012"), format!("1234{BULLETS}"));
        assert_eq!(secret_hint("abcdefghijklmno"), format!("abcd{BULLETS}"));
        assert_eq!(secret_hint("1234567890123456789"), format!("1234{BULLETS}"));

        // Boundary 20+ chars: 4 prefix + bullets + 4 suffix
        assert_eq!(secret_hint("12345678901234567890"), format!("1234{BULLETS}7890"));
        assert_eq!(secret_hint("sk-live-0123456789abcdefgh"), format!("sk-l{BULLETS}efgh"));
    }

    #[test]
    fn test_secret_hint_multibyte() {
        // Japanese: 5 chars -> bullets only
        assert_eq!(secret_hint("あいうえお"), BULLETS);
        // 6 chars -> 2 chars + bullets
        assert_eq!(secret_hint("あいうえおか"), format!("あい{BULLETS}"));
        // 11 chars
        assert_eq!(secret_hint("あいうえおかきくけこさ"), format!("あい{BULLETS}"));
        // 12 chars -> 4 chars + bullets
        assert_eq!(secret_hint("あいうえおかきくけこさし"), format!("あいうえ{BULLETS}"));
        // 19 chars -> 4 chars + bullets
        assert_eq!(secret_hint("あいうえおかきくけこさしすせそたちつて"), format!("あいうえ{BULLETS}"));
        // 20 chars -> 4 chars + bullets + 4 chars
        assert_eq!(secret_hint("あいうえおかきくけこさしすせそたちつてと"), format!("あいうえ{BULLETS}ちつてと"));

        // Emojis (multibyte code points)
        assert_eq!(secret_hint("🦀🚀🔑"), BULLETS);
        assert_eq!(secret_hint("🦀🚀🔑🔒🛡️🔐"), format!("🦀🚀{BULLETS}"));
        assert_eq!(secret_hint("🦀🚀🔑🔒🛡️🔐💎⭐🎉🔥0123456789"), format!("🦀🚀🔑🔒{BULLETS}6789"));
    }

    #[test]
    fn test_never_shows_most_of_the_key() {
        // For any string of length 0 to 100, the revealed characters must never exceed 50% of total length.
        for len in 0..=100 {
            let s: String = (0..len).map(|i| (b'a' + (i % 26) as u8) as char).collect();
            let hint = secret_hint(&s);
            let unmasked_count = hint.chars().filter(|&c| c != '•').count();
            if len == 0 {
                assert_eq!(unmasked_count, 0);
            } else {
                let fraction = unmasked_count as f64 / len as f64;
                assert!(fraction <= 0.40001, "Length {len} revealed {unmasked_count}/{len} = {fraction} (> 40%!)");
            }
        }
    }

    #[test]
    fn test_api_credential_secret_extraction() {
        let mut it = Item::default();
        it.title = "API Item".into();
        let mut rec = ItemRecord::new(it);
        rec.category = Category::ApiCredential;
        rec.sections = vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![
                Field { id: "username".into(), label: "username".into(), kind: FieldKind::Text, value: Some("alice".into()), removed: false },
                Field { id: "credential".into(), label: "credential".into(), kind: FieldKind::Concealed, value: Some("sk-1234567890abcdefghij".into()), removed: false },
            ],
        }];

        assert_eq!(api_credential_secret(&rec), Some("sk-1234567890abcdefghij"));

        // If not ApiCredential category, returns None
        rec.category = Category::Login;
        assert_eq!(api_credential_secret(&rec), None);

        // If removed, skips
        rec.category = Category::ApiCredential;
        rec.sections[0].fields[1].removed = true;
        assert_eq!(api_credential_secret(&rec), None);
    }
}
