use super::*;


#[test]
fn devices_settings_onboarding_favicon() {
    let e = env();
    let ds = e.app.list_devices().unwrap();
    assert_eq!(ds.len(), 1);
    assert!(ds[0].current && !ds[0].revoked);
    assert!(e.app.revoke_device("nonexistent-device", false).is_err());
    let s = e.app.get_settings();
    assert_eq!(s, Settings::default());
    let mut s2 = s.clone();
    s2.theme = Theme::Light; s2.language = Language::Th; s2.auto_lock_minutes = 15;
    assert_eq!(e.app.set_settings(s2.clone()).unwrap(), s2);
    assert_eq!(e.app.get_settings(), s2);
    let json = serde_json::to_value(&s2).unwrap();
    assert_eq!(json["autoLockMinutes"], 15);
    assert_eq!(json["language"], "th");
    let mut s3 = s2.clone(); s3.biometric_unlock = true;
    assert!(e.app.set_settings(s3).unwrap_err().to_string().contains("not supported"));
    let o = e.app.set_onboarding(OnboardingPatch { emergency_kit_saved: Some(true), ..Default::default() }).unwrap();
    assert!(o.emergency_kit_saved && !o.dismissed);
    assert_eq!(e.app.onboarding(), o);
    // onboarding dismissed persists across restart in backend
    let o_dismiss = e.app.set_onboarding(OnboardingPatch { dismissed: Some(true), ..Default::default() }).unwrap();
    assert!(o_dismiss.dismissed);
    assert!(e.app.onboarding().dismissed);
    let app2 = App::new(e._cfg.path().to_path_buf(), e.clip.clone());
    assert!(app2.onboarding().dismissed);
    let o_restore = app2.set_onboarding(OnboardingPatch { dismissed: Some(false), ..Default::default() }).unwrap();
    assert!(!o_restore.dismissed);
    let app3 = App::new(e._cfg.path().to_path_buf(), e.clip.clone());
    assert!(!app3.onboarding().dismissed);
    // settings file has no secrets
    let raw = std::fs::read_to_string(e._cfg.path().join(SETTINGS_FILE)).unwrap();
    assert!(!raw.contains("correct horse") && !raw.contains(&e.sk));
    assert_eq!(e.app.favicon_url("https://example.invalid"), None);
}

#[test]
fn credentials_change_password_and_rotate_secret_key() {
    let e = env();
    let path = e.app.status().unwrap().vault_path;
    let kit = e.app.emergency_kit().unwrap();
    assert!(kit.contains("not shown") && !kit.contains(&e.sk));
    assert!(e.app.change_password("wrong", "new-password-123").is_err());
    e.app.change_password("correct horse", "new-password-123").unwrap();
    let r = e.app.rotate_secret_key().unwrap();
    assert!(r.emergency_kit.contains("Secret Key: "));
    assert!(!r.emergency_kit.contains(&e.sk));
    assert_eq!(e.app.emergency_kit().unwrap(), r.emergency_kit, "fresh kit available until lock");
    let new_sk = r.emergency_kit.lines().find(|l| l.starts_with("Secret Key: ")).unwrap().trim_start_matches("Secret Key: ").to_string();
    e.app.lock();
    assert!(e.app.unlock(&path, "new-password-123", Some(&e.sk)).is_err());
    assert!(e.app.unlock(&path, "new-password-123", Some(&new_sk)).unwrap().unlocked);
    assert!(e.app.emergency_kit().unwrap().contains("not shown"));
}

#[test]
fn settings_hibp_defaults_off_and_old_files_parse() {
    assert!(!Settings::default().hibp_check);
    assert_eq!(Settings::default().watchtower_unused_months, 12);
    let old = r#"{"autoLockMinutes":5,"clipboardClearSecs":90,"theme":"dark","language":"en","biometricUnlock":false,"quickAccessHotkey":"X"}"#;
    let s: Settings = serde_json::from_str(old).unwrap();
    assert!(!s.hibp_check);
    assert_eq!(s.watchtower_unused_months, 12, "older settings files default the unused check to 12 months");
    let json = serde_json::to_string(&Settings::default()).unwrap();
    assert!(json.contains("\"hibpCheck\":false"));
    assert!(json.contains("\"watchtowerUnusedMonths\":12"));
}

