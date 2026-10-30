use super::*;


#[test]
fn status_lock_unlock_and_vaults() {
    let e = env();
    let vs = e.app.list_vaults().unwrap();
    assert_eq!(vs.len(), 1);
    assert_eq!((vs[0].icon.as_deref(), vs[0].color.as_deref(), vs[0].item_count, vs[0].shared), (None, None, 0, false));
    let path = e.app.status().unwrap().vault_path;
    e.app.lock();
    let st = e.app.status().unwrap();
    assert!(!st.unlocked);
    assert_eq!(st.vault_path, path, "last vault path remembered in settings file");
    assert!(e.app.list_vaults().unwrap_err().to_string().contains("locked"));
    assert!(e.app.unlock(&path, "wrong", Some(&e.sk)).is_err());
    assert!(e.app.unlock(&path, "correct horse", Some(&e.sk)).unwrap().unlocked);
}

#[test]
fn items_crud_subtitles_and_no_secret_leak() {
    let e = env();
    let l = login(&e, "Example Mail", "alice@example.invalid", "FAKE-very-secret-pw-123!");
    assert_eq!(l.subtitle, "alice@example.invalid");
    let c = card(&e);
    assert_eq!(c.subtitle, "Visa •••• 1111");
    { let d = e.app.get_item(&e.vault, &c.id).unwrap();
      let num = d.sections.iter().flat_map(|s| &s.fields).find(|f| f.id == "number").unwrap();
      assert_eq!((num.value.as_deref(), num.masked.as_deref()), (None, Some("•••• •••• •••• 1111")));
      assert!(d.sections.iter().flat_map(|s| &s.fields).filter(|f| f.id != "number").all(|f| f.masked.is_none()), "cvv/pin stay fully masked"); }
    assert!(c.favorite);
    let d = e.app.get_item(&e.vault, &c.id).unwrap();
    let json = serde_json::to_string(&d).unwrap();
    assert!(!json.contains("4111 1111") && !json.contains("\"123\""), "{json}");
    assert!(json.contains("\"vaultId\"") && json.contains("\"passwordStrength\"") && json.contains("\"hasTotp\""));
    assert_eq!(d.sections[0].fields[2].value.as_deref(), Some("12/30"));
    let ld = e.app.get_item(&e.vault, &l.id).unwrap();
    assert!(ld.password_strength.unwrap() > 50);
    assert!(!serde_json::to_string(&e.app.list_items(&Filter { kind: FilterKind::All, vault_id: None, tag: None, category: None, query: None, sort: None }).unwrap()).unwrap().contains("FAKE-very"));
    assert_eq!(&*e.app.reveal_field(&e.vault, &c.id, "cvv").unwrap(), "123");
    assert_eq!(&*e.app.reveal_field(&e.vault, &l.id, "password").unwrap(), "FAKE-very-secret-pw-123!");
    // update: secret fields sent back as null keep their value; title change adds history
    let mut secs = d.sections.clone();
    secs[0].fields.push(fld("pin", FieldKind::Concealed, Some("9876")));
    e.app.update_item(&c.id, &ItemInput { vault_id: e.vault.clone(), category: Category::CreditCard, title: "Example Bank Visa".into(), sections: secs,
        notes: None, tags: Some(vec!["finance".into()]), favorite: None, url: None }).unwrap();
    assert_eq!(&*e.app.reveal_field(&e.vault, &c.id, "number").unwrap(), "4111 1111 1111 1111");
    assert_eq!(&*e.app.reveal_field(&e.vault, &c.id, "pin").unwrap(), "9876");
    let d2 = e.app.get_item(&e.vault, &c.id).unwrap();
    assert_eq!(d2.summary.title, "Example Bank Visa");
    assert_eq!(d2.history.len(), 1);
    let old = e.app.item_version(&e.vault, &c.id, d2.history[0].version).unwrap();
    assert_eq!(old.summary.title, "Example Bank");
    assert!(old.sections.iter().flat_map(|s| &s.fields).any(|f| f.id == "pin" && f.diff.as_deref() == Some("added")));
    // restore version 1
    let restored = e.app.restore_item_version(&e.vault, &c.id, d2.history[0].version).unwrap();
    assert_eq!(restored.summary.title, "Example Bank");
    assert_eq!(restored.history.len(), 2);
    // verifying get_item matches restored
    let cur = e.app.get_item(&e.vault, &c.id).unwrap();
    assert_eq!(cur.summary.title, "Example Bank");
    assert_eq!(cur.history.len(), 2);
    // history entry 0 is the pre-restore version (version 2: Example Bank Visa)
    let pre_restore_snap = e.app.item_version(&e.vault, &c.id, cur.history[0].version).unwrap();
    assert_eq!(pre_restore_snap.summary.title, "Example Bank Visa");
    // reveal historical concealed field
    assert_eq!(&*e.app.reveal_field_version(&e.vault, &c.id, "cvv", Some(1)).unwrap(), "123");
    // undo restore (restore version 2)
    let undone = e.app.restore_item_version(&e.vault, &c.id, cur.history[0].version).unwrap();
    assert_eq!(undone.summary.title, "Example Bank Visa");
    // test error cases
    assert!(e.app.restore_item_version(&e.vault, &c.id, 9999).is_err());
    e.app.delete_item(&e.vault, &c.id).unwrap();
    assert!(e.app.restore_item_version(&e.vault, &c.id, 1).is_err());
    e.app.restore_item(&e.vault, &c.id).unwrap();
    assert_eq!(e.app.list_tags().unwrap(), vec!["finance".to_string(), "work".to_string()]);
    // invalid input
    assert!(e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::SecureNote, title: " ".into(), sections: vec![], notes: None, tags: None, favorite: None, url: None }).is_err());
}

