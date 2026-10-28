//! `gitpass gen`: modes, flag validation, help text and the concealed copy path. All data FAKE.
use gitpass_cli::clip::Clipboard;
use gitpass_cli::gen::{self, Args};
use gitpass_core::generate::WORDLIST;
use gitpass_pasteboard::PasteboardMarkers;
use std::cell::RefCell;
use std::rc::Rc;

fn gp(args: &[&str]) -> std::process::Output {
    let home = tempfile::tempdir().unwrap();
    std::process::Command::new(env!("CARGO_BIN_EXE_gitpass"))
        .args(args)
        .env("HOME", home.path())
        .env("GITPASS_PATH", home.path().join("vault"))
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap()
}

fn out(o: &std::process::Output) -> String { String::from_utf8_lossy(&o.stdout).trim_end_matches('\n').to_string() }
fn err(o: &std::process::Output) -> String { String::from_utf8_lossy(&o.stderr).to_string() }

const SYMBOLS: &str = "!@#$%^&*-_=+?";

#[test]
fn default_and_lengths_match_todays_output() {
    let o = gp(&["gen"]);
    assert!(o.status.success(), "{}", err(&o));
    let pw = out(&o);
    assert_eq!(pw.chars().count(), 24, "{pw}");
    assert!(pw.chars().any(|c| c.is_ascii_digit()), "{pw}");
    assert!(pw.chars().any(|c| SYMBOLS.contains(c)), "{pw}");

    assert_eq!(out(&gp(&["gen", "32"])).chars().count(), 32);
    assert_eq!(out(&gp(&["gen", "6"])).chars().count(), 6);
    // The CLI has always used 4 as the floor (the desktop floors character passwords at 8).
    assert_eq!(out(&gp(&["gen", "0"])).chars().count(), 4);
}

#[test]
fn character_mode_toggles() {
    let no_symbols = out(&gp(&["gen", "24", "--no-symbols"]));
    assert_eq!(no_symbols.chars().count(), 24);
    assert!(!no_symbols.chars().any(|c| SYMBOLS.contains(c)), "{no_symbols}");
    assert!(no_symbols.chars().any(|c| c.is_ascii_digit()), "{no_symbols}");

    let no_digits = out(&gp(&["gen", "24", "--no-digits"]));
    assert!(!no_digits.chars().any(|c| c.is_ascii_digit()), "{no_digits}");
    assert!(no_digits.chars().any(|c| SYMBOLS.contains(c)), "{no_digits}");

    let letters = out(&gp(&["gen", "24", "--no-symbols", "--no-digits"]));
    assert!(letters.chars().all(|c| c.is_ascii_alphabetic()), "{letters}");
}

#[test]
fn passphrase_mode_uses_the_shared_wordlist() {
    let pw = out(&gp(&["gen", "--words", "5"]));
    let words: Vec<&str> = pw.split('-').collect();
    assert_eq!(words.len(), 5, "{pw}");
    let mut with_digit = 0;
    for w in &words {
        let alpha: String = w.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        assert!(WORDLIST.contains(&alpha.as_str()), "{w}");
        if alpha.len() < w.len() {
            assert!(w[alpha.len()..].chars().all(|c| c.is_ascii_digit()), "{w}");
            with_digit += 1;
        }
    }
    assert_eq!(with_digit, 1, "one word carries the digit by default: {pw}");

    let titled = out(&gp(&["gen", "--words", "4", "--no-digits", "--separator", ".", "--capitalize"]));
    let words: Vec<&str> = titled.split('.').collect();
    assert_eq!(words.len(), 4, "{titled}");
    for w in &words {
        assert!(w.chars().next().unwrap().is_ascii_uppercase(), "{w}");
        assert!(WORDLIST.contains(&w.to_lowercase().as_str()), "{w}");
    }
}

#[test]
fn pin_mode_is_digits_only() {
    let pin = out(&gp(&["gen", "--pin", "6"]));
    assert_eq!(pin.len(), 6, "{pin}");
    assert!(pin.chars().all(|c| c.is_ascii_digit()), "{pin}");
    // Same clamp as the desktop PIN generator.
    assert_eq!(out(&gp(&["gen", "--pin", "2"])).len(), 4);
}

#[test]
fn invalid_combinations_fail_with_a_clear_message() {
    for (args, needle) in [
        (vec!["gen", "--pin", "6", "--words", "5"], "mutually exclusive"),
        (vec!["gen", "32", "--words", "5"], "not both"),
        (vec!["gen", "32", "--pin", "6"], "not both"),
        (vec!["gen", "--separator", "x", "--pin", "6"], "only applies to --words"),
        (vec!["gen", "--capitalize"], "only applies to --words"),
        (vec!["gen", "--pin", "6", "--no-digits"], "cannot be combined with --pin"),
        (vec!["gen", "--clear-secs", "5"], "--copy"),
    ] {
        let o = gp(&args);
        assert!(!o.status.success(), "{args:?} should fail");
        assert!(err(&o).contains(needle), "{args:?}: {}", err(&o));
        assert!(out(&o).is_empty(), "{args:?} printed a secret");
    }
}

