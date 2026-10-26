//! Format v4 key features: real device revocation (key rotation) and X25519 vault sharing.
use gitpass_core::crypto::*;
use gitpass_core::device;
use gitpass_core::keys;
use gitpass_core::store::{self, Item, ItemFile, MemberFile, Session, UnlockOpts};
use std::fs;
use std::path::Path;

const PW: &str = "correct horse battery";
fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn opts() -> UnlockOpts { UnlockOpts { accept_rollback: false, target_kdf: fast() } }
fn unlock(root: &Path, sk: &SecretKey, name: &str) -> anyhow::Result<Session> { Session::unlock_device(root, PW, sk, &opts(), Some(name)) }
fn item(t: &str, pw: &str) -> Item { let mut i = Item::default(); i.title = t.into(); i.password = Some(pw.into()); i.updated_at = "1".into(); i }
fn item_file(root: &Path, vid: &str, iid: &str) -> ItemFile {
    serde_json::from_slice(&fs::read(root.join("vaults").join(vid).join("items").join(format!("{iid}.enc"))).unwrap()).unwrap()
}

#[test]
fn sealed_box_roundtrip_and_binding() {
    let a = keys::KeyPair::generate();
    let b = keys::KeyPair::generate();
    let bx = keys::seal_to(&a.public, b"hello", b"aad").unwrap();
    assert_eq!(&keys::open_sealed(&a, &bx, b"aad").unwrap()[..], b"hello");
    assert!(keys::open_sealed(&a, &bx, b"other").is_err(), "aad bound");
    assert!(keys::open_sealed(&b, &bx, b"aad").is_err(), "wrong recipient");
    assert!(keys::seal_to(&[0u8; 32], b"x", b"").is_err(), "low-order point refused");
    assert_eq!(keys::parse_pub(&keys::encode_pub(&a.public)).unwrap(), a.public);
    assert_eq!(keys::fingerprint(&a.public).len(), 32);
}

#[test]
fn revoke_rotates_account_and_vault_keys() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let sk = store::init(r, PW, fast()).unwrap();
    // device A ("stolen") sets up v4 keys, then device B revokes it
    let a = unlock(r, &sk, "stolen").unwrap();
    a.ensure_identity().unwrap();
    let a_id = a.device_id.clone();
    let vid = a.create_vault("personal").unwrap();
    let old_iid = a.add_item(&vid, &item("old", "p1")).unwrap();
    let old_vk = Key32(a.vault_key_bytes_for_tests(&vid).unwrap());
    let old_ak = a.account_key_bytes_for_tests();
    drop(a);
    fs::remove_file(r.join(device::LOCAL_DEVICE_FILE)).unwrap();
    let mut b = unlock(r, &sk, "good").unwrap();
    let reg = b.devices().unwrap();
    assert_eq!(reg.format, device::DEVICES_FORMAT, "registry v4 once devices carry public keys");
    assert!(reg.get(&b.device_id).unwrap().public_key.is_some());
    let epoch0 = b.epoch().unwrap();
    assert!(b.revoke_device_and_rotate(&b.device_id.clone(), PW, &sk).is_err(), "cannot rotate-revoke self");
    let rep = b.revoke_device_and_rotate(&a_id[..8], PW, &sk).unwrap();
    assert_eq!(rep.device_id, a_id);
    assert_eq!(rep.vaults_rotated, vec![vid.clone()]);
    assert!(b.epoch().unwrap() > epoch0 + 2);
    assert!(!r.join(store::REVOKE_PENDING_FILE).exists());
    assert!(b.devices().unwrap().get(&a_id).unwrap().revoked);
    assert_ne!(b.account_key_bytes_for_tests(), old_ak, "account key rotated");
    assert_ne!(b.vault_key_bytes_for_tests(&vid).unwrap(), old_vk.0, "vault key rotated");
    // new item: old vault key can no longer decrypt it; old items were re-encrypted too
    let new_iid = b.add_item(&vid, &item("new", "p2")).unwrap();
    for iid in [&new_iid, &old_iid] {
        let f = item_file(r, &vid, iid);
        assert!(open(&old_vk, &f.envelope, &store::item_aad(&vid, iid, f.version)).is_err());
    }
    // old account key can no longer unwrap the vault key file
    let kf: store::VaultKeyFile = serde_json::from_slice(&fs::read(r.join("vaults").join(&vid).join("vault.key.enc")).unwrap()).unwrap();
    assert!(unwrap_key(&Key32(old_ak), &kf.envelope, &store::vault_key_aad(&vid, kf.key_version)).is_err());
    assert_eq!(b.read_item(&vid, &old_iid).unwrap().1.password.as_deref(), Some("p1"));
    drop(b);
    // still opens from a fresh unlock (integrity verified)
    unlock(r, &sk, "good").unwrap();
}

