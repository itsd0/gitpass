use base64::{engine::general_purpose::STANDARD as B64, Engine};
use gitpass::crypto::*;
use gitpass::store::{self, Item, ItemFile, Session};

// Fast params for unit-level crypto tests; full flow test uses production MVP defaults.
fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }

#[test]
fn kdf_is_deterministic_and_input_sensitive() {
    let sk = SecretKey::generate();
    let a = derive_auk("hunter2!!", &sk, b"0123456789abcdef", "acct", &fast()).unwrap();
    let b = derive_auk("hunter2!!", &sk, b"0123456789abcdef", "acct", &fast()).unwrap();
    assert_eq!(a.0, b.0);
    let c = derive_auk("hunter3!!", &sk, b"0123456789abcdef", "acct", &fast()).unwrap();
    assert_ne!(a.0, c.0);
    let d = derive_auk("hunter2!!", &SecretKey::generate(), b"0123456789abcdef", "acct", &fast()).unwrap();
    assert_ne!(a.0, d.0);
}

#[test]
fn secret_key_roundtrip_and_checksum() {
    for _ in 0..200 {
        let sk = SecretKey::generate();
        let s = sk.encode();
        assert_eq!(s.len(), "A3-XXXXXX-XXXXXX-XXXXX-XXXXX-XXXXX".len());
        assert_eq!(SecretKey::parse(&s).unwrap().0, sk.0);
        assert_eq!(SecretKey::parse(&s.to_lowercase()).unwrap().0, sk.0);
    }
    let s = SecretKey::generate().encode();
    let mut b = s.into_bytes();
    let i = 10; b[i] = if b[i] == b'2' { b'3' } else { b'2' };
    assert!(SecretKey::parse(std::str::from_utf8(&b).unwrap()).is_err());
}

#[test]
fn item_roundtrip() {
    let k = Key32::random();
    let aad = store::item_aad("v", "i", 1);
    let env = seal(&k, b"secret item json", &aad).unwrap();
    assert_eq!(open(&k, &env, &aad).unwrap(), b"secret item json");
    // wrong AAD (e.g. version rollback / item swap) fails
    assert!(open(&k, &env, &store::item_aad("v", "i", 2)).is_err());
}

#[test]
fn wrong_key_fails() {
    let env = seal(&Key32::random(), b"x", b"").unwrap();
    let err = open(&Key32::random(), &env, b"").unwrap_err().to_string();
    assert!(err.contains("authentication tag"));
}

#[test]
fn tamper_fails() {
    let k = Key32::random();
    let mut env = seal(&k, b"hello world", b"aad").unwrap();
    let mut ct = B64.decode(&env.ct).unwrap();
    ct[3] ^= 0x01;
    env.ct = B64.encode(ct);
    assert!(open(&k, &env, b"aad").is_err());
}

#[test]
fn full_flow_on_temp_dir() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let sk = store::init(root, "correct horse battery", KdfParams::default()).unwrap();
    let skstr = sk.encode();
    let sk2 = SecretKey::parse(&skstr).unwrap();
    for f in ["format.json", "keyset.enc"] { assert!(root.join(f).exists()); }

    let s = Session::unlock(root, "correct horse battery", &sk2).unwrap();
    let vid = s.create_vault("personal").unwrap();
    assert_eq!(s.resolve_vault("personal").unwrap(), vid);
    let item = Item { title: "GitHub".into(), username: Some("leeky".into()), password: Some("p@ss".into()),
        url: Some("https://github.com".into()), notes: None, totp: None, updated_at: store::now() };
    let iid = s.add_item(&vid, &item).unwrap();

    // no plaintext on disk
    let raw = std::fs::read_to_string(root.join("vaults").join(&vid).join("items").join(format!("{iid}.enc"))).unwrap();
    assert!(!raw.contains("p@ss") && !raw.contains("leeky"));

    // fresh session reads it back
    let s2 = Session::unlock(root, "correct horse battery", &sk2).unwrap();
    let found = s2.find_items(&vid, "git").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1, item);

    // edit bumps version
    let mut e = item.clone(); e.password = Some("new".into());
    s2.update_item(&vid, &iid, &e).unwrap();
    let (v, got) = s2.read_item(&vid, &iid).unwrap();
    assert_eq!((v, got.password.as_deref()), (2, Some("new")));

    // tampering with stored version (rollback attempt) breaks AAD
    let p = root.join("vaults").join(&vid).join("items").join(format!("{iid}.enc"));
    let mut f: ItemFile = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    f.version = 1;
    std::fs::write(&p, serde_json::to_vec(&f).unwrap()).unwrap();
    assert!(s2.read_item(&vid, &iid).is_err());

    // wrong password / wrong secret key cannot unlock
    assert!(Session::unlock(root, "wrong password", &sk2).is_err());
    assert!(Session::unlock(root, "correct horse battery", &SecretKey::generate()).is_err());

    s2.remove_item(&vid, &iid).unwrap();
    assert!(s2.list_items(&vid).unwrap().is_empty());
}
