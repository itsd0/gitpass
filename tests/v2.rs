//! Format v2 tests: KATs, migration, tamper/rollback detection, rotation, padding, proptest.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use gitpass::crypto::*;
use gitpass::migrate::{self, v1, MigrateOpts};
use gitpass::store::{self, Format, Item, Session, UnlockOpts};
use std::fs;
use std::path::Path;

const PW: &str = "correct horse battery";
fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn opts() -> UnlockOpts { UnlockOpts { accept_rollback: false, target_kdf: fast() } }
fn unlock(root: &Path, pw: &str, sk: &SecretKey) -> anyhow::Result<Session> { Session::unlock_with(root, pw, sk, &opts()) }
fn item(title: &str, pw: &str) -> Item {
    Item { title: title.into(), username: Some("me".into()), password: Some(pw.into()), url: None, notes: None, totp: None, updated_at: "1".into() }
}
fn setup() -> (tempfile::TempDir, SecretKey, String, String) {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    let s = unlock(d.path(), PW, &sk).unwrap();
    let vid = s.create_vault("personal").unwrap();
    let iid = s.add_item(&vid, &item("GitHub", "p1")).unwrap();
    (d, sk, vid, iid)
}
fn ipath(root: &Path, vid: &str, iid: &str) -> std::path::PathBuf { root.join("vaults").join(vid).join("items").join(format!("{iid}.enc")) }
fn assert_refused(root: &Path, sk: &SecretKey) {
    let e = unlock(root, PW, sk).err().expect("unlock should have been refused");
    eprintln!("refused as expected: {e:#}");
}

// ---------------- known-answer vectors ----------------
fn unhex(s: &str) -> Vec<u8> { hex::decode(s).unwrap() }

#[test]
fn kat_argon2id_rfc9106() {
    // RFC 9106 section 5.3 Argon2id test vector
    use argon2::{Algorithm, Argon2, AssociatedData, ParamsBuilder, Version};
    let params = ParamsBuilder::new().m_cost(32).t_cost(3).p_cost(4).output_len(32)
        .data(AssociatedData::new(&[0x04; 12]).unwrap()).build().unwrap();
    let a = Argon2::new_with_secret(&[0x03; 8], Algorithm::Argon2id, Version::V0x13, params).unwrap();
    let mut out = [0u8; 32];
    a.hash_password_into(&[0x01; 32], &[0x02; 16], &mut out).unwrap();
    assert_eq!(out.to_vec(), unhex("0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659"));
}

#[test]
fn kat_hkdf_sha256_rfc5869_case1() {
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(&unhex("000102030405060708090a0b0c")), &[0x0b; 22]);
    let mut okm = [0u8; 42];
    hk.expand(&unhex("f0f1f2f3f4f5f6f7f8f9"), &mut okm).unwrap();
    assert_eq!(okm.to_vec(), unhex("3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"));
}

#[test]
fn kat_gitpass_golden_auk_and_ciphertext() {
    // Golden values recorded from this implementation (regression guard for the KDF/AEAD pipeline).
    let sk = SecretKey([0x11; 16]);
    assert_eq!(sk.encode(), "A3-2K46AK-46AK46-AK46A-K46AK-46AKD");
    let auk = derive_auk("correct horse battery staple", &sk, &[0x22; 16], "00000000-0000-4000-8000-000000000000", &fast()).unwrap();
    assert_eq!(auk.0.to_vec(), unhex("174bda6662e96eabec9fb338b7aa3230ca185658524ccffaf37542cef886e029"));
    let e = seal_with_nonce(&auk, b"gitpass golden plaintext", b"golden-aad", [0x33; 24]).unwrap();
    assert_eq!(e.ct, "h1SzKib/0Pm/pWNfd2q8G3nG1hFSdvQZbRBsPS8v9SDuotG7PEeP9w==");
    assert_eq!(open(&auk, &e, b"golden-aad").unwrap(), b"gitpass golden plaintext");
}

#[test]
fn default_kdf_is_256mib() {
    let k = KdfParams::default();
    assert_eq!((k.m_kib, k.t, k.p), (262144, 3, 4));
}

