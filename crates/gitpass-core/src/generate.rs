//! Password, passphrase and PIN generation shared by the CLI and the desktop app.
//!
//! One implementation so `gitpass gen`, the desktop generator (⌘⇧G) and the browser host
//! produce the same shapes of secrets.

use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use rand::Rng;

/// Character passwords are 8..=128 characters (the floor the desktop app and the browser host always used).
pub const MIN_LENGTH: usize = 8;
pub const MAX_LENGTH: usize = 128;
/// Passphrases are 3..=20 words.
pub const MIN_WORDS: usize = 3;
pub const MAX_WORDS: usize = 20;
/// PINs are 4..=16 digits.
pub const MIN_PIN: usize = 4;
pub const MAX_PIN: usize = 16;

/// EFF large wordlist (7,776 words, CC BY 3.0, https://www.eff.org/dice), words only.
/// Dice numbers are not included. Four words contain a hyphen (`drop-down`, `felt-tip`,
/// `t-shirt`, `yo-yo`), so a `-` join cannot be counted by splitting on `-`.
const EFF_WORDLIST_TXT: &str = include_str!("eff_large_wordlist.txt");

/// Passphrase words. Drawn with `SliceRandom::choose` from `OsRng`, which samples
/// uniformly (no modulo bias). ~12.9 bits per word (`log2(7776)`).
pub static WORDLIST: std::sync::LazyLock<Vec<&'static str>> =
    std::sync::LazyLock::new(|| EFF_WORDLIST_TXT.lines().filter(|w| !w.is_empty()).collect());

/// Entropy of one word drawn uniformly from [`WORDLIST`]: `log2(7776)`.
pub fn word_entropy_bits() -> f64 { (WORDLIST.len() as f64).log2() }

/// Entropy of a passphrase of `words` words from [`WORDLIST`].
pub fn passphrase_entropy_bits(words: usize) -> f64 { words as f64 * word_entropy_bits() }

/// Word count when `pw` is a passphrase of [`WORDLIST`] words joined by `-`, `.`, `_` or
/// space. Each word may have its first letter capitalized. At most one word may carry a
/// single trailing digit (the generator's digits option). `None` for anything else,
/// including character-mode passwords and PINs.
pub fn passphrase_word_count(pw: &str) -> Option<usize> {
    passphrase_parts(pw).map(|p| p.len())
}

/// Entropy of `pw` when [`passphrase_word_count`] recognizes it.
pub fn passphrase_entropy(pw: &str) -> Option<f64> {
    passphrase_word_count(pw).map(passphrase_entropy_bits)
}

fn in_wordlist(word: &str) -> bool { WORDLIST.binary_search(&word).is_ok() }

/// Lowercase wordlist key for a token: first letter may be capitalized, and the token
/// may end with one digit. `None` when the shape is not `[A-Za-z][a-z-]*[0-9]?`.
fn word_key(token: &str) -> Option<String> {
    if token.is_empty() { return None; }
    let (stem, stripped) = if token.as_bytes().last().is_some_and(|b| b.is_ascii_digit()) {
        (&token[..token.len() - 1], true)
    } else {
        (token, false)
    };
    if stem.is_empty() || (stripped && stem.as_bytes().last().is_some_and(|b| b.is_ascii_digit())) { return None; }
    let mut chars = stem.chars();
    let first = chars.next()?;
    if !chars.all(|c| c.is_ascii_lowercase() || c == '-') { return None; }
    if first.is_ascii_lowercase() { return Some(stem.to_string()); }
    if first.is_ascii_uppercase() {
        let mut s = String::with_capacity(stem.len());
        s.push(first.to_ascii_lowercase());
        s.push_str(&stem[first.len_utf8()..]);
        return Some(s);
    }
    None
}

fn token_in_wordlist(token: &str) -> bool { word_key(token).as_deref().is_some_and(in_wordlist) }

