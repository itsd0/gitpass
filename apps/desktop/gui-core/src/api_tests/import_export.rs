use super::*;


#[test]
fn import_file_dry_run_and_real() {
    let e = env();
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("chrome.csv");
    std::fs::write(&f, "name,url,username,password\nExample,https://a.example.invalid,alice,FAKE1\nOther,https://b.example.invalid,bob,FAKE2\n").unwrap();
    let r = e.app.import_file(&e.vault, "chrome-csv", &f.display().to_string(), true, None).unwrap();
    assert_eq!((r.parsed, r.added, r.duplicates), (2, 2, 0));
    assert_eq!(e.app.list_vaults().unwrap()[0].item_count, 0);
    let r = e.app.import_file(&e.vault, "chrome-csv", &f.display().to_string(), false, None).unwrap();
    assert_eq!(r.added, 2);
    assert!(e.app.onboarding().imported);
    let r = e.app.import_file(&e.vault, "chrome-csv", &f.display().to_string(), false, None).unwrap();
    assert_eq!((r.added, r.duplicates), (0, 2));
    assert!(e.app.import_file(&e.vault, "nope", &f.display().to_string(), true, None).is_err());
}

#[test]
fn import_file_password_protected_bitwarden_json() {
    let e = env();
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("bitwarden-password-protected.json");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../crates/gitpass-core/tests/fixtures/import/bitwarden-password-protected.json");
    std::fs::copy(fixture, &f).unwrap();
    let path = f.display().to_string();
    // Missing and wrong passwords fail with the core errors; nothing is written.
    let err = e.app.import_file(&e.vault, "bitwarden-json", &path, true, None).unwrap_err().to_string();
    assert!(err.contains("export password is required"), "{err}");
    let err = e.app.import_file(&e.vault, "bitwarden-json", &path, true, Some("wrong")).unwrap_err().to_string();
    assert!(err.contains("wrong export password"), "{err}");
    assert_eq!(e.app.list_vaults().unwrap()[0].item_count, 0);
    // The export password decrypts the preview, and the real import stores the items.
    let r = e.app.import_file(&e.vault, "bitwarden-json", &path, true, Some("fake-export-password")).unwrap();
    assert_eq!((r.parsed, r.added, r.duplicates), (2, 2, 0));
    assert_eq!(r.categories.get("login"), Some(&1));
    let r = e.app.import_file(&e.vault, "bitwarden-json", &path, false, Some("fake-export-password")).unwrap();
    assert_eq!(r.added, 2);
}

#[test]
fn import_file_apple_passwords_csv() {
    let e = env();
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("apple.csv");
    std::fs::write(&f, "Title,URL,Username,Password,Notes,OTPAuth\nApple Service,https://apple.example.invalid,alice,FAKE1,Notes line,otpauth://totp/Apple:alice?secret=JBSWY3DPEHPK3PXP\n").unwrap();
    let r = e.app.import_file(&e.vault, "apple-passwords-csv", &f.display().to_string(), true, None).unwrap();
    assert_eq!((r.parsed, r.added, r.duplicates), (1, 1, 0));
    assert_eq!(e.app.list_vaults().unwrap()[0].item_count, 0);
    let r = e.app.import_file(&e.vault, "apple-passwords-csv", &f.display().to_string(), false, None).unwrap();
    assert_eq!(r.added, 1);
    let items = e.app.list_items(&Filter { kind: FilterKind::All, vault_id: Some(e.vault.clone()), tag: None, category: None, query: None, sort: None }).unwrap();
    let it = items.iter().find(|i| i.title == "Apple Service").unwrap();
    assert_eq!(it.category, Category::Login);
    let d = e.app.get_item(&e.vault, &it.id).unwrap();
    assert!(d.has_totp);
}