// ---------------- migration ----------------
fn v1_fixture() -> (tempfile::TempDir, SecretKey, Vec<(String, String, Item)>) {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let sk = v1::init_fixture(r, PW, fast()).unwrap();
    let mut items = vec![];
    for vname in ["personal", "work"] {
        let vid = v1::create_vault_fixture(r, PW, &sk, vname).unwrap();
        for i in 0..3 {
            let it = item(&format!("{vname}-{i}"), &"x".repeat(i * 300 + 1));
            let iid = v1::add_item_fixture(r, PW, &sk, &vid, &it).unwrap();
            items.push((vid.clone(), iid, it));
        }
    }
    (d, sk, items)
}
fn check_roundtrip(r: &Path, sk: &SecretKey, items: &[(String, String, Item)]) {
    let s = unlock(r, PW, sk).unwrap();
    assert_eq!(s.fmt.version, 2);
    let names: Vec<String> = s.list_vaults().unwrap().into_iter().map(|(_, m)| m.name.clone()).collect();
    assert_eq!(names, vec!["personal", "work"]);
    for (vid, iid, it) in items { assert_eq!(&s.read_item(vid, iid).unwrap().1, it); }
    for (vid, _, _) in items { assert!(!r.join("vaults").join(vid).join("meta.json").exists()); }
}

#[test]
fn migrate_v1_to_v2_roundtrip_with_git_commit() {
    let (d, sk, items) = v1_fixture();
    let r = d.path();
    let git = |a: &[&str]| std::process::Command::new("git").arg("-C").arg(r).args(a).output().unwrap();
    git(&["init", "-q"]); git(&["-c", "user.email=t@t", "-c", "user.name=t", "add", "-A"]);
    git(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "v1"]);
    // v2 code refuses to unlock v1 without migrating
    assert!(unlock(r, PW, &sk).unwrap_err().to_string().contains("migrate"));
    // wrong password: migration refuses and doesn't touch anything
    assert!(migrate::migrate(r, "wrong pw!!", &sk, MigrateOpts::default()).is_err());
    std::env::set_var("GIT_AUTHOR_NAME", "t"); std::env::set_var("GIT_AUTHOR_EMAIL", "t@t");
    std::env::set_var("GIT_COMMITTER_NAME", "t"); std::env::set_var("GIT_COMMITTER_EMAIL", "t@t");
    let b = migrate::migrate(r, PW, &sk, MigrateOpts::default()).unwrap();
    assert!(b.join("format.json").exists());
    let fb: Format = serde_json::from_slice(&fs::read(b.join("format.json")).unwrap()).unwrap();
    assert_eq!(fb.version, 1);
    check_roundtrip(r, &sk, &items);
    let log = String::from_utf8(git(&["log", "--format=%s"]).stdout).unwrap();
    assert_eq!(log.lines().next().unwrap(), "migrate to format v2");
    let tracked = String::from_utf8(git(&["ls-files"]).stdout).unwrap();
    assert!(!tracked.contains(".gitpass-backup") && !tracked.contains(".gitpass-state") && tracked.contains("manifest.enc"));
    // migrating twice is refused
    assert!(migrate::migrate(r, PW, &sk, MigrateOpts::default()).unwrap_err().to_string().contains("already"));
}

#[test]
fn interrupted_migration_is_recoverable_at_every_step() {
    for step in 1..=3u8 {
        let (d, sk, items) = v1_fixture();
        let r = d.path();
        let e = migrate::migrate(r, PW, &sk, MigrateOpts { fail_after: Some(step) }).unwrap_err();
        assert!(e.to_string().contains("injected"));
        // format.json is written last, so the repo still says v1 and v2 unlock refuses
        let f: Format = serde_json::from_slice(&fs::read(r.join("format.json")).unwrap()).unwrap();
        assert_eq!(f.version, 1, "step {step}");
        assert!(unlock(r, PW, &sk).is_err());
        // re-running migrate recovers and completes
        migrate::migrate(r, PW, &sk, MigrateOpts::default()).unwrap();
        check_roundtrip(r, &sk, &items);
        assert!(!r.join(".gitpass-migrate-tmp").exists() && !r.join(".gitpass-migrating").exists());
    }
}

// ---------------- tamper / rollback detection ----------------
#[test]
fn tamper_delete_item_file() {
    let (d, sk, vid, iid) = setup();
    fs::remove_file(ipath(d.path(), &vid, &iid)).unwrap();
    assert_refused(d.path(), &sk);
}

#[test]
fn tamper_restore_older_item_file() {
    let (d, sk, vid, iid) = setup();
    let old = fs::read(ipath(d.path(), &vid, &iid)).unwrap();
    unlock(d.path(), PW, &sk).unwrap().update_item(&vid, &iid, &item("GitHub", "p2")).unwrap();
    fs::write(ipath(d.path(), &vid, &iid), old).unwrap();
    assert_refused(d.path(), &sk);
}

#[test]
fn tamper_extra_item_file_injected() {
    let (d, sk, vid, iid) = setup();
    fs::copy(ipath(d.path(), &vid, &iid), ipath(d.path(), &vid, "deadbeef")).unwrap();
    assert_refused(d.path(), &sk);
}

