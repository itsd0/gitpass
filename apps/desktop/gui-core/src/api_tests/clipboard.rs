use super::*;
use crate::api::display::is_secret_copy;


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
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
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
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    let d = e.app.get_item(&e.vault, &it.id).unwrap();
    assert!(d.has_totp);
    assert!(d.sections[0].fields.iter().all(|f| f.kind != FieldKind::Totp || f.value.is_none()));
    let t = e.app.totp_now(&e.vault, &it.id).unwrap();
    assert_eq!((t.code.len(), t.period), (6, 30));
    e.app.copy_field(&e.vault, &it.id, "totp").unwrap();
    assert_eq!(e.clip.get_text().unwrap().len(), 6);
    assert_eq!(e.clip.markers(), Some(PasteboardMarkers::SECRET));
    assert!(e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::Login, title: "bad".into(), url: None,
        sections: vec![Section { id: "x".into(), title: None, fields: vec![fld("t", FieldKind::Totp, Some("not base32!!"))] }], notes: None, tags: None, favorite: None, related_item_ids: None }).is_err());
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
