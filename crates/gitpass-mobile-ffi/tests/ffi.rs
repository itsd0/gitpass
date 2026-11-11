use gitpass_mobile_ffi::{
    create_account, generate_password, merge_item_records, open_vault, totp_code, ItemInput,
};

fn empty_input(title: &str) -> ItemInput {
    ItemInput {
        title: title.to_string(),
        username: None,
        password: None,
        url: None,
        notes: None,
        totp: None,
        category: "login".to_string(),
        tags: vec![],
        favorite: false,
        sections: vec![],
    }
}

#[test]
fn vault_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let secret = create_account(root.to_str().unwrap().to_string(), "correct horse battery staple".to_string()).unwrap();
    assert!(secret.starts_with("A3-"), "secret key format: {secret}");
    let vault = open_vault(
        root.to_str().unwrap().to_string(),
        "correct horse battery staple".to_string(),
        secret,
    )
    .unwrap();
    let vid = vault.create_vault("Personal".to_string(), None).unwrap();
    assert!(!vid.is_empty());
    let vaults = vault.list_vaults().unwrap();
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].name, "Personal");
    assert_eq!(vaults[0].item_count, 0);

    let mut input = empty_input("Example");
    input.username = Some("alice".to_string());
    input.password = Some("s3cret!".to_string());
    input.url = Some("https://example.com".to_string());
    input.totp = Some("JBSWY3DPEHPK3PXP".to_string());
    let iid = vault.create_item(vid.clone(), input).unwrap();
    let items = vault.list_items(vid.clone()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Example");
    assert_eq!(items[0].subtitle, "alice");
    assert!(items[0].has_totp);
    // List summaries must not leak the password.
    assert!(!format!("{items:?}").contains("s3cret"));

    let detail = vault.get_item(vid.clone(), iid.clone()).unwrap();
    assert_eq!(detail.password.as_deref(), Some("s3cret!"));
    assert_eq!(detail.username.as_deref(), Some("alice"));

    vault.set_favorite(vid.clone(), iid.clone(), true).unwrap();
    assert!(vault.get_item(vid.clone(), iid.clone()).unwrap().favorite);

    let mut updated = empty_input("Example (renamed)");
    updated.username = Some("alice".to_string());
    updated.password = Some("n3w-secret!".to_string());
    vault.update_item(vid.clone(), iid.clone(), updated).unwrap();
    assert_eq!(vault.get_item(vid.clone(), iid.clone()).unwrap().password.as_deref(), Some("n3w-secret!"));

    vault.delete_item(vid.clone(), iid.clone()).unwrap();
    assert!(vault.get_item(vid.clone(), iid.clone()).unwrap().deleted);
    vault.restore_item(vid.clone(), iid.clone()).unwrap();
    assert!(!vault.get_item(vid.clone(), iid.clone()).unwrap().deleted);
}

#[test]
fn update_with_extra_sections_keeps_login_fields() {
    use gitpass_mobile_ffi::{FfiField, FfiSection};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let secret = create_account(root.to_str().unwrap().to_string(), "correct horse battery staple".to_string()).unwrap();
    let vault = open_vault(root.to_str().unwrap().to_string(), "correct horse battery staple".to_string(), secret).unwrap();
    let vid = vault.create_vault("Personal".to_string(), None).unwrap();
    let mut input = empty_input("With extra");
    input.username = Some("bob".to_string());
    input.password = Some("keep-me".to_string());
    input.sections = vec![FfiSection {
        id: "extra".to_string(),
        title: Some("Additional fields".to_string()),
        fields: vec![FfiField { id: "f1".to_string(), label: "Answer".to_string(), kind: "text".to_string(), value: Some("old".to_string()) }],
    }];
    let iid = vault.create_item(vid.clone(), input).unwrap();
    // Update: change the extra field, keep login fields the same.
    let mut updated = empty_input("With extra");
    updated.username = Some("bob".to_string());
    updated.password = Some("keep-me".to_string());
    updated.sections = vec![FfiSection {
        id: "extra".to_string(),
        title: Some("Additional fields".to_string()),
        fields: vec![FfiField { id: "f2".to_string(), label: "Other".to_string(), kind: "text".to_string(), value: Some("new".to_string()) }],
    }];
    vault.update_item(vid.clone(), iid.clone(), updated).unwrap();
    let detail = vault.get_item(vid.clone(), iid.clone()).unwrap();
    assert_eq!(detail.username.as_deref(), Some("bob"));
    assert_eq!(detail.password.as_deref(), Some("keep-me"));
    let extra_values: Vec<&str> = detail.sections.iter()
        .flat_map(|s| s.fields.iter())
        .filter_map(|f| f.value.as_deref())
        .collect();
    assert!(extra_values.contains(&"new"), "new extra field kept: {extra_values:?}");
    assert!(!extra_values.contains(&"old"), "removed extra field must not resurrect: {extra_values:?}");
}

