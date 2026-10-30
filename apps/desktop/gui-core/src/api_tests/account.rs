use super::*;


#[test]
fn identity_sharing_members() {
    let e = env();
    let other = env();
    let id = e.app.identity().unwrap();
    let oid = other.app.identity().unwrap();
    assert!(!id.public_key.is_empty() && id.fingerprint != oid.fingerprint);
    let m = e.app.share_vault(&e.vault, &oid.public_key, Some("Bob (fake)".into())).unwrap();
    assert_eq!((m.fingerprint.as_str(), m.label.as_deref(), m.is_self), (oid.fingerprint.as_str(), Some("Bob (fake)"), false));
    let ms = e.app.list_members(&e.vault).unwrap();
    assert_eq!(ms.len(), 2);
    assert!(ms.iter().any(|m| m.is_self && m.fingerprint == id.fingerprint));
    let v = &e.app.list_vaults().unwrap()[0];
    assert!(v.shared);
    // accept on the other account (share file lives in e's vault dir)
    let share = e._vault.path().join("vaults").join(&e.vault).join("members").join(format!("{}.enc", oid.fingerprint));
    // other already has a vault named "Personal": rename conflict is reported
    assert!(other.app.accept_share(&share.display().to_string()).unwrap_err().to_string().contains("already exists"));
    e.app.unshare_vault(&e.vault, &oid.fingerprint).unwrap();
    assert!(e.app.list_members(&e.vault).unwrap().is_empty());
}

#[test]
fn accept_share_imports_vault() {
    let e = env();
    let other = env();
    let x = other.app.create_vault("Team", None, None, None).unwrap();
    let it = other.app.create_item(&ItemInput { vault_id: x.id.clone(), category: Category::Wifi, title: "Office WiFi".into(), url: None,
        sections: vec![Section { id: "w".into(), title: None, fields: vec![fld("ssid", FieldKind::Text, Some("FakeNet")), fld("pw", FieldKind::Concealed, Some("FAKE-wifi"))] }],
        notes: None, tags: None, favorite: None }).unwrap();
    let me = e.app.identity().unwrap();
    other.app.share_vault(&x.id, &me.public_key, None).unwrap();
    let share = other._vault.path().join("vaults").join(&x.id).join("members").join(format!("{}.enc", me.fingerprint));
    let v = e.app.accept_share(&share.display().to_string()).unwrap();
    assert_eq!((v.name.as_str(), v.item_count), ("Team", 1));
    assert_eq!(&*e.app.reveal_field(&v.id, &it.id, "pw").unwrap(), "FAKE-wifi");
    assert_eq!(it.subtitle, "FakeNet");
}

#[test]
fn sync_status_without_git() {
    let e = env();
    let s = e.app.sync_status().unwrap();
    assert_eq!(s.remote, None);
    assert_eq!((s.ahead, s.behind), (0, 0));
}

#[test]
fn fingerprint_of_matches_identity_and_share() {
    let e = env();
    let me = e.app.identity().unwrap();
    assert_eq!(e.app.fingerprint_of(&me.public_key).unwrap(), me.fingerprint);
    assert_eq!(e.app.fingerprint_of(&format!("  gitpass-x25519:{}\n", me.public_key)).unwrap(), me.fingerprint);
    let other = B64.encode([7u8; 32]);
    let f = e.app.fingerprint_of(&other).unwrap();
    assert_eq!(f.len(), 32);
    assert_ne!(f, me.fingerprint);
    let m = e.app.share_vault(&e.vault, &other, Some("Alex (fake)".into())).unwrap();
    assert_eq!(m.fingerprint, f);
    assert!(e.app.fingerprint_of("not a key").is_err());
    assert!(e.app.fingerprint_of(&B64.encode([1u8; 16])).is_err());
}

#[test]
fn create_account_and_biometric_unlock() {
    let root = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let dir = root.path().join("v");
    let app = App::new(cfg.path().to_path_buf(), Arc::new(MemClipboard::default())).with_biometric(Box::new(MemBio(Default::default())));
    app.set_unlock_opts(UnlockOpts { accept_rollback: false, target_kdf: fast() });
    let st = app.status().unwrap();
    assert!(!st.unlocked);
    assert!(app.create_account(&dir.display().to_string(), "short", false).is_err());
    let r = app.create_account(&dir.display().to_string(), "correct horse", false).unwrap();
    assert!(r.secret_key.starts_with("A3-") || !r.secret_key.is_empty());
    assert!(!dir.join("secret.key").exists());
    assert!(app.status().unwrap().unlocked);
    assert_eq!(app.list_vaults().unwrap()[0].name, "Personal");
    assert!(app.create_account(&dir.display().to_string(), "correct horse", false).is_err());
    // enable biometrics while unlocked, then unlock with it
    let mut s = app.get_settings(); s.biometric_unlock = true;
    app.set_settings(s.clone()).unwrap();
    app.lock();
    let st = app.status().unwrap();
    assert!(st.vault_exists && st.biometric_available);
    assert!(app.unlock_biometric("").unwrap().unlocked);
    // settings file never contains the secrets
    let raw = std::fs::read_to_string(cfg.path().join(SETTINGS_FILE)).unwrap();
    assert!(!raw.contains("correct horse") && !raw.contains(&r.secret_key));
    s.biometric_unlock = false; app.set_settings(s).unwrap();
    app.lock();
    assert!(app.unlock_biometric("").is_err());
}

