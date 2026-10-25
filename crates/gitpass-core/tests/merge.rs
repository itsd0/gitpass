//! Format v3 field-level merge tests.
use gitpass_core::crypto::*;
use gitpass_core::merge::*;
use gitpass_core::store::{self, Item, Session, UnlockOpts};
use std::path::Path;
use std::process::Command;

const PW: &str = "correct horse battery";
fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn opts() -> UnlockOpts { UnlockOpts { accept_rollback: false, target_kdf: fast() } }
fn base() -> Item {
    Item { title: "GitHub".into(), username: Some("me".into()), password: Some("p1".into()), url: Some("https://github.com".into()),
        notes: None, totp: None, updated_at: "100".into() }
}
fn rec(item: Item, c: FieldClocks) -> ItemRecord { ItemRecord { item, clocks: Some(c) } }

#[test]
fn non_overlapping_edits_both_survive() {
    let c0 = FieldClocks::all(1000);
    let mut l = base(); l.password = Some("local-pw".into());
    let mut lc = c0; lc.password = 2000;
    let mut r = base(); r.notes = Some("remote note".into());
    let mut rc = c0; rc.notes = 2500;
    let (m, rep) = merge_records(&rec(l, lc), &rec(r, rc));
    assert_eq!(m.item.password.as_deref(), Some("local-pw"));
    assert_eq!(m.item.notes.as_deref(), Some("remote note"));
    assert_eq!(rep.from_remote, vec!["notes"]);
    assert_eq!(rep.from_local, vec!["password"]);
    assert!(!rep.has_conflicts());
    assert_eq!(m.clocks.unwrap().password, 2000);
    assert_eq!(m.clocks.unwrap().notes, 2500);
    assert_eq!(m.item.updated_at, "2"); // max clock 2500 ms -> 2 s
}

#[test]
fn overlapping_edit_higher_clock_wins() {
    let c0 = FieldClocks::all(1000);
    let mut l = base(); l.password = Some("L".into());
    let mut r = base(); r.password = Some("R".into());
    let (mut lc, mut rc) = (c0, c0); lc.password = 3000; rc.password = 4000;
    let (m, rep) = merge_records(&rec(l.clone(), lc), &rec(r.clone(), rc));
    assert_eq!(m.item.password.as_deref(), Some("R"));
    assert_eq!(rep.from_remote, vec!["password"]);
    // symmetric: other device converges to same value
    let (m2, _) = merge_records(&rec(r, rc), &rec(l, lc));
    assert_eq!(m2.item, m.item);
    assert_eq!(m2.clocks, m.clocks);
}

#[test]
fn equal_clock_different_value_is_conflict_local_kept() {
    let c = FieldClocks::all(5000);
    let mut l = base(); l.username = Some("alice".into());
    let mut r = base(); r.username = Some("bob".into());
    let (m, rep) = merge_records(&rec(l, c), &rec(r, c));
    assert_eq!(m.item.username.as_deref(), Some("alice"));
    assert_eq!(rep.conflicts, vec!["username"]);
    assert!(rep.from_remote.is_empty());
}

#[test]
fn equal_clock_equal_value_is_clean() {
    let c = FieldClocks::all(5000);
    let (m, rep) = merge_records(&rec(base(), c), &rec(base(), c));
    assert_eq!(rep, MergeReport::default());
    assert_eq!(m.item.title, "GitHub");
}

#[test]
fn tombstone_clear_beats_older_edit_and_loses_to_newer() {
    let c0 = FieldClocks::all(1000);
    let mut cleared = base(); cleared.url = None;
    let mut cc = c0; cc.url = 3000;
    let mut edited = base(); edited.url = Some("https://new".into());
    let mut ec = c0; ec.url = 2000;
    let (m, rep) = merge_records(&rec(edited.clone(), ec), &rec(cleared.clone(), cc));
    assert_eq!(m.item.url, None, "newer clear wins");
    assert_eq!(rep.from_remote, vec!["url"]);
    ec.url = 4000;
    let (m, _) = merge_records(&rec(cleared, cc), &rec(edited, ec));
    assert_eq!(m.item.url.as_deref(), Some("https://new"), "newer edit beats older clear");
}

#[test]
fn stamp_bumps_only_changed_fields_monotonically() {
    let old = base();
    let oc = FieldClocks::all(9_000);
    let mut new = base(); new.password = Some("x".into()); new.notes = Some("n".into());
    // `now` behind old clocks (skewed device): still strictly increases
    let c = stamp(Some((&old, oc)), &new, 5_000);
    assert_eq!(c.password, 9_001);
    assert_eq!(c.notes, 9_001);
    assert_eq!(c.title, 9_000);
    assert_eq!(c.username, 9_000);
}

#[test]
fn legacy_items_synthesize_clocks_from_updated_at() {
    let v2_json = r#"{"title":"T","username":null,"password":"p","url":null,"notes":null,"totp":null,"updated_at":"1700000000"}"#;
    let r: ItemRecord = serde_json::from_str(v2_json).unwrap();
    assert!(r.clocks.is_none());
    assert_eq!(r.clocks_or_legacy(), FieldClocks::all(1_700_000_000_000));
    // v3 JSON is a superset: still parses as a v2 Item (older readers ignore `clocks`)
    let v3 = serde_json::to_string(&ItemRecord { item: r.item.clone(), clocks: Some(FieldClocks::all(1)) }).unwrap();
    assert!(v3.contains("\"clocks\""));
    let _: Item = serde_json::from_str(&v3).unwrap();
    // merge_items on plain Items = legacy whole-item LWW
    let mut a = base(); a.updated_at = "200".into(); a.password = Some("new".into());
    let (m, rep) = merge_items(&base(), &a);
    assert_eq!(m.password.as_deref(), Some("new"));
    assert!(rep.from_remote.contains(&"password"));
}