#[test]
fn wrong_password_is_an_error_not_a_vault() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let secret = create_account(root.to_str().unwrap().to_string(), "pw-one".to_string()).unwrap();
    let bad = open_vault(root.to_str().unwrap().to_string(), "pw-two".to_string(), secret);
    assert!(bad.is_err(), "wrong password must not unlock");
}

#[test]
fn totp_and_generator() {
    let code = totp_code("JBSWY3DPEHPK3PXP".to_string()).unwrap();
    assert_eq!(code.code.len(), 6);
    assert!(code.code.chars().all(|c| c.is_ascii_digit()));
    assert!((1..=30).contains(&code.seconds_remaining));
    assert!(totp_code("not base32!!!".to_string()).is_err());
    let pw = generate_password(20);
    assert_eq!(pw.len(), 20);
}

#[test]
fn merge_prefers_higher_clock() {
    use gitpass_core::merge::ItemRecord;
    use gitpass_core::store::Item;
    let local = ItemRecord::new(Item {
        title: "Local".into(),
        username: Some("local-user".into()),
        password: None,
        url: None,
        notes: None,
        totp: None,
        updated_at: "1".into(),
    });
    let remote = ItemRecord::new(Item {
        title: "Remote".into(),
        username: Some("remote-user".into()),
        password: None,
        url: None,
        notes: None,
        totp: None,
        updated_at: "1".into(),
    });
    // Identical clocks, different values: local is kept (desktop rule),
    // so the merged title must be the local one.
    let merged_json = merge_item_records(
        serde_json::to_string(&local).unwrap(),
        serde_json::to_string(&remote).unwrap(),
    )
    .unwrap();
    let merged: ItemRecord = serde_json::from_str(&merged_json).unwrap();
    assert_eq!(merged.item.title, "Local");
}

#[test]
fn tampered_keyset_does_not_unlock() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let secret = create_account(root.to_str().unwrap().to_string(), "pw-one-ok".to_string()).unwrap();
    let path = root.join("keyset.enc");
    let mut value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let ct = value["ct"].as_str().unwrap().to_string();
    assert!(ct.len() > 8, "ciphertext too short");
    let mut chars = ct.into_bytes();
    let index = chars.len() / 2;
    chars[index] = if chars[index] == b'A' { b'B' } else { b'A' };
    value["ct"] = serde_json::Value::String(String::from_utf8(chars).unwrap());
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let opened = open_vault(root.to_str().unwrap().to_string(), "pw-one-ok".to_string(), secret);
    let message = match opened {
        Ok(_) => panic!("tampered keyset must not unlock"),
        Err(error) => error.to_string(),
    };
    assert!(
        message.contains("tampered") || message.contains("unlock failed"),
        "unexpected failure: {message}"
    );
}

#[test]
fn tampered_item_does_not_decrypt() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let secret = create_account(root.to_str().unwrap().to_string(), "pw-one-ok".to_string()).unwrap();
    let vault = open_vault(root.to_str().unwrap().to_string(), "pw-one-ok".to_string(), secret).unwrap();
    let vid = vault.create_vault("Personal".to_string(), None).unwrap();
    let mut input = empty_input("Keep");
    input.password = Some("do-not-leak".to_string());
    let iid = vault.create_item(vid.clone(), input).unwrap();
    let item_path = root.join("vaults").join(&vid).join("items").join(format!("{iid}.enc"));
    let mut bytes = std::fs::read(&item_path).unwrap();
    assert!(!bytes.is_empty());
    let index = bytes.len() / 2;
    bytes[index] ^= 0xff;
    std::fs::write(&item_path, &bytes).unwrap();
    let message = vault.get_item(vid, iid).expect_err("tampered item must fail").to_string();
    assert!(!message.contains("do-not-leak"), "{message}");
}

#[test]
fn dropping_the_vault_lets_it_open_again() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let secret = create_account(root.to_str().unwrap().to_string(), "pw-one-ok".to_string()).unwrap();
    {
        let vault = open_vault(root.to_str().unwrap().to_string(), "pw-one-ok".to_string(), secret.clone()).unwrap();
        assert!(vault.list_vaults().unwrap().is_empty());
    }
    let vault = open_vault(root.to_str().unwrap().to_string(), "pw-one-ok".to_string(), secret).unwrap();
    let vid = vault.create_vault("Personal".to_string(), None).unwrap();
    assert_eq!(vault.list_vaults().unwrap().len(), 1);
    assert!(!vid.is_empty());
}