#[test]
fn tamper_swap_keyset_for_older() {
    let (d, sk, _, _) = setup();
    let r = d.path();
    let old_ks = fs::read(r.join("keyset.enc")).unwrap();
    let old_fmt = fs::read(r.join("format.json")).unwrap();
    unlock(r, PW, &sk).unwrap().change_password("new password 1", &sk, fast()).unwrap();
    // old keyset alone (with new format.json): refused
    fs::write(r.join("keyset.enc"), &old_ks).unwrap();
    assert!(unlock(r, PW, &sk).is_err());
    assert!(unlock(r, "new password 1", &sk).is_err());
    // old keyset + old format.json (full key-file rollback): AK opens, but manifest keyset hash mismatches
    fs::write(r.join("format.json"), &old_fmt).unwrap();
    let e = unlock(r, PW, &sk).err().unwrap().to_string();
    assert!(e.contains("keyset.enc"), "{e}");
}

#[test]
fn tamper_flip_byte_in_manifest() {
    let (d, sk, _, _) = setup();
    let p = d.path().join("manifest.enc");
    let mut env: Envelope = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    let mut ct = B64.decode(&env.ct).unwrap();
    ct[10] ^= 1;
    env.ct = B64.encode(ct);
    fs::write(&p, serde_json::to_vec(&env).unwrap()).unwrap();
    assert_refused(d.path(), &sk);
    // even --accept-rollback cannot bypass an unauthenticated manifest
    assert!(Session::unlock_with(d.path(), PW, &sk, &UnlockOpts { accept_rollback: true, target_kdf: fast() }).is_err());
}

#[test]
fn tamper_lower_kdf_params() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let strong = KdfParams { alg: "argon2id".into(), m_kib: 16384, t: 2, p: 1 };
    let sk = store::init(r, PW, strong.clone()).unwrap();
    let o = UnlockOpts { accept_rollback: false, target_kdf: strong.clone() };
    Session::unlock_with(r, PW, &sk, &o).unwrap();
    let mut f: Format = serde_json::from_slice(&fs::read(r.join("format.json")).unwrap()).unwrap();
    f.kdf = fast();
    fs::write(r.join("format.json"), serde_json::to_vec(&f).unwrap()).unwrap();
    assert!(Session::unlock_with(r, PW, &sk, &o).is_err());
    assert!(Session::unlock_with(r, PW, &sk, &opts()).is_err());
    // below the absolute floor is rejected outright
    f.kdf.m_kib = 64;
    fs::write(r.join("format.json"), serde_json::to_vec(&f).unwrap()).unwrap();
    assert!(Session::unlock_with(r, PW, &sk, &o).unwrap_err().to_string().contains("floor"));
}

#[test]
fn whole_repo_rollback_detected_by_epoch_and_tofu_limit() {
    let (d, sk, vid, _) = setup();
    let r = d.path();
    let snap = tempfile::tempdir().unwrap();
    gitpass::fsutil::copy_dir(r, snap.path(), &|n| n == ".gitpass-state.json").unwrap();
    unlock(r, PW, &sk).unwrap().add_item(&vid, &item("New", "p3")).unwrap();
    // replace entire repo content (self-consistent older snapshot) but keep local state
    for n in ["vaults", "keyset.enc", "manifest.enc", "format.json"] {
        let p = r.join(n); if p.is_dir() { fs::remove_dir_all(&p).unwrap() } else { fs::remove_file(&p).unwrap() }
    }
    gitpass::fsutil::copy_dir(snap.path(), r, &|_| false).unwrap();
    let e = unlock(r, PW, &sk).err().unwrap().to_string();
    assert!(e.contains("epoch"), "{e}");
    // --accept-rollback overrides, and epoch moves forward past the last-seen one
    let last = store::last_epoch(r, &unlock_any(r, &sk).fmt.account_id).unwrap();
    assert!(last > 3);
    // TOFU: a "fresh device" (no state file) accepts the old snapshot silently -- documented limitation
    fs::remove_file(r.join(".gitpass-state.json")).unwrap();
    let snap2 = tempfile::tempdir().unwrap();
    gitpass::fsutil::copy_dir(snap.path(), snap2.path(), &|_| false).unwrap();
    assert!(unlock(snap2.path(), PW, &sk).is_ok());
}
fn unlock_any(r: &Path, sk: &SecretKey) -> Session {
    Session::unlock_with(r, PW, sk, &UnlockOpts { accept_rollback: true, target_kdf: fast() }).unwrap()
}

