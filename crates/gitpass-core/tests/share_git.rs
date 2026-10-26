//! End-to-end vault sharing + revocation over a real (local, bare) git remote between two
//! independent accounts with separate temp vault dirs. All data is FAKE.
//!
//! Steps (mirrors docs in README "Verifying sharing and revocation"):
//! 1. init account A and account B in separate temp dirs; create a bare repo as A's remote.
//! 2. B publishes identity.pub; A loads it, shares vault "team" to it, `remote add` + sync (push).
//! 3. B clones the bare repo (read-only view of A's checkout) and accepts `vaults/<id>/members/<B>.enc`.
//!    B reads A's item.
//! 4. A unshares B (vault key rotates), adds a new item, syncs (push). B pulls its clone.
//! 5. B's member file is gone; B's held (old) key cannot open the new item or the re-encrypted old one;
//!    nor the rotated vault meta; key_version increased and the vault key changed.
use gitpass_core::crypto::*;
use gitpass_core::store::{self, Item, ItemFile, Session, UnlockOpts};
use gitpass_core::{keys, remote, sync};
use std::fs;
use std::path::Path;
use std::process::Command;

const PW: &str = "correct horse battery";
fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn opts() -> UnlockOpts { UnlockOpts { accept_rollback: false, target_kdf: fast() } }
fn item(t: &str, pw: &str) -> Item { let mut i = Item::default(); i.title = t.into(); i.password = Some(pw.into()); i.updated_at = "1".into(); i }
fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").arg("-C").arg(dir).args(args)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@example.invalid").output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).to_string()
}
fn account(name: &str) -> (tempfile::TempDir, SecretKey, Session) {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    let s = Session::unlock_device(d.path(), PW, &sk, &opts(), Some(name)).unwrap();
    (d, sk, s)
}
fn item_file(vdir: &Path, iid: &str) -> ItemFile {
    serde_json::from_slice(&fs::read(vdir.join("items").join(format!("{iid}.enc"))).unwrap()).unwrap()
}

#[test]
fn share_then_revoke_over_bare_git_remote() {
    if !remote::git_available() { eprintln!("git not installed; skipping"); return; }
    let (da, ska, a) = account("alice-mac");
    let (db, _skb, b) = account("bob-mac");
    let bare = tempfile::tempdir().unwrap();
    git(bare.path(), &["init", "--bare", "-q", "-b", "main"]);
    let url = bare.path().to_str().unwrap().to_string();

    // 2. share
    let vid = a.create_vault("team").unwrap();
    let iid1 = a.add_item(&vid, &item("db", "s3cret-FAKE")).unwrap();
    let (bpub_real, bfpr) = b.identity().unwrap();
    let bpub = keys::load_pub(db.path().join(store::IDENTITY_PUB_FILE).to_str().unwrap()).unwrap();
    assert_eq!(bpub, bpub_real);
    a.share_vault(&vid, &bpub).unwrap();
    let kv0 = a.load_manifest().unwrap().vaults[&vid].key_version;
    let vk0 = a.vault_key_bytes_for_tests(&vid).unwrap();
    remote::add(da.path(), &url, true).unwrap();
    sync::sync(da.path()).unwrap();

    // 3. B clones and accepts
    let clone_parent = tempfile::tempdir().unwrap();
    let clone = clone_parent.path().join("alice-vault");
    git(clone_parent.path(), &["clone", "-q", &url, clone.to_str().unwrap()]);
    let share = clone.join("vaults").join(&vid).join("members").join(format!("{bfpr}.enc"));
    assert!(share.exists(), "member file was pushed to the remote");
    assert!(!clone.join("secret.key").exists(), "secret key never pushed");
    let stale_share = fs::read(&share).unwrap();
    assert_eq!(b.accept_share(&share).unwrap(), vid);
    assert_eq!(b.read_item(&vid, &iid1).unwrap().1.password.as_deref(), Some("s3cret-FAKE"));
    assert_eq!(b.vault_key_bytes_for_tests(&vid).unwrap(), vk0);

    // 4. revoke + new item + push
    assert_eq!(a.unshare_vault(&vid, &bfpr).unwrap(), bfpr);
    let iid2 = a.add_item(&vid, &item("after-revoke", "new-FAKE")).unwrap();
    let m = a.load_manifest().unwrap();
    assert!(m.vaults[&vid].key_version > kv0, "key_version bumped");
    let vk1 = a.vault_key_bytes_for_tests(&vid).unwrap();
    assert_ne!(vk0, vk1, "vault key rotated");
    sync::sync(da.path()).unwrap();
    git(&clone, &["pull", "-q", "--rebase", "origin", "main"]);

    // 5. B is locked out of everything new
    assert!(!share.exists(), "B's member file removed from the remote");
    let vdir = clone.join("vaults").join(&vid);
    let held = Key32(b.vault_key_bytes_for_tests(&vid).unwrap());
    let f2 = item_file(&vdir, &iid2);
    assert!(open(&held, &f2.envelope, &store::item_aad(&vid, &iid2, f2.version)).is_err(), "B cannot decrypt new item");
    let f1 = item_file(&vdir, &iid1);
    assert!(open(&held, &f1.envelope, &store::item_aad(&vid, &iid1, f1.version)).is_err(), "old item re-encrypted under new key");
    assert!(open(&Key32(vk1), &f2.envelope, &store::item_aad(&vid, &iid2, f2.version)).is_ok());
    // the rotated vault metadata is also unreadable with the held key
    let menv: Envelope = serde_json::from_slice(&fs::read(vdir.join("meta.enc")).unwrap()).unwrap();
    assert!(open(&held, &menv, &store::meta_aad(&vid)).is_err(), "B cannot read rotated vault meta");
    let _ = stale_share;
    // A still unlocks cleanly (manifest integrity + epoch)
    drop(a);
    Session::unlock_device(da.path(), PW, &ska, &opts(), Some("alice-mac")).unwrap();
}
