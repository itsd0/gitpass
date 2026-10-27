//! Contract API tests against a temp vault. All data is FAKE.
use crate::api::*;
use gitpass_core::audit::PwnedRangeClient;
use gitpass_core::crypto::KdfParams;
use gitpass_core::import::{self, ImportFormat};
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::store::{self, UnlockOpts};
use std::sync::Arc;
use base64::{engine::general_purpose::STANDARD as B64, Engine};

fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }

struct MockHibp;
impl PwnedRangeClient for MockHibp {
    fn range(&self, prefix: &str) -> anyhow::Result<String> {
        // SHA1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8
        Ok(if prefix == "5BAA6" { "1E4C9B93F3F0682250B6CF8331B7EE68FD8:9999\r\n0000000000000000000000000000000000A:0".into() } else { String::new() })
    }
}

struct Env { _vault: tempfile::TempDir, _cfg: tempfile::TempDir, app: App, clip: Arc<MemClipboard>, sk: String, vault: String }

fn env() -> Env {
    let d = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), "correct horse", fast()).unwrap().encode();
    let clip = Arc::new(MemClipboard::default());
    let app = App::new(cfg.path().to_path_buf(), clip.clone()).with_hibp(Box::new(MockHibp));
    app.set_unlock_opts(UnlockOpts { accept_rollback: false, target_kdf: fast() });
    let st = app.unlock(&d.path().display().to_string(), "correct horse", Some(&sk)).unwrap();
    assert!(st.unlocked && st.account.is_some());
    // unlock auto-creates the default "Personal" vault for a fresh account
    let vs = app.list_vaults().unwrap();
    assert_eq!(vs.len(), 1); assert_eq!(vs[0].name, "Personal");
    let v = vs[0].clone();
    Env { vault: v.id, _vault: d, _cfg: cfg, app, clip, sk }
}
fn fld(id: &str, kind: FieldKind, v: Option<&str>) -> Field { Field { id: id.into(), label: id.into(), kind, value: v.map(Into::into), masked: None } }
fn login(e: &Env, title: &str, user: &str, pw: &str) -> ItemSummary {
    e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::Login, title: title.into(), url: Some("https://example.invalid".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some(user)), fld("password", FieldKind::Concealed, Some(pw))] }],
        notes: None, tags: Some(vec!["work".into()]), favorite: None }).unwrap()
}
fn card(e: &Env) -> ItemSummary {
    e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::CreditCard, title: "Example Bank".into(), url: None,
        sections: vec![Section { id: "card".into(), title: Some("Card".into()), fields: vec![fld("number", FieldKind::Concealed, Some("4111 1111 1111 1111")),
            fld("cvv", FieldKind::Concealed, Some("123")), fld("expiry", FieldKind::MonthYear, Some("12/30"))] }],
        notes: Some("fake".into()), tags: None, favorite: Some(true) }).unwrap()
}

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
    let f = |kind: FilterKind| Filter { kind, vault_id: Some(e.vault.clone()), tag: Some("work".into()), category: Some(Category::CreditCard), query: None, sort: Some(Sort::Title) };
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
fn copy_field_clears_clipboard_only_if_unchanged() {
    let e = env();
    let mut s = e.app.get_settings();
    s.clipboard_clear_secs = 1;
    e.app.set_settings(s).unwrap();
    let l = login(&e, "Alpha", "a", "pw-FAKE-1");
    e.app.copy_field(&e.vault, &l.id, "password").unwrap();
    assert_eq!(e.clip.get_text().as_deref(), Some("pw-FAKE-1"));
    std::thread::sleep(std::time::Duration::from_millis(1600));
    assert_eq!(e.clip.get_text().as_deref(), Some(""));
    e.app.copy_field(&e.vault, &l.id, "username").unwrap();
    e.clip.set_text("user copied something else").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1600));
    assert_eq!(e.clip.get_text().as_deref(), Some("user copied something else"));
}

