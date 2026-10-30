//! Password strength, entropy estimation, and personal info detection for security audits.

use super::url::{decode_punycode, extract_site_name};
use super::AuditOpts;

/// The most common leaked passwords. Matching is by substring (see [`weakness`]), so entries
/// double as fragments: a password containing `qwerty123` is flagged even with affixes.
/// Kept separate from [`DICT_WORDS`]: common passwords are matched first and cost fewer guesses.
const COMMON: &[&str] = &[
    "password", "123456", "12345678", "qwerty", "letmein", "welcome", "admin", "iloveyou", "monkey",
    "dragon", "abc123", "111111", "passw0rd", "trustno1", "football", "baseball", "sunshine", "master",
    "123456789", "1234567", "12345", "1234567890", "000000", "654321", "123123",
    "qwerty123", "qwertyuiop", "asdfghjkl", "zxcvbnm", "1q2w3e4r", "1qaz2wsx",
    "password1", "password123", "admin123", "welcome123", "changeme", "p@ssw0rd", "passw0rd123",
];

/// Common English words (and month names) that show up as the word half of
/// `word + year/digits` passwords (`summer2024`, `soccer123`). Deliberately disjoint from
/// both [`COMMON`] and the generator wordlist in `crate::generate::WORDLIST`, so a generated
/// passphrase is never flagged just for containing one of its own words; a dictionary word
/// only lowers the estimate when it combines with a year, digits, or another pattern token
/// (see [`segment`]). Words are matched after leetspeak normalisation, so `5ummer` counts.
const DICT_WORDS: &[&str] = &[
    "summer", "autumn", "february", "november", "forever", "together", "internet",
    "killer", "batman", "pokemon", "starwars", "charlie", "thomas", "jordan", "michael",
    "jennifer", "jessica", "ashley", "daniel", "andrew", "joshua", "michelle", "lovely",
    "money", "whatever", "hello", "world", "cookie", "chocolate", "soccer", "hockey",
    "tennis", "flower", "golden", "diamond", "pearl", "bear", "shark", "snake", "angel",
    "devil", "demon", "ghost", "pirate", "warrior", "knight", "magic", "lightning",
    "rainbow",
];

/// US QWERTY rows, then columns. A straight walk is a run of adjacent keys on one line
/// in either direction (`asdfgh`, `1qaz`, `0987`). Diagonals are not modelled: they are
/// rare in leaked passwords and the row/column set already covers the common walks.
const KEYBOARD_LINES: &[&str] = &[
    "1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm",
    "1qaz", "2wsx", "3edc", "4rfv", "5tgb", "6yhn", "7ujm", "8ik,", "9ol.", "0p;/",
];

