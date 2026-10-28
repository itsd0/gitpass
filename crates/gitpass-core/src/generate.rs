//! Password, passphrase and PIN generation shared by the CLI and the desktop app.
//!
//! One implementation so `gitpass gen`, the desktop generator (⌘⇧G) and the browser host
//! produce the same shapes of secrets.

use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use rand::Rng;

/// Character passwords are clamped to 4..=128; 4 is the historical `gitpass gen` floor.
pub const MIN_LENGTH: usize = 4;
pub const MAX_LENGTH: usize = 128;
/// Passphrases are 3..=20 words.
pub const MIN_WORDS: usize = 3;
pub const MAX_WORDS: usize = 20;
/// PINs are 4..=16 digits.
pub const MIN_PIN: usize = 4;
pub const MAX_PIN: usize = 16;

/// 128 short, easy-to-spell words: ~7 bits of entropy per word.
pub const WORDLIST: &[&str] = &["acorn","amber","anchor","apple","arrow","aspen","atlas","badge","bamboo","banjo","beacon","birch","bison","blossom","breeze","bridge",
    "cactus","canyon","carbon","cedar","cherry","cinder","citrus","clover","cobalt","comet","coral","cosmos","crane","crystal","dahlia","delta",
    "desert","dolphin","dragon","drift","eagle","ember","falcon","fern","fjord","forest","fossil","galaxy","garnet","geyser","glacier","granite",
    "harbor","hazel","heron","horizon","indigo","island","ivory","jasmine","jungle","kayak","kernel","lagoon","lantern","lava","lemon","lilac",
    "lotus","lunar","magnet","mango","maple","marble","meadow","meteor","mint","mosaic","nectar","nebula","oasis","ocean","olive","onyx",
    "orbit","orchid","otter","panda","pebble","pepper","phoenix","pine","planet","plum","polar","prairie","quartz","quill","radar","raven",
    "reef","ripple","river","rocket","saffron","salmon","sapphire","savanna","sequoia","shadow","sierra","silver","sparrow","spruce","summit","sunset",
    "thistle","thunder","tiger","topaz","tulip","tundra","turtle","velvet","violet","walnut","willow","winter","zebra","zenith","yarrow","yonder"];

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
        let plain = generate(&opts(5, true, false, true, false));
        let parts: Vec<&str> = plain.split('-').collect();
        assert_eq!(parts.len(), 5, "{plain}");
        assert!(parts.iter().all(|w| WORDLIST.contains(w)), "{plain}");

        let dotted = Options { separator: '.', capitalize: true, ..opts(4, true, false, true, false) };
        let titled = generate(&dotted);
        let parts: Vec<&str> = titled.split('.').collect();
        assert_eq!(parts.len(), 4, "{titled}");
        assert!(parts.iter().all(|w| w.chars().next().unwrap().is_ascii_uppercase()), "{titled}");

        let digits = Options { digits: true, ..opts(6, true, true, true, false) };
        let with_digit = generate(&digits);
        let parts: Vec<&str> = with_digit.split('-').collect();
        assert_eq!(parts.len(), 6, "{with_digit}");
        assert_eq!(parts.iter().filter(|w| w.ends_with(|c: char| c.is_ascii_digit())).count(), 1, "{with_digit}");
    }

    #[test]
    fn pin_is_digits_only_and_clamped() {
        let pin = generate(&opts(6, false, true, false, true));
        assert_eq!(pin.len(), 6);
        assert!(pin.bytes().all(|b| b.is_ascii_digit()), "{pin}");
        assert_eq!(generate(&opts(1, false, true, false, true)).len(), MIN_PIN);
        assert_eq!(generate(&opts(99, false, true, false, true)).len(), MAX_PIN);
    }

    #[test]
    fn wordlist_has_128_unique_ascii_words() {
        assert_eq!(WORDLIST.len(), 128);
        let unique: std::collections::BTreeSet<_> = WORDLIST.iter().collect();
        assert_eq!(unique.len(), WORDLIST.len());
        assert!(WORDLIST.iter().all(|w| w.bytes().all(|b| b.is_ascii_lowercase())));
    }
}
