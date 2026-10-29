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

/// The EFF large wordlist (7,776 words, CC BY 3.0, https://www.eff.org/dice),
/// words only (dice numbers stripped), one word per line. A few words contain a
/// hyphen (`drop-down`, `felt-tip`, `t-shirt`, `yo-yo`), so a `-`-joined passphrase
/// cannot be word-counted by splitting on `-`.
const EFF_WORDLIST_TXT: &str = include_str!("eff_large_wordlist.txt");

/// Passphrase words: the EFF large wordlist, ~12.9 bits of entropy per word.
/// Words are drawn with `SliceRandom::choose` from the existing CSPRNG (`OsRng`),
/// which samples uniformly (no modulo bias). Parsed once on first use from the
/// checked-in text file above, so the file stays the single source of truth.
pub static WORDLIST: std::sync::LazyLock<Vec<&'static str>> =
    std::sync::LazyLock::new(|| EFF_WORDLIST_TXT.lines().collect());

/// Bits of entropy per passphrase word: log2(7776).
pub const WORD_ENTROPY_BITS: f64 = 12.924813;

/// Entropy of a passphrase of `words` words drawn uniformly from [`WORDLIST`].
/// Callers that display a passphrase strength should use this, not a
/// character-pool estimate, which overstates word entropy.
pub fn passphrase_entropy_bits(words: usize) -> f64 { words as f64 * WORD_ENTROPY_BITS }

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
        // `.` separator: exact word counts. `-` joins cannot be counted by splitting,
        // because four EFF words themselves contain a hyphen (e.g. `drop-down`).
        let plain = generate(&Options { separator: '.', ..opts(5, true, false, true, false) });
        let parts: Vec<&str> = plain.split('.').collect();
        assert_eq!(parts.len(), 5, "{plain}");
        assert!(parts.iter().all(|w| WORDLIST.contains(w)), "{plain}");

        let dotted = Options { separator: '.', capitalize: true, ..opts(4, true, false, true, false) };
        let titled = generate(&dotted);
        let parts: Vec<&str> = titled.split('.').collect();
        assert_eq!(parts.len(), 4, "{titled}");
        assert!(parts.iter().all(|w| w.chars().next().unwrap().is_ascii_uppercase()), "{titled}");

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
        let unique: std::collections::BTreeSet<_> = WORDLIST.iter().collect();
        assert_eq!(unique.len(), WORDLIST.len());
        assert!(WORDLIST.iter().all(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')));
        // The checked-in text file is the single source: the list is exactly its lines.
        assert!(WORDLIST.iter().copied().eq(EFF_WORDLIST_TXT.lines()));
        assert!((WORD_ENTROPY_BITS - (WORDLIST.len() as f64).log2()).abs() < 1e-6);
        assert_eq!(passphrase_entropy_bits(5), 5.0 * WORD_ENTROPY_BITS);
    }

    #[test]
    fn passphrase_sample_uses_many_distinct_words() {
        // 100 passphrases x 20 words = 2000 uniform draws from 7776 words:
        // expect ~1770 distinct, so 1000 is a very conservative floor.
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
