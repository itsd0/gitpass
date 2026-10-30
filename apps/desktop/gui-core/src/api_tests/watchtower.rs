use super::*;


#[test]
fn watchtower_mfa_advice_and_passkey_suppression() {
    let e = env();
    // Fake credentials; the URL is a real domain only to exercise the bundled offline list.
    let l = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Login, title: "GitHub Fake".into(), url: Some("https://github.com/login".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![
            fld("username", FieldKind::Text, Some("fake@example.invalid")),
            fld("password", FieldKind::Concealed, Some("FAKE-Xq7#vR2!mK9@pL4$wZ"))] }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert!(r.totp_available.contains(&l.id));
    assert!(r.passkey_available.contains(&l.id));
    assert!(!r.missing_totp.contains(&l.id));
    // A stored passkey on the same rp_id suppresses only the passkey advice.
    e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Passkey, title: "GitHub passkey (fake)".into(), url: Some("https://github.com".into()),
        sections: vec![Section { id: "main".into(), title: None, fields: vec![fld("rp_id", FieldKind::Text, Some("github.com"))] }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert!(r.totp_available.contains(&l.id));
    assert!(!r.passkey_available.contains(&l.id));

    // Stored passkey on a subdomain (accounts.google.com) suppresses passkey advice for google.com
    let g = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Login, title: "Google Fake".into(), url: Some("https://google.com/login".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![
            fld("username", FieldKind::Text, Some("fake@example.invalid")),
            fld("password", FieldKind::Concealed, Some("FAKE-Xq7#vR2!mK9@pL4$wZ"))] }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert!(r.passkey_available.contains(&g.id));
    assert!(r.totp_available.contains(&g.id));
    assert!(!r.missing_totp.contains(&g.id));

    e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Passkey, title: "Google passkey (fake)".into(), url: Some("https://accounts.google.com".into()),
        sections: vec![Section { id: "main".into(), title: None, fields: vec![fld("rp_id", FieldKind::Text, Some("accounts.google.com"))] }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert!(!r.passkey_available.contains(&g.id));
}

#[test]
fn watchtower_score_weighting_weak_and_missing_totp() {
    // 2 logins, one weak; both have a URL and no TOTP (example.invalid is not in the offline list):
    // penalty = 80·(1/2) + 10·(2/2) = 50 → score 50.
    let e = env();
    login(&e, "Weak fake", "weak@example.invalid", "password");
    login(&e, "Strong fake", "strong@example.invalid", "Xq7#vR2!mK9@pL4$wZ");
    let r = e.app.watchtower(false).unwrap();
    assert_eq!((r.weak.len(), r.reused.len(), r.old.len(), r.breached.len(), r.missing_totp.len()), (1, 0, 0, 0, 2));
    assert_eq!(r.score, 50);
}

#[test]
fn item_change_password_url_and_detail_findings() {
    let e = env();
    let makelogin = |title: &str, url: &str| e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Login, title: title.into(), url: Some(url.into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![
            fld("username", FieldKind::Text, Some("fake@example.invalid")),
            fld("password", FieldKind::Concealed, Some("password")),
        ] }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();

    // http is upgraded, path/query dropped, and the URL travels on the summary + detail.
    let l = makelogin("Forum (fake)", "http://forum.example.com/login?token=fake");
    let want = "https://forum.example.com/.well-known/change-password";
    assert_eq!(l.change_password_url.as_deref(), Some(want));
    assert_eq!(e.app.item_change_password_url(&e.vault, &l.id).unwrap().as_deref(), Some(want));
    let d = e.app.get_item(&e.vault, &l.id).unwrap();
    assert_eq!(d.summary.change_password_url.as_deref(), Some(want));
    assert!(d.findings.iter().any(|f| f.kind == "weak"));
    assert!(d.findings.iter().any(|f| f.kind == "insecure_url"));
    assert!(d.findings.iter().any(|f| f.description.contains("weak password")));

    // Loopback: no change-password URL (and no insecure_url finding), but other findings remain.
    let n = makelogin("Local (fake)", "http://127.0.0.1/login");
    assert_eq!(n.change_password_url, None);
    assert_eq!(e.app.item_change_password_url(&e.vault, &n.id).unwrap(), None);
    let d = e.app.get_item(&e.vault, &n.id).unwrap();
    assert!(d.findings.iter().all(|f| f.kind != "insecure_url"));
    assert!(d.findings.iter().any(|f| f.kind == "weak"));
    assert!(d.findings.iter().any(|f| f.kind == "missing_totp"));
}

#[test]
fn watchtower_score_weighting_reuse_counts_every_item() {
    // Both items of a reuse group are in `bad`: 80·(2/2) + 10·(2/2) = 90 → score 10.
    let e = env();
    login(&e, "Reuse A", "a@example.invalid", "Xq7#vR2!mK9@pL4$wZ");
    login(&e, "Reuse B", "b@example.invalid", "Xq7#vR2!mK9@pL4$wZ");
    let r = e.app.watchtower(false).unwrap();
    assert_eq!((r.reused.len(), r.missing_totp.len()), (1, 2));
    assert_eq!(r.score, 10);
}

#[test]
fn watchtower_score_rounds_to_nearest_integer() {
    // 3 logins, one weak: 80/3 + 10·(3/3) = 36.67 → 100 − 36.67 = 63.33 → 63.
    let e = env();
    login(&e, "Weak fake", "w@example.invalid", "password");
    login(&e, "Strong one", "s1@example.invalid", "Xq7#vR2!mK9@pL4$wZ");
    login(&e, "Strong two", "s2@example.invalid", "Another#Strong9Pass!x");
    assert_eq!(e.app.watchtower(false).unwrap().score, 63);
}

#[test]
fn watchtower_score_counts_documented_totp_sites_as_missing_2fa() {
    // github.com is in the offline list, so the finding is TotpAvailable, not MissingTotp. It still
    // means "2FA not set up" and keeps the 10-point missing-TOTP weight: 10·(1/1) → score 90.
    let e = env();
    e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::Login, title: "GitHub Fake".into(),
        url: Some("https://github.com/login".into()), sections: vec![Section { id: "login".into(), title: None,
        fields: vec![fld("username", FieldKind::Text, Some("fake@example.invalid")),
            fld("password", FieldKind::Concealed, Some("Xq7#vR2!mK9@pL4$wZ"))] }], notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert_eq!((r.missing_totp.len(), r.totp_available.len()), (0, 1));
    assert_eq!(r.score, 90);
}

#[test]
fn watchtower_score_without_passwords_and_with_an_old_password() {
    // No password anywhere: the score stays 100 even though TOTP is missing.
    let e = env();
    e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::Login, title: "No password".into(),
        url: Some("https://example.invalid".into()), sections: vec![Section { id: "login".into(), title: None,
        fields: vec![fld("username", FieldKind::Text, Some("nopw@example.invalid"))] }], notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert_eq!((r.missing_totp.len(), r.score), (1, 100));

    // A backdated import exercises the old term: 10·(1/1) + 10·(2/2) = 20 → score 80.
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("old-fake.json");
    std::fs::write(&f, r#"{"encrypted":false,"items":[{"id":"1","type":1,"name":"Old fake","revisionDate":"2020-01-02T03:04:05.000Z","login":{"username":"old@example.invalid","password":"Xq7#vR2!mK9@pL4$wZ","uris":[{"uri":"https://example.invalid"}]}}]}"#).unwrap();
    e.app.import_file(&e.vault, "bitwarden-json", &f.display().to_string(), false, None).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert_eq!((r.old.len(), r.missing_totp.len(), r.score), (1, 2, 80));
}

#[test]
fn watchtower_flags_insecure_http_urls() {
    let e = env();
    let l1 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Login,
        title: "Insecure Forum".into(),
        url: Some("http://forum.example.org/login".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some("user")), fld("password", FieldKind::Concealed, Some("StrongPassword123!"))] }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    let _l2 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Login,
        title: "Secure Site".into(),
        url: Some("https://secure.example.org/login".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some("user")), fld("password", FieldKind::Concealed, Some("StrongPassword123!"))] }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    let _l3 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Login,
        title: "Localhost Site".into(),
        url: Some("http://localhost:8080".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some("user")), fld("password", FieldKind::Concealed, Some("StrongPassword123!"))] }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();

    let rep = e.app.watchtower(false).unwrap();
    assert_eq!(rep.insecure_urls, vec![l1.id]);
}