#[test]
fn settings_watchtower_unused_months_accepts_only_known_values() {
    let e = env();
    for ok in [0u64, 6, 12, 24] {
        let saved = e.app.set_settings(Settings { watchtower_unused_months: ok, ..e.app.get_settings() }).unwrap();
        assert_eq!(saved.watchtower_unused_months, ok);
    }
    assert!(e.app.set_settings(Settings { watchtower_unused_months: 5, ..e.app.get_settings() }).is_err());
    // Fresh items are not unused, so the new report field stays empty.
    login(&e, "Recent fake", "recent@example.invalid", "Xq7#vR2!mK9@pL4$wZ");
    assert!(e.app.watchtower(false).unwrap().unused.is_empty());
}

#[test]
fn settings_system_lock_toggles_default_on_and_backward_compatibility() {
    let s = Settings::default();
    assert!(s.lock_on_sleep);
    assert!(s.lock_on_screen_lock);
    assert!(s.lock_on_user_switch);

    // Legacy JSON without sleep/screen/userSwitch fields
    let legacy_json = r#"{"autoLockMinutes":5,"clipboardClearSecs":90,"theme":"dark","language":"en","biometricUnlock":false,"quickAccessHotkey":"X"}"#;
    let parsed: Settings = serde_json::from_str(legacy_json).unwrap();
    assert!(parsed.lock_on_sleep);
    assert!(parsed.lock_on_screen_lock);
    assert!(parsed.lock_on_user_switch);

    // Roundtrip with disabled values
    let mut custom = s;
    custom.lock_on_sleep = false;
    custom.lock_on_screen_lock = false;
    custom.lock_on_user_switch = false;
    let serialized = serde_json::to_string(&custom).unwrap();
    let deserialized: Settings = serde_json::from_str(&serialized).unwrap();
    assert!(!deserialized.lock_on_sleep);
    assert!(!deserialized.lock_on_screen_lock);
    assert!(!deserialized.lock_on_user_switch);
}