#[test]
fn concurrent_reads_stay_stable() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let secret = create_account(root.to_str().unwrap().to_string(), "pw-one-ok".to_string()).unwrap();
    let vault = open_vault(root.to_str().unwrap().to_string(), "pw-one-ok".to_string(), secret).unwrap();
    let vid = vault.create_vault("Personal".to_string(), None).unwrap();
    let mut input = empty_input("Stable");
    input.username = Some("alice".to_string());
    let iid = vault.create_item(vid.clone(), input).unwrap();
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let vault = &vault;
            let vid = &vid;
            let iid = &iid;
            scope.spawn(move || {
                for _ in 0..20 {
                    let items = vault.list_items(vid.clone()).expect("list");
                    assert_eq!(items.len(), 1);
                    assert_eq!(items[0].title, "Stable");
                    let detail = vault.get_item(vid.clone(), iid.clone()).expect("get");
                    assert_eq!(detail.title, "Stable");
                    assert_eq!(detail.username.as_deref(), Some("alice"));
                }
            });
        }
    });
}

#[test]
fn android_login_edit_and_trash_preserve_extra_fields() {
    use gitpass_mobile_ffi::{FfiField, FfiSection};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("fake-vault").to_str().unwrap().to_string();
    let password = "FAKE-test-password-only".to_string();
    let secret = create_account(root.clone(), password.clone()).unwrap();
    let vault = open_vault(root.clone(), password.clone(), secret.clone()).unwrap();
    let vid = vault.create_vault("Fake".into(), None).unwrap();
    let mut input = empty_input("Original");
    input.password = Some("FAKE-old".into());
    input.tags = vec!["tag".into()];
    input.favorite = true;
    input.sections = vec![FfiSection {
        id: "custom".into(),
        title: Some("Extra".into()),
        fields: vec![FfiField {
            id: "x".into(),
            label: "Extra".into(),
            kind: "concealed".into(),
            value: Some("FAKE-extra".into()),
        }],
    }];
    let id = vault.create_item(vid.clone(), input).unwrap();
    let old = vault.get_item(vid.clone(), id.clone()).unwrap();
    let mut sections = old.sections.clone();
    for section in &mut sections {
        if section.id == "login" {
            for field in &mut section.fields {
                if field.id == "password" {
                    field.value = Some("FAKE-new".into());
                }
            }
        }
    }
    vault
        .update_item(
            vid.clone(),
            id.clone(),
            ItemInput {
                title: "Edited".into(),
                username: old.username,
                password: Some("FAKE-new".into()),
                url: old.url,
                notes: old.notes,
                totp: old.totp,
                category: old.category,
                tags: old.tags,
                favorite: old.favorite,
                sections,
            },
        )
        .unwrap();
    vault.delete_item(vid.clone(), id.clone()).unwrap();
    assert!(vault.get_item(vid.clone(), id.clone()).unwrap().deleted);
    vault.restore_item(vid.clone(), id.clone()).unwrap();
    drop(vault);
    let reopened = open_vault(root, password, secret).unwrap();
    let edited = reopened.get_item(vid, id).unwrap();
    assert!(!edited.deleted);
    assert_eq!(edited.title, "Edited");
    assert_eq!(edited.password.as_deref(), Some("FAKE-new"));
    assert_eq!(edited.tags, vec!["tag"]);
    assert!(edited.favorite);
    assert!(edited
        .sections
        .iter()
        .flat_map(|s| &s.fields)
        .any(|f| f.id == "x" && f.value.as_deref() == Some("FAKE-extra")));
}

#[test]
fn android_tools_use_core_rfc6238_rollover_and_supported_generator_lengths() {
    let spec = gitpass_core::totp::parse("otpauth://totp/FAKE?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8&algorithm=SHA1&period=30").unwrap();
    assert_eq!(
        gitpass_core::totp::code_at(&spec, 59),
        ("94287082".into(), 1)
    );
    assert_eq!(
        gitpass_core::totp::code_at(&spec, 60),
        ("37359152".into(), 30)
    );
    assert_eq!(
        gitpass_core::totp::code_at(&spec, 59),
        ("94287082".into(), 1)
    );
    for length in [12, 20, 64] {
        let generated = generate_password(length);
        assert_eq!(generated.len(), length as usize);
        assert!(!generated.chars().any(char::is_whitespace));
    }
}

