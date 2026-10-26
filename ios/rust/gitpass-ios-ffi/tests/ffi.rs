use gitpass_ios_ffi::{
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
    use gitpass_ios_ffi::{FfiField, FfiSection};
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