#[test]
fn every_mutation_bumps_epoch() {
    let (d, sk, vid, iid) = setup();
    let mut s = unlock(d.path(), PW, &sk).unwrap();
    let mut e = s.epoch().unwrap();
    let mut step = |s: &Session| { let n = s.epoch().unwrap(); assert!(n > e); e = n; };
    s.update_item(&vid, &iid, &item("GitHub", "p9")).unwrap(); step(&s);
    s.add_item(&vid, &item("x", "y")).unwrap(); step(&s);
    s.remove_item(&vid, &iid).unwrap(); step(&s);
    s.create_vault("other").unwrap(); step(&s);
    s.rotate_vault(&vid).unwrap(); step(&s);
    s.change_password(PW, &sk, fast()).unwrap(); step(&s);
    s.rotate_account_key(PW, &sk).unwrap(); step(&s);
}

// ---------------- KDF auto-upgrade ----------------
#[test]
fn kdf_auto_upgrade_on_unlock() {
    let (d, sk, vid, iid) = setup();
    let r = d.path();
    let target = KdfParams { alg: "argon2id".into(), m_kib: 16384, t: 2, p: 1 };
    let before = unlock(r, PW, &sk).unwrap().epoch().unwrap();
    let s = Session::unlock_with(r, PW, &sk, &UnlockOpts { accept_rollback: false, target_kdf: target.clone() }).unwrap();
    assert!(s.kdf_upgraded);
    assert!(s.epoch().unwrap() > before);
    let f: Format = serde_json::from_slice(&fs::read(r.join("format.json")).unwrap()).unwrap();
    assert_eq!((f.kdf.m_kib, f.kdf.t), (16384, 2));
    // still opens, no second upgrade
    let s2 = Session::unlock_with(r, PW, &sk, &UnlockOpts { accept_rollback: false, target_kdf: target }).unwrap();
    assert!(!s2.kdf_upgraded);
    assert_eq!(s2.read_item(&vid, &iid).unwrap().1.password.as_deref(), Some("p1"));
}

#[test]
fn interrupted_rewrap_rolls_forward() {
    let (d, sk, vid, iid) = setup();
    let r = d.path();
    // simulate crash after writing format.json.new + keyset.enc.new (as passwd would)
    let tmp = tempfile::tempdir().unwrap();
    gitpass::fsutil::copy_dir(r, tmp.path(), &|_| false).unwrap();
    unlock(tmp.path(), PW, &sk).unwrap().change_password("brand new pw", &sk, fast()).unwrap();
    fs::copy(tmp.path().join("format.json"), r.join("format.json.new")).unwrap();
    fs::copy(tmp.path().join("keyset.enc"), r.join("keyset.enc.new")).unwrap();
    let s = unlock(r, "brand new pw", &sk).unwrap();
    assert_eq!(s.read_item(&vid, &iid).unwrap().1.password.as_deref(), Some("p1"));
    assert!(!r.join("format.json.new").exists());
}

// ---------------- rotation ----------------
#[test]
fn passwd_old_fails_new_works() {
    let (d, sk, vid, iid) = setup();
    let mut s = unlock(d.path(), PW, &sk).unwrap();
    s.change_password("another password", &sk, fast()).unwrap();
    assert!(unlock(d.path(), PW, &sk).is_err());
    let s = unlock(d.path(), "another password", &sk).unwrap();
    assert_eq!(s.read_item(&vid, &iid).unwrap().1.password.as_deref(), Some("p1"));
}

#[test]
fn rotate_secret_key_old_fails_new_works() {
    let (d, sk, vid, iid) = setup();
    let mut s = unlock(d.path(), PW, &sk).unwrap();
    let nsk = s.rotate_secret_key(PW, fast()).unwrap();
    assert!(unlock(d.path(), PW, &sk).is_err());
    assert_eq!(unlock(d.path(), PW, &nsk).unwrap().read_item(&vid, &iid).unwrap().1.password.as_deref(), Some("p1"));
}