#[test]
fn totp_now_and_totp_field_copy() {
    let e = env();
    let it = e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::Login, title: "2FA".into(), url: None,
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some("u")), fld("totp", FieldKind::Totp, Some("JBSWY3DPEHPK3PXP"))] }],
        notes: None, tags: None, favorite: None }).unwrap();
    let d = e.app.get_item(&e.vault, &it.id).unwrap();
    assert!(d.has_totp);
    assert!(d.sections[0].fields.iter().all(|f| f.kind != FieldKind::Totp || f.value.is_none()));
    let t = e.app.totp_now(&e.vault, &it.id).unwrap();
    assert_eq!((t.code.len(), t.period), (6, 30));
    e.app.copy_field(&e.vault, &it.id, "totp").unwrap();
    assert_eq!(e.clip.get_text().unwrap().len(), 6);
    assert!(e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::Login, title: "bad".into(), url: None,
        sections: vec![Section { id: "x".into(), title: None, fields: vec![fld("t", FieldKind::Totp, Some("not base32!!"))] }], notes: None, tags: None, favorite: None }).is_err());
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
}

#[test]
fn tools_generate_strength_watchtower_hibp() {
    let e = env();
    let p = e.app.generate_password(24, true, true, false);
    assert_eq!(p.len(), 24);
    assert!(p.chars().any(|c| c.is_ascii_digit()) && p.chars().any(|c| "!@#$%^&*-_=+?".contains(c)));
    assert!(e.app.generate_password(20, false, false, false).chars().all(|c| c.is_ascii_alphabetic()));
    assert_eq!(e.app.generate_password(5, true, false, true).split('-').count(), 5);
    assert!(e.app.password_strength("password") <= 10);
    assert!(e.app.password_strength(&p) >= 90);
    assert_eq!(e.app.password_strength(""), 0);
    let a = login(&e, "Weak1", "a", "password");
    let b = login(&e, "Weak2", "b", "password");
    let _c = login(&e, "Strong", "c", &p);
    let r = e.app.watchtower(false).unwrap();
    assert!(!r.hibp_checked && r.breached.is_empty());
    assert!(r.weak.contains(&a.id) && r.weak.contains(&b.id));
    assert_eq!(r.reused.len(), 1);
    assert_eq!(r.missing_totp.len(), 3);
    assert!(r.totp_available.is_empty() && r.passkey_available.is_empty());
    assert!(r.score < 100);
    // HIBP is opt-in: without Settings.hibpCheck no network check happens even if asked.
    assert!(!e.app.get_settings().hibp_check);
    let r = e.app.watchtower(true).unwrap();
    assert!(!r.hibp_checked && r.breached.is_empty());
    e.app.set_settings(Settings { hibp_check: true, ..e.app.get_settings() }).unwrap();
    let r = e.app.watchtower(true).unwrap();
    assert!(r.hibp_checked);
    let mut br = r.breached.clone(); br.sort();
    let mut want = vec![a.id.clone(), b.id.clone()]; want.sort();
    assert_eq!(br, want);
}

#[test]
fn watchtower_mfa_advice_and_passkey_suppression() {
    let e = env();
    // Fake credentials; the URL is a real domain only to exercise the bundled offline list.
    let l = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Login, title: "GitHub Fake".into(), url: Some("https://github.com/login".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![
            fld("username", FieldKind::Text, Some("fake@example.invalid")),
            fld("password", FieldKind::Concealed, Some("FAKE-Xq7#vR2!mK9@pL4$wZ"))] }],
        notes: None, tags: None, favorite: None,
    }).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert!(r.totp_available.contains(&l.id));
    assert!(r.passkey_available.contains(&l.id));
    assert!(!r.missing_totp.contains(&l.id));
    // A stored passkey on the same rp_id suppresses only the passkey advice.
    e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Passkey, title: "GitHub passkey (fake)".into(), url: Some("https://github.com".into()),
        sections: vec![Section { id: "main".into(), title: None, fields: vec![fld("rp_id", FieldKind::Text, Some("github.com"))] }],
        notes: None, tags: None, favorite: None,
    }).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert!(r.totp_available.contains(&l.id));
    assert!(!r.passkey_available.contains(&l.id));

    // Stored passkey on a subdomain (accounts.google.com) suppresses passkey advice for google.com
    let g = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Login, title: "Google Fake".into(), url: Some("https://google.com/login".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![
            fld("username", FieldKind::Text, Some("fake@example.invalid")),
            fld("password", FieldKind::Concealed, Some("FAKE-Xq7#vR2!mK9@pL4$wZ"))] }],
        notes: None, tags: None, favorite: None,
    }).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert!(r.passkey_available.contains(&g.id));
    assert!(r.totp_available.contains(&g.id));
    assert!(!r.missing_totp.contains(&g.id));

    e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Passkey, title: "Google passkey (fake)".into(), url: Some("https://accounts.google.com".into()),
        sections: vec![Section { id: "main".into(), title: None, fields: vec![fld("rp_id", FieldKind::Text, Some("accounts.google.com"))] }],
        notes: None, tags: None, favorite: None,
    }).unwrap();
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
            fld("password", FieldKind::Concealed, Some("Xq7#vR2!mK9@pL4$wZ"))] }], notes: None, tags: None, favorite: None }).unwrap();
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
        fields: vec![fld("username", FieldKind::Text, Some("nopw@example.invalid"))] }], notes: None, tags: None, favorite: None }).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert_eq!((r.missing_totp.len(), r.score), (1, 100));

    // A backdated import exercises the old term: 10·(1/1) + 10·(2/2) = 20 → score 80.
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("old-fake.json");
    std::fs::write(&f, r#"{"encrypted":false,"items":[{"id":"1","type":1,"name":"Old fake","revisionDate":"2020-01-02T03:04:05.000Z","login":{"username":"old@example.invalid","password":"Xq7#vR2!mK9@pL4$wZ","uris":[{"uri":"https://example.invalid"}]}}]}"#).unwrap();
    e.app.import_file(&e.vault, "bitwarden-json", &f.display().to_string(), false).unwrap();
    let r = e.app.watchtower(false).unwrap();
    assert_eq!((r.old.len(), r.missing_totp.len(), r.score), (1, 2, 80));
}

