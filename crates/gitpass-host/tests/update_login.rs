//! `update_login`: popup-only login editing, history and website handling (temp vaults only).
mod common;
use common::*;
use gitpass_core::crypto::SecretKey;
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{Session, UnlockOpts};
use serde_json::json;

fn record(f: &V5) -> ItemRecord {
    let sk = SecretKey::parse(std::fs::read_to_string(&f.host.cfg.secret_key_path).unwrap().trim()).unwrap();
    let s = Session::unlock_with(&f.host.cfg.vault_path, PW, &sk, &UnlockOpts { accept_rollback: false, target_kdf: fast() }).unwrap();
    let (vid, iid) = f.login.split_once(':').unwrap();
    s.read_record(vid, iid).unwrap().1
}

#[test]
fn update_login_edits_fields_and_records_history() {
    let mut f = v5_vault();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    assert!(record(&f).history.is_empty());

    let r = ok(&mut f.host, json!({
        "cmd":"update_login", "item_id":f.login, "source":"popup",
        "title":"GitHub (work)", "username":"octo@example.invalid", "password":"FAKE-new-pw",
        "urls":["https://github.com/login", "github.com/enterprise"], "notes":"edited note"
    }));
    assert_eq!(r["ok"], true);

    let d = ok(&mut f.host, json!({"cmd":"get_item","item_id":f.login}))["item"].clone();
    assert_eq!(d["title"], "GitHub (work)");
    assert_eq!(d["subtitle"], "octo@example.invalid");
    assert_eq!(d["url"], "https://github.com/login");
    assert_eq!(d["notes"], "edited note");
    let fields = d["fields"].as_array().unwrap();
    assert_eq!(fields.iter().find(|x| x["name"] == "username").unwrap()["value"], "octo@example.invalid");
    let extra = fields.iter().find(|x| x["name"] == "url1").expect("second website");
    assert_eq!(extra["value"], "https://github.com/enterprise");
    assert_eq!(extra["label"], "website");
    assert_eq!(extra["kind"], "url");
    assert!(fields.iter().all(|x| x["name"] != "url2"));
    assert_eq!(ok(&mut f.host, json!({"cmd":"reveal","item_id":f.login,"field":"password","source":"popup"}))["value"], "FAKE-new-pw");

    // The previous content is kept in history, exactly like a desktop edit.
    let rec = record(&f);
    assert_eq!(rec.history.len(), 1);
    assert_eq!(rec.history[0].snapshot.item.title, "GitHub");
    assert_eq!(rec.history[0].snapshot.item.username.as_deref(), Some("leeky"));
    assert_eq!(rec.history[0].snapshot.item.password.as_deref(), Some("FAKE-gh-pw"));

    // Absent keys keep their value; an empty username clears it.
    ok(&mut f.host, json!({"cmd":"update_login","item_id":f.login,"source":"popup","title":"GitHub"}));
    let d = ok(&mut f.host, json!({"cmd":"get_item","item_id":f.login}))["item"].clone();
    assert_eq!(d["title"], "GitHub");
    assert_eq!(d["subtitle"], "octo@example.invalid");
    assert_eq!(d["url"], "https://github.com/login");
    assert_eq!(d["notes"], "edited note");
    ok(&mut f.host, json!({"cmd":"update_login","item_id":f.login,"source":"popup","username":""}));
    assert_eq!(ok(&mut f.host, json!({"cmd":"get_item","item_id":f.login}))["item"]["subtitle"], "");
    assert_eq!(record(&f).history.len(), 3);
}

#[test]
fn update_login_rejects_content_scripts_and_bad_input() {
    let mut f = v5_vault();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));

    // Content scripts (and any sender without the extension-page stamp) are refused.
    assert_eq!(err(&mut f.host, json!({"cmd":"update_login","item_id":f.login,"title":"X"})).0, "denied");
    assert_eq!(err(&mut f.host, json!({"cmd":"update_login","item_id":f.login,"source":"content","title":"X"})).0, "denied");
    assert_eq!(err(&mut f.host, json!({"cmd":"update_login","item_id":f.login,"source":"page","password":"X"})).0, "denied");

    assert_eq!(err(&mut f.host, json!({"cmd":"update_login","item_id":f.login,"source":"popup","title":"  "})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"update_login","item_id":f.login,"source":"popup","urls":["ftp://example.invalid"]})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"update_login","item_id":f.card,"source":"popup","title":"X"})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"update_login","item_id":"nohost","source":"popup","title":"X"})).0, "bad_request");

    // Nothing was written.
    let d = ok(&mut f.host, json!({"cmd":"get_item","item_id":f.login}))["item"].clone();
    assert_eq!(d["title"], "GitHub");
    assert_eq!(d["url"], "https://github.com/login");
    assert!(record(&f).history.is_empty());
}