#[test]
fn filters_favorite_archive_delete_restore_purge_move_duplicate() {
    let e = env();
    let a = login(&e, "Alpha", "a", "pw-a-FAKE-xx");
    let b = card(&e);
    let n = e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::SecureNote, title: "Note".into(),
        sections: vec![Section { id: "s".into(), title: None, fields: vec![fld("body", FieldKind::Multiline, Some("hello"))] }], notes: None, tags: None, favorite: None, url: None }).unwrap();
    assert_eq!(n.subtitle, "hello");
    let f = |kind: FilterKind| Filter { kind, vault_id: Some(e.vault.clone()), tag: Some("work".into()), category: (kind == FilterKind::Category).then_some(Category::CreditCard), query: None, sort: Some(Sort::Title) };
    let ids = |v: Vec<ItemSummary>| v.into_iter().map(|s| s.title).collect::<Vec<_>>();
    assert_eq!(ids(e.app.list_items(&f(FilterKind::All)).unwrap()), vec!["Alpha", "Example Bank", "Note"]);
    assert_eq!(ids(e.app.list_items(&f(FilterKind::Favorites)).unwrap()), vec!["Example Bank"]);
    assert_eq!(ids(e.app.list_items(&f(FilterKind::Tag)).unwrap()), vec!["Alpha"]);
    assert_eq!(ids(e.app.list_items(&f(FilterKind::Category)).unwrap()), vec!["Example Bank"]);
    assert_eq!(ids(e.app.list_items(&f(FilterKind::Vault)).unwrap()).len(), 3);
    let mut q = f(FilterKind::All); q.query = Some("alp".into());
    assert_eq!(ids(e.app.list_items(&q).unwrap()), vec!["Alpha"]);
    e.app.set_favorite(&e.vault, &a.id, true).unwrap();
    assert_eq!(e.app.list_items(&f(FilterKind::Favorites)).unwrap().len(), 2);
    e.app.archive_item(&e.vault, &a.id, true).unwrap();
    assert_eq!(ids(e.app.list_items(&f(FilterKind::Archive)).unwrap()), vec!["Alpha"]);
    assert_eq!(e.app.list_items(&f(FilterKind::All)).unwrap().len(), 2);
    e.app.delete_item(&e.vault, &n.id).unwrap();
    let del = e.app.list_items(&f(FilterKind::Deleted)).unwrap();
    assert_eq!(del.len(), 1);
    assert!(del[0].deleted_at.is_some());
    assert_eq!(e.app.list_vaults().unwrap()[0].item_count, 2);
    e.app.restore_item(&e.vault, &n.id).unwrap();
    assert!(e.app.list_items(&f(FilterKind::Deleted)).unwrap().is_empty());
    e.app.delete_item(&e.vault, &n.id).unwrap();
    e.app.purge_item(&e.vault, &n.id).unwrap();
    assert!(e.app.get_item(&e.vault, &n.id).is_err());
    let dup = e.app.duplicate_item(&e.vault, &b.id).unwrap();
    assert_eq!(dup.title, "Example Bank copy");
    assert_eq!(&*e.app.reveal_field(&e.vault, &dup.id, "cvv").unwrap(), "123");
    let v2 = e.app.create_vault("Shared", None, None, None).unwrap();
    let moved = e.app.move_item(&e.vault, &b.id, &v2.id).unwrap();
    assert_eq!(moved.vault_id, v2.id);
    assert!(e.app.get_item(&e.vault, &b.id).is_err());
    assert_eq!(&*e.app.reveal_field(&v2.id, &moved.id, "number").unwrap(), "4111 1111 1111 1111");
}

