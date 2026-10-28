//! Contract API tests against a temp vault. All data is FAKE.
use crate::api::*;
use gitpass_core::audit::PwnedRangeClient;
use gitpass_core::crypto::KdfParams;
use gitpass_core::import::{self, ImportFormat};
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::store::{self, UnlockOpts};
use gitpass_pasteboard::PasteboardMarkers;
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
fn fld(id: &str, kind: FieldKind, v: Option<&str>) -> Field { Field { id: id.into(), label: id.into(), kind, value: v.map(Into::into), masked: None, diff: None } }
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
fn api_cred(e: &Env, title: &str, user: &str, cred: &str, expires: &str) -> ItemSummary {
    e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::ApiCredential, title: title.into(), url: Some("https://api.example.invalid".into()),
        sections: vec![Section { id: "main".into(), title: None, fields: vec![
            fld("username", FieldKind::Text, Some(user)), fld("credential", FieldKind::Concealed, Some(cred)),
            fld("type", FieldKind::Text, Some("token")), fld("expires", FieldKind::Date, Some(expires))] }],
        notes: None, tags: None, favorite: None }).unwrap()
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
fn copied_secrets_carry_pasteboard_markers() {
    let e = env();
    let l = login(&e, "Marked", "user", "pw-FAKE-marked");
    e.app.copy_field(&e.vault, &l.id, "password").unwrap();
    assert_eq!(e.clip.markers(), Some(PasteboardMarkers::SECRET));
    assert_eq!(e.clip.get_text().as_deref(), Some("pw-FAKE-marked"));
    e.app.copy_field(&e.vault, &l.id, "username").unwrap();
    assert_eq!(e.clip.markers(), None, "username is not a secret");
    let c = card(&e);
    for field in ["number", "cvv"] {
        e.app.copy_field(&e.vault, &c.id, field).unwrap();
        assert_eq!(e.clip.markers(), Some(PasteboardMarkers::SECRET), "card {field}");
    }
    e.app.copy_field(&e.vault, &c.id, "expiry").unwrap();
    assert_eq!(e.clip.markers(), None, "expiry is not a secret");
    let api = e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::ApiCredential, title: "API".into(), url: None,
        sections: vec![Section { id: "main".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some("svc")), fld("credential", FieldKind::Concealed, Some("FAKE-token"))] }],
        notes: None, tags: None, favorite: None }).unwrap();
    e.app.copy_field(&e.vault, &api.id, "credential").unwrap();
    assert_eq!(e.clip.markers(), Some(PasteboardMarkers::SECRET));
    e.app.copy_field(&e.vault, &api.id, "username").unwrap();
    assert_eq!(e.clip.markers(), None);
}