/// Split `pw` into word tokens when it is a passphrase.
fn passphrase_parts(pw: &str) -> Option<Vec<&str>> {
    if pw.is_empty() { return None; }
    let dot = pw.contains('.');
    let under = pw.contains('_');
    let space = pw.contains(' ');
    let joins = [dot, under, space].into_iter().filter(|on| *on).count();
    if joins > 1 { return None; }
    let sep = if dot { '.' } else if under { '_' } else if space { ' ' } else if pw.contains('-') { '-' } else { return None; };
    if pw.chars().any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == sep)) { return None; }
    let parts = if sep == '-' { split_hyphen_join(pw)? } else { split_plain_join(pw, sep)? };
    if !(MIN_WORDS..=MAX_WORDS).contains(&parts.len()) { return None; }
    let mut digits = 0usize;
    for part in &parts {
        if !token_in_wordlist(part) { return None; }
        if part.as_bytes().last().is_some_and(|b| b.is_ascii_digit()) { digits += 1; }
    }
    if digits > 1 { return None; }
    Some(parts)
}

fn split_plain_join(pw: &str, sep: char) -> Option<Vec<&str>> {
    let parts: Vec<&str> = pw.split(sep).collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) { return None; }
    Some(parts)
}

/// `-` is both the join and the hyphen inside four EFF words. Try a plain word up to the
/// next `-`, then a hyphenated word spanning one internal `-`, and backtrack if the rest
/// does not parse. The left half of each hyphenated word is not itself in the list, so
/// the successful parse is unique for this wordlist.
fn split_hyphen_join(pw: &str) -> Option<Vec<&str>> {
    fn rec<'a>(s: &'a str, out: &mut Vec<&'a str>) -> bool {
        if s.is_empty() { return !out.is_empty(); }
        if out.len() >= MAX_WORDS { return false; }
        let mut cands: Vec<(&str, &str)> = Vec::new();
        if let Some(h1) = s.find('-') {
            let plain = &s[..h1];
            if token_in_wordlist(plain) { cands.push((plain, &s[h1 + 1..])); }
            let after = &s[h1 + 1..];
            if let Some(h2) = after.find('-') {
                let end = h1 + 1 + h2;
                let hy = &s[..end];
                if token_in_wordlist(hy) { cands.push((hy, &s[end + 1..])); }
            } else if token_in_wordlist(s) {
                cands.push((s, ""));
            }
        } else if token_in_wordlist(s) {
            cands.push((s, ""));
        }
        cands.sort_by_key(|(tok, _)| std::cmp::Reverse(tok.len()));
        for (tok, rest) in cands {
            out.push(tok);
            if rec(rest, out) { return true; }
            out.pop();
        }
        false
    }
    let mut out = Vec::new();
    if rec(pw, &mut out) { Some(out) } else { None }
}

/// Character-mode digits skip 0 and 1 (easy to confuse with O and l).
const DIGITS: &[u8] = b"23456789";
/// PINs use every digit.
const PIN_DIGITS: &[u8] = b"0123456789";
const SYMBOLS: &[u8] = b"!@#$%^&*-_=+?";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Characters (random), words (passphrase) or digits (PIN).
    pub length: usize,
    pub symbols: bool,
    pub digits: bool,
    pub words: bool,
    pub pin: bool,
    /// Joins passphrase words (`-` in the desktop app unless symbols are off, then `.`).
    pub separator: char,
    /// Upper-cases the first letter of every passphrase word.
    pub capitalize: bool,
}

/// What kind of secret to generate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Random characters; `symbols`/`digits` pick the classes.
    Random,
    /// A passphrase of words; `digits` adds one digit to one word.
    Words,
    /// Digits only.
    Pin,
}

impl Mode {
    /// Inclusive length range for the mode (characters, words or digits).
    pub fn range(self) -> (usize, usize) {
        match self {
            Mode::Random => (MIN_LENGTH, MAX_LENGTH),
            Mode::Words => (MIN_WORDS, MAX_WORDS),
            Mode::Pin => (MIN_PIN, MAX_PIN),
        }
    }
    /// Length used when the caller gives none.
    pub fn default_length(self) -> usize {
        match self {
            Mode::Random => 20,
            Mode::Words => 5,
            Mode::Pin => 6,
        }
    }
    fn unit(self) -> &'static str {
        match self {
            Mode::Random => "characters",
            Mode::Words => "words",
            Mode::Pin => "digits",
        }
    }
}