#[test]
fn empty_trash_purges_only_trashed_items_in_one_commit() {
    let e = env();
    let active = login(&e, "Active", "active", "pw-FAKE-active");
    let archived = login(&e, "Archived", "archived", "pw-FAKE-archived");
    let trashed = login(&e, "Trashed", "trashed", "pw-FAKE-trashed");
    let trashed_archived = login(&e, "Trashed archived", "ta", "pw-FAKE-ta");
    e.app.archive_item(&e.vault, &archived.id, true).unwrap();
    e.app.delete_item(&e.vault, &trashed.id).unwrap();
    e.app.delete_item(&e.vault, &trashed_archived.id).unwrap();
    e.app.archive_item(&e.vault, &trashed_archived.id, true).unwrap();
    // a second vault proves the purge spans vaults in one commit
    let v2 = e.app.create_vault("Other", None, None, None).unwrap();
    let elsewhere = e.app.create_item(&ItemInput { vault_id: v2.id.clone(), category: Category::SecureNote, title: "Elsewhere".into(),
        sections: vec![Section { id: "s".into(), title: None, fields: vec![fld("body", FieldKind::Multiline, Some("FAKE"))] }],
        notes: None, tags: None, favorite: None, url: None }).unwrap();
    e.app.delete_item(&v2.id, &elsewhere.id).unwrap();

    let epoch = e.app.session_epoch().unwrap();
    assert_eq!(e.app.empty_trash().unwrap(), 3);
    assert_eq!(e.app.session_epoch().unwrap(), epoch + 1, "all purges land in one commit");
    for id in [&active.id, &archived.id] { assert!(e.app.get_item(&e.vault, id).is_ok(), "live items untouched"); }
    for id in [&trashed.id, &trashed_archived.id] { assert!(e.app.get_item(&e.vault, id).is_err()); }
    assert!(e.app.get_item(&v2.id, &elsewhere.id).is_err());
    assert!(e.app.list_items(&Filter { kind: FilterKind::Deleted, vault_id: None, tag: None, category: None, query: None, sort: None }).unwrap().is_empty());

    // no-op on an empty trash: nothing purged, no extra commit
    let epoch = e.app.session_epoch().unwrap();
    assert_eq!(e.app.empty_trash().unwrap(), 0);
    assert_eq!(e.app.session_epoch().unwrap(), epoch);
}

#[test]
fn attachments_via_paths() {
    let e = env();
    let n = card(&e);
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("statement.pdf");
    std::fs::write(&src, b"%PDF-fake").unwrap();
    let a = e.app.add_attachment(&e.vault, &n.id, &src.display().to_string()).unwrap();
    assert_eq!((a.name.as_str(), a.size, a.mime.as_str()), ("statement.pdf", 9, "application/pdf"));
    assert_eq!(e.app.get_item(&e.vault, &n.id).unwrap().attachments, vec![a.clone()]);
    let dst = dir.path().join("out.pdf");
    e.app.save_attachment(&e.vault, &n.id, &a.id, &dst.display().to_string()).unwrap();
    assert_eq!(std::fs::read(&dst).unwrap(), b"%PDF-fake");
    e.app.remove_attachment(&e.vault, &n.id, &a.id).unwrap();
    assert!(e.app.get_item(&e.vault, &n.id).unwrap().attachments.is_empty());
    assert!(e.app.remove_attachment(&e.vault, &n.id, &a.id).is_err());
}

/// Build a generator request the way the desktop commands do (length given, named fields).

