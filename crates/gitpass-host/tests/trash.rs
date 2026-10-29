//! `trash_item` / `restore_item`: popup-only soft delete that lands in the desktop's Recently Deleted.
mod common;
use common::*;
use gitpass_core::crypto::SecretKey;
use gitpass_core::store::{Session, UnlockOpts};
use serde_json::json;

/// `deleted_at` as written to disk (what the desktop's Recently Deleted view reads).
fn stored_deleted(f: &V5) -> Option<u64> {
    let sk = SecretKey::parse(std::fs::read_to_string(&f.host.cfg.secret_key_path).unwrap().trim()).unwrap();
    let s = Session::unlock_with(&f.host.cfg.vault_path, PW, &sk, &UnlockOpts { accept_rollback: false, target_kdf: fast() }).unwrap();
    let (vid, iid) = f.login.split_once(':').unwrap();
    s.read_record(vid, iid).unwrap().1.deleted_at
}

fn titles(f: &mut gitpass_host::handler::Host) -> Vec<String> {
    ok(f, json!({"cmd":"list_items"}))["items"].as_array().unwrap().iter()
        .map(|i| i["title"].as_str().unwrap().to_string()).collect()
}

#[test]
fn trash_hides_the_item_and_restore_brings_it_back() {
    let mut f = v5_vault();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    assert!(titles(&mut f.host).iter().any(|t| t == "GitHub"));
    assert_eq!(stored_deleted(&f), None);

    // Popup-only, like reveal/update_login: no stamp (content script) or another source is refused.
    assert_eq!(err(&mut f.host, json!({"cmd":"trash_item","item_id":f.login})).0, "denied");
    assert_eq!(err(&mut f.host, json!({"cmd":"trash_item","item_id":f.login,"source":"page"})).0, "denied");
    assert_eq!(stored_deleted(&f), None, "a refused call must not write");

    assert_eq!(ok(&mut f.host, json!({"cmd":"trash_item","item_id":f.login,"source":"popup"}))["ok"], true);
    assert!(stored_deleted(&f).is_some(), "soft delete on disk: the desktop shows it in Recently Deleted");
    assert!(!titles(&mut f.host).iter().any(|t| t == "GitHub"), "trashed item leaves list_items");
    let m = ok(&mut f.host, json!({"cmd":"match","origin":"https://github.com"}));
    assert!(m["items"].as_array().unwrap().is_empty(), "{m}");
    assert_eq!(err(&mut f.host, json!({"cmd":"get_item","item_id":f.login})).0, "not_found");
    assert_eq!(err(&mut f.host, json!({"cmd":"reveal","item_id":f.login,"field":"password","source":"popup"})).0, "not_found");
    // Trashing a trashed item is a not_found, not a second delete.
    assert_eq!(err(&mut f.host, json!({"cmd":"trash_item","item_id":f.login,"source":"popup"})).0, "not_found");

    assert_eq!(ok(&mut f.host, json!({"cmd":"restore_item","item_id":f.login,"source":"popup"}))["ok"], true);
    assert_eq!(stored_deleted(&f), None);
    assert!(titles(&mut f.host).iter().any(|t| t == "GitHub"));
    assert_eq!(ok(&mut f.host, json!({"cmd":"match","origin":"https://github.com"}))["items"].as_array().unwrap().len(), 1);
    assert_eq!(ok(&mut f.host, json!({"cmd":"reveal","item_id":f.login,"field":"password","source":"popup"}))["value"], "FAKE-gh-pw");

    // Restore is popup-only too; unknown ids are not_found, malformed ids are bad_request.
    assert_eq!(err(&mut f.host, json!({"cmd":"restore_item","item_id":f.login})).0, "denied");
    assert_eq!(err(&mut f.host, json!({"cmd":"restore_item","item_id":format!("{}:missing", f.vid),"source":"popup"})).0, "not_found");
    assert_eq!(err(&mut f.host, json!({"cmd":"trash_item","item_id":"nocolon","source":"popup"})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"restore_item","item_id":"nocolon","source":"popup"})).0, "bad_request");
}

#[test]
fn trash_and_restore_need_an_unlocked_vault() {
    let mut f = v5_vault();
    assert_eq!(err(&mut f.host, json!({"cmd":"trash_item","item_id":f.login,"source":"popup"})).0, "locked");
    assert_eq!(err(&mut f.host, json!({"cmd":"restore_item","item_id":f.login,"source":"popup"})).0, "locked");
}
