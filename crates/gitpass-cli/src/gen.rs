//! `gitpass gen`: shared password/passphrase/PIN generation plus an optional concealed pasteboard copy.
use anyhow::{bail, Result};
use gitpass_core::generate::{generate, Options};
use gitpass_pasteboard::PasteboardMarkers;
use std::io::Write;
use std::time::Duration;
use zeroize::Zeroizing;

use crate::clip::{self, Clipboard};

pub const DEFAULT_LENGTH: usize = 24;
pub const DEFAULT_CLEAR_SECS: u64 = 30;

/// Raw `gitpass gen` flags; `None`/`false` means the flag was not given.
#[derive(Clone, Debug)]
pub struct Args {
    pub len: Option<usize>,
    pub words: Option<usize>,
    pub pin: Option<usize>,
    pub separator: Option<char>,
    pub capitalize: bool,
    pub no_symbols: bool,
    pub no_digits: bool,
    pub copy: bool,
    pub clear_secs: u64,
}

impl Default for Args {
    fn default() -> Self {
        Self { len: None, words: None, pin: None, separator: None, capitalize: false,
            no_symbols: false, no_digits: false, copy: false, clear_secs: DEFAULT_CLEAR_SECS }
    }
}

/// Validate the flags and map them onto the core generator options.
pub fn options(a: &Args) -> Result<Options> {
    if a.words.is_some() && a.pin.is_some() {
        bail!("--words and --pin are mutually exclusive");
    }
    if a.len.is_some() && (a.words.is_some() || a.pin.is_some()) {
        bail!("give either a password length or --words/--pin, not both");
    }
    if a.separator.is_some() && a.words.is_none() {
        bail!("--separator only applies to --words");
    }
    if a.capitalize && a.words.is_none() {
        bail!("--capitalize only applies to --words");
    }
    if a.no_digits && a.pin.is_some() {
        bail!("--no-digits cannot be combined with --pin (a PIN is always digits)");
    }
    let words = a.words.is_some();
    Ok(Options {
        length: a.words.or(a.pin).or(a.len).unwrap_or(DEFAULT_LENGTH),
        symbols: !a.no_symbols,
        digits: !a.no_digits,
        words,
        pin: a.pin.is_some(),
        separator: a.separator.unwrap_or(if a.no_symbols { '.' } else { '-' }),
        capitalize: a.capitalize,
    })
}

/// Generate the password string according to the given options.
pub fn generate_pw(a: &Args) -> Result<String> {
    Ok(generate(&options(a)?))
}

/// Output the generated password: write to stdout or copy to the clipboard.
pub fn output_pw(
    a: &Args,
    pw: &str,
    open_clipboard: impl FnOnce() -> Result<Box<dyn Clipboard>>,
    out: &mut dyn Write,
    sleep: impl FnOnce(Duration),
) -> Result<()> {
    let pw = Zeroizing::new(pw.to_string());
    if !a.copy {
        writeln!(out, "{}", pw.as_str())?;
        return Ok(());
    }
    match open_clipboard() {
        Err(e) => eprintln!("warning: clipboard unavailable ({e}); the generated password was not printed (drop --copy to show it)"),
        Ok(mut cb) if a.clear_secs == 0 => {
            clip::set_with_markers(cb.as_mut(), pw.as_str(), Some(PasteboardMarkers::GENERATED))?;
            eprintln!("copied generated password to the clipboard (not auto-cleared)");
        }
        Ok(mut cb) => {
            eprintln!("copied generated password to the clipboard; clearing in {}s (keep this running, Ctrl-C leaves it)", a.clear_secs);
            let cleared = clip::copy_then_clear(cb.as_mut(), pw.as_str(), Some(PasteboardMarkers::GENERATED),
                Duration::from_secs(a.clear_secs), sleep)?;
            eprintln!("{}", if cleared { "clipboard cleared" } else { "clipboard changed since copy; left untouched" });
        }
    }
    Ok(())
}

/// Generate and either print the secret or copy it as concealed/transient — never both.
///
/// `open_clipboard` is called only with `--copy`; the pasteboard is cleared after `clear_secs`
/// unless the user copied something else in the meantime.
pub fn run(
    a: &Args,
    open_clipboard: impl FnOnce() -> Result<Box<dyn Clipboard>>,
    out: &mut dyn Write,
    sleep: impl FnOnce(Duration),
) -> Result<()> {
    let pw = generate_pw(a)?;
    output_pw(a, &pw, open_clipboard, out, sleep)
}