#[test]
fn contract_json_shapes() {
    let e = env();
    let l = login(&e, "Alpha", "a", "pw-FAKE");
    let v = serde_json::to_value(&l).unwrap();
    for k in ["id", "vaultId", "category", "title", "subtitle", "favorite", "archived", "deletedAt", "tags", "updatedAt", "createdAt", "url", "expiry"] { assert!(v.get(k).is_some(), "{k}"); }
    assert_eq!(v["category"], "login");
    assert!(v["expiry"].is_null());
    let c = card(&e);
    let vc = serde_json::to_value(&c).unwrap();
    assert_eq!(vc["expiry"], "12/30");
    let d = serde_json::to_value(e.app.get_item(&e.vault, &l.id).unwrap()).unwrap();
    for k in ["sections", "notes", "attachments", "history", "passwordStrength", "hasTotp", "vaultId", "expiry"] { assert!(d.get(k).is_some(), "{k}"); }
    assert!(d["sections"][0]["fields"][1]["value"].is_null());
    let vi = serde_json::to_value(&e.app.list_vaults().unwrap()[0]).unwrap();
    for k in ["id", "name", "description", "itemCount", "shared", "members", "icon", "color"] { assert!(vi.get(k).is_some(), "{k}"); }
    let f: Filter = serde_json::from_str(r#"{"kind":"category","category":"credit_card","sort":"created"}"#).unwrap();
    assert_eq!(f.category, Some(Category::CreditCard));
    let inp: ItemInput = serde_json::from_str(r#"{"vaultId":"x","category":"wifi","title":"t","sections":[{"id":"s","title":null,"fields":[{"id":"a","label":"A","kind":"month_year","value":"01/30"}]}]}"#).unwrap();
    assert_eq!(inp.sections[0].fields[0].kind, FieldKind::MonthYear);
    let w = serde_json::to_value(e.app.watchtower(false).unwrap()).unwrap();
    for k in ["score", "weak", "reused", "old", "breached", "missingTotp", "expired", "expiringSoon", "insecureUrls", "duplicates", "checkedAt", "hibpChecked"] { assert!(w.get(k).is_some(), "{k}"); }
}

#[test]
fn item_summary_carries_expiry_for_card_and_api_credential_only() {
    let e = env();
    let l = login(&e, "Login Item", "user@example.invalid", "super-secret-login-password");
    let c = card(&e);
    let a = api_cred(&e, "Cloud API Token", "svc-deploy", "sk_live_FAKE_SECRET_KEY_12345678", "2027-12-31");

    // 1. Rust struct values
    assert_eq!(l.expiry, None);
    assert_eq!(c.expiry.as_deref(), Some("12/30"));
    assert_eq!(a.expiry.as_deref(), Some("2027-12-31"));

    // 2. list_items carries expiry for card and api_credential only
    let items = e.app.list_items(&Filter { kind: FilterKind::All, vault_id: None, tag: None, category: None, query: None, sort: None }).unwrap();
    let find = |id: &str| items.iter().find(|i| i.id == id).unwrap();
    assert_eq!(find(&l.id).expiry, None);
    assert_eq!(find(&c.id).expiry.as_deref(), Some("12/30"));
    assert_eq!(find(&a.id).expiry.as_deref(), Some("2027-12-31"));

    // 3. Serialized JSON contains expiry, and NO secret values (password, card number, CVV, credential)
    for (item, secret_values) in [
        (&l, vec!["super-secret-login-password"]),
        // CVV is matched as a whole JSON string: a bare "123" also appears in random ids/timestamps.
        (&c, vec!["4111 1111 1111 1111", "\"123\""]),
        (&a, vec!["sk_live_FAKE_SECRET_KEY_12345678"]),
    ] {
        let json_str = serde_json::to_string(item).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        for secret in secret_values {
            assert!(!json_str.contains(secret), "serialized summary must not leak secret '{secret}'");
        }

        match item.category {
            Category::CreditCard => assert_eq!(json_val["expiry"], "12/30"),
            Category::ApiCredential => assert_eq!(json_val["expiry"], "2027-12-31"),
            _ => assert!(json_val["expiry"].is_null()),
        }
    }

    // 4. Removed fields or empty values give None
    let empty_expiry_card = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::CreditCard,
        title: "Empty Expiry Card".into(),
        url: None,
        sections: vec![Section { id: "card".into(), title: None, fields: vec![
            fld("number", FieldKind::Concealed, Some("4111 2222 3333 4444")),
            fld("expiry", FieldKind::MonthYear, Some("   ")),
        ] }],
        notes: None, tags: None, favorite: None,
    }).unwrap();
    assert_eq!(empty_expiry_card.expiry, None);

    // 5. Non-card/non-api categories (e.g. secure_note, identity) with an expiry field still give None
    let note = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::SecureNote,
        title: "Note with Expiry Field".into(),
        url: None,
        sections: vec![Section { id: "main".into(), title: None, fields: vec![
            fld("expiry", FieldKind::Text, Some("12/30")),
        ] }],
        notes: None, tags: None, favorite: None,
    }).unwrap();
    assert_eq!(note.expiry, None);
}