#[test]
fn rotate_account_key_old_keyset_cannot_open_new_vault_keys() {
    let (d, sk, vid, iid) = setup();
    let r = d.path();
    let old_ks = fs::read(r.join("keyset.enc")).unwrap(); // what an attacker has from git history
    let old_fmt: Format = serde_json::from_slice(&fs::read(r.join("format.json")).unwrap()).unwrap();
    let mut s = unlock(r, PW, &sk).unwrap();
    let old_ak = Key32(s.account_key_bytes_for_tests());
    s.rotate_account_key(PW, &sk).unwrap();
    assert_ne!(s.account_key_bytes_for_tests(), old_ak.0);
    // attacker opens the OLD account key from history with leaked pw+sk...
    let auk = derive_auk(PW, &sk, &B64.decode(&old_fmt.salt).unwrap(), &old_fmt.account_id, &old_fmt.kdf).unwrap();
    let env: Envelope = serde_json::from_slice(&old_ks).unwrap();
    let ak = unwrap_key(&auk, &env, &store::keyset_aad(&old_fmt)).unwrap();
    assert_eq!(ak.0, old_ak.0);
    // ...but it cannot unwrap the CURRENT vault key file nor the manifest
    let vkf: store::VaultKeyFile = serde_json::from_slice(&fs::read(r.join("vaults").join(&vid).join("vault.key.enc")).unwrap()).unwrap();
    assert!(unwrap_key(&ak, &vkf.envelope, &store::vault_key_aad(&vid, vkf.key_version)).is_err());
    // with vault rotation, old vault keys (from history) can't open current items either
    let old_item = fs::read(ipath(r, &vid, &iid)).unwrap();
    s.rotate_vault(&vid).unwrap();
    assert_ne!(fs::read(ipath(r, &vid, &iid)).unwrap(), old_item);
    let s = unlock(r, PW, &sk).unwrap();
    assert_eq!(s.read_item(&vid, &iid).unwrap().1.password.as_deref(), Some("p1"));
    assert_eq!(s.list_vaults().unwrap()[0].1.name, "personal");
}

// ---------------- padding ----------------
#[test]
fn padding_hides_length_within_bucket() {
    let (d, sk, vid, _) = setup();
    let s = unlock(d.path(), PW, &sk).unwrap();
    let a = s.add_item(&vid, &item("a", "x")).unwrap();
    let b = s.add_item(&vid, &item("bbbbbbbbbbbb", &"y".repeat(200))).unwrap();
    let c = s.add_item(&vid, &item("c", &"z".repeat(3000))).unwrap();
    let len = |i: &str| fs::metadata(ipath(d.path(), &vid, i)).unwrap().len();
    assert_eq!(len(&a), len(&b));
    assert!(len(&c) > len(&a));
    assert_eq!([store::bucket(0), store::bucket(508), store::bucket(509), store::bucket(4092), store::bucket(4093)], [512, 512, 1024, 4096, 8192]);
}

#[test]
fn vault_description_encrypted() {
    let (d, sk, _, _) = setup();
    let s = unlock(d.path(), PW, &sk).unwrap();
    let v = s.create_vault_with("Secret Project", Some("top secret desc".into())).unwrap();
    let raw = fs::read_to_string(d.path().join("vaults").join(&v).join("meta.enc")).unwrap();
    assert!(!raw.contains("Secret") && !raw.contains("desc"));
    assert_eq!(s.vault_meta(&v).unwrap().description.as_deref(), Some("top secret desc"));
}

#[test]
fn refuses_newer_format() {
    let (d, sk, _, _) = setup();
    let mut f: Format = serde_json::from_slice(&fs::read(d.path().join("format.json")).unwrap()).unwrap();
    f.version = 3;
    fs::write(d.path().join("format.json"), serde_json::to_vec(&f).unwrap()).unwrap();
    assert!(unlock(d.path(), PW, &sk).unwrap_err().to_string().contains("newer"));
}

// ---------------- proptest ----------------
mod prop {
    use super::*;
    use proptest::prelude::*;
    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]
        #[test]
        fn pad_roundtrip(data in proptest::collection::vec(any::<u8>(), 0..10000)) {
            let p = store::pad(&data);
            prop_assert_eq!(p.len(), store::bucket(data.len()));
            prop_assert_eq!(store::unpad(&p).unwrap(), data);
        }
        #[test]
        fn item_roundtrip_through_store(title in ".{0,64}", user in proptest::option::of(".{0,64}"),
                                        pw in proptest::option::of(".{0,200}"), notes in proptest::option::of(".{0,3000}")) {
            thread_local!(static S: (tempfile::TempDir, SecretKey, String) = {
                let (d, sk, vid, _) = setup(); (d, sk, vid)
            });
            S.with(|(d, sk, vid)| {
                let s = unlock(d.path(), PW, sk).unwrap();
                let it = Item { title: title.clone(), username: user.clone(), password: pw.clone(), url: None,
                    notes: notes.clone(), totp: None, updated_at: "0".into() };
                let iid = s.add_item(vid, &it).unwrap();
                assert_eq!(s.read_item(vid, &iid).unwrap().1, it);
            });
        }
    }
}