fn setup() -> (tempfile::TempDir, SecretKey, Session, String, String) {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    let s = Session::unlock_with(d.path(), PW, &sk, &opts()).unwrap();
    let vid = s.create_vault("personal").unwrap();
    let iid = s.add_item(&vid, &base()).unwrap();
    (d, sk, s, vid, iid)
}

#[test]
fn session_round_trip_writes_clocks_and_merges() {
    let (d, sk, s, vid, iid) = setup();
    let (v1, r1) = s.read_record(&vid, &iid).unwrap();
    let c1 = r1.clocks.expect("v3 writes always emit clocks");
    assert_eq!(s.load_manifest().unwrap().format, 3);
    // local edit of password
    let mut it = r1.item.clone(); it.password = Some("local".into());
    std::thread::sleep(std::time::Duration::from_millis(3));
    s.update_item(&vid, &iid, &it).unwrap();
    let (_, r2) = s.read_record(&vid, &iid).unwrap();
    let c2 = r2.clocks.unwrap();
    assert!(c2.password > c1.password);
    assert_eq!(c2.notes, c1.notes);
    // "remote" edited notes concurrently from the v1 state
    let mut remote = r1.item.clone(); remote.notes = Some("from laptop".into());
    let mut rc = c1; rc.notes = c2.password + 10;
    let epoch_before = s.epoch().unwrap();
    let rep = s.merge_item(&vid, &iid, &ItemRecord { item: remote, clocks: Some(rc) }).unwrap();
    assert_eq!(rep.from_remote, vec!["notes"]);
    assert_eq!(rep.from_local, vec!["password"]);
    let (v3, r3) = s.read_record(&vid, &iid).unwrap();
    assert_eq!(r3.item.password.as_deref(), Some("local"));
    assert_eq!(r3.item.notes.as_deref(), Some("from laptop"));
    assert_eq!(v3, v1 + 2);
    assert!(s.epoch().unwrap() > epoch_before);
    drop(s);
    // integrity still verifies on fresh unlock
    let s = Session::unlock_with(d.path(), PW, &sk, &opts()).unwrap();
    assert_eq!(s.read_item(&vid, &iid).unwrap().1.notes.as_deref(), Some("from laptop"));
}

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn sync_merge_two_devices_non_overlapping() {
    let (a, sk, s, vid, iid) = setup();
    drop(s);
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--bare", "-q", "-b", "main"]);
    let ap = a.path();
    git(ap, &["init", "-q", "-b", "main"]);
    for (k, v) in [("user.email", "t@t"), ("user.name", "t")] { git(ap, &["config", k, v]); }
    git(ap, &["remote", "add", "origin", remote.path().to_str().unwrap()]);
    gitpass_core::sync::sync_merge(ap, PW, &sk, &opts()).unwrap();
    // device B clones
    let b = tempfile::tempdir().unwrap();
    let bp = b.path().join("b");
    let o = Command::new("git").args(["clone", "-q", remote.path().to_str().unwrap(), bp.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success());
    for (k, v) in [("user.email", "t@t"), ("user.name", "t")] { git(&bp, &["config", k, v]); }
    // A edits password, B edits notes (concurrently, both unpushed)
    let sa = Session::unlock_with(ap, PW, &sk, &opts()).unwrap();
    let mut it = sa.read_item(&vid, &iid).unwrap().1; it.password = Some("A-pw".into());
    std::thread::sleep(std::time::Duration::from_millis(3));
    sa.update_item(&vid, &iid, &it).unwrap();
    drop(sa);
    let sb = Session::unlock_with(&bp, PW, &sk, &opts()).unwrap();
    let mut it = sb.read_item(&vid, &iid).unwrap().1; it.notes = Some("B-note".into());
    sb.update_item(&vid, &iid, &it).unwrap();
    let new_b = sb.add_item(&vid, &Item { title: "OnlyB".into(), username: None, password: None, url: None, notes: None, totp: None, updated_at: "1".into() }).unwrap();
    drop(sb);
    // A pushes first; B's plain rebase would conflict on manifest.enc + item file; --merge resolves.
    gitpass_core::sync::sync_merge(ap, PW, &sk, &opts()).unwrap();
    let r = gitpass_core::sync::sync_merge(&bp, PW, &sk, &opts()).unwrap();
    eprintln!("{}", r.log);
    assert_eq!(r.merged.len(), 1);
    assert_eq!(r.merged[0].2.from_remote, vec!["password"]);
    assert_eq!(r.merged[0].2.from_local, vec!["notes"]);
    assert_eq!(r.reapplied.len(), 1);
    // A pulls (fast-forward) and sees both edits + B's new item
    gitpass_core::sync::sync_merge(ap, PW, &sk, &opts()).unwrap();
    let sa = Session::unlock_with(ap, PW, &sk, &opts()).unwrap();
    let it = sa.read_item(&vid, &iid).unwrap().1;
    assert_eq!(it.password.as_deref(), Some("A-pw"));
    assert_eq!(it.notes.as_deref(), Some("B-note"));
    assert_eq!(sa.read_item(&vid, &new_b).unwrap().1.title, "OnlyB");
}