#[test]
fn emergency_kit_data_reveal_rules() {
    let root = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let dir = root.path().join("v");
    let app = App::new(cfg.path().to_path_buf(), Arc::new(MemClipboard::default())).with_biometric(Box::new(MemBio(Default::default())));
    app.set_unlock_opts(UnlockOpts { accept_rollback: false, target_kdf: fast() });
    let r = app.create_account(&dir.display().to_string(), "correct horse", false).unwrap();
    // right after create_account the kit carries the fresh Secret Key
    let k = app.emergency_kit_data(None).unwrap();
    assert_eq!(k.secret_key.as_deref(), Some(r.secret_key.as_str()));
    assert!(!k.account_id.is_empty() && k.vault_path.ends_with("v") && k.remote.is_none());
    // after lock+unlock the key is hidden until the master password is re-entered
    app.lock();
    app.unlock(&dir.display().to_string(), "correct horse", Some(&r.secret_key)).unwrap();
    assert!(app.emergency_kit_data(None).unwrap().secret_key.is_none());
    assert!(app.emergency_kit_data(Some("wrong password")).is_err());
    assert_eq!(app.emergency_kit_data(Some("correct horse")).unwrap().secret_key.as_deref(), Some(r.secret_key.as_str()));
    // after rotation the new key is shown without a re-prompt
    app.rotate_secret_key().unwrap();
    let k2 = app.emergency_kit_data(None).unwrap();
    assert!(k2.secret_key.is_some() && k2.secret_key.as_deref() != Some(r.secret_key.as_str()));
    app.lock();
    assert!(app.emergency_kit_data(None).is_err());
}

#[test]
fn remote_connect_sync_disconnect_with_local_bare_repo() {
    let e = env();
    let bare = tempfile::tempdir().unwrap();
    let ok = std::process::Command::new("git").args(["init", "--bare", "-q", "-b", "main"]).arg(bare.path()).status().map(|s| s.success()).unwrap_or(false);
    if !ok { eprintln!("git unavailable; skipping"); return; }
    let url = bare.path().display().to_string();
    // a local path is only accepted with allow_local (tests); the public API refuses it
    assert!(e.app.remote_connect(&url).unwrap_err().to_string().starts_with(gitpass_core::remote::E_BAD_URL));
    let st = e.app.remote_connect_opts(&url, true).unwrap();
    assert_eq!(st.remote.as_deref(), Some(url.as_str()));
    assert_eq!((st.ahead, st.behind), (0, 0));
    assert!(e.app.onboarding().repo_connected);
    login(&e, "Remote test", "me", "FAKE-pw-1");
    let r = e.app.sync(false).unwrap();
    assert!(r.message.contains("pushed"), "{}", r.message);
    let st = e.app.sync_status().unwrap();
    let vp = e.app.status().unwrap().vault_path;
    let por = std::process::Command::new("git").args(["-C", &vp, "status", "--porcelain"]).output().unwrap();
    eprintln!("after sync porcelain: {}", String::from_utf8_lossy(&por.stdout));
    assert!(!st.dirty && st.ahead == 0 && st.behind == 0 && st.last_sync.is_some(), "{st:?}");
    let o = std::process::Command::new("git").arg("-C").arg(bare.path()).args(["ls-tree", "-r", "--name-only", "main"]).output().unwrap();
    let files = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(!files.contains("secret.key") && !files.contains(".gitpass-"), "{files}");
    let st = e.app.remote_disconnect().unwrap();
    assert!(st.remote.is_none());
    assert!(!e.app.onboarding().repo_connected);
}

fn porcelain(p: &str) -> String {
    let o = std::process::Command::new("git").args(["-C", p, "status", "--porcelain"]).output().unwrap();
    String::from_utf8_lossy(&o.stdout).to_string()
}
fn file_hash(p: &std::path::Path) -> String { gitpass_core::store::hash_file(p).unwrap() }

#[test]
fn sync_twice_leaves_working_tree_clean() {
    let e = env();
    let bare = tempfile::tempdir().unwrap();
    let ok = std::process::Command::new("git").args(["init", "--bare", "-q", "-b", "main"]).arg(bare.path()).status().map(|s| s.success()).unwrap_or(false);
    if !ok { eprintln!("git unavailable; skipping"); return; }
    let vp = e.app.status().unwrap().vault_path;
    let root = std::path::PathBuf::from(&vp);
    e.app.remote_connect_opts(&bare.path().display().to_string(), true).unwrap();
    login(&e, "Clean tree", "me", "FAKE-pw-2");
    let (fmt0, ks0) = (file_hash(&root.join("format.json")), file_hash(&root.join("keyset.enc")));
    for round in 1..=2 {
        let r = e.app.sync(false).unwrap();
        assert_eq!(porcelain(&vp), "", "dirty after sync #{round}: {}", r.message);
        let st = e.app.sync_status().unwrap();
        assert!(!st.dirty && st.ahead == 0 && st.behind == 0, "round {round}: {st:?}");
    }
    // merge sync too
    e.app.sync(true).unwrap();
    assert_eq!(porcelain(&vp), "");
    // sync never re-wraps the keyset with default KDF params
    assert_eq!((file_hash(&root.join("format.json")), file_hash(&root.join("keyset.enc"))), (fmt0, ks0));
    // a no-op unlock does not rewrite anything either
    e.app.lock();
    e.app.unlock(&vp, "correct horse", Some(&e.sk)).unwrap();
    assert_eq!(porcelain(&vp), "", "unlock rewrote files");
}