#[test]
fn update_item_assigns_ids_for_new_fields_and_keeps_null_secrets() {
    let e = env();
    let c = card(&e);
    let d = e.app.get_item(&e.vault, &c.id).unwrap();
    // UI shape: unchanged concealed fields come back value=null; new field + new section have id ""
    let mut secs = d.sections.clone();
    for f in &mut secs[0].fields { if f.kind == FieldKind::Concealed { assert!(f.value.is_none()); } }
    secs[0].fields.push(fld("", FieldKind::Concealed, Some("2468")));
    secs[0].fields.push(fld("", FieldKind::Text, Some("note-a")));
    secs.push(Section { id: "".into(), title: Some("Extra".into()), fields: vec![fld("", FieldKind::Concealed, Some("s3cr3t-extra"))] });
    e.app.update_item(&c.id, &ItemInput { vault_id: e.vault.clone(), category: Category::CreditCard, title: "Example Bank".into(), sections: secs,
        notes: None, tags: None, favorite: None, url: None }).unwrap();
    assert_eq!(&*e.app.reveal_field(&e.vault, &c.id, "number").unwrap(), "4111 1111 1111 1111");
    assert_eq!(&*e.app.reveal_field(&e.vault, &c.id, "cvv").unwrap(), "123");
    let d2 = e.app.get_item(&e.vault, &c.id).unwrap();
    let all: Vec<_> = d2.sections.iter().flat_map(|s| s.fields.iter()).collect();
    assert!(all.iter().all(|f| !f.id.is_empty()) && d2.sections.iter().all(|s| !s.id.is_empty()));
    let mut ids: Vec<_> = all.iter().map(|f| f.id.clone()).collect(); ids.sort(); ids.dedup();
    assert_eq!(ids.len(), all.len());
    let pin = all.iter().find(|f| f.kind == FieldKind::Concealed && f.id.starts_with("f_") && d2.sections[0].fields.iter().any(|g| g.id == f.id)).unwrap();
    assert_eq!(&*e.app.reveal_field(&e.vault, &c.id, &pin.id).unwrap(), "2468");
    let extra = d2.sections.iter().find(|s| s.title.as_deref() == Some("Extra")).unwrap();
    assert_eq!(&*e.app.reveal_field(&e.vault, &c.id, &extra.fields[0].id).unwrap(), "s3cr3t-extra");
    assert!(all.iter().any(|f| f.value.as_deref() == Some("note-a")));
    // create_item with empty ids works too
    let n = e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::ApiCredential, title: "API (fake)".into(), url: None,
        sections: vec![Section { id: "".into(), title: None, fields: vec![fld("", FieldKind::Concealed, Some("k1")), fld("", FieldKind::Concealed, Some("k2"))] }],
        notes: None, tags: None, favorite: None }).unwrap();
    let nd = e.app.get_item(&e.vault, &n.id).unwrap();
    assert_eq!(nd.sections[0].fields.len(), 2);
    assert_ne!(nd.sections[0].fields[0].id, nd.sections[0].fields[1].id);
}

