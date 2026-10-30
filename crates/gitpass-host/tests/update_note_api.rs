//! `update_login` for secure notes and API credentials (temp vault, fake data only).
mod common;
use common::*;
use gitpass_core::crypto::SecretKey;
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{Item, Session, UnlockOpts};
use serde_json::{json, Value};

fn record(f: &V5, id: &str) -> ItemRecord {
    let sk = SecretKey::parse(std::fs::read_to_string(&f.host.cfg.secret_key_path).unwrap().trim()).unwrap();
    let s = Session::unlock_with(&f.host.cfg.vault_path, PW, &sk, &UnlockOpts { accept_rollback: false, target_kdf: fast() }).unwrap();
    let (vid, iid) = id.split_once(':').unwrap();
    s.read_record(vid, iid).unwrap().1
}

fn by_title(items: &Value, title: &str) -> String {
    items.as_array().unwrap().iter().find(|i| i["title"] == title).unwrap()["id"].as_str().unwrap().to_string()
}

fn field<'a>(item: &'a Value, name: &str) -> &'a Value {
    item["fields"].as_array().unwrap().iter().find(|f| f["name"] == name).unwrap()
}

struct Ids { note: String, body: String, api: String, host_only: String }

fn fixture() -> (V5, Ids) {
    let mut f = v5_vault_with(|s, vid| {
        let mut note = Item::default();
        note.title = "Imported note".into();
        note.notes = Some("FAKE imported note".into());
        let mut imported = ItemRecord::new(note);
        imported.category = Category::SecureNote;
        imported.tags = vec!["old".into()];
        s.add_record(vid, imported).unwrap();

        s.add_record(vid, rec(Category::SecureNote, "Template note", vec![
            f("note", FieldKind::Multiline, "FAKE-body"),
            f("credential", FieldKind::Concealed, "FAKE-should-stay"),
        ])).unwrap();

        let mut api = rec(Category::ApiCredential, "Stripe", vec![
            f("username", FieldKind::Text, "demo-user"),
            f("credential", FieldKind::Concealed, "FAKE-sk-old"),
            f("hostname", FieldKind::Url, "https://api.example.invalid"),
            f("expires", FieldKind::Date, "2026-01-01"),
        ]);
        api.item.notes = Some("old api note".into());
        s.add_record(vid, api).unwrap();

        let mut bare = Item::default();
        bare.title = "Bare key".into();
        bare.url = Some("https://bare.example.invalid".into());
        let mut bare_rec = ItemRecord::new(bare);
        bare_rec.category = Category::ApiCredential;
        s.add_record(vid, bare_rec).unwrap();
    });
    ok(&mut f.host, json!({"cmd": "unlock", "password": PW}));
    let items = ok(&mut f.host, json!({"cmd": "list_items"}))["items"].clone();
    let ids = Ids {
        note: by_title(&items, "Imported note"),
        body: by_title(&items, "Template note"),
        api: by_title(&items, "Stripe"),
        host_only: by_title(&items, "Bare key"),
    };
    (f, ids)
}

#[test]
fn update_login_edits_secure_note_title_notes_and_tags() {
    let (mut f, ids) = fixture();
    assert!(record(&f, &ids.note).history.is_empty());

    let r = ok(&mut f.host, json!({
        "cmd": "update_login", "item_id": ids.note, "source": "popup",
        "title": "Imported note (edited)", "notes": "FAKE edited note",
        "tags": [" demo ", "", "demo", "home"]
    }));
    assert_eq!(r["ok"], true);

    let d = ok(&mut f.host, json!({"cmd": "get_item", "item_id": ids.note}))["item"].clone();
    assert_eq!(d["title"], "Imported note (edited)");
    assert_eq!(d["notes"], "FAKE edited note");
    assert_eq!(d["tags"], json!(["demo", "home"]));
    assert_eq!(record(&f, &ids.note).history.len(), 1);

    // Absent keys keep their value; an empty notes/tags clears them.
    ok(&mut f.host, json!({"cmd": "update_login", "item_id": ids.note, "source": "popup", "title": "Imported note"}));
    let d = ok(&mut f.host, json!({"cmd": "get_item", "item_id": ids.note}))["item"].clone();
    assert_eq!(d["title"], "Imported note");
    assert_eq!(d["notes"], "FAKE edited note");
    assert_eq!(d["tags"], json!(["demo", "home"]));
    ok(&mut f.host, json!({"cmd": "update_login", "item_id": ids.note, "source": "popup", "notes": "  ", "tags": []}));
    let d = ok(&mut f.host, json!({"cmd": "get_item", "item_id": ids.note}))["item"].clone();
    assert_eq!(d["notes"], Value::Null);
    assert_eq!(d["tags"], json!([]));
}

