//! HIBP k-anonymity: with a recording mock HTTP layer, prove the client is only ever handed the
//! 5-char uppercase SHA-1 prefix (never the password, full hash or suffix), padding rows are ignored,
//! and suffix matching is case-insensitive.
use gitpass_core::audit::{self, PwnedRangeClient, HIBP_RANGE_URL};
use sha1::{Digest, Sha1};
use std::cell::RefCell;

struct Recorder { seen: RefCell<Vec<String>>, body: String }
impl PwnedRangeClient for Recorder {
    fn range(&self, p: &str) -> anyhow::Result<String> { self.seen.borrow_mut().push(format!("{HIBP_RANGE_URL}{p}")); Ok(self.body.clone()) }
}

#[test]
fn only_five_hex_prefix_leaves_the_process() {
    assert_eq!(HIBP_RANGE_URL, "https://api.pwnedpasswords.com/range/");
    let pws = ["password", "correct horse battery staple", "Zq9!x"];
    let rec = Recorder { seen: RefCell::new(vec![]), body: "1e4c9b93f3f0682250b6cf8331b7ee68fd8:42\r\n0000000000000000000000000000000000A:0\r\n".into() };
    let mut cache = Default::default();
    for pw in pws {
        let full: String = Sha1::digest(pw.as_bytes()).iter().map(|b| format!("{b:02X}")).collect();
        let n = audit::pwned_count(&rec, pw, &mut cache).unwrap();
        let url = rec.seen.borrow().last().cloned().unwrap();
        let sent = url.strip_prefix(HIBP_RANGE_URL).unwrap();
        assert_eq!(sent, &full[..5]);
        assert!(sent.len() == 5 && sent.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));
        assert!(!sent.contains(pw) && !sent.contains(&full[5..]));
        if pw == "password" { assert_eq!(n, 42, "lowercase suffix in body still matches"); } else { assert_eq!(n, 0); }
    }
    assert_eq!(rec.seen.borrow().len(), 3);
}