#[test]
fn watchtower_ignore_stop_ignoring_and_score_change() {
    let e = env();
    // Item with weak password and insecure HTTP url
    let item = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Login,
        title: "Test Insecure Router".into(),
        sections: vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![
                Field { id: "username".into(), label: "username".into(), kind: FieldKind::Text, value: Some("admin".into()), masked: None, diff: None },
                Field { id: "password".into(), label: "password".into(), kind: FieldKind::Concealed, value: Some("123456".into()), masked: None, diff: None },
            ],
        }],
        notes: None,
        tags: None,
        favorite: None,
        url: Some("http://router.example.com/admin".into()), related_item_ids: None }).unwrap();

    // 1. Initial watchtower run: weak, insecureUrls, and score penalized (< 100)
    let rep1 = e.app.watchtower(false).unwrap();
    assert!(rep1.weak.contains(&item.id));
    assert!(rep1.insecure_urls.contains(&item.id));
    assert!(rep1.ignored.is_empty());
    let score_before = rep1.score;
    assert!(score_before < 100);

    // 2. Ignore weak warning
    e.app.ignore_finding(&e.vault, &item.id, "weak").unwrap();
    let rep2 = e.app.watchtower(false).unwrap();
    assert!(!rep2.weak.contains(&item.id));
    assert!(rep2.insecure_urls.contains(&item.id));
    assert_eq!(rep2.ignored.len(), 1);
    assert_eq!(rep2.ignored[0].item_id, item.id);
    assert_eq!(rep2.ignored[0].kind, "weak");
    assert!(rep2.score > score_before); // score improved

    // 3. Ignore insecure_url warning as well
    e.app.ignore_finding(&e.vault, &item.id, "insecure_url").unwrap();
    let rep3 = e.app.watchtower(false).unwrap();
    assert!(!rep3.weak.contains(&item.id));
    assert!(!rep3.insecure_urls.contains(&item.id));
    assert_eq!(rep3.ignored.len(), 2);

    // Verify item detail contains ignored_findings
    let det = e.app.get_item(&e.vault, &item.id).unwrap();
    assert_eq!(det.summary.ignored_findings, vec!["weak", "insecure_url"]);

    // 4. Stop ignoring "weak"
    e.app.unignore_finding(&e.vault, &item.id, "weak").unwrap();
    let rep4 = e.app.watchtower(false).unwrap();
    assert!(rep4.weak.contains(&item.id));
    assert!(!rep4.insecure_urls.contains(&item.id));
    assert_eq!(rep4.ignored.len(), 1);
    assert_eq!(rep4.ignored[0].kind, "insecure_url");
    assert_eq!(rep4.score, score_before); // score back to initial penalized score
}