#[test]
fn import_file_dry_run_and_real() {
    let e = env();
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("chrome.csv");
    std::fs::write(&f, "name,url,username,password\nExample,https://a.example.invalid,alice,FAKE1\nOther,https://b.example.invalid,bob,FAKE2\n").unwrap();
    let r = e.app.import_file(&e.vault, "chrome-csv", &f.display().to_string(), true).unwrap();
    assert_eq!((r.parsed, r.added, r.duplicates), (2, 2, 0));
    assert_eq!(e.app.list_vaults().unwrap()[0].item_count, 0);
    let r = e.app.import_file(&e.vault, "chrome-csv", &f.display().to_string(), false).unwrap();
    assert_eq!(r.added, 2);
    assert!(e.app.onboarding().imported);
    let r = e.app.import_file(&e.vault, "chrome-csv", &f.display().to_string(), false).unwrap();
    assert_eq!((r.added, r.duplicates), (0, 2));
    assert!(e.app.import_file(&e.vault, "nope", &f.display().to_string(), true).is_err());
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
    let ir = e.app.import_file(&v2.id, "gitpass-json", &plain.display().to_string(), false).unwrap();
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
fn sync_status_without_git() {
    let e = env();
    let s = e.app.sync_status().unwrap();
    assert_eq!(s.remote, None);
    assert_eq!((s.ahead, s.behind), (0, 0));
}

#[test]
fn contract_json_shapes() {
    let e = env();
    let l = login(&e, "Alpha", "a", "pw-FAKE");
    let v = serde_json::to_value(&l).unwrap();
    for k in ["id", "vaultId", "category", "title", "subtitle", "favorite", "archived", "deletedAt", "tags", "updatedAt", "createdAt", "url"] { assert!(v.get(k).is_some(), "{k}"); }
    assert_eq!(v["category"], "login");
    let d = serde_json::to_value(e.app.get_item(&e.vault, &l.id).unwrap()).unwrap();
    for k in ["sections", "notes", "attachments", "history", "passwordStrength", "hasTotp", "vaultId"] { assert!(d.get(k).is_some(), "{k}"); }
    assert!(d["sections"][0]["fields"][1]["value"].is_null());
    let vi = serde_json::to_value(&e.app.list_vaults().unwrap()[0]).unwrap();
    for k in ["id", "name", "description", "itemCount", "shared", "members", "icon", "color"] { assert!(vi.get(k).is_some(), "{k}"); }
    let f: Filter = serde_json::from_str(r#"{"kind":"category","category":"credit_card","sort":"created"}"#).unwrap();
    assert_eq!(f.category, Some(Category::CreditCard));
    let inp: ItemInput = serde_json::from_str(r#"{"vaultId":"x","category":"wifi","title":"t","sections":[{"id":"s","title":null,"fields":[{"id":"a","label":"A","kind":"month_year","value":"01/30"}]}]}"#).unwrap();
    assert_eq!(inp.sections[0].fields[0].kind, FieldKind::MonthYear);
    let w = serde_json::to_value(e.app.watchtower(false).unwrap()).unwrap();
    for k in ["score", "weak", "reused", "old", "breached", "missingTotp", "checkedAt", "hibpChecked"] { assert!(w.get(k).is_some(), "{k}"); }
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
fn settings_hibp_defaults_off_and_old_files_parse() {
    assert!(!Settings::default().hibp_check);
    let old = r#"{"autoLockMinutes":5,"clipboardClearSecs":90,"theme":"dark","language":"en","biometricUnlock":false,"quickAccessHotkey":"X"}"#;
    let s: Settings = serde_json::from_str(old).unwrap();
    assert!(!s.hibp_check);
    assert!(serde_json::to_string(&Settings::default()).unwrap().contains("\"hibpCheck\":false"));
}

struct MemBio(std::sync::Mutex<Option<(String, String, String)>>);
impl BiometricStore for MemBio {
    fn supported(&self) -> bool { true }
    fn store(&self, v: &str, p: &str, k: &str) -> anyhow::Result<()> { *self.0.lock().unwrap() = Some((v.into(), p.into(), k.into())); Ok(()) }
    fn load(&self, v: &str, _: &str) -> anyhow::Result<(zeroize::Zeroizing<String>, zeroize::Zeroizing<String>)> {
        let g = self.0.lock().unwrap(); let (vv, p, k) = g.clone().ok_or_else(|| anyhow::anyhow!("none"))?;
        assert_eq!(vv, v); Ok((p.into(), k.into()))
    }
    fn delete(&self, _: &str) -> anyhow::Result<()> { *self.0.lock().unwrap() = None; Ok(()) }
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
    let app = App::new(cfg.path().to_path_buf(), Arc::new(MemClipboard::default()));
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
fn card_brand_and_labels() {
    for (n, b) in [("4111111111111111", Some("Visa")), ("5555555555554444", Some("Mastercard")), ("2223003122003222", Some("Mastercard")),
                   ("378282246310005", Some("Amex")), ("6011111111111117", Some("Discover")), ("3530111333300000", Some("JCB")),
                   ("30569309025904", Some("Diners Club")), ("6200000000000005", Some("UnionPay")), ("9999000011117575", None)] {
        assert_eq!(card_brand(n), b, "{n}");
    }
    assert_eq!(card_label("4000123412347575"), "Visa •••• 7575");
    assert_eq!(card_label("9999000011117575"), "•••• 7575");
    assert_eq!(card_mask("4000123412347575"), "•••• •••• •••• 7575");
    assert!(!card_label("4000123412347575").contains("4000"));
}

#[test]
fn card_subtitle_with_cardholder_first_and_imported_labels() {
    let e = env();
    let mk = |fields: Vec<Field>| e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::CreditCard, title: "C".into(), url: None,
        sections: vec![Section { id: "main".into(), title: None, fields }], notes: None, tags: None, favorite: None }).unwrap();
    // template order: cardholder name comes before the number
    let a = mk(vec![Field { id: "cardholder".into(), label: "cardholder name".into(), kind: FieldKind::Text, value: Some("Demo User".into()), masked: None },
                    Field { id: "type".into(), label: "type".into(), kind: FieldKind::Text, value: Some("Visa".into()), masked: None },
                    fld("number", FieldKind::Concealed, Some("4000 1234 5678 4432")), fld("cvv", FieldKind::Concealed, Some("999"))]);
    assert_eq!(a.subtitle, "Visa •••• 4432");
    // imported card: random field id, label "Card Number"
    let b = mk(vec![Field { id: "f_x1".into(), label: "Card Number".into(), kind: FieldKind::Concealed, value: Some("5555555555554444".into()), masked: None }]);
    assert_eq!(b.subtitle, "Mastercard •••• 4444");
    let listed = e.app.list_items(&Filter { kind: FilterKind::All, vault_id: None, tag: None, category: None, query: None, sort: None }).unwrap();
    assert!(listed.iter().any(|s| s.subtitle == "Visa •••• 4432"));
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