#[test]
fn help_lists_the_new_flags() {
    let o = gp(&["gen", "--help"]);
    assert!(o.status.success());
    let help = String::from_utf8_lossy(&o.stdout).to_string();
    for flag in ["--words", "--pin", "--separator", "--capitalize", "--no-symbols", "--no-digits", "--copy", "--clear-secs"] {
        assert!(help.contains(flag), "help misses {flag}:\n{help}");
    }
}

#[test]
fn options_map_flags_onto_the_core_generator() {
    let d = gen::options(&Args::default()).unwrap();
    assert_eq!((d.length, d.words, d.pin, d.symbols, d.digits, d.separator, d.capitalize), (24, false, false, true, true, '-', false));

    let words = gen::options(&Args { words: Some(5), no_symbols: true, ..Args::default() }).unwrap();
    assert_eq!((words.length, words.words, words.separator), (5, true, '.'), "desktop parity: --no-symbols joins with '.'");
    let words = gen::options(&Args { words: Some(5), separator: Some('_'), no_symbols: true, ..Args::default() }).unwrap();
    assert_eq!(words.separator, '_', "an explicit separator wins");
    let pin = gen::options(&Args { pin: Some(6), ..Args::default() }).unwrap();
    assert_eq!((pin.length, pin.pin, pin.words), (6, true, false));
    let len = gen::options(&Args { len: Some(32), ..Args::default() }).unwrap();
    assert_eq!(len.length, 32);
}

#[derive(Default)]
struct Recorder {
    last_set: Option<String>,
    last_markers: Option<PasteboardMarkers>,
    content: String,
    cleared: usize,
}

impl Clipboard for Recorder {
    fn set(&mut self, t: &str) -> anyhow::Result<()> { self.last_set = Some(t.to_string()); self.last_markers = None; self.content = t.to_string(); Ok(()) }
    fn get(&mut self) -> anyhow::Result<String> { Ok(self.content.clone()) }
    fn clear(&mut self) -> anyhow::Result<()> { self.content.clear(); self.cleared += 1; Ok(()) }
    fn set_marked(&mut self, t: &str, m: PasteboardMarkers) -> anyhow::Result<()> { self.set(t)?; self.last_markers = Some(m); Ok(()) }
}

struct Shared(Rc<RefCell<Recorder>>);

impl Clipboard for Shared {
    fn set(&mut self, t: &str) -> anyhow::Result<()> { self.0.borrow_mut().set(t) }
    fn get(&mut self) -> anyhow::Result<String> { self.0.borrow_mut().get() }
    fn clear(&mut self) -> anyhow::Result<()> { self.0.borrow_mut().clear() }
    fn set_marked(&mut self, t: &str, m: PasteboardMarkers) -> anyhow::Result<()> { self.0.borrow_mut().set_marked(t, m) }
}

#[test]
fn copy_goes_to_the_clipboard_and_never_to_stdout() {
    let rec = Rc::new(RefCell::new(Recorder::default()));
    let open = { let rec = rec.clone(); move || Ok(Box::new(Shared(rec)) as Box<dyn Clipboard>) };
    let mut stdout = Vec::new();
    gen::run(&Args { copy: true, ..Args::default() }, open, &mut stdout, |_| {}).unwrap();

    assert!(stdout.is_empty(), "--copy must not print the secret: {}", String::from_utf8_lossy(&stdout));
    let r = rec.borrow();
    let copied = r.last_set.as_deref().expect("value reached the clipboard");
    assert_eq!(copied.chars().count(), 24);
    assert_eq!(r.last_markers, Some(PasteboardMarkers::GENERATED), "concealed + transient + auto-generated");
    assert_eq!(r.cleared, 1, "auto-cleared after the timer");
}

#[test]
fn copy_without_auto_clear_and_without_a_clipboard_never_prints_the_secret() {
    let rec = Rc::new(RefCell::new(Recorder::default()));
    let open = { let rec = rec.clone(); move || Ok(Box::new(Shared(rec)) as Box<dyn Clipboard>) };
    let mut stdout = Vec::new();
    gen::run(&Args { copy: true, clear_secs: 0, ..Args::default() }, open, &mut stdout, |_| panic!("clear-secs 0 must not sleep")).unwrap();
    assert!(stdout.is_empty());
    {
        let r = rec.borrow();
        assert_eq!(r.last_markers, Some(PasteboardMarkers::GENERATED));
        assert_eq!(r.cleared, 0, "0 means don't wait/clear");
    }

    let mut stdout = Vec::new();
    gen::run(&Args { copy: true, ..Args::default() }, || Err(anyhow::anyhow!("headless")), &mut stdout, |_| {}).unwrap();
    assert!(stdout.is_empty(), "a failed copy must not fall back to printing the secret");
}

#[test]
fn without_copy_the_secret_is_printed_once() {
    let mut stdout = Vec::new();
    gen::run(&Args::default(), || panic!("no clipboard without --copy"), &mut stdout, |_| {}).unwrap();
    let s = String::from_utf8(stdout).unwrap();
    assert_eq!(s.lines().count(), 1);
    assert_eq!(s.trim_end().chars().count(), 24);
}