#[test]
fn secret_copy_decision_covers_kinds_and_legacy_card_fields() {
    let f = |id: &str, kind: FieldKind, label: &str| gitpass_core::item::Field { id: id.into(), label: label.into(), kind, value: Some("FAKE".into()), removed: false };
    assert!(is_secret_copy(Category::Login, &f("password", FieldKind::Concealed, "password")));
    assert!(is_secret_copy(Category::Login, &f("otp", FieldKind::Totp, "one-time password")));
    assert!(is_secret_copy(Category::ApiCredential, &f("credential", FieldKind::Concealed, "credential")));
    assert!(is_secret_copy(Category::CreditCard, &f("number", FieldKind::Text, "number")), "legacy plain-text card number");
    assert!(is_secret_copy(Category::CreditCard, &f("cvv", FieldKind::Text, "cvv")), "legacy plain-text CVV");
    assert!(!is_secret_copy(Category::Login, &f("username", FieldKind::Text, "username")));
    assert!(!is_secret_copy(Category::Login, &f("website", FieldKind::Url, "website")));
    assert!(!is_secret_copy(Category::CreditCard, &f("expiry", FieldKind::MonthYear, "expiry")));
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
fn copy_field_cleared_on_vault_lock() {
    let e = env();
    let mut s = e.app.get_settings();
    s.clipboard_clear_secs = 60;
    e.app.set_settings(s).unwrap();
    let l = login(&e, "Beta", "b", "pw-FAKE-secret-lock");
    e.app.copy_field(&e.vault, &l.id, "password").unwrap();
    assert_eq!(e.clip.get_text().as_deref(), Some("pw-FAKE-secret-lock"));
    e.app.lock();
    assert_eq!(e.clip.get_text().as_deref(), Some(""));
}

#[test]
fn copy_field_not_cleared_on_vault_lock_if_user_copied_else() {
    let e = env();
    let mut s = e.app.get_settings();
    s.clipboard_clear_secs = 60;
    e.app.set_settings(s).unwrap();
    let l = login(&e, "Gamma", "g", "pw-FAKE-secret-lock-2");
    e.app.copy_field(&e.vault, &l.id, "password").unwrap();
    e.clip.set_text("external confidential text").unwrap();
    e.app.lock();
    assert_eq!(e.clip.get_text().as_deref(), Some("external confidential text"));
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
    assert_eq!(e.clip.markers(), Some(PasteboardMarkers::SECRET));
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
    let p = e.app.generate_password(24, true, true, false, false);
    assert_eq!(p.len(), 24);
    assert!(p.chars().any(|c| c.is_ascii_digit()) && p.chars().any(|c| "!@#$%^&*-_=+?".contains(c)));
    assert!(e.app.generate_password(20, false, false, false, false).chars().all(|c| c.is_ascii_alphabetic()));
    assert_eq!(e.app.generate_password(5, true, false, true, false).split('-').count(), 5);
    let pin = e.app.generate_password(6, false, false, false, true);
    assert_eq!(pin.len(), 6);
    assert!(pin.chars().all(|c| c.is_ascii_digit()));
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
fn item_change_password_url_and_detail_findings() {
    let e = env();
    let makelogin = |title: &str, url: &str| e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(), category: Category::Login, title: title.into(), url: Some(url.into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![
            fld("username", FieldKind::Text, Some("fake@example.invalid")),
            fld("password", FieldKind::Concealed, Some("password")),
        ] }],
        notes: None, tags: None, favorite: None,
    }).unwrap();

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
fn watchtower_flags_insecure_http_urls() {
    let e = env();
    let l1 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Login,
        title: "Insecure Forum".into(),
        url: Some("http://forum.example.org/login".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some("user")), fld("password", FieldKind::Concealed, Some("StrongPassword123!"))] }],
        notes: None, tags: None, favorite: None
    }).unwrap();
    let _l2 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Login,
        title: "Secure Site".into(),
        url: Some("https://secure.example.org/login".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some("user")), fld("password", FieldKind::Concealed, Some("StrongPassword123!"))] }],
        notes: None, tags: None, favorite: None
    }).unwrap();
    let _l3 = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Login,
        title: "Localhost Site".into(),
        url: Some("http://localhost:8080".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some("user")), fld("password", FieldKind::Concealed, Some("StrongPassword123!"))] }],
        notes: None, tags: None, favorite: None
    }).unwrap();

    let rep = e.app.watchtower(false).unwrap();
    assert_eq!(rep.insecure_urls, vec![l1.id]);
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
        (&c, vec!["4111 1111 1111 1111", "123"]),
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
    let a = mk(vec![Field { id: "cardholder".into(), label: "cardholder name".into(), kind: FieldKind::Text, value: Some("Demo User".into()), masked: None, diff: None },
                    Field { id: "type".into(), label: "type".into(), kind: FieldKind::Text, value: Some("Visa".into()), masked: None, diff: None },
                    fld("number", FieldKind::Concealed, Some("4000 1234 5678 4432")), fld("cvv", FieldKind::Concealed, Some("999"))]);
    assert_eq!(a.subtitle, "Visa •••• 4432");
    // imported card: random field id, label "Card Number"
    let b = mk(vec![Field { id: "f_x1".into(), label: "Card Number".into(), kind: FieldKind::Concealed, value: Some("5555555555554444".into()), masked: None, diff: None }]);
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

#[test]
fn copy_password_clears_clipboard_only_if_unchanged() {
    let e = env();
    let mut s = e.app.get_settings();
    s.clipboard_clear_secs = 1;
    e.app.set_settings(s).unwrap();
    e.app.copy_password("generated-secret-123").unwrap();
    assert_eq!(e.clip.get_text().as_deref(), Some("generated-secret-123"));
    std::thread::sleep(std::time::Duration::from_millis(1600));
    assert_eq!(e.clip.get_text().as_deref(), Some(""));
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
        url: Some("http://router.example.com/admin".into()),
    }).unwrap();

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
        notes: None, tags: None, favorite: None,
    }).unwrap();

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
        notes: None, tags: None, favorite: None,
    }).unwrap();

    let rep = e.app.watchtower(false).unwrap();
    assert!(rep.weak.contains(&item1.id));
    assert!(rep.weak.contains(&item2.id));
    assert_eq!(rep.weak_reasons.get(&item1.id).map(String::as_str), Some("contains the username"));
    assert_eq!(rep.weak_reasons.get(&item2.id).map(String::as_str), Some("contains the site name"));
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