#[test]
fn rotate_credentials_invalidates_old_password_and_secret() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let sk = store::init(r, PW, fast()).unwrap();
    let mut s = unlock(r, &sk, "a").unwrap();
    s.ensure_identity().unwrap();
    let nsk = s.rotate_credentials("brand new password", fast()).unwrap();
    drop(s);
    assert!(unlock(r, &sk, "a").is_err());
    let s = Session::unlock_device(r, "brand new password", &nsk, &opts(), Some("a")).unwrap();
    s.device_public_key().unwrap();
}

#[test]
fn interrupted_revoke_is_rolled_forward_on_unlock() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    let sk = store::init(r, PW, fast()).unwrap();
    let a = unlock(r, &sk, "victim").unwrap();
    let a_id = a.device_id.clone();
    let vid = a.create_vault("v").unwrap();
    let vk0 = a.vault_key_bytes_for_tests(&vid).unwrap();
    drop(a);
    fs::remove_file(r.join(device::LOCAL_DEVICE_FILE)).unwrap();
    // simulate a crash right after the pending marker was written
    unlock(r, &sk, "b").unwrap();
    fs::write(r.join(store::REVOKE_PENDING_FILE), format!("{{\"device_id\":\"{a_id}\"}}")).unwrap();
    let b = unlock(r, &sk, "b").unwrap();
    assert!(!r.join(store::REVOKE_PENDING_FILE).exists());
    assert!(b.devices().unwrap().get(&a_id).unwrap().revoked);
    assert_ne!(b.vault_key_bytes_for_tests(&vid).unwrap(), vk0);
}

/// Two independent accounts; returns (dir, sk, session) for each.
fn account(name: &str) -> (tempfile::TempDir, SecretKey, Session) {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    let s = unlock(d.path(), &sk, name).unwrap();
    (d, sk, s)
}

#[test]
fn share_accept_round_trip_between_accounts() {
    let (da, ska, alice) = account("alice");
    let (db, _skb, bob) = account("bob");
    let vid = alice.create_vault("team").unwrap();
    let iid = alice.add_item(&vid, &item("db", "s3cret")).unwrap();
    // bob publishes identity.pub; alice shares to the file path
    let (bpub, bfpr) = bob.identity().unwrap();
    let pubfile = db.path().join(store::IDENTITY_PUB_FILE);
    assert_eq!(keys::load_pub(pubfile.to_str().unwrap()).unwrap(), bpub);
    let share = alice.share_vault(&vid, &bpub).unwrap();
    assert!(share.ends_with(format!("vaults/{vid}/members/{bfpr}.enc")));
    let m = alice.load_manifest().unwrap();
    assert_eq!(m.format, store::MANIFEST_FORMAT_V4);
    assert!(m.vaults[&vid].members.contains_key(&bfpr), "members covered by manifest");
    assert!(m.identity.is_some());
    assert_eq!(alice.members(&vid).unwrap().len(), 1);
    // bob accepts
    let got = bob.accept_share(&share).unwrap();
    assert_eq!(got, vid);
    assert_eq!(bob.read_item(&vid, &iid).unwrap().1.password.as_deref(), Some("s3cret"));
    assert_eq!(bob.vault_key_bytes_for_tests(&vid).unwrap(), alice.vault_key_bytes_for_tests(&vid).unwrap());
    assert!(bob.accept_share(&share).is_err(), "double accept refused");
    // a third account cannot open bob's share
    let (_dc, _skc, carol) = account("carol");
    assert!(carol.accept_share(&share).is_err());
    // both accounts still pass integrity on a fresh unlock
    drop(alice); drop(bob);
    unlock(da.path(), &ska, "alice").unwrap();
}