#[test]
fn export_vault_round_trip_and_errors() {
    let e = env();
    let _l = login(&e, "Export Login", "alice@example.invalid", "FAKE-Strong-Export-1!");
    let _c = card(&e); // "Example Bank"
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("vault.json");
    let r = e.app.export_items(&ExportRequest { vault_id: Some(e.vault.clone()), items: None, format: "gitpass-json".into(), encrypted: false, passphrase: None, path: plain.display().to_string() }).unwrap();
    assert_eq!((r.items, r.encrypted, r.format.as_str()), (2, false, "gitpass-json"));
    assert_eq!(r.path, std::path::absolute(&plain).unwrap().display().to_string());
    let body = std::fs::read_to_string(&plain).unwrap();
    assert!(body.contains("FAKE-Strong-Export-1!") && body.contains("4111 1111 1111 1111") && body.contains("\"gitpass_export\": 1"));
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; assert_eq!(std::fs::metadata(&plain).unwrap().permissions().mode() & 0o777, 0o600); }
    // round-trip export -> GUI import into a fresh vault
    let v2 = e.app.create_vault("Imported", None, None, None).unwrap();
    let ir = e.app.import_file(&v2.id, "gitpass-json", &plain.display().to_string(), false, None).unwrap();
    assert_eq!(ir.added, 2);
    let titles: Vec<String> = e.app.list_items(&Filter { kind: FilterKind::All, vault_id: Some(v2.id.clone()), tag: None, category: None, query: None, sort: None }).unwrap().into_iter().map(|i| i.title).collect();
    assert!(titles.contains(&"Export Login".to_string()) && titles.contains(&"Example Bank".to_string()));
    // encrypted export decrypts with the passphrase (and only with it)
    let enc = dir.path().join("vault.enc.json");
    let r = e.app.export_items(&ExportRequest { vault_id: Some(e.vault.clone()), items: None, format: "gitpass-json".into(), encrypted: true, passphrase: Some("fake-export-passphrase".into()), path: enc.display().to_string() }).unwrap();
    assert!(r.encrypted);
    let body = std::fs::read_to_string(&enc).unwrap();
    assert!(gitpass_core::import::is_encrypted_export(&body));
    assert_eq!(import::parse(ImportFormat::GitpassJson, &body, Some("fake-export-passphrase")).unwrap().items.len(), 2);
    assert!(import::parse(ImportFormat::GitpassJson, &body, Some("wrong-passphrase")).is_err());
    // invalid combinations fail before writing
    let bad = |req: ExportRequest| e.app.export_items(&req).is_err();
    assert!(bad(ExportRequest { vault_id: Some(e.vault.clone()), items: None, format: "csv".into(), encrypted: true, passphrase: Some("fake-export-passphrase".into()), path: plain.display().to_string() }));
    assert!(bad(ExportRequest { vault_id: Some(e.vault.clone()), items: None, format: "gitpass-json".into(), encrypted: true, passphrase: Some("short".into()), path: plain.display().to_string() }));
    assert!(bad(ExportRequest { vault_id: None, items: None, format: "gitpass-json".into(), encrypted: false, passphrase: None, path: plain.display().to_string() }));
    assert!(bad(ExportRequest { vault_id: Some(e.vault.clone()), items: None, format: "xml".into(), encrypted: false, passphrase: None, path: plain.display().to_string() }));
    assert!(bad(ExportRequest { vault_id: Some(e.vault.clone()), items: Some(vec![]), format: "gitpass-json".into(), encrypted: false, passphrase: None, path: plain.display().to_string() }));
}

#[test]
fn export_selected_cross_vault_csv_and_duplicates() {
    let e = env();
    let a = login(&e, "Selected Login", "alice@example.invalid", "FAKE-Selected-Pass-1!");
    let v2 = e.app.create_vault("Second", None, None, None).unwrap();
    let b = e.app.create_item(&ItemInput { vault_id: v2.id.clone(), category: Category::Login, title: "Second Login".into(), url: Some("https://second.example.invalid".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some("bob@example.invalid")), fld("password", FieldKind::Concealed, Some("FAKE-Second-Pass-2!"))] }],
        notes: None, tags: None, favorite: None }).unwrap();
    let _unselected = login(&e, "Unselected", "carol@example.invalid", "FAKE-Unselected-Pass-3!");
    let ref_a = ItemRef { vault_id: e.vault.clone(), item_id: a.id.clone() };
    let ref_b = ItemRef { vault_id: v2.id.clone(), item_id: b.id.clone() };
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("selected.json");
    let r = e.app.export_items(&ExportRequest { vault_id: None, items: Some(vec![ref_a.clone(), ref_b.clone()]), format: "gitpass-json".into(), encrypted: false, passphrase: None, path: json.display().to_string() }).unwrap();
    assert_eq!(r.items, 2);
    let raw = std::fs::read_to_string(&json).unwrap();
    assert!(raw.contains("Selected Login") && raw.contains("Second Login") && !raw.contains("Unselected"));
    assert_eq!(import::parse(ImportFormat::GitpassJson, &raw, None).unwrap().items.len(), 2);
    // duplicate refs export once
    let dup = dir.path().join("dupe.json");
    let r = e.app.export_items(&ExportRequest { vault_id: None, items: Some(vec![ref_a.clone(), ref_a.clone()]), format: "gitpass-json".into(), encrypted: false, passphrase: None, path: dup.display().to_string() }).unwrap();
    assert_eq!(r.items, 1);
    // CSV is login fields only and only the selected login
    let csv = dir.path().join("selected.csv");
    let r = e.app.export_items(&ExportRequest { vault_id: None, items: Some(vec![ref_a.clone()]), format: "csv".into(), encrypted: false, passphrase: None, path: csv.display().to_string() }).unwrap();
    assert_eq!((r.items, r.format.as_str(), r.encrypted), (1, "csv", false));
    let body = std::fs::read_to_string(&csv).unwrap();
    assert!(body.starts_with("title,username,password,url,notes,totp,updated_at") && body.contains("FAKE-Selected-Pass-1!") && !body.contains("Second Login"));
    // soft-deleted selections are skipped; an all-deleted selection is an error
    e.app.delete_item(&v2.id, &b.id).unwrap();
    assert!(e.app.export_items(&ExportRequest { vault_id: None, items: Some(vec![ref_b]), format: "gitpass-json".into(), encrypted: false, passphrase: None, path: dup.display().to_string() }).is_err());
}