/// A caller's generator request, in named fields so no call site passes `symbols`/`digits` positionally.
/// Turn it into [`Options`] with [`Options::for_request`], which owns the defaults and limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub mode: Mode,
    /// `None` uses [`Mode::default_length`].
    pub length: Option<usize>,
    pub symbols: bool,
    pub digits: bool,
    /// `None` joins words with `-`, or `.` when `symbols` is off.
    pub separator: Option<char>,
    pub capitalize: bool,
}

impl Request {
    /// Symbols and digits on, default length and separator, no capitalization.
    pub fn new(mode: Mode) -> Self {
        Self { mode, length: None, symbols: true, digits: true, separator: None, capitalize: false }
    }
}

impl Options {
    /// The one place that maps a request onto generator options: the length default per mode, the
    /// separator rule and the length limits. A length outside [`Mode::range`] is an error rather than
    /// being clamped, so the CLI, the desktop app and the browser host answer the same way.
    pub fn for_request(r: &Request) -> Result<Options, String> {
        let (min, max) = r.mode.range();
        let length = r.length.unwrap_or_else(|| r.mode.default_length());
        if !(min..=max).contains(&length) {
            return Err(format!("length must be {min}..={max} {}", r.mode.unit()));
        }
        Ok(Options {
            length,
            symbols: r.symbols,
            digits: r.digits,
            words: r.mode == Mode::Words,
            pin: r.mode == Mode::Pin,
            separator: r.separator.unwrap_or(if r.symbols { '-' } else { '.' }),
            capitalize: r.capitalize,
        })
    }
}

/// Generate a secret. `pin` wins over `words`, and both win over the character mode.
pub fn generate(opts: &Options) -> String {
    let mut rng = OsRng;
    if opts.pin {
        let n = opts.length.clamp(MIN_PIN, MAX_PIN);
        return (0..n).map(|_| *PIN_DIGITS.choose(&mut rng).unwrap() as char).collect();
    }
    if opts.words {
        let n = opts.length.clamp(MIN_WORDS, MAX_WORDS);
        let mut words: Vec<String> = (0..n).map(|_| passphrase_word(&mut rng, opts.capitalize)).collect();
        if opts.digits {
            let i = rng.gen_range(0..words.len());
            words[i].push_str(&rng.gen_range(0..10).to_string());
        }
        let sep = opts.separator.to_string();
        return words.join(&sep);
    }
    let len = opts.length.clamp(MIN_LENGTH, MAX_LENGTH);
    let mut pool: Vec<u8> = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".to_vec();
    if opts.digits { pool.extend_from_slice(DIGITS); }
    if opts.symbols { pool.extend_from_slice(SYMBOLS); }
    loop {
        let p: String = (0..len).map(|_| *pool.choose(&mut rng).unwrap() as char).collect();
        let ok = (!opts.digits || p.bytes().any(|b| DIGITS.contains(&b)))
            && (!opts.symbols || p.bytes().any(|b| SYMBOLS.contains(&b)));
        if ok { return p; }
    }
}

/// One character class of a site's password rules: draw from `chars`; when `required`, the password
/// holds at least one of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharClass {
    pub chars: Vec<u8>,
    pub required: bool,
}

/// Lowercase / uppercase letters (no look-alikes, as in the plain generator) and digits for site rules.
pub const LOWER: &[u8] = b"abcdefghijkmnopqrstuvwxyz";
pub const UPPER: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
/// The symbols the plain generator uses when a site does not restrict them.
pub const DEFAULT_SYMBOLS: &[u8] = SYMBOLS;
/// Digits without the 0/1 look-alikes.
pub const DEFAULT_DIGITS: &[u8] = DIGITS;

