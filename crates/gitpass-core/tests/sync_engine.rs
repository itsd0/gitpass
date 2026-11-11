//! Sync engine end-to-end tests (issue #452): two replicas of one account
//! diverge, plan, apply, and converge — plus a [`MemoryRemoteRepo`] round
//! trip of the merged tree. All vaults are temp dirs with fake data.
use gitpass_core::crypto::*;
use gitpass_core::store::{self, Item, Session};
use gitpass_core::sync_engine::{plan_sync, MemoryRemoteRepo, RemoteRepo};
use std::fs;
use std::path::Path;

const PW: &str = "correct horse battery";
fn fast() -> KdfParams {
    KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 }
}
fn item(t: &str, pw: &str) -> Item {
    let mut i = Item::default();
    i.title = t.into();
    i.password = Some(pw.into());
    i.updated_at = "1700000000".into();
    i
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for e in fs::read_dir(src).unwrap().filter_map(|e| e.ok()) {
        let (s, d) = (e.path(), dst.join(e.file_name()));
        if e.file_type().unwrap().is_dir() {
            copy_dir(&s, &d);
        } else {
            fs::copy(&s, &d).unwrap();
        }
    }
}

/// All vault files (allowed paths only) as (repo-relative path, bytes).
fn vault_files(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
            let p = e.path();
            if e.file_type().unwrap().is_dir() {
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(root) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if gitpass_core::sync_engine::is_allowed_path(&rel) {
                    out.push((rel, fs::read(&p).unwrap()));
                }
            }
        }
    }
    out.sort();
    out
}

#[test]
fn diverged_replicas_converge_through_plan_and_apply() {
    let d = tempfile::tempdir().unwrap();
    let a_root = d.path().join("a");
    fs::create_dir_all(&a_root).unwrap();
    let sk = store::init(&a_root, PW, fast()).unwrap();
    let s = Session::unlock(&a_root, PW, &sk).unwrap();
    let vid = s.create_vault("Personal").unwrap();
    let i1 = s.add_item(&vid, &item("One", "pw-one")).unwrap();
    let i2 = s.add_item(&vid, &item("Two", "pw-two")).unwrap();
    drop(s);
    // Base snapshot: the last-synced state both sides descend from.
    let base = Session::unlock_readonly(&a_root, PW, &sk).unwrap().snapshot_sync().unwrap();

    // Replica B: a file copy of the same account.
    let b_root = d.path().join("b");
    copy_dir(&a_root, &b_root);

    // A: change item 1's password, add item 3.
    let s = Session::unlock(&a_root, PW, &sk).unwrap();
    let mut one = s.read_item(&vid, &i1).unwrap().1;
    one.password = Some("pw-one-new".into());
    s.update_item(&vid, &i1, &one).unwrap();
    let _i3 = s.add_item(&vid, &item("Three", "pw-three")).unwrap();
    drop(s);

    // B: change item 1's title (a different field), hard-delete item 2, add item 4.
    let s = Session::unlock(&b_root, PW, &sk).unwrap();
    let mut one_b = s.read_item(&vid, &i1).unwrap().1;
    one_b.title = "One (renamed)".into();
    s.update_item(&vid, &i1, &one_b).unwrap();
    s.remove_item(&vid, &i2).unwrap();
    let _i4 = s.add_item(&vid, &item("Four", "pw-four")).unwrap();
    drop(s);

    let a = Session::unlock_readonly(&a_root, PW, &sk).unwrap().snapshot_sync().unwrap();
    let b = Session::unlock_readonly(&b_root, PW, &sk).unwrap().snapshot_sync().unwrap();
    let plan = plan_sync(&a, &base, &b, store::now_secs()).unwrap();
    assert!(plan.changed);
    assert!(plan.local_changes);
    assert!(plan.conflicts.is_empty(), "conflicts: {:?}", plan.conflicts.iter().map(|c| c.describe()).collect::<Vec<_>>());
    assert_eq!(plan.new_epoch, a.manifest.epoch.max(b.manifest.epoch) + 1);

    // Apply to A; the merged state must verify clean.
    let s = Session::unlock(&a_root, PW, &sk).unwrap();
    let rep = s.apply_sync_plan(&plan).unwrap();
    assert_eq!(rep.new_epoch, plan.new_epoch);
    assert_eq!(s.epoch().unwrap(), plan.new_epoch);
    let (_, merged_one) = s.read_record(&vid, &i1).unwrap();
    assert_eq!(merged_one.item.title, "One (renamed)"); // remote's field win
    assert_eq!(merged_one.item.password.as_deref(), Some("pw-one-new")); // local's field win
    assert!(s.read_record(&vid, &i2).is_err(), "remote delete must propagate");
    assert_eq!(s.list_items(&vid).unwrap().len(), 3); // 1, 3, 4
    let problems = store::verify(&a_root, &s.load_manifest().unwrap());
    assert!(problems.is_empty(), "verify after apply: {problems:?}");
    drop(s);

    // A second apply of the same plan is refused: the epoch moved on.
    let s = Session::unlock(&a_root, PW, &sk).unwrap();
    assert!(s.apply_sync_plan(&plan).is_err());
    drop(s);

    // Push the merged tree to the in-memory remote and read it back.
    let files = vault_files(&a_root);
    assert!(!files.is_empty());
    let mut repo = MemoryRemoteRepo::new();
    let c1 = repo.create_commit(None, &files, "sync: merged").unwrap();
    repo.update_ref(&c1, None).unwrap();
    let listed = repo.list_tree(&c1).unwrap();
    assert_eq!(listed.len(), files.len());
    for e in &listed {
        let want = files.iter().find(|(p, _)| p == &e.path).expect("listed path must be in the tree");
        assert_eq!(repo.get_blob(&e.sha).unwrap(), want.1);
    }
}

