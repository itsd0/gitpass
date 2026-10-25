use gitpass_core::search::{clear_winner, rank, score_str};
use gitpass_core::store::Item;
use gitpass_core::totp::{base32_decode, code_at, hotp, parse, Algo};

const RFC_B32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"; // "12345678901234567890"

#[test]
fn base32_roundtrip_known() {
    assert_eq!(base32_decode(RFC_B32).unwrap(), b"12345678901234567890");
    assert_eq!(base32_decode("jbsw y3dp ehpk 3pxp").unwrap(), b"Hello!\xde\xad\xbe\xef");
    assert!(base32_decode("not*b32").is_err());
}

#[test]
fn rfc4226_hotp_vectors() {
    let k = b"12345678901234567890";
    let want = ["755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583", "399871", "520489"];
    for (c, w) in want.iter().enumerate() { assert_eq!(hotp(k, c as u64, 6, Algo::Sha1), *w); }
}

#[test]
fn rfc6238_totp_vectors() {
    let s1 = format!("otpauth://totp/x?secret={RFC_B32}&digits=8");
    let sp = parse(&s1).unwrap();
    for (t, w) in [(59u64, "94287082"), (1111111109, "07081804"), (1111111111, "14050471"),
                   (1234567890, "89005924"), (2000000000, "69279037"), (20000000000, "65353130")] {
        assert_eq!(code_at(&sp, t).0, w, "t={t}");
    }
    assert_eq!(code_at(&sp, 59).1, 1);
    // SHA256 / SHA512 seeds from RFC 6238 appendix B
    assert_eq!(hotp(b"12345678901234567890123456789012", 59 / 30, 8, Algo::Sha256), "46119246");
    assert_eq!(hotp(b"1234567890123456789012345678901234567890123456789012345678901234", 59 / 30, 8, Algo::Sha512), "90693936");
}

#[test]
fn totp_parse_raw_and_uri() {
    let raw = parse("JBSWY3DPEHPK3PXP").unwrap();
    assert_eq!((raw.digits, raw.period, raw.algo), (6, 30, Algo::Sha1));
    let (c, rem) = code_at(&raw, 1_700_000_000);
    assert_eq!(c.len(), 6); assert!(c.chars().all(|d| d.is_ascii_digit())); assert!((1..=30).contains(&rem));
    let u = parse("otpauth://totp/ACME%20Co:jo@x.com?secret=JBSWY3DPEHPK3PXP&issuer=ACME&algorithm=SHA256&period=60").unwrap();
    assert_eq!((u.period, u.algo), (60, Algo::Sha256));
    assert!(parse("otpauth://hotp/x?secret=JBSWY3DPEHPK3PXP").is_err());
    assert!(parse("otpauth://totp/x?issuer=a").is_err());
}

fn item(t: &str, u: &str, url: &str) -> Item {
    Item { title: t.into(), username: Some(u.into()), password: Some("pw".into()), url: Some(url.into()),
        notes: None, totp: None, updated_at: String::new() }
}

#[test]
fn fuzzy_scores_order() {
    assert!(score_str("git", "git") > score_str("git", "github"));
    assert!(score_str("git", "github") > score_str("git", "my gitlab"));
    assert!(score_str("git", "my gitlab") > score_str("git", "digit"));
    assert!(score_str("gh", "github").is_some()); // subsequence
    assert!(score_str("zz", "github").is_none());
}

#[test]
fn fuzzy_rank_and_winner() {
    let items = vec![
        ("a1".to_string(), item("GitHub", "alice", "https://github.com")),
        ("b2".to_string(), item("GitLab", "bob", "https://gitlab.com")),
        ("c3".to_string(), item("Bank", "carol", "https://bank.example")),
        ("d4".to_string(), item("Email", "dave@github.com", "https://mail.example")),
    ];
    let r = rank("github", items.clone());
    assert_eq!(r[0].1, "a1");
    assert_eq!(clear_winner(&r), Some(0));
    assert!(r.iter().any(|x| x.1 == "d4")); // username match
    let r = rank("git", items.clone());
    assert!(r.len() >= 2 && clear_winner(&r).is_none(), "ambiguous: {:?}", r.iter().map(|x| (&x.1, x.0)).collect::<Vec<_>>());
    let r = rank("bnk", items.clone());
    assert_eq!(r.len(), 1); assert_eq!(r[0].1, "c3");
    assert_eq!(rank("c3", items.clone())[0].1, "c3"); // id
    assert!(rank("qqq", items).is_empty());
}