#[test]
fn api_credential_masked_hint_in_summary_and_detail_without_secret_leak() {
    let e = env();
    const LONG_KEY: &str = "sk-live-0123456789abcdefgh";
    let api = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::ApiCredential,
        title: "Stripe Production".into(),
        url: None,
        sections: vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![
                fld("username", FieldKind::Text, Some("svc-payments")),
                fld("credential", FieldKind::Concealed, Some(LONG_KEY)),
            ],
        }],
        notes: None,
        tags: None,
        favorite: None,
    }).unwrap();

    let d = e.app.get_item(&e.vault, &api.id).unwrap();
    assert_eq!(d.summary.subtitle, "sk-l••••••••efgh");
    let cred_field = d.sections.iter().flat_map(|s| &s.fields).find(|f| f.id == "credential").unwrap();
    assert_eq!(cred_field.masked.as_deref(), Some("sk-l••••••••efgh"));
    assert_eq!(cred_field.value, None);

    // Assert full secret never appears in serialized DTO
    let json_str = serde_json::to_string(&d).unwrap();
    assert!(!json_str.contains("0123456789abcd"), "secret middle leaked in JSON: {json_str}");

    // Reveal returns the full secret unchanged
    let revealed = e.app.reveal_field(&e.vault, &api.id, "credential").unwrap();
    assert_eq!(&*revealed, LONG_KEY);

    // Also test short and tiny keys
    let short_api = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::ApiCredential,
        title: "Short API".into(),
        url: None,
        sections: vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![fld("credential", FieldKind::Concealed, Some("short-12345"))],
        }],
        notes: None,
        tags: None,
        favorite: None,
    }).unwrap();
    let short_d = e.app.get_item(&e.vault, &short_api.id).unwrap();
    assert_eq!(short_d.summary.subtitle, "sh••••••••");
    assert_eq!(short_d.sections[0].fields[0].masked.as_deref(), Some("sh••••••••"));
    assert_eq!(short_d.sections[0].fields[0].value, None);

    let tiny_api = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::ApiCredential,
        title: "Tiny API".into(),
        url: None,
        sections: vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![fld("credential", FieldKind::Concealed, Some("tiny"))],
        }],
        notes: None,
        tags: None,
        favorite: None,
    }).unwrap();
    let tiny_d = e.app.get_item(&e.vault, &tiny_api.id).unwrap();
    assert_eq!(tiny_d.summary.subtitle, "••••••••");
    assert_eq!(tiny_d.sections[0].fields[0].masked.as_deref(), Some("••••••••"));
    assert_eq!(tiny_d.sections[0].fields[0].value, None);
}

#[test]
fn vault_update_and_delete() {
    let e = env();
    assert!(e.app.delete_vault(&e.vault, "whatever").is_err()); // last vault / wrong name
    let v2 = e.app.create_vault("Temp", None, None, None).unwrap();
    let u = e.app.update_vault(&v2.id, "Renamed", Some("d".into()), Some("work".into()), Some("#ff0000".into())).unwrap();
    assert_eq!((u.name.as_str(), u.icon.as_deref(), u.color.as_deref()), ("Renamed", Some("work"), Some("#ff0000")));
    assert!(e.app.delete_vault(&v2.id, "Temp").is_err());
    e.app.delete_vault(&v2.id, "Renamed").unwrap();
    let vs = e.app.list_vaults().unwrap();
    assert_eq!(vs.len(), 1);
    assert!(e.app.delete_vault(&vs[0].id, &vs[0].name).is_err());
}

#[test]
fn bulk_items_actions() {
    let e = env();
    let a = login(&e, "Alpha", "a", "FAKE-1");
    let b = login(&e, "Beta", "b", "FAKE-2");
    let c = login(&e, "Gamma", "c", "FAKE-3");
    let r = |x: &ItemSummary| ItemRef { vault_id: e.vault.clone(), item_id: x.id.clone() };
    let req = |action: &str, items: Vec<ItemRef>| BulkRequest { action: action.into(), items, tag: None, to_vault_id: None };
    let f = |kind: FilterKind| Filter { kind, vault_id: None, tag: None, category: None, query: None, sort: None };
    assert_eq!(e.app.bulk_items(&req("favorite", vec![r(&a), r(&b)])).unwrap().count, 2);
    assert_eq!(e.app.list_items(&f(FilterKind::Favorites)).unwrap().len(), 2);
    let mut t = req("add_tag", vec![r(&a), r(&c)]); t.tag = Some("  bulk ".into());
    assert_eq!(e.app.bulk_items(&t).unwrap().count, 2);
    assert_eq!(e.app.list_tags().unwrap(), vec!["bulk", "work"]);
    t.tag = Some(" ".into());
    assert!(e.app.bulk_items(&t).is_err());
    t.action = "remove_tag".into(); t.tag = Some("bulk".into()); t.items = vec![r(&c)];
    e.app.bulk_items(&t).unwrap();
    assert!(e.app.get_item(&e.vault, &c.id).unwrap().summary.tags == vec!["work".to_string()]);
    e.app.bulk_items(&req("archive", vec![r(&a), r(&b)])).unwrap();
    assert_eq!(e.app.list_items(&f(FilterKind::Archive)).unwrap().len(), 2);
    e.app.bulk_items(&req("unarchive", vec![r(&a), r(&b)])).unwrap();
    e.app.bulk_items(&req("trash", vec![r(&a), r(&b), r(&c)])).unwrap();
    assert_eq!(e.app.list_items(&f(FilterKind::Deleted)).unwrap().len(), 3);
    e.app.bulk_items(&req("restore", vec![r(&c)])).unwrap();
    assert_eq!(e.app.bulk_items(&req("purge", vec![r(&a), r(&b)])).unwrap().count, 2);
    assert!(e.app.list_items(&f(FilterKind::Deleted)).unwrap().is_empty());
    let v2 = e.app.create_vault("Second", None, None, None).unwrap();
    let mut m = req("copy", vec![r(&c)]); m.to_vault_id = Some(v2.id.clone());
    let cp = e.app.bulk_items(&m).unwrap();
    assert_eq!(cp.items[0].vault_id, v2.id);
    assert_eq!(&*e.app.reveal_field(&v2.id, &cp.items[0].item_id, "password").unwrap(), "FAKE-3");
    m.action = "move".into();
    let mv = e.app.bulk_items(&m).unwrap();
    assert!(e.app.get_item(&e.vault, &c.id).is_err());
    assert_eq!(e.app.get_item(&v2.id, &mv.items[0].item_id).unwrap().summary.title, "Gamma");
    m.to_vault_id = Some("nope".into());
    assert!(e.app.bulk_items(&m).is_err());
    assert!(e.app.bulk_items(&req("explode", vec![])).unwrap().count == 0);
    assert!(e.app.bulk_items(&req("explode", vec![r(&a)])).is_err());
}

