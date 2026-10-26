use super::*;
use gitpass_core::crypto::KdfParams;
use gitpass_core::store;

fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
fn opts() -> UnlockOpts { UnlockOpts { accept_rollback: false, target_kdf: fast() } }

fn setup() -> (tempfile::TempDir, String, GuiState) {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), "correct horse", fast()).unwrap();
    let enc = sk.encode();
    let st = GuiState::new(5);
    st.unlock_with_opts(d.path(), "correct horse", parse_secret_key(&enc).unwrap(), Some("test-dev".into()), &opts()).unwrap();
    (d, enc, st)
}
fn input(t: &str, u: &str, p: &str) -> ItemInput { ItemInput { title: t.into(), username: Some(u.into()), password: Some(p.into()), ..Default::default() } }

#[test]
fn unlock_ok_and_wrong_password_and_locked_errors() {
    let (d, enc, st) = setup();
    let s = st.status().unwrap();
    assert!(s.unlocked && s.device_id.is_some());
    st.lock();
    assert!(!st.is_unlocked());
    assert!(st.list_vaults().unwrap_err().to_string().contains("locked"));
    let st2 = GuiState::new(5);
    let e = st2.unlock_with_opts(d.path(), "wrong", parse_secret_key(&enc).unwrap(), None, &opts()).unwrap_err();
    assert!(e.to_string().contains("unlock failed"), "{e}");
    assert!(!st2.is_unlocked());
    // public unlock rejects non-vault dirs
    let empty = tempfile::tempdir().unwrap();
    assert!(st2.unlock(empty.path(), "x", parse_secret_key(&enc).unwrap(), None, false).is_err());
}

#[test]
fn secret_key_from_file_and_paste() {
    let (d, enc, _st) = setup();
    let f = d.path().join("sk.txt");
    std::fs::write(&f, format!("  {enc}\n")).unwrap();
    assert_eq!(read_secret_key_file(&f).unwrap().0, parse_secret_key(&enc).unwrap().0);
    assert!(parse_secret_key("A3-NOPE").is_err());
}

#[test]
fn add_list_get_and_secrets_not_in_dtos() {
    let (_d, _enc, st) = setup();
    let v = st.create_vault("Personal").unwrap();
    assert_eq!(st.list_vaults().unwrap().iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), vec!["Personal"]);
    let id = st.add_item(&v, input("GitHub", "alice", "s3cret!")).unwrap();
    st.add_item(&v, input("Amazon", "bob", "pw2")).unwrap();
    let l = st.list_items(&v, "").unwrap();
    assert_eq!(l.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), vec!["Amazon", "GitHub"]);
    let json = serde_json::to_string(&l).unwrap_or_default();
    assert!(!json.contains("s3cret"));
    let det = st.get_item(&v, &id).unwrap();
    assert!(det.has_password && det.username.as_deref() == Some("alice"));
    assert_eq!(&*st.field_value(&v, &id, Field::Password).unwrap(), "s3cret!");
    assert!(st.add_item(&v, input("  ", "", "")).is_err());
}

#[test]
fn fuzzy_search_ranks() {
    let (_d, _enc, st) = setup();
    let v = st.create_vault("P").unwrap();
    for t in ["GitHub", "GitLab", "Amazon", "Gmail"] { st.add_item(&v, input(t, "u", "p")).unwrap(); }
    let r = st.list_items(&v, "ghub").unwrap();
    assert_eq!(r[0].title, "GitHub");
    assert!(r.iter().all(|x| x.score.is_some()));
    assert!(st.list_items(&v, "amzn").unwrap().iter().any(|x| x.title == "Amazon"));
    assert!(st.list_items(&v, "zzzzqqq").unwrap().is_empty());
}

#[test]
fn edit_delete_totp_and_gen() {
    let (_d, _enc, st) = setup();
    let v = st.create_vault("P").unwrap();
    let id = st.add_item(&v, input("Site", "u", "old")).unwrap();
    // edit without password keeps it
    st.update_item(&v, &id, ItemInput { title: "Site2".into(), totp: Some("JBSWY3DPEHPK3PXP".into()), ..Default::default() }).unwrap();
    assert_eq!(&*st.field_value(&v, &id, Field::Password).unwrap(), "old");
    assert_eq!(st.get_item(&v, &id).unwrap().title, "Site2");
    let t = st.totp(&v, &id).unwrap();
    assert_eq!(t.code.len(), 6);
    assert!(t.remaining >= 1 && t.remaining <= 30 && t.period == 30);
    assert!(st.update_item(&v, &id, ItemInput { title: "x".into(), totp: Some("not base32 !!".into()), ..Default::default() }).is_err());
    st.update_item(&v, &id, ItemInput { title: "Site2".into(), password: Some("new".into()), ..Default::default() }).unwrap();
    assert_eq!(&*st.field_value(&v, &id, Field::Password).unwrap(), "new");
    st.delete_item(&v, &id).unwrap();
    assert!(st.list_items(&v, "").unwrap().is_empty());
    assert_eq!(gen_password(20).len(), 20);
    assert_eq!(gen_password(2).len(), 8);
}

#[test]
fn auto_lock_after_idle() {
    let (_d, _enc, st) = setup();
    st.set_auto_lock_duration(Duration::from_millis(50));
    std::thread::sleep(Duration::from_millis(80));
    assert!(st.check_idle());
    assert!(!st.is_unlocked());
}

#[test]
fn devices_and_sync_without_git() {
    let (_d, _enc, st) = setup();
    let ds = st.devices().unwrap();
    assert_eq!(ds.len(), 1);
    assert!(ds[0].current && ds[0].name == "test-dev");
    assert!(st.sync(false).unwrap_err().to_string().contains("not a git repository"));
}

#[test]
fn sync_local_repo_no_remote() {
    let (d, _enc, st) = setup();
    let ok = std::process::Command::new("git").args(["init", "-q"]).current_dir(d.path()).status().map(|s| s.success()).unwrap_or(false);
    if !ok { eprintln!("git unavailable; skipping"); return; }
    for (k, v) in [("user.email", "t@t"), ("user.name", "t")] {
        std::process::Command::new("git").args(["config", k, v]).current_dir(d.path()).status().unwrap();
    }
    let r = st.sync_with_target(false, Some(fast())).unwrap();
    assert!(r.log.contains("no 'origin' remote"), "{}", r.log);
    assert!(r.log.contains("integrity verified"));
    assert!(st.list_vaults().is_ok());
}