/// Generate a `length`-character password from site rules: every character comes from some class and
/// every `required` class appears at least once. `None` when the rules cannot be met (no usable
/// characters, an empty class, or more required classes than `length`).
pub fn generate_with_classes(length: usize, classes: &[CharClass]) -> Option<String> {
    let required: Vec<&CharClass> = classes.iter().filter(|c| c.required).collect();
    if length == 0 || classes.is_empty() || classes.iter().any(|c| c.chars.is_empty()) || required.len() > length {
        return None;
    }
    let mut rng = OsRng;
    let mut pool: Vec<u8> = classes.iter().flat_map(|c| c.chars.iter().copied()).collect();
    pool.sort_unstable();
    pool.dedup();
    let mut out: Vec<u8> = required.iter().map(|c| *c.chars.choose(&mut rng).unwrap()).collect();
    while out.len() < length {
        out.push(*pool.choose(&mut rng).unwrap());
    }
    out.shuffle(&mut rng);
    Some(out.into_iter().map(char::from).collect())
}

fn passphrase_word(rng: &mut impl Rng, capitalize: bool) -> String {
    let word = WORDLIST.choose(rng).unwrap();
    let mut out = String::with_capacity(word.len());
    let mut chars = word.chars();
    if let Some(first) = chars.next() {
        if capitalize { out.push(first.to_ascii_uppercase()); } else { out.push(first); }
        out.extend(chars);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(length: usize, symbols: bool, digits: bool, words: bool, pin: bool) -> Options {
        Options { length, symbols, digits, words, pin, separator: '-', capitalize: false }
    }

    #[test]
    fn random_respects_length_charset_and_clamps() {
        let p = generate(&opts(24, true, true, false, false));
        assert_eq!(p.len(), 24);
        assert_eq!(generate(&opts(2, true, true, false, false)).len(), MIN_LENGTH);
        assert_eq!(generate(&opts(500, true, true, false, false)).len(), MAX_LENGTH);
        assert!(generate(&opts(64, true, true, false, false)).bytes().all(|b| {
            b.is_ascii_alphabetic() || DIGITS.contains(&b) || SYMBOLS.contains(&b)
        }));
    }

    #[test]
    fn for_request_owns_defaults_limits_and_separator() {
        let o = Options::for_request(&Request::new(Mode::Random)).unwrap();
        assert_eq!((o.length, o.symbols, o.digits, o.words, o.pin), (20, true, true, false, false));
        assert_eq!(Options::for_request(&Request::new(Mode::Words)).unwrap().length, 5);
        assert_eq!(Options::for_request(&Request::new(Mode::Pin)).unwrap().length, 6);

        let mut w = Request::new(Mode::Words);
        assert_eq!(Options::for_request(&w).unwrap().separator, '-');
        w.symbols = false;
        assert_eq!(Options::for_request(&w).unwrap().separator, '.');
        w.separator = Some('_');
        assert_eq!(Options::for_request(&w).unwrap().separator, '_');

        for (mode, ok, bad) in [
            (Mode::Random, [MIN_LENGTH, MAX_LENGTH], [MIN_LENGTH - 1, MAX_LENGTH + 1]),
            (Mode::Words, [MIN_WORDS, MAX_WORDS], [MIN_WORDS - 1, MAX_WORDS + 1]),
            (Mode::Pin, [MIN_PIN, MAX_PIN], [MIN_PIN - 1, MAX_PIN + 1]),
        ] {
            for n in ok {
                assert_eq!(Options::for_request(&Request { length: Some(n), ..Request::new(mode) }).unwrap().length, n);
            }
            for n in bad {
                assert!(Options::for_request(&Request { length: Some(n), ..Request::new(mode) }).is_err(), "{mode:?} {n}");
            }
        }
    }

    #[test]
    fn random_toggles_drop_the_character_classes() {
        for _ in 0..20 {
            let letters = generate(&opts(32, false, false, false, false));
            assert!(letters.bytes().all(|b| b.is_ascii_alphabetic()), "{letters}");
            let plain = generate(&opts(32, false, true, false, false));
            assert!(plain.bytes().all(|b| b.is_ascii_alphanumeric()), "{plain}");
            assert!(plain.bytes().any(|b| b.is_ascii_digit()), "{plain}");
            let both = generate(&opts(32, true, true, false, false));
            assert!(both.bytes().any(|b| DIGITS.contains(&b)), "{both}");
            assert!(both.bytes().any(|b| SYMBOLS.contains(&b)), "{both}");
        }
    }

    #[test]
    fn passphrase_words_separator_capitalize_and_digit() {
        // `.` is never inside a word, so the split count is exact. `-` joins are checked
        // with passphrase_word_count because four EFF words themselves contain a hyphen.
        let plain = generate(&Options { separator: '.', ..opts(5, true, false, true, false) });
        let parts: Vec<&str> = plain.split('.').collect();
        assert_eq!(parts.len(), 5, "{plain}");
        assert!(parts.iter().all(|w| WORDLIST.contains(w)), "{plain}");

        let dashed = generate(&opts(5, true, false, true, false));
        assert_eq!(passphrase_word_count(&dashed), Some(5), "{dashed}");
        assert!(dashed.contains('-'), "{dashed}");

        let dotted = Options { separator: '.', capitalize: true, ..opts(4, true, false, true, false) };
        let titled = generate(&dotted);
        let parts: Vec<&str> = titled.split('.').collect();
        assert_eq!(parts.len(), 4, "{titled}");
        assert!(parts.iter().all(|w| w.chars().next().unwrap().is_ascii_uppercase()), "{titled}");
        assert!(parts.iter().all(|w| WORDLIST.contains(&w.to_ascii_lowercase().as_str())), "{titled}");

        let digits = Options { digits: true, separator: '.', ..opts(6, true, true, true, false) };
        let with_digit = generate(&digits);
        let parts: Vec<&str> = with_digit.split('.').collect();
        assert_eq!(parts.len(), 6, "{with_digit}");
        assert_eq!(parts.iter().filter(|w| w.ends_with(|c: char| c.is_ascii_digit())).count(), 1, "{with_digit}");
        assert!(parts.iter().all(|w| WORDLIST.contains(&w.trim_end_matches(|c: char| c.is_ascii_digit()))), "{with_digit}");
    }

    #[test]
    fn pin_is_digits_only_and_clamped() {
        let pin = generate(&opts(6, false, true, false, true));
        assert_eq!(pin.len(), 6);
        assert!(pin.bytes().all(|b| b.is_ascii_digit()), "{pin}");
        assert_eq!(generate(&opts(1, false, true, false, true)).len(), MIN_PIN);
        assert_eq!(generate(&opts(99, false, true, false, true)).len(), MAX_PIN);
    }

    fn class(chars: &[u8], required: bool) -> CharClass { CharClass { chars: chars.to_vec(), required } }

    #[test]
    fn site_rules_use_only_allowed_classes_and_all_required_ones() {
        let classes = [class(LOWER, true), class(UPPER, true), class(DEFAULT_DIGITS, true), class(b"-_", false)];
        for _ in 0..200 {
            let p = generate_with_classes(16, &classes).unwrap();
            assert_eq!(p.len(), 16, "{p}");
            assert!(p.bytes().all(|b| LOWER.contains(&b) || UPPER.contains(&b) || DEFAULT_DIGITS.contains(&b) || b"-_".contains(&b)), "{p}");
            assert!(p.bytes().any(|b| LOWER.contains(&b)), "{p}");
            assert!(p.bytes().any(|b| UPPER.contains(&b)), "{p}");
            assert!(p.bytes().any(|b| DEFAULT_DIGITS.contains(&b)), "{p}");
        }
    }

    #[test]
    fn site_rules_required_symbol_set_is_honored_even_when_more_symbols_are_allowed() {
        let classes = [class(LOWER, false), class(b"-", true), class(b"-_!", false)];
        for _ in 0..200 {
            let p = generate_with_classes(8, &classes).unwrap();
            assert!(p.contains('-'), "{p}");
            assert!(p.bytes().all(|b| LOWER.contains(&b) || b"-_!".contains(&b)), "{p}");
        }
    }

    #[test]
    fn site_rules_that_cannot_be_met_return_none() {
        assert!(generate_with_classes(8, &[]).is_none());
        assert!(generate_with_classes(0, &[class(LOWER, false)]).is_none());
        assert!(generate_with_classes(8, &[class(b"", false)]).is_none());
        let four = [class(LOWER, true), class(UPPER, true), class(DEFAULT_DIGITS, true), class(DEFAULT_SYMBOLS, true)];
        assert!(generate_with_classes(3, &four).is_none());
        assert_eq!(generate_with_classes(4, &four).unwrap().len(), 4);
    }

    #[test]
    fn wordlist_is_the_eff_large_list() {
        assert_eq!(WORDLIST.len(), 7776);
        let unique: std::collections::BTreeSet<_> = WORDLIST.iter().copied().collect();
        assert_eq!(unique.len(), WORDLIST.len());
        assert!(WORDLIST.iter().all(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')));
        assert!(WORDLIST.iter().copied().eq(EFF_WORDLIST_TXT.lines().filter(|w| !w.is_empty())));
        let mut sorted = WORDLIST.clone();
        sorted.sort_unstable();
        assert_eq!(sorted.as_slice(), WORDLIST.as_slice(), "binary search requires the checked-in order");
        assert!((word_entropy_bits() - (7776f64).log2()).abs() < 1e-12);
        assert_eq!(passphrase_entropy_bits(5), 5.0 * word_entropy_bits());
        // The hyphen splitter depends on this: each hyphenated word's left half is not a word.
        for w in WORDLIST.iter().filter(|w| w.contains('-')) {
            let (left, _) = w.split_once('-').unwrap();
            assert!(!in_wordlist(left), "{w}");
        }
    }

    #[test]
    fn passphrase_entropy_is_log2_7776_per_word() {
        let five = "abacus.abdomen.abdominal.abide.ability";
        assert_eq!(passphrase_word_count(five), Some(5));
        assert_eq!(passphrase_entropy(five), Some(passphrase_entropy_bits(5)));
        assert_eq!(passphrase_word_count("Abacus.Abdomen.Abdominal"), Some(3));
        assert_eq!(passphrase_word_count("abacus3-abdomen-abdominal"), Some(3));
        assert_eq!(passphrase_word_count("drop-down-abacus-abdomen"), Some(3));
        assert_eq!(passphrase_word_count("drop-down.abacus.felt-tip"), Some(3));
        assert_eq!(passphrase_word_count("t-shirt_yo-yo_abacus_zoom"), Some(4));
        assert_eq!(passphrase_word_count("Abacus Abdomen Abdominal"), Some(3));
        assert_eq!(passphrase_word_count("Yo-yo.T-shirt.Drop-down"), Some(3));
        assert_eq!(passphrase_word_count("abacus"), None);
        assert_eq!(passphrase_word_count("abacus-notaword-abdominal"), None);
        assert_eq!(passphrase_word_count("abacus1-abdomen2-abdominal"), None);
        assert_eq!(passphrase_word_count("abacus.abdomen.abdominal!"), None);
        assert_eq!(passphrase_word_count("k7QmN4pL2xR9wT"), None);
        assert_eq!(passphrase_word_count("123456"), None);
    }

    #[test]
    fn passphrase_sample_uses_many_distinct_words() {
        // 100 passphrases x 20 words = 2000 uniform draws from 7776 words.
        // The expected distinct count is about 1770, so 1000 is a conservative floor.
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..100 {
            let p = generate(&Options { separator: '.', ..opts(20, true, false, true, false) });
            for w in p.split('.') {
                assert!(WORDLIST.contains(&w), "{p}");
                seen.insert(w.to_string());
            }
        }
        assert!(seen.len() > 1000, "only {} distinct words in 2000 draws", seen.len());
    }
}