/// Guesses for one pattern token, as log2 values is avoided here: these are raw guess counts
/// used by [`segment`] (converted with `log2` when summed into bits).
/// - A dictionary word: the list size times 2 for a capitalised variant.
/// - A straight keyboard walk: ~200 (starts × directions), independent of its length — the
///   zxcvbn insight that a long walk is barely harder to guess than a short one.
/// - A sequence run: ~100 (alphabet/digit starts × directions).
/// - A year: ~200 (1900-2099).
/// - A common password: ~2000 (a top-list rank allowance).
fn token_guesses(kind: PatternKind) -> f64 {
    match kind {
        PatternKind::Common => 2000.0,
        PatternKind::Word => DICT_WORDS.len() as f64 * 2.0,
        PatternKind::Walk => 200.0,
        PatternKind::Sequence => 100.0,
        PatternKind::Year => 200.0,
        // Never used: a repeated chunk is priced by `repeated_chunk_bits`, not here.
        PatternKind::Repeated => 1.0,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PatternKind {
    Common,
    Word,
    Walk,
    Sequence,
    Year,
    /// The whole password is one short single-class chunk repeated (`abcabcabc`).
    Repeated,
}

impl PatternKind {
    fn label(self) -> &'static str {
        match self {
            PatternKind::Common => "common password",
            PatternKind::Word => "common word",
            PatternKind::Walk => "keyboard pattern",
            PatternKind::Sequence => "sequence pattern",
            PatternKind::Year => "year",
            PatternKind::Repeated => "repeated pattern",
        }
    }
}

/// Characters of a password prepared for pattern segmentation: lowercased, with the
/// word-joining separators (`-`, `_`, `.`, whitespace) removed so `word-year` segments
/// the same as `wordyear`. When `leet` is true the string is [`normalize_for_match`]
/// output instead (separators stripped, leetspeak mapped), which catches `5ummer2024`.
/// Symbols other than separators are kept in the plain form so digit/letter structure
/// (years, sequences, walks) survives; the leet form drops them, which is why
/// [`pattern_estimate`] segments both forms and takes the cheaper estimate.
fn seg_chars(pw: &str, leet: bool) -> Vec<char> {
    if leet {
        return normalize_for_match(pw).chars().collect();
    }
    let mut out = Vec::with_capacity(pw.len());
    for c in pw.chars() {
        for lc in c.to_lowercase() {
            if lc.is_whitespace() || lc == '-' || lc == '_' || lc == '.' {
                continue;
            }
            out.push(lc);
        }
    }
    out
}

/// Length of the longest word from `list` matching `s` at `i` (0 when none matches).
fn longest_word_at(s: &[char], i: usize, list: &[&str]) -> usize {
    let mut best = 0;
    for word in list {
        let w: Vec<char> = word.chars().collect();
        if w.len() > best && i + w.len() <= s.len() && s[i..i + w.len()] == w[..] {
            best = w.len();
        }
    }
    best
}

/// Length of the longest straight keyboard walk starting at `i` (0 when shorter than 4
/// would be the only walk; callers treat < 4 as "no walk").
fn walk_len_at(s: &[char], i: usize) -> usize {
    let mut best = 0;
    for line in KEYBOARD_LINES {
        let lc: Vec<char> = line.chars().collect();
        for (p, &c) in lc.iter().enumerate() {
            if c != s[i] {
                continue;
            }
            for dir in [1isize, -1] {
                let mut len = 1;
                let mut pos = p as isize;
                while i + len < s.len() {
                    let next = pos + dir;
                    if next < 0 || next as usize >= lc.len() || lc[next as usize] != s[i + len] {
                        break;
                    }
                    pos = next;
                    len += 1;
                }
                best = best.max(len);
            }
        }
    }
    best
}

/// Length of the longest run of consecutive characters starting at `i` that steps by
/// a constant +1 or -1 within one class (lowercase letters or digits): `abcdef`, `987654`.
fn sequence_len_at(s: &[char], i: usize) -> usize {
    if i + 1 >= s.len() {
        return 0;
    }
    let (a, b) = (s[i], s[i + 1]);
    let same_class = (a.is_ascii_lowercase() && b.is_ascii_lowercase())
        || (a.is_ascii_digit() && b.is_ascii_digit());
    if !same_class {
        return 0;
    }
    let delta = b as i32 - a as i32;
    if delta != 1 && delta != -1 {
        return 0;
    }
    let mut len = 2;
    while i + len < s.len() {
        let (prev, cur) = (s[i + len - 1], s[i + len]);
        let same_class = (prev.is_ascii_lowercase() && cur.is_ascii_lowercase())
            || (prev.is_ascii_digit() && cur.is_ascii_digit());
        if !same_class || cur as i32 - prev as i32 != delta {
            break;
        }
        len += 1;
    }
    len
}

fn year_at(s: &[char], i: usize) -> bool {
    if i + 4 > s.len() || !s[i..i + 4].iter().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let year: u32 = s[i..i + 4].iter().collect::<String>().parse().unwrap_or(0);
    (1900..=2099).contains(&year)
}

/// The pattern token, if any, starting at `i`: longest dictionary/common match wins over
/// walks, sequences and years (a word that is also a walk, e.g. `qwerty`, is a common
/// password first).
fn pattern_token_at(s: &[char], i: usize) -> Option<(PatternKind, usize)> {
    let common = longest_word_at(s, i, COMMON);
    let word = longest_word_at(s, i, DICT_WORDS);
    if common >= 4 && common >= word {
        return Some((PatternKind::Common, common));
    }
    if word >= 4 {
        return Some((PatternKind::Word, word));
    }
    let walk = walk_len_at(s, i);
    if walk >= 4 {
        return Some((PatternKind::Walk, walk));
    }
    let seq = sequence_len_at(s, i);
    if seq >= 4 {
        return Some((PatternKind::Sequence, seq));
    }
    if year_at(s, i) {
        return Some((PatternKind::Year, 4));
    }
    None
}

/// Greedy cheapest-decomposition estimate for one prepared form of the password.
/// Pattern tokens cost their fixed guess counts; everything else is charged the naive
/// per-class entropy of the run, so a long random tail keeps the total high and only a
/// password mostly explained by patterns comes out cheap.
fn segment(s: &[char]) -> (f64, Vec<PatternKind>) {
    let mut bits = 0.0;
    let mut kinds: Vec<PatternKind> = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if let Some((kind, len)) = pattern_token_at(s, i) {
            bits += token_guesses(kind).log2();
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
            i += len;
            continue;
        }
        let mut j = i + 1;
        while j < s.len() && pattern_token_at(s, j).is_none() {
            j += 1;
        }
        let chunk: String = s[i..j].iter().collect();
        bits += entropy_bits(&chunk);
        i = j;
    }
    (bits, kinds)
}