#[test]
fn settings_show_menu_bar_icon_defaults_on_and_backward_compatibility() {
    let s = Settings::default();
    assert!(s.show_menu_bar_icon);

    // Legacy JSON without the showMenuBarIcon field
    let legacy_json = r#"{"autoLockMinutes":5,"clipboardClearSecs":90,"theme":"dark","language":"en","biometricUnlock":false,"quickAccessHotkey":"X"}"#;
    let parsed: Settings = serde_json::from_str(legacy_json).unwrap();
    assert!(parsed.show_menu_bar_icon);

    // Roundtrip with the setting disabled
    let mut custom = s;
    custom.show_menu_bar_icon = false;
    let serialized = serde_json::to_string(&custom).unwrap();
    assert!(serialized.contains(r#""showMenuBarIcon":false"#));
    let deserialized: Settings = serde_json::from_str(&serialized).unwrap();
    assert!(!deserialized.show_menu_bar_icon);
}

#[test]
fn settings_hide_from_screen_recording_defaults_on_and_backward_compatibility() {
    let s = Settings::default();
    assert!(s.hide_from_screen_recording);

    // Legacy JSON without the hideFromScreenRecording field
    let legacy_json = r#"{"autoLockMinutes":5,"clipboardClearSecs":90,"theme":"dark","language":"en","biometricUnlock":false,"quickAccessHotkey":"X"}"#;
    let parsed: Settings = serde_json::from_str(legacy_json).unwrap();
    assert!(parsed.hide_from_screen_recording);

    // Roundtrip with the setting disabled
    let mut custom = s;
    custom.hide_from_screen_recording = false;
    let serialized = serde_json::to_string(&custom).unwrap();
    assert!(serialized.contains("\"hideFromScreenRecording\":false"));
    let deserialized: Settings = serde_json::from_str(&serialized).unwrap();
    assert!(!deserialized.hide_from_screen_recording);
}

#[test]
fn system_lock_event_dispatch_enabled_and_disabled() {
    let e = env();
    let path = e.app.status().unwrap().vault_path;

    // 1. LockTrigger::Sleep
    assert!(e.app.handle_system_lock_event(LockTrigger::Sleep));
    assert!(!e.app.status().unwrap().unlocked);
    // Calling again when already locked returns false
    assert!(!e.app.handle_system_lock_event(LockTrigger::Sleep));

    // Unlock again and disable lock_on_sleep
    e.app.unlock(&path, "correct horse", Some(&e.sk)).unwrap();
    e.app.set_settings(Settings { lock_on_sleep: false, ..e.app.get_settings() }).unwrap();
    assert!(!e.app.handle_system_lock_event(LockTrigger::Sleep));
    assert!(e.app.status().unwrap().unlocked);

    // 2. LockTrigger::ScreenLock
    assert!(e.app.handle_system_lock_event(LockTrigger::ScreenLock));
    assert!(!e.app.status().unwrap().unlocked);
    e.app.unlock(&path, "correct horse", Some(&e.sk)).unwrap();
    e.app.set_settings(Settings { lock_on_screen_lock: false, ..e.app.get_settings() }).unwrap();
    assert!(!e.app.handle_system_lock_event(LockTrigger::ScreenLock));
    assert!(e.app.status().unwrap().unlocked);

    // 3. LockTrigger::UserSwitch
    // Also verify clipboard clear on system lock event
    let l = login(&e, "TestUserSwitch", "u", "pw-secret-switch");
    e.app.copy_field(&e.vault, &l.id, "password").unwrap();
    assert_eq!(e.clip.get_text().as_deref(), Some("pw-secret-switch"));
    assert!(e.app.handle_system_lock_event(LockTrigger::UserSwitch));
    assert!(!e.app.status().unwrap().unlocked);
    assert_eq!(e.clip.get_text().as_deref(), Some(""));
}

#[test]
fn last_used_tracking_and_sort_recent() {
    let e = env();
    let l1 = login(&e, "Beta App", "u1", "FAKE-pw1");
    let l2 = login(&e, "Alpha App", "u2", "FAKE-pw2");
    let l3 = login(&e, "Gamma App", "u3", "FAKE-pw3");

    // 1. Initial items have no last_used
    let detail1 = e.app.get_item(&e.vault, &l1.id).unwrap();
    assert_eq!(detail1.summary.last_used, None);

    // 2. Revealing l1 updates last_used
    let _ = e.app.reveal_field(&e.vault, &l1.id, "password").unwrap();
    let detail1_after = e.app.get_item(&e.vault, &l1.id).unwrap();
    let t1 = detail1_after.summary.last_used.expect("l1 should have last_used after reveal");
    assert!(t1 > 0);

    // 3. Copying field on l3 updates last_used
    std::thread::sleep(std::time::Duration::from_millis(1100));
    e.app.copy_field(&e.vault, &l3.id, "password").unwrap();
    let detail3_after = e.app.get_item(&e.vault, &l3.id).unwrap();
    let t3 = detail3_after.summary.last_used.expect("l3 should have last_used after copy");
    assert!(t3 >= t1);

    // 4. Sort::Recent returns l3, l1, then l2 (never used)
    let recent_items = e.app.list_items(&Filter {
        kind: FilterKind::All,
        vault_id: None,
        tag: None,
        category: None,
        query: None,
        sort: Some(Sort::Recent),
    }).unwrap();
    assert_eq!(recent_items[0].id, l3.id);
    assert_eq!(recent_items[1].id, l1.id);
    assert_eq!(recent_items[2].id, l2.id);

    // 5. Sort::Title returns Alpha, Beta, Gamma
    let title_items = e.app.list_items(&Filter {
        kind: FilterKind::All,
        vault_id: None,
        tag: None,
        category: None,
        query: None,
        sort: Some(Sort::Title),
    }).unwrap();
    assert_eq!(title_items[0].id, l2.id);
    assert_eq!(title_items[1].id, l1.id);
    assert_eq!(title_items[2].id, l3.id);
}

#[test]
fn settings_quick_access_hotkey_roundtrip_and_defaults() {
    let s = Settings::default();
    assert_eq!(s.quick_access_hotkey, "CommandOrControl+Shift+Space");

    // Legacy JSON without quickAccessHotkey defaults to default combo
    let legacy_json = r#"{"autoLockMinutes":5,"clipboardClearSecs":90,"theme":"dark","language":"en","biometricUnlock":false}"#;
    let parsed: Settings = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(parsed.quick_access_hotkey, "CommandOrControl+Shift+Space");

    // Serialization and deserialization roundtrip with custom combo
    let mut custom = s;
    custom.quick_access_hotkey = "CommandOrControl+Alt+K".into();
    let serialized = serde_json::to_string(&custom).unwrap();
    assert!(serialized.contains(r#""quickAccessHotkey":"CommandOrControl+Alt+K""#));
    let deserialized: Settings = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.quick_access_hotkey, "CommandOrControl+Alt+K");

    // Roundtrip with disabled/empty hotkey
    let mut disabled = deserialized;
    disabled.quick_access_hotkey = "".into();
    let serialized_disabled = serde_json::to_string(&disabled).unwrap();
    let deserialized_disabled: Settings = serde_json::from_str(&serialized_disabled).unwrap();
    assert_eq!(deserialized_disabled.quick_access_hotkey, "");

    // App set_settings and get_settings persistence roundtrip
    let e = env();
    let initial = e.app.get_settings();
    assert_eq!(initial.quick_access_hotkey, "CommandOrControl+Shift+Space");
    let updated = e.app.set_settings(Settings { quick_access_hotkey: "CommandOrControl+Shift+J".into(), ..initial }).unwrap();
    assert_eq!(updated.quick_access_hotkey, "CommandOrControl+Shift+J");
    assert_eq!(e.app.get_settings().quick_access_hotkey, "CommandOrControl+Shift+J");
}

#[test]
fn settings_render_notes_markdown_roundtrip_and_defaults() {
    let s = Settings::default();
    assert!(s.render_notes_markdown);

    // Legacy JSON without renderNotesMarkdown defaults to true
    let legacy_json = r#"{"autoLockMinutes":5,"clipboardClearSecs":90,"theme":"dark","language":"en","biometricUnlock":false}"#;
    let parsed: Settings = serde_json::from_str(legacy_json).unwrap();
    assert!(parsed.render_notes_markdown);

    // Custom setting disabled
    let mut custom = s;
    custom.render_notes_markdown = false;
    let serialized = serde_json::to_string(&custom).unwrap();
    assert!(serialized.contains(r#""renderNotesMarkdown":false"#));
    let deserialized: Settings = serde_json::from_str(&serialized).unwrap();
    assert!(!deserialized.render_notes_markdown);

    // App set_settings and get_settings persistence roundtrip
    let e = env();
    let initial = e.app.get_settings();
    assert!(initial.render_notes_markdown);
    let updated = e.app.set_settings(Settings { render_notes_markdown: false, ..initial }).unwrap();
    assert!(!updated.render_notes_markdown);
    assert!(!e.app.get_settings().render_notes_markdown);
}

#[test]
fn settings_list_density_roundtrip_and_defaults() {
    let s = Settings::default();
    assert_eq!(s.list_density, ListDensity::Comfortable);

    // Legacy JSON without listDensity defaults to comfortable
    let legacy_json = r#"{"autoLockMinutes":5,"clipboardClearSecs":90,"theme":"dark","language":"en","biometricUnlock":false}"#;
    let parsed: Settings = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(parsed.list_density, ListDensity::Comfortable);

    // Compact round-trips through JSON and app persistence
    let mut custom = s;
    custom.list_density = ListDensity::Compact;
    let serialized = serde_json::to_string(&custom).unwrap();
    assert!(serialized.contains(r#""listDensity":"compact""#));
    let deserialized: Settings = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.list_density, ListDensity::Compact);

    let e = env();
    let initial = e.app.get_settings();
    assert_eq!(initial.list_density, ListDensity::Comfortable);
    let updated = e.app.set_settings(Settings { list_density: ListDensity::Compact, ..initial }).unwrap();
    assert_eq!(updated.list_density, ListDensity::Compact);
    assert_eq!(e.app.get_settings().list_density, ListDensity::Compact);
}

#[test]
fn unlock_throttling_and_biometric_reset() {
    gitpass_core::unlock_throttle::reset_memory();
    let root = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let dir = root.path().join("v");
    let app = App::new(cfg.path().to_path_buf(), Arc::new(MemClipboard::default())).with_biometric(Box::new(MemBio(Default::default())));
    app.set_unlock_opts(UnlockOpts { accept_rollback: false, target_kdf: fast() });
    let r = app.create_account(&dir.display().to_string(), "correct horse", false).unwrap();
    let mut s = app.get_settings();
    s.biometric_unlock = true;
    app.set_settings(s).unwrap();
    app.lock();

    let vp = dir.display().to_string();

    // 1st wrong attempt
    let err1 = app.unlock(&vp, "wrong-1", Some(&r.secret_key)).unwrap_err();
    assert!(err1.to_string().contains("unlock failed"));

    // 2nd wrong attempt
    let err2 = app.unlock(&vp, "wrong-2", Some(&r.secret_key)).unwrap_err();
    assert!(err2.to_string().contains("unlock failed"));

    // 3rd wrong attempt
    let err3 = app.unlock(&vp, "wrong-3", Some(&r.secret_key)).unwrap_err();
    assert!(err3.to_string().contains("unlock failed"));

    // 4th attempt is throttled
    let err4 = app.unlock(&vp, "correct horse", Some(&r.secret_key)).unwrap_err();
    assert!(err4.to_string().starts_with("throttled: try again in "));

    // Status reports throttledSecs
    let st = app.status().unwrap();
    assert!(!st.unlocked);
    assert!(st.throttled_secs.is_some());
    assert!(st.throttled_secs.unwrap() >= 1);

    // Biometric unlock is NOT throttled by password failures and succeeds
    let bio_st = app.unlock_biometric(&vp).unwrap();
    assert!(bio_st.unlocked);

    // Throttle is now reset
    app.lock();
    let post_st = app.status().unwrap();
    assert!(!post_st.unlocked);
    assert_eq!(post_st.throttled_secs, None);

    // Normal password unlock now succeeds
    let ok_st = app.unlock(&vp, "correct horse", Some(&r.secret_key)).unwrap();
    assert!(ok_st.unlocked);
}

#[test]
fn settings_last_seen_version_roundtrip() {
    let e = env();
    let initial = e.app.get_settings();
    assert_eq!(initial.last_seen_version, None);

    let updated = e.app.set_settings(Settings {
        last_seen_version: Some("0.6.5".to_string()),
        ..initial.clone()
    }).unwrap();
    assert_eq!(updated.last_seen_version, Some("0.6.5".to_string()));
    assert_eq!(e.app.get_settings().last_seen_version, Some("0.6.5".to_string()));

    // Deserialization from JSON missing lastSeenVersion defaults to None
    let json = serde_json::to_string(&initial).unwrap();
    assert!(!json.contains("lastSeenVersion"));
    let de: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(de.last_seen_version, None);

    // Serialization with Some("0.6.5") produces camelCase lastSeenVersion
    let json_with_ver = serde_json::to_string(&updated).unwrap();
    assert!(json_with_ver.contains(r#""lastSeenVersion":"0.6.5""#));
    let de_with_ver: Settings = serde_json::from_str(&json_with_ver).unwrap();
    assert_eq!(de_with_ver.last_seen_version, Some("0.6.5".to_string()));
}

#[test]
fn set_rotation_stores_validates_and_clears_the_reminder() {
    let e = env();
    let a = login(&e, "Rotating", "me@example.invalid", "FAKE-Rotation-Pass-7");
    assert_eq!(e.app.get_item(&e.vault, &a.id).unwrap().summary.rotate_every_days, None);
    assert!(e.app.set_rotation(&e.vault, &a.id, Some(45)).is_err(), "only 30/90/180/365 are accepted");
    e.app.set_rotation(&e.vault, &a.id, Some(90)).unwrap();
    let d = e.app.get_item(&e.vault, &a.id).unwrap();
    assert_eq!(d.summary.rotate_every_days, Some(90));
    assert_eq!(d.summary.rotation, None, "a fresh password is neither due nor soon");
    // listing carries it too, and an ordinary edit keeps it
    assert_eq!(e.app.list_items(&Filter { kind: FilterKind::All, vault_id: None, tag: None, category: None, query: None, sort: None }).unwrap().iter().find(|i| i.id == a.id).unwrap().rotate_every_days, Some(90));
    e.app.update_item(&a.id, &ItemInput { vault_id: e.vault.clone(), category: Category::Login, title: "Rotating 2".into(), url: None,
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("password", FieldKind::Concealed, Some("FAKE-Rotation-Pass-7"))] }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    assert_eq!(e.app.get_item(&e.vault, &a.id).unwrap().summary.rotate_every_days, Some(90));
    e.app.set_rotation(&e.vault, &a.id, None).unwrap();
    assert_eq!(e.app.get_item(&e.vault, &a.id).unwrap().summary.rotate_every_days, None);
    let r = e.app.watchtower(false).unwrap();
    assert!(r.rotation_due.is_empty() && r.rotation_soon.is_empty());
}

#[test]
fn related_items_lifecycle_linking_unlinking_missing_state_and_cross_vault() {
    let e = env();
    // 1. Create two login items and an API credential in the default vault
    let item_a = login(&e, "GitHub Login", "octocat", "correct horse");
    let item_b = login(&e, "Google Login", "alice", "battery staple");
    let item_c = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::ApiCredential,
        title: "GitHub API Token".into(),
        url: Some("https://api.github.com".into()),
        sections: vec![Section {
            id: "creds".into(),
            title: None,
            fields: vec![fld("credential", FieldKind::Concealed, Some("ghp_fake_token_1234567890"))],
        }],
        notes: None,
        tags: None,
        favorite: None,
        related_item_ids: None,
    }).unwrap();

    // 2. Old items without related_item_ids load with empty relatedItems and relatedItemIds
    let detail_a = e.app.get_item(&e.vault, &item_a.id).unwrap();
    assert!(detail_a.related_items.is_empty());
    assert!(detail_a.related_item_ids.is_empty());

    // 3. Link item A to item C (GitHub API Token) and item B (Google Login)
    e.app.add_related_item(&e.vault, &item_a.id, &item_c.id).unwrap();
    e.app.add_related_item(&e.vault, &item_a.id, &item_b.id).unwrap();

    // Linking again is idempotent
    e.app.add_related_item(&e.vault, &item_a.id, &item_b.id).unwrap();

    // Cannot link an item to itself
    let self_err = e.app.add_related_item(&e.vault, &item_a.id, &item_a.id);
    assert!(self_err.is_err());
    assert!(self_err.unwrap_err().to_string().contains("cannot link an item to itself"));

    // Check detail A
    let detail_a = e.app.get_item(&e.vault, &item_a.id).unwrap();
    assert_eq!(detail_a.related_item_ids, vec![item_c.id.clone(), item_b.id.clone()]);
    assert_eq!(detail_a.related_items.len(), 2);

    let rel_c = &detail_a.related_items[0];
    assert_eq!(rel_c.id, item_c.id);
    assert_eq!(rel_c.vault_id.as_deref(), Some(e.vault.as_str()));
    assert_eq!(rel_c.category, Some(Category::ApiCredential));
    assert_eq!(rel_c.title, "GitHub API Token");
    assert!(rel_c.available);

    let rel_b = &detail_a.related_items[1];
    assert_eq!(rel_b.id, item_b.id);
    assert_eq!(rel_b.vault_id.as_deref(), Some(e.vault.as_str()));
    assert_eq!(rel_b.category, Some(Category::Login));
    assert_eq!(rel_b.title, "Google Login");
    assert_eq!(rel_b.subtitle.as_deref(), Some("alice"));
    assert!(rel_b.available);

    // 4. Cross-vault linking: create a second vault and an item in it
    let v2 = e.app.create_vault("Work Vault", None, None, None).unwrap();
    let v2_id = v2.id;
    let item_v2 = e.app.create_item(&ItemInput {
        vault_id: v2_id.clone(),
        category: Category::Server,
        title: "Deploy Server".into(),
        url: Some("192.168.1.100".into()),
        sections: vec![Section {
            id: "server".into(),
            title: None,
            fields: vec![fld("username", FieldKind::Text, Some("deploy"))],
        }],
        notes: None,
        tags: None,
        favorite: None,
        related_item_ids: None,
    }).unwrap();

    e.app.add_related_item(&e.vault, &item_a.id, &item_v2.id).unwrap();
    let detail_a = e.app.get_item(&e.vault, &item_a.id).unwrap();
    assert_eq!(detail_a.related_items.len(), 3);
    let rel_v2 = &detail_a.related_items[2];
    assert_eq!(rel_v2.id, item_v2.id);
    assert_eq!(rel_v2.vault_id.as_deref(), Some(v2_id.as_str()));
    assert_eq!(rel_v2.title, "Deploy Server");
    assert!(rel_v2.available);

    // 5. Missing / deleted state: delete item B (soft-delete)
    e.app.delete_item(&e.vault, &item_b.id).unwrap();
    let detail_a = e.app.get_item(&e.vault, &item_a.id).unwrap();
    assert_eq!(detail_a.related_items.len(), 3);
    let rel_b_del = &detail_a.related_items[1];
    assert_eq!(rel_b_del.id, item_b.id);
    assert!(!rel_b_del.available);
    assert_eq!(rel_b_del.title, "Item not available");
    assert_eq!(rel_b_del.vault_id, None);
    assert_eq!(rel_b_del.category, None);

    // 6. Remove missing link: remove item B from item A
    e.app.remove_related_item(&e.vault, &item_a.id, &item_b.id).unwrap();
    let detail_a = e.app.get_item(&e.vault, &item_a.id).unwrap();
    assert_eq!(detail_a.related_items.len(), 2);
    assert!(!detail_a.related_item_ids.contains(&item_b.id));

    // 7. Update item with related_item_ids via ItemInput
    let updated_input = ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Login,
        title: "GitHub Login Updated".into(),
        sections: detail_a.sections.clone(),
        notes: detail_a.notes.clone(),
        tags: Some(detail_a.summary.tags.clone()),
        favorite: Some(detail_a.summary.favorite),
        url: detail_a.summary.url.clone(),
        related_item_ids: Some(vec![item_v2.id.clone()]),
    };
    e.app.update_item(&item_a.id, &updated_input).unwrap();
    let detail_a_updated = e.app.get_item(&e.vault, &item_a.id).unwrap();
    assert_eq!(detail_a_updated.related_item_ids, vec![item_v2.id.clone()]);
    assert_eq!(detail_a_updated.related_items.len(), 1);
    assert_eq!(detail_a_updated.related_items[0].id, item_v2.id);
}

#[test]
fn backlinks_linked_from_lifecycle_and_deletion() {
    let e = env();
    let item_a = login(&e, "GitHub Login", "octocat", "correct horse");
    let item_b = login(&e, "Google Login", "alice", "battery staple");

    // Initially, neither has related items or backlinks
    let detail_b = e.app.get_item(&e.vault, &item_b.id).unwrap();
    assert!(detail_b.linked_from.is_empty());
    let detail_a = e.app.get_item(&e.vault, &item_a.id).unwrap();
    assert!(detail_a.linked_from.is_empty());

    // A links to B
    e.app.add_related_item(&e.vault, &item_a.id, &item_b.id).unwrap();

    // B lists A under Linked from; A lists B under Related items
    let detail_b = e.app.get_item(&e.vault, &item_b.id).unwrap();
    assert_eq!(detail_b.linked_from.len(), 1);
    let backlink_a = &detail_b.linked_from[0];
    assert_eq!(backlink_a.id, item_a.id);
    assert_eq!(backlink_a.title, "GitHub Login");
    assert_eq!(backlink_a.subtitle.as_deref(), Some("octocat"));
    assert_eq!(backlink_a.category, Some(Category::Login));
    assert_eq!(backlink_a.vault_id.as_deref(), Some(e.vault.as_str()));
    assert!(backlink_a.available);

    let detail_a = e.app.get_item(&e.vault, &item_a.id).unwrap();
    assert!(detail_a.linked_from.is_empty());
    assert_eq!(detail_a.related_items.len(), 1);
    assert_eq!(detail_a.related_items[0].id, item_b.id);

    // Verify backlink cache is populated and hits on subsequent read
    let stamp = e.app.state.change_stamp().unwrap();
    assert!(e.app.backlink_cache.lock().unwrap().as_ref().is_some_and(|c| c.stamp == stamp));
    let detail_b_cached = e.app.get_item(&e.vault, &item_b.id).unwrap();
    assert_eq!(detail_b_cached.linked_from, detail_b.linked_from);

    // Cross-vault linking: create a second vault and item C in it that also links to B
    let v2 = e.app.create_vault("Work", None, None, None).unwrap();
    let item_c = e.app.create_item(&ItemInput {
        vault_id: v2.id.clone(),
        category: Category::Server,
        title: "Database Server".into(),
        sections: vec![],
        notes: None,
        tags: None,
        favorite: None,
        url: None,
        related_item_ids: Some(vec![item_b.id.clone()]),
    }).unwrap();

    let detail_b = e.app.get_item(&e.vault, &item_b.id).unwrap();
    assert_eq!(detail_b.linked_from.len(), 2);
    assert!(detail_b.linked_from.iter().any(|r| r.id == item_a.id));
    assert!(detail_b.linked_from.iter().any(|r| r.id == item_c.id && r.vault_id.as_deref() == Some(v2.id.as_str())));

    // Deleting A removes it from B linked from (Recently Deleted excluded)
    e.app.delete_item(&e.vault, &item_a.id).unwrap();
    let detail_b = e.app.get_item(&e.vault, &item_b.id).unwrap();
    assert_eq!(detail_b.linked_from.len(), 1);
    assert_eq!(detail_b.linked_from[0].id, item_c.id);

    // Restoring A brings it back under Linked from
    e.app.restore_item(&e.vault, &item_a.id).unwrap();
    let detail_b = e.app.get_item(&e.vault, &item_b.id).unwrap();
    assert_eq!(detail_b.linked_from.len(), 2);
    assert!(detail_b.linked_from.iter().any(|r| r.id == item_a.id));
}
