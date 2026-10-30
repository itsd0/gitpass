use super::*;


fn gen_pw(mode: gitpass_core::generate::Mode, length: usize, symbols: bool, digits: bool, separator: Option<char>, capitalize: Option<bool>) -> anyhow::Result<String> {
    generate(&gitpass_core::generate::Request { mode, length: Some(length), symbols, digits, separator, capitalize: capitalize.unwrap_or(false) })
}

#[test]
fn generate_words_separator_capitalize_and_rejection() {
    // Explicit separator joins the words; capitalize upper-cases each word's first letter.
    let p = gen_pw(Mode::Words, 5, true, false, Some('_'), Some(true)).unwrap();
    let words: Vec<&str> = p.split('_').collect();
    assert_eq!(words.len(), 5);
    assert!(words.iter().all(|w| w.chars().next().unwrap().is_ascii_uppercase()
        && w[1..].chars().all(|c| c.is_ascii_lowercase())), "{p}");
    let spaced = gen_pw(Mode::Words, 4, true, false, Some(' '), None).unwrap();
    assert_eq!(spaced.split(' ').count(), 4);
    // Omitted values keep the historical defaults: '-' with symbols, '.' without, no capitalization.
    assert_eq!(gen_pw(Mode::Words, 3, true, false, None, None).unwrap().split('-').count(), 3);
    let dotted = gen_pw(Mode::Words, 3, false, false, None, None).unwrap();
    assert_eq!(dotted.split('.').count(), 3);
    assert!(dotted.chars().all(|c| c.is_ascii_lowercase() || c == '.'));
    // Anything outside the offered separators is a bad request.
    assert!(gen_pw(Mode::Words, 3, true, false, Some('!'), None).is_err());
    assert!(gen_pw(Mode::Words, 3, true, false, Some('/'), None).is_err());
    // Separator/capitalize do not change the random-mode shape (still 20 characters).
    assert_eq!(gen_pw(Mode::Random, 20, true, true, Some('_'), Some(true)).unwrap().len(), 20);
}

#[test]
fn tools_generate_strength_watchtower_hibp() {
    let e = env();
    let p = gen_pw(Mode::Random, 24, true, true, None, None).unwrap();
    assert_eq!(p.len(), 24);
    assert!(p.chars().any(|c| c.is_ascii_digit()) && p.chars().any(|c| "!@#$%^&*-_=+?".contains(c)));
    assert!(gen_pw(Mode::Random, 20, false, false, None, None).unwrap().chars().all(|c| c.is_ascii_alphabetic()));
    assert_eq!(gen_pw(Mode::Words, 5, true, false, None, None).unwrap().split('-').count(), 5);
    let pin = gen_pw(Mode::Pin, 6, false, false, None, None).unwrap();
    assert_eq!(pin.len(), 6);
    assert!(pin.chars().all(|c| c.is_ascii_digit()));
    // Same shapes and limits as the browser host and the CLI.
    assert_eq!(gen_pw(Mode::Words, 4, false, false, None, None).unwrap().split('.').count(), 4);
    assert!(gen_pw(Mode::Random, 7, true, true, None, None).is_err());
    assert!(gen_pw(Mode::Random, 129, true, true, None, None).is_err());
    assert!(gen_pw(Mode::Pin, 2, true, true, None, None).is_err());
    assert!(gen_pw(Mode::Words, 21, true, true, None, None).is_err());
    assert!(e.app.password_strength("password") <= 10);
    assert!(e.app.password_strength(&p) >= 90);
    assert_eq!(e.app.password_strength(""), 0);
    let a = login(&e, "Weak1", "a", "password");
    let b = login(&e, "Weak2", "b", "password");
    let _c = login(&e, "Strong", "c", &p);
    let r = e.app.watchtower(false).unwrap();
    assert!(!r.hibp_checked && r.breached.is_empty());
    assert!(r.weak.contains(&a.id) && r.weak.contains(&b.id));
    assert_eq!(r.reused.len(), 1);
    assert_eq!(r.missing_totp.len(), 3);
    assert!(r.totp_available.is_empty() && r.passkey_available.is_empty());
    assert!(r.unused.is_empty(), "fresh items are never unused");
    assert!(r.score < 100);
    // HIBP is opt-in: without Settings.hibpCheck no network check happens even if asked.
    assert!(!e.app.get_settings().hibp_check);
    let r = e.app.watchtower(true).unwrap();
    assert!(!r.hibp_checked && r.breached.is_empty());
    e.app.set_settings(Settings { hibp_check: true, ..e.app.get_settings() }).unwrap();
    let r = e.app.watchtower(true).unwrap();
    assert!(r.hibp_checked);
    let mut br = r.breached.clone(); br.sort();
    let mut want = vec![a.id.clone(), b.id.clone()]; want.sort();
    assert_eq!(br, want);
}