#[test]
fn tag_rename_merge_delete_and_lifecycle() {
    let e = env();
    // 1. Setup items across active, archived, and trashed states
    let item1 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Login, title: "Item 1".into(),
        url: None, sections: vec![], notes: None, tags: Some(vec!["alpha".into(), "beta".into()]), favorite: None,
    }).unwrap();

    let item2 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Login, title: "Item 2".into(),
        url: None, sections: vec![], notes: None, tags: Some(vec!["alpha".into()]), favorite: None,
    }).unwrap();
    e.app.archive_item(&e.vault, &item2.id, true).unwrap();

    let item3 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Login, title: "Item 3".into(),
        url: None, sections: vec![], notes: None, tags: Some(vec!["alpha".into(), "gamma".into()]), favorite: None,
    }).unwrap();
    e.app.delete_item(&e.vault, &item3.id).unwrap();

    let item4 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Login, title: "Item 4".into(),
        url: None, sections: vec![], notes: None, tags: Some(vec!["beta".into()]), favorite: None,
    }).unwrap();

    assert_eq!(e.app.list_tags().unwrap(), vec!["alpha", "beta"]);

    // Validation errors
    assert!(e.app.rename_tag("", "new").is_err());
    assert!(e.app.rename_tag("alpha", "").is_err());
    assert!(e.app.delete_tag("").is_err());

    // No-op rename to same name
    let noop = e.app.rename_tag("alpha", "alpha").unwrap();
    assert_eq!(noop.count, 0);
    assert!(noop.items.is_empty());

    // No-op on unknown tag
    let unknown_rename = e.app.rename_tag("nonexistent", "anything").unwrap();
    assert_eq!(unknown_rename.count, 0);
    assert!(unknown_rename.items.is_empty());

    let unknown_delete = e.app.delete_tag("nonexistent").unwrap();
    assert_eq!(unknown_delete.count, 0);
    assert!(unknown_delete.items.is_empty());

    // 2. Rename "alpha" -> "delta" across all items
    let res = e.app.rename_tag("alpha", "delta").unwrap();
    assert_eq!(res.count, 3);
    assert_eq!(res.items.len(), 3);

    assert_eq!(e.app.get_item(&e.vault, &item1.id).unwrap().summary.tags, vec!["delta", "beta"]);
    assert_eq!(e.app.get_item(&e.vault, &item2.id).unwrap().summary.tags, vec!["delta"]);
    assert_eq!(e.app.get_item(&e.vault, &item3.id).unwrap().summary.tags, vec!["delta", "gamma"]);
    assert_eq!(e.app.get_item(&e.vault, &item4.id).unwrap().summary.tags, vec!["beta"]);
    assert_eq!(e.app.list_tags().unwrap(), vec!["beta", "delta"]);

    // 3. Merge "delta" -> "beta" (item 1 dedupes to single "beta")
    let merge_res = e.app.rename_tag("delta", "beta").unwrap();
    assert_eq!(merge_res.count, 3);

    assert_eq!(e.app.get_item(&e.vault, &item1.id).unwrap().summary.tags, vec!["beta"]);
    assert_eq!(e.app.get_item(&e.vault, &item2.id).unwrap().summary.tags, vec!["beta"]);
    assert_eq!(e.app.get_item(&e.vault, &item3.id).unwrap().summary.tags, vec!["beta", "gamma"]);
    assert_eq!(e.app.get_item(&e.vault, &item4.id).unwrap().summary.tags, vec!["beta"]);
    assert_eq!(e.app.list_tags().unwrap(), vec!["beta"]);

    // 4. Delete "beta"
    let del_res = e.app.delete_tag("beta").unwrap();
    assert_eq!(del_res.count, 4);

    assert_eq!(e.app.get_item(&e.vault, &item1.id).unwrap().summary.tags, Vec::<String>::new());
    assert_eq!(e.app.get_item(&e.vault, &item2.id).unwrap().summary.tags, Vec::<String>::new());
    assert_eq!(e.app.get_item(&e.vault, &item3.id).unwrap().summary.tags, vec!["gamma"]);
    assert_eq!(e.app.get_item(&e.vault, &item4.id).unwrap().summary.tags, Vec::<String>::new());
    assert_eq!(e.app.list_tags().unwrap(), Vec::<String>::new());

    e.app.restore_item(&e.vault, &item3.id).unwrap();
    assert_eq!(e.app.list_tags().unwrap(), vec!["gamma"]);
}