#[test]
fn watchtower_groups_duplicate_logins() {
    let e = env();
    let l1 = login(&e, "Service A", "alice", "pw1-super-strong-fake");
    let l2 = login(&e, "Service B", "Alice", "pw2-super-strong-fake");
    let l3 = login(&e, "Service C", "bob", "pw3-super-strong-fake");

    let rep = e.app.watchtower(false).unwrap();
    assert_eq!(rep.duplicates.len(), 1);
    let mut group = rep.duplicates[0].clone();
    group.sort();
    let mut expected = vec![l1.id.clone(), l2.id.clone()];
    expected.sort();
    assert_eq!(group, expected);
    assert!(!group.contains(&l3.id));
}

#[test]
fn watchtower_flags_passwords_containing_personal_info() {
    let e = env();
    let item1 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Login,
        title: "User Password Test".into(),
        url: Some("https://gitlab.com".into()),
        sections: vec![Section {
            id: "login".into(),
            title: None,
            fields: vec![fld("username", FieldKind::Text, Some("alice")), fld("password", FieldKind::Concealed, Some("github-alice-2024!"))],
        }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();

    let item2 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Login,
        title: "Site Password Test".into(),
        url: Some("https://login.github.com".into()),
        sections: vec![Section {
            id: "login".into(),
            title: None,
            fields: vec![fld("username", FieldKind::Text, Some("charlie")), fld("password", FieldKind::Concealed, Some("G1thub#99-Secure!"))],
        }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();

    let rep = e.app.watchtower(false).unwrap();
    assert!(rep.weak.contains(&item1.id));
    assert!(rep.weak.contains(&item2.id));
    assert_eq!(rep.weak_reasons.get(&item1.id).map(String::as_str), Some("contains the username"));
    assert_eq!(rep.weak_reasons.get(&item2.id).map(String::as_str), Some("contains the site name"));
}

#[test]
fn watchtower_flags_reused_old_password() {
    let e = env();
    let retired = "FAKE-Retired-Pass-123!";
    let b = login(&e, "Retired Source", "bob@example.invalid", retired);
    // Rotating B's password retires the old value into B's password history.
    e.app.update_item(&b.id, &ItemInput { vault_id: e.vault.clone(), category: Category::Login, title: "Retired Source".into(), url: Some("https://example.invalid".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some("bob@example.invalid")), fld("password", FieldKind::Concealed, Some("FAKE-New-Strong-Pass-456!"))] }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    let a = login(&e, "Reused Old", "alice@example.invalid", retired);
    let r = e.app.watchtower(false).unwrap();
    assert_eq!(r.reused_old_password.len(), 1);
    let hit = &r.reused_old_password[0];
    assert_eq!(hit.item_id, a.id);
    assert_eq!(hit.other_title, "Retired Source");
    assert!(!hit.same_item);
    assert!(!hit.retired_on.is_empty());
    // The shared core score applies the half-weight penalty.
    assert!(r.score < 100);
}