#[test]
fn remote_only_vault_is_adopted() {
    let d = tempfile::tempdir().unwrap();
    let a_root = d.path().join("a");
    fs::create_dir_all(&a_root).unwrap();
    let sk = store::init(&a_root, PW, fast()).unwrap();
    let s = Session::unlock(&a_root, PW, &sk).unwrap();
    let v1 = s.create_vault("Personal").unwrap();
    let _ = s.add_item(&v1, &item("One", "pw-one")).unwrap();
    drop(s);
    let base = Session::unlock_readonly(&a_root, PW, &sk).unwrap().snapshot_sync().unwrap();

    let b_root = d.path().join("b");
    copy_dir(&a_root, &b_root);
    let s = Session::unlock(&b_root, PW, &sk).unwrap();
    let v2 = s.create_vault("Work").unwrap();
    let wi = s.add_item(&v2, &item("Work item", "pw-work")).unwrap();
    drop(s);

    let a = Session::unlock_readonly(&a_root, PW, &sk).unwrap().snapshot_sync().unwrap();
    let b = Session::unlock_readonly(&b_root, PW, &sk).unwrap().snapshot_sync().unwrap();
    let plan = plan_sync(&a, &base, &b, store::now_secs()).unwrap();
    let s = Session::unlock(&a_root, PW, &sk).unwrap();
    let rep = s.apply_sync_plan(&plan).unwrap();
    assert_eq!(rep.vaults_created, 1);
    assert_eq!(s.list_vaults().unwrap().len(), 2);
    let (_, rec) = s.read_record(&v2, &wi).unwrap();
    assert_eq!(rec.item.title, "Work item");
    let problems = store::verify(&a_root, &s.load_manifest().unwrap());
    assert!(problems.is_empty(), "verify after vault adopt: {problems:?}");
}