#[test]
fn item_type_picker_filters_every_view_including_recently_deleted() {
    let e = env();
    let a = login(&e, "Alpha", "a", "pw-a-FAKE-xx");
    let b = card(&e);
    let f = |kind: FilterKind, category: Option<Category>| Filter { kind, vault_id: Some(e.vault.clone()), tag: None, category, query: None, sort: Some(Sort::Title) };
    let titles = |v: Vec<ItemSummary>| v.into_iter().map(|s| s.title).collect::<Vec<_>>();
    assert_eq!(titles(e.app.list_items(&f(FilterKind::All, Some(Category::Login))).unwrap()), vec!["Alpha"]);
    assert_eq!(titles(e.app.list_items(&f(FilterKind::Vault, Some(Category::CreditCard))).unwrap()), vec!["Example Bank"]);
    e.app.delete_item(&e.vault, &a.id).unwrap();
    e.app.delete_item(&e.vault, &b.id).unwrap();
    assert_eq!(e.app.list_items(&f(FilterKind::Deleted, None)).unwrap().len(), 2);
    assert_eq!(titles(e.app.list_items(&f(FilterKind::Deleted, Some(Category::CreditCard))).unwrap()), vec!["Example Bank"]);
    assert_eq!(titles(e.app.list_items(&f(FilterKind::Deleted, Some(Category::Login))).unwrap()), vec!["Alpha"]);
    assert!(e.app.list_items(&f(FilterKind::Deleted, Some(Category::Passkey))).unwrap().is_empty());
}

#[test]
fn test_list_items_with_search_query_operators() {
    let e = env();
    e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Login,
        title: "Work GitHub".into(),
        url: Some("https://github.com/myorg".into()),
        sections: vec![],
        notes: None,
        tags: Some(vec!["work".into(), "code".into()]),
        favorite: Some(true),
    }).unwrap();

    e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::CreditCard,
        title: "Personal Visa".into(),
        url: None,
        sections: vec![],
        notes: None,
        tags: Some(vec!["finance".into()]),
        favorite: Some(false),
    }).unwrap();

    let f = |q: &str| Filter {
        kind: FilterKind::All,
        vault_id: None,
        tag: None,
        category: None,
        query: Some(q.into()),
        sort: None,
    };

    let all = e.app.list_items(&f("")).unwrap();
    assert_eq!(all.len(), 2);

    let tag_work = e.app.list_items(&f("tag:work")).unwrap();
    assert_eq!(tag_work.len(), 1);
    assert_eq!(tag_work[0].title, "Work GitHub");

    let type_card = e.app.list_items(&f("type:card")).unwrap();
    assert_eq!(type_card.len(), 1);
    assert_eq!(type_card[0].title, "Personal Visa");

    let is_fav = e.app.list_items(&f("is:favorite")).unwrap();
    assert_eq!(is_fav.len(), 1);
    assert_eq!(is_fav[0].title, "Work GitHub");

    let neg_tag = e.app.list_items(&f("-tag:work")).unwrap();
    assert_eq!(neg_tag.len(), 1);
    assert_eq!(neg_tag[0].title, "Personal Visa");

    let url_host = e.app.list_items(&f("url:github.com")).unwrap();
    assert_eq!(url_host.len(), 1);
    assert_eq!(url_host[0].title, "Work GitHub");
}