#[test]
fn unshare_rotates_vault_key_and_keeps_other_members() {
    let (_da, _ska, alice) = account("alice");
    let (_db, _skb, bob) = account("bob");
    let (_dc, _skc, carol) = account("carol");
    let vid = alice.create_vault("team").unwrap();
    alice.add_item(&vid, &item("x", "1")).unwrap();
    let (bpub, bfpr) = bob.identity().unwrap();
    let (cpub, cfpr) = carol.identity().unwrap();
    let bshare = alice.share_vault(&vid, &bpub).unwrap();
    let old_bshare = fs::read(&bshare).unwrap();
    alice.share_vault(&vid, &cpub).unwrap();
    let vk0 = alice.vault_key_bytes_for_tests(&vid).unwrap();
    assert_eq!(alice.unshare_vault(&vid, &bfpr[..10]).unwrap(), bfpr);
    let vk1 = alice.vault_key_bytes_for_tests(&vid).unwrap();
    assert_ne!(vk0, vk1, "vault key rotated on unshare");
    assert!(!bshare.exists());
    let mem: Vec<String> = alice.members(&vid).unwrap().into_iter().map(|(f, _)| f).collect();
    assert_eq!(mem, vec![cfpr.clone()]);
    // bob's stale share only yields the OLD key, which cannot read new items
    let new_iid = alice.add_item(&vid, &item("after", "2")).unwrap();
    let f = item_file(&alice.root, &vid, &new_iid);
    assert!(open(&Key32(vk0), &f.envelope, &store::item_aad(&vid, &new_iid, f.version)).is_err());
    let _ = old_bshare;
    // carol's re-sealed share carries the NEW key
    let cshare = alice.root.join("vaults").join(&vid).join("members").join(format!("{cfpr}.enc"));
    carol.accept_share(&cshare).unwrap();
    assert_eq!(carol.vault_key_bytes_for_tests(&vid).unwrap(), vk1);
    assert_eq!(carol.read_item(&vid, &new_iid).unwrap().1.password.as_deref(), Some("2"));
}

#[test]
fn tampered_member_file_rejected() {
    let (da, ska, alice) = account("alice");
    let (_db, _skb, bob) = account("bob");
    let vid = alice.create_vault("team").unwrap();
    let (bpub, _) = bob.identity().unwrap();
    let share = alice.share_vault(&vid, &bpub).unwrap();
    let orig = fs::read(&share).unwrap();
    // 1) flip a ciphertext byte
    let mut mf: MemberFile = serde_json::from_slice(&orig).unwrap();
    let mut ct = base64_decode(&mf.sealed.envelope.ct); ct[0] ^= 1; mf.sealed.envelope.ct = base64_encode(&ct);
    fs::write(&share, serde_json::to_vec_pretty(&mf).unwrap()).unwrap();
    assert!(bob.accept_share(&share).is_err());
    // 2) change authenticated header (key_version) without touching ciphertext
    let mut mf: MemberFile = serde_json::from_slice(&orig).unwrap();
    mf.key_version += 1;
    fs::write(&share, serde_json::to_vec_pretty(&mf).unwrap()).unwrap();
    assert!(bob.accept_share(&share).is_err());
    // 3) the owner's manifest also detects the modified member file
    drop(alice);
    let e = unlock(da.path(), &ska, "alice").unwrap_err().to_string();
    assert!(e.contains("members") && e.contains("does not match manifest"), "{e}");
    // 4) an extra, unlisted member file is flagged too
    fs::write(&share, &orig).unwrap();
    unlock(da.path(), &ska, "alice").unwrap();
    fs::write(share.with_file_name("ffffffffffffffffffffffffffffffff.enc"), &orig).unwrap();
    assert!(unlock(da.path(), &ska, "alice").unwrap_err().to_string().contains("unexpected member"));
}

fn base64_decode(s: &str) -> Vec<u8> { use base64::Engine; base64::engine::general_purpose::STANDARD.decode(s).unwrap() }
fn base64_encode(b: &[u8]) -> String { use base64::Engine; base64::engine::general_purpose::STANDARD.encode(b) }

#[test]
fn v3_accounts_stay_v3_until_identity_created() {
    let (_d, _sk, s) = account("a");
    assert_eq!(s.load_manifest().unwrap().format, store::MANIFEST_FORMAT_V3);
    assert_eq!(s.devices().unwrap().format, device::DEVICES_FORMAT_V3);
    assert!(!s.root.join(store::IDENTITY_PUB_FILE).exists());
    s.identity().unwrap();
    assert_eq!(s.load_manifest().unwrap().format, store::MANIFEST_FORMAT_V4);
    assert!(fs::read_to_string(s.root.join(".gitignore")).unwrap().contains(device::LOCAL_DEVICE_KEY_FILE));
}