#[test]
fn update_login_writes_the_secure_note_body_field() {
    let (mut f, ids) = fixture();
    ok(&mut f.host, json!({
        "cmd": "update_login", "item_id": ids.body, "source": "popup",
        "title": "Template note", "notes": "FAKE-body-edited", "note_field": "note", "tags": ["dev"]
    }));
    let d = ok(&mut f.host, json!({"cmd": "get_item", "item_id": ids.body}))["item"].clone();
    assert_eq!(field(&d, "note")["value"], "FAKE-body-edited");
    assert_eq!(d["notes"], Value::Null);
    assert_eq!(d["tags"], json!(["dev"]));
    // The concealed field is not a note body, and a missing field is refused.
    assert_eq!(err(&mut f.host, json!({"cmd": "update_login", "item_id": ids.body, "source": "popup", "notes": "x", "note_field": "credential"})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd": "update_login", "item_id": ids.body, "source": "popup", "notes": "x", "note_field": "missing"})).0, "bad_request");
    assert_eq!(field(&ok(&mut f.host, json!({"cmd": "get_item", "item_id": ids.body}))["item"], "note")["value"], "FAKE-body-edited");
    assert_eq!(ok(&mut f.host, json!({"cmd": "reveal", "item_id": ids.body, "field": "credential", "source": "popup"}))["value"], "FAKE-should-stay");
}

#[test]
fn update_login_edits_api_credential_and_keeps_an_untouched_secret() {
    let (mut f, ids) = fixture();
    let r = ok(&mut f.host, json!({
        "cmd": "update_login", "item_id": ids.api, "source": "popup",
        "title": "Stripe test", "username": "octo@example.invalid", "credential": "FAKE-sk-new",
        "hostname": "dashboard.example.invalid", "expires": "2027-06-01", "notes": "FAKE rotated"
    }));
    assert_eq!(r["ok"], true);

    let d = ok(&mut f.host, json!({"cmd": "get_item", "item_id": ids.api}))["item"].clone();
    assert_eq!(d["title"], "Stripe test");
    assert_eq!(d["notes"], "FAKE rotated");
    assert_eq!(field(&d, "username")["value"], "octo@example.invalid");
    assert_eq!(field(&d, "hostname")["value"], "https://dashboard.example.invalid");
    assert_eq!(field(&d, "expires")["value"], "2027-06-01");
    let cred = field(&d, "credential");
    assert_eq!(cred["concealed"], true);
    assert!(cred.get("value").is_none(), "credential never leaves get_item");
    assert_eq!(ok(&mut f.host, json!({"cmd": "reveal", "item_id": ids.api, "field": "credential", "source": "popup"}))["value"], "FAKE-sk-new");
    assert_eq!(record(&f, &ids.api).history.len(), 1);

    // Absent credential keeps the secret; empty username/hostname/expiry/notes clear.
    ok(&mut f.host, json!({"cmd": "update_login", "item_id": ids.api, "source": "popup", "title": "Stripe"}));
    assert_eq!(ok(&mut f.host, json!({"cmd": "reveal", "item_id": ids.api, "field": "credential", "source": "popup"}))["value"], "FAKE-sk-new");
    ok(&mut f.host, json!({
        "cmd": "update_login", "item_id": ids.api, "source": "popup",
        "username": "", "credential": "", "hostname": "", "expires": "", "notes": ""
    }));
    let d = ok(&mut f.host, json!({"cmd": "get_item", "item_id": ids.api}))["item"].clone();
    assert_eq!(field(&d, "username")["value"], Value::Null);
    assert_eq!(field(&d, "hostname")["value"], Value::Null);
    assert_eq!(field(&d, "expires")["value"], Value::Null);
    assert_eq!(d["notes"], Value::Null);
    assert_eq!(err(&mut f.host, json!({"cmd": "reveal", "item_id": ids.api, "field": "credential", "source": "popup"})).0, "not_found");

    // A URL stored on the item (no hostname field) is updated in place.
    ok(&mut f.host, json!({"cmd": "update_login", "item_id": ids.host_only, "source": "popup", "hostname": "https://other.example.invalid"}));
    let d = ok(&mut f.host, json!({"cmd": "get_item", "item_id": ids.host_only}))["item"].clone();
    assert_eq!(d["url"], "https://other.example.invalid");
    assert!(d["fields"].as_array().unwrap().iter().all(|x| x["name"] != "hostname"));
}

#[test]
fn update_login_rejects_note_and_api_edits_that_are_not_from_the_popup() {
    let (mut f, ids) = fixture();
    assert_eq!(err(&mut f.host, json!({"cmd": "update_login", "item_id": ids.note, "title": "X"})).0, "denied");
    assert_eq!(err(&mut f.host, json!({"cmd": "update_login", "item_id": ids.note, "source": "content", "notes": "X"})).0, "denied");
    assert_eq!(err(&mut f.host, json!({"cmd": "update_login", "item_id": ids.api, "source": "page", "credential": "X"})).0, "denied");

    assert_eq!(err(&mut f.host, json!({"cmd": "update_login", "item_id": ids.note, "source": "popup", "title": "  "})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd": "update_login", "item_id": ids.note, "source": "popup", "tags": "demo"})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd": "update_login", "item_id": ids.api, "source": "popup", "hostname": "ftp://example.invalid"})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd": "update_login", "item_id": ids.api, "source": "popup", "expires": "2026-13-01"})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd": "update_login", "item_id": ids.api, "source": "popup", "expires": "06/01/2027"})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd": "update_login", "item_id": f.card, "source": "popup", "title": "X"})).0, "bad_request");

    let d = ok(&mut f.host, json!({"cmd": "get_item", "item_id": ids.api}))["item"].clone();
    assert_eq!(d["title"], "Stripe");
    assert_eq!(field(&d, "hostname")["value"], "https://api.example.invalid");
    assert_eq!(ok(&mut f.host, json!({"cmd": "reveal", "item_id": ids.api, "field": "credential", "source": "popup"}))["value"], "FAKE-sk-old");
    assert!(record(&f, &ids.api).history.is_empty());
    assert_eq!(ok(&mut f.host, json!({"cmd": "get_item", "item_id": ids.note}))["item"]["notes"], "FAKE imported note");
}