/// Desktop resurrect regression (`sync_merge` limitation: "an item deleted
/// remotely but still present locally is resurrected"). A soft-deleted item
/// is synced, then purged (hard delete) on replica A; replica B still holds
/// the untouched trashed copy. Planning either direction must keep it gone.
#[test]
fn purged_item_stays_deleted_on_both_replicas() {
    let d = tempfile::tempdir().unwrap();
    let a_root = d.path().join("a");
    fs::create_dir_all(&a_root).unwrap();
    let sk = store::init(&a_root, PW, fast()).unwrap();
    let s = Session::unlock(&a_root, PW, &sk).unwrap();
    let vid = s.create_vault("Personal").unwrap();
    let keep = s.add_item(&vid, &item("Keep", "pw-keep")).unwrap();
    let gone = s.add_item(&vid, &item("Gone", "pw-gone")).unwrap();
    s.soft_delete(&vid, &gone).unwrap();
    drop(s);
    let base = Session::unlock_readonly(&a_root, PW, &sk).unwrap().snapshot_sync().unwrap();
    let b_root = d.path().join("b");
    copy_dir(&a_root, &b_root);

    // A empties the trash.
    let s = Session::unlock(&a_root, PW, &sk).unwrap();
    s.purge_item(&vid, &gone).unwrap();
    drop(s);
    let a = Session::unlock_readonly(&a_root, PW, &sk).unwrap().snapshot_sync().unwrap();
    let b = Session::unlock_readonly(&b_root, PW, &sk).unwrap().snapshot_sync().unwrap();

    // A pulls B: B's stale trashed copy must not come back.
    let plan_a = plan_sync(&a, &base, &b, store::now_secs()).unwrap();
    assert!(plan_a.conflicts.is_empty());
    assert!(
        !plan_a.actions.iter().any(|x| matches!(x, gitpass_core::sync_engine::SyncAction::UpsertItem { item_id, .. } if *item_id == gone)),
        "purged item resurrected: {:?}",
        plan_a.actions
    );

    // B pulls A: the purge propagates.
    let plan_b = plan_sync(&b, &base, &a, store::now_secs()).unwrap();
    let s = Session::unlock(&b_root, PW, &sk).unwrap();
    let rep = s.apply_sync_plan(&plan_b).unwrap();
    assert_eq!(rep.deleted_items, 1);
    assert!(s.read_record(&vid, &gone).is_err(), "purge must propagate to B");
    assert!(s.read_record(&vid, &keep).is_ok());
    assert!(store::verify(&b_root, &s.load_manifest().unwrap()).is_empty());
}

/// A remote vault-key rotation (e.g. device revocation) is adopted even
/// though this replica edited the vault and added an attachment since base:
/// every kept item and the local attachment end up under the new key.
#[test]
fn remote_rotation_is_adopted_over_local_edits_and_rekeys_attachments() {
    let d = tempfile::tempdir().unwrap();
    let a_root = d.path().join("a");
    fs::create_dir_all(&a_root).unwrap();
    let sk = store::init(&a_root, PW, fast()).unwrap();
    let s = Session::unlock(&a_root, PW, &sk).unwrap();
    let vid = s.create_vault("Personal").unwrap();
    let i1 = s.add_item(&vid, &item("One", "pw-one")).unwrap();
    drop(s);
    let base = Session::unlock_readonly(&a_root, PW, &sk).unwrap().snapshot_sync().unwrap();
    let b_root = d.path().join("b");
    copy_dir(&a_root, &b_root);

    // B rotates the vault key (what revoke_device_and_rotate does per vault).
    let s = Session::unlock(&b_root, PW, &sk).unwrap();
    s.rotate_vault(&vid).unwrap();
    drop(s);
    // A edits item 1, adds item 2 and an attachment, all under the old key.
    let s = Session::unlock(&a_root, PW, &sk).unwrap();
    let mut one = s.read_item(&vid, &i1).unwrap().1;
    one.password = Some("pw-one-new".into());
    s.update_item(&vid, &i1, &one).unwrap();
    let i2 = s.add_item(&vid, &item("Two", "pw-two")).unwrap();
    let att = s.add_attachment(&vid, &i2, "note.txt", "text/plain", b"FAKE attachment bytes").unwrap();
    drop(s);

    let a = Session::unlock_readonly(&a_root, PW, &sk).unwrap().snapshot_sync().unwrap();
    let b = Session::unlock_readonly(&b_root, PW, &sk).unwrap().snapshot_sync().unwrap();
    let plan = plan_sync(&a, &base, &b, store::now_secs()).unwrap();
    let s = Session::unlock(&a_root, PW, &sk).unwrap();
    s.apply_sync_plan(&plan).unwrap();
    let m = s.load_manifest().unwrap();
    assert!(store::verify(&a_root, &m).is_empty(), "verify after rotation adopt");
    assert_eq!(
        fs::read(a_root.join("vaults").join(&vid).join("vault.key.enc")).unwrap(),
        fs::read(b_root.join("vaults").join(&vid).join("vault.key.enc")).unwrap(),
        "remote rotation must be adopted"
    );
    assert_eq!(s.read_record(&vid, &i1).unwrap().1.item.password.as_deref(), Some("pw-one-new"));
    assert_eq!(s.read_record(&vid, &i2).unwrap().1.item.title, "Two");
    assert_eq!(s.read_attachment(&vid, &i2, &att.id).unwrap(), b"FAKE attachment bytes");
    drop(s);
    // A fresh unlock reads everything under the new key too.
    let s = Session::unlock_readonly(&a_root, PW, &sk).unwrap();
    assert_eq!(s.read_attachment(&vid, &i2, &att.id).unwrap(), b"FAKE attachment bytes");
    assert_eq!(s.list_items(&vid).unwrap().len(), 2);
}