/// Bits for a password that is exactly one short chunk repeated, when the chunk is
/// ASCII and single-class (all letters, all digits, or all symbols). The single-class
/// guard matters: a mixed-class chunk such as `Xq7#` repeated is a long random-looking
/// string, not a cracker pattern, and non-ASCII repeats are out of scope for this
/// estimator. Returns `None` when the password is not such a repeat.
fn repeated_chunk_bits(s: &[char]) -> Option<f64> {
    if !s.iter().all(|c| c.is_ascii()) || s.len() < 4 {
        return None;
    }
    for chunk_len in 2..=8 {
        if s.len() % chunk_len != 0 || s.len() / chunk_len < 2 {
            continue;
        }
        let chunk = &s[..chunk_len];
        let single_class = chunk.iter().all(|c| c.is_ascii_lowercase())
            || chunk.iter().all(|c| c.is_ascii_digit())
            || chunk.iter().all(|c| !c.is_ascii_alphanumeric());
        if !single_class {
            continue;
        }
        if s.chunks(chunk_len).all(|c| c == chunk) {
            let chunk_str: String = chunk.iter().collect();
            let reps = (s.len() / chunk_len) as f64;
            return Some(entropy_bits(&chunk_str) + reps.log2());
        }
    }
    None
}

/// Pattern-aware guess estimate for a password: the cheapest of the plain and
/// leetspeak-normalised segmentations, and of the repeated-chunk estimate, when that
/// estimate is cheaper than the naive [`entropy_bits`] and at least one pattern token
/// was found. Returns the estimate in bits plus a human-readable reason naming the
/// patterns found, e.g. `"common word + year"`, `"keyboard pattern"`.
/// Returns `None` for passwords with no detectable pattern (random passwords and
/// generated passphrases segment to roughly their naive entropy).
pub fn pattern_estimate(pw: &str) -> Option<(f64, String)> {
    let naive = entropy_bits(pw);
    let mut best: Option<(f64, Vec<PatternKind>)> = None;
    for leet in [false, true] {
        let chars = seg_chars(pw, leet);
        let (bits, kinds) = segment(&chars);
        if !kinds.is_empty() && bits < naive && best.as_ref().is_none_or(|(b, _)| bits < *b) {
            best = Some((bits, kinds));
        }
    }
    let flat = seg_chars(pw, false);
    if let Some(bits) = repeated_chunk_bits(&flat) {
        if bits < naive && best.as_ref().is_none_or(|(b, _)| bits < *b) {
            best = Some((bits, vec![PatternKind::Repeated]));
        }
    }
    best.map(|(bits, kinds)| {
        let reason = kinds
            .iter()
            .map(|k| k.label())
            .collect::<Vec<_>>()
            .join(" + ");
        (bits, reason)
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
    // A password can clear the naive length x pool estimate while still being cheap
    // to guess (`Summer2024!` is ~72 naive bits). The pattern estimate is the min of
    // the two, with a specific reason when it is the one below the threshold.
    if let Some((bits, reason)) = pattern_estimate(pw) {
        if bits < o.min_entropy_bits {
            return Some(format!("{reason} (~{bits:.0} bits)"));
        }
    }
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

    fn weak_reason(pw: &str) -> Option<String> {
        weakness(pw, &AuditOpts::default())
    }

    #[test]
    fn test_weakness_pattern_word_year() {
        // ~72 naive bits, but a dictionary word + a year: the pattern estimate wins.
        let reason = weak_reason("Summer2024!!").expect("word + year must be weak");
        assert!(reason.starts_with("common word + year"), "{reason}");
        assert!(reason.contains("bits"), "{reason}");
    }

    #[test]
    fn test_weakness_pattern_word_year_leet() {
        // Leetspeak must not hide the word: 5 -> s normalises to `summer`.
        // (The year half is masked in that same normalised form because 0 -> o
        // and 4 -> a, so the cheaper estimate names the word half only; the
        // password is still flagged either way.)
        let reason = weak_reason("5ummer2024!!").expect("leet word + year must be weak");
        assert!(reason.starts_with("common word"), "{reason}");
    }

    #[test]
    fn test_weakness_pattern_keyboard_walk_and_columns() {
        // Partial row/column walks that are not themselves common-password entries.
        let reason = weak_reason("Sdfghj4567!x").expect("row walk must be weak");
        assert!(reason.starts_with("keyboard pattern"), "{reason}");
        let reason = weak_reason("2wsx3edc4rfv!").expect("column walks must be weak");
        assert!(reason.starts_with("keyboard pattern"), "{reason}");
        // A full named walk is caught even earlier, by the common-password check.
        assert_eq!(
            weak_reason("Asdfghjkl123!"),
            Some("contains a very common password".into())
        );
    }

    #[test]
    fn test_weakness_pattern_sequence() {
        let reason = weak_reason("Abcdefgh1234!").expect("sequence must be weak");
        assert!(reason.starts_with("sequence pattern"), "{reason}");
        // Letter-only descending run (digit runs report as keyboard walks, since
        // the digit row is also a walk; see the precedence assertion below).
        let reason = weak_reason("Zyxwvutsrqpo!").expect("descending sequence must be weak");
        assert!(reason.starts_with("sequence pattern"), "{reason}");
        // A descending run containing a top-list fragment is caught even earlier.
        assert_eq!(
            weak_reason("9876543210ab!"),
            Some("contains a very common password".into())
        );
    }

    #[test]
    fn test_weakness_pattern_repeated_chunk() {
        // 18 chars of one 3-letter chunk: ~85 naive bits, ~16 pattern bits.
        let reason = weak_reason("abcabcabcabcabcabc").expect("repeated chunk must be weak");
        assert!(reason.starts_with("repeated pattern"), "{reason}");
    }

    #[test]
    fn test_weakness_pattern_keeps_strong_and_generated_passwords() {
        // Random characters: no pattern token explains the password cheaply.
        assert_eq!(weak_reason("Xq7#vR2!mK9@pL4$wZ"), None);
        assert_eq!(weak_reason("k7#Qv2!mZ9$xR4&wL2"), None);
        // Generated passphrase shape (words from the generator wordlist): the
        // dictionary list deliberately excludes generator words, and the long
        // random-looking tail keeps the estimate above the threshold.
        assert_eq!(weak_reason("acorn-amber-anchor-atlas-badge"), None);
        // Mixed-class repeats are not cracker patterns (mirrors the long-input
        // edge case in the integration tests).
        assert_eq!(weak_reason(&"Xq7#".repeat(25)), None);
    }

    #[test]
    fn test_pattern_dict_disjoint_from_generator_and_common() {
        // Guard for the passphrase guarantee above: if a future wordlist change
        // adds a dictionary word to the generator, this test fails loudly here
        // instead of silently flagging generated passphrases in audits.
        let words: &[&str] = &crate::generate::WORDLIST;
        for word in DICT_WORDS {
            assert!(!words.contains(word), "{word} is a generator word");
            assert!(!COMMON.contains(word), "{word} is a common password");
        }
    }

    #[test]
    fn test_pattern_estimate_direct() {
        // Direct contract: a fully patterned password estimates far below naive.
        let (bits, reason) = pattern_estimate("Summer2024!!").expect("pattern estimate");
        assert!(bits < entropy_bits("Summer2024!!"), "{bits}");
        assert_eq!(reason, "common word + year");
        // No pattern at all: no estimate, so callers fall back to naive entropy.
        assert_eq!(pattern_estimate("Xq7#vR2!mK9@pL4$wZ"), None);
    }
}