#[test]
fn first_pull_authenticates_whole_fixture_without_rewriting_encrypted_files() {
    use gitpass_mobile_ffi::validate_first_pull_snapshot;
    use std::{collections::BTreeMap, fs, path::Path};
    fn encrypted_files(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() { encrypted_files(root, &path, out); }
            else {
                let name = path.strip_prefix(root).unwrap().to_string_lossy().to_string();
                if name.ends_with(".enc") || name == "format.json" || name == "identity.pub" {
                    out.insert(name, fs::read(path).unwrap());
                }
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let password = "FAKE first pull fixture password".to_string();
    let secret = create_account(source.to_str().unwrap().into(), password.clone()).unwrap();
    let vault = open_vault(source.to_str().unwrap().into(), password.clone(), secret.clone()).unwrap();
    let vid = vault.create_vault("FAKE Personal".into(), None).unwrap();
    let iid = vault.create_item(vid.clone(), empty_input("FAKE imported item")).unwrap();
    vault.delete_item(vid.clone(), iid.clone()).unwrap(); // Trash must also authenticate.
    drop(vault);
    let mut original = BTreeMap::new();
    encrypted_files(&source, &source, &mut original);
    for variant in ["valid", "wrong-password", "wrong-secret", "keyset", "manifest", "item", "missing-item", "excessive-budget"] {
        let staging = dir.path().join(variant);
        for (name, bytes) in &original {
            let target = staging.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, bytes).unwrap();
        }
        match variant {
            "keyset" => fs::write(staging.join("keyset.enc"), b"FAKE tampered").unwrap(),
            "manifest" => fs::write(staging.join("manifest.enc"), b"FAKE tampered").unwrap(),
            "item" => fs::write(staging.join(format!("vaults/{vid}/items/{iid}.enc")), b"FAKE tampered").unwrap(),
            "missing-item" => fs::remove_file(staging.join(format!("vaults/{vid}/items/{iid}.enc"))).unwrap(),
            "excessive-budget" => {
                let path = staging.join("format.json");
                let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                value["kdf"]["m_kib"] = serde_json::json!(u32::MAX);
                fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            _ => (),
        }
        let result = validate_first_pull_snapshot(staging.to_str().unwrap().into(),
            if variant == "wrong-password" { "FAKE wrong password".into() } else { password.clone() },
            if variant == "wrong-secret" { "FAKE invalid Secret Key".into() } else { secret.clone() });
        assert_eq!(result.is_ok(), variant == "valid", "fixture variant {variant}");
        if variant == "valid" {
            let mut after = BTreeMap::new();
            encrypted_files(&staging, &staging, &mut after);
            assert_eq!(original, after, "validation must not rewrite encrypted data");
        }
        let mut preserved = BTreeMap::new();
        encrypted_files(&source, &source, &mut preserved);
        assert_eq!(original, preserved, "existing account must never be touched");
    }
}

/// Shared encrypted fixtures (issue #451): every committed fixture must
/// open through the exported bridge API and decrypt to its committed
/// `expected.json`. The bridge has no generator-history export, so that
/// section is covered by the core test only; everything else must match.
#[test]
fn shared_fixtures_match_expected() {
    use gitpass_core::merge::ItemRecord;
    use gitpass_core::test_fixtures;
    use std::path::Path;
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../crates/gitpass-core/tests/fixtures/shared");
    for v in test_fixtures::VERSIONS {
        let dir = base.join(test_fixtures::dir_name(v));
        let want: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap())
                .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let copy = tmp.path().join(test_fixtures::dir_name(v));
        test_fixtures::copy_fixture(&dir, &copy).unwrap();
        // Read the version BEFORE opening: a normal unlock re-wraps the
        // keyset to the production KDF and bumps format.json to v5.
        let fmt: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(copy.join("format.json")).unwrap())
                .unwrap();
        assert_eq!(fmt["version"].as_u64(), Some(v as u64), "v{v} format.json version");
        let secret = std::fs::read_to_string(copy.join("secret.key.txt")).unwrap();
        let vault = open_vault(
            copy.to_str().unwrap().to_string(),
            test_fixtures::FIXTURE_PASSWORD.to_string(),
            secret.trim().to_string(),
        )
        .unwrap();
        let mut vaults = vec![];
        for info in vault.list_vaults().unwrap() {
            let mut items = vec![];
            for sum in vault.list_items(info.id.clone()).unwrap() {
                let rec: ItemRecord = serde_json::from_str(
                    &vault.record_json(info.id.clone(), sum.id.clone()).unwrap(),
                )
                .unwrap();
                items.push(test_fixtures::semantic_record(&rec));
            }
            vaults.push(test_fixtures::vault_json(
                &info.name,
                info.description.as_deref(),
                info.icon.as_deref(),
                info.color.as_deref(),
                items,
            ));
        }
        vaults.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        let got = serde_json::json!({"format_version": fmt["version"], "vaults": vaults});
        let mut want_subset = want.clone();
        want_subset.as_object_mut().unwrap().remove("generator_history");
        assert_eq!(got, want_subset, "v{v} decrypted content via the bridge");
    }
}