/// The shared encrypted fixtures (#451) as sync replicas: every version
/// plans as a no-op against itself, and on v5 a delete made on one copy
/// propagates to the other without resurrecting anything.
#[test]
fn shared_fixtures_as_sync_replicas() {
    use gitpass_core::test_fixtures;
    let shared = test_fixtures::shared_fixtures_dir(Path::new(env!("CARGO_MANIFEST_DIR")));
    let opts = store::UnlockOpts { accept_rollback: false, target_kdf: fast() };
    for v in test_fixtures::VERSIONS {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("r");
        test_fixtures::copy_fixture(&shared.join(test_fixtures::dir_name(v)), &root).unwrap();
        let sk = test_fixtures::read_secret_key(&root).unwrap();
        let s = Session::unlock_with_effects(&root, test_fixtures::FIXTURE_PASSWORD, &sk, &opts, None, store::UnlockEffects::NONE).unwrap();
        let snap = s.snapshot_sync().unwrap();
        assert!(!snap.items.is_empty(), "v{v} fixture has items");
        let plan = plan_sync(&snap, &snap, &snap, store::now_secs()).unwrap();
        assert!(!plan.changed && !plan.local_changes && plan.actions.is_empty(), "v{v} self-sync must be a no-op: changed={} local={} actions={} gh={} dev={}", plan.changed, plan.local_changes, plan.actions.len(), plan.generator_history.is_some(), plan.devices.is_some());
    }

    let d = tempfile::tempdir().unwrap();
    let src = shared.join(test_fixtures::dir_name(5));
    let (a_root, b_root) = (d.path().join("a"), d.path().join("b"));
    test_fixtures::copy_fixture(&src, &a_root).unwrap();
    test_fixtures::copy_fixture(&src, &b_root).unwrap();
    let sk = test_fixtures::read_secret_key(&a_root).unwrap();
    let pw = test_fixtures::FIXTURE_PASSWORD;
    // Base = the pristine fixture both copies descend from (read-only open).
    let base = Session::unlock_readonly(&a_root, pw, &sk).unwrap().snapshot_sync().unwrap();
    let (vid, iid) = base
        .items
        .iter()
        .find(|(_, r)| r.deleted_at.is_none() && r.attachments.is_empty())
        .map(|(k, _)| k.clone())
        .unwrap();
    let unlock = |root: &Path| Session::unlock_with(root, pw, &sk, &opts).unwrap();
    let s = unlock(&a_root);
    s.purge_item(&vid, &iid).unwrap();
    let a = s.snapshot_sync().unwrap();
    drop(s);
    // B plans from the session that applies (its unlock may touch devices).
    let s = unlock(&b_root);
    let b = s.snapshot_sync().unwrap();
    let plan = plan_sync(&b, &base, &a, store::now_secs()).unwrap();
    assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts.iter().map(|c| c.describe()).collect::<Vec<_>>());
    let rep = s.apply_sync_plan(&plan).unwrap();
    assert_eq!(rep.deleted_items, 1);
    assert!(s.read_record(&vid, &iid).is_err());
    assert_eq!(s.snapshot_sync().unwrap().items.len(), base.items.len() - 1);
    assert!(store::verify(&b_root, &s.load_manifest().unwrap()).is_empty());
    // And A pulling B back writes nothing for the purged item: no resurrection.
    let b2 = s.snapshot_sync().unwrap();
    drop(s);
    let a2 = unlock(&a_root).snapshot_sync().unwrap();
    // B merged from A's state `a`, so that is the base for A pulling B.
    let back = plan_sync(&a2, &a, &b2, store::now_secs()).unwrap();
    assert!(!back.actions.iter().any(|x| matches!(x, gitpass_core::sync_engine::SyncAction::UpsertItem { item_id, .. } if *item_id == iid)));
}
