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

#[test]
fn archive_and_unarchive_flow_and_list_archived_and_deleted_omit_secrets() {
    let mut f = v5_vault();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));

    // Initially both lists are empty
    let arch_init = ok(&mut f.host, json!({"cmd":"list_archived"}));
    assert!(arch_init["items"].as_array().unwrap().is_empty());
    let del_init = ok(&mut f.host, json!({"cmd":"list_deleted"}));
    assert!(del_init["items"].as_array().unwrap().is_empty());

    // Archive permissions: requires popup source
    assert_eq!(err(&mut f.host, json!({"cmd":"archive_item","item_id":f.login})).0, "denied");
    assert_eq!(err(&mut f.host, json!({"cmd":"archive_item","item_id":f.login,"source":"page"})).0, "denied");

    // Archive item
    let arch_res = ok(&mut f.host, json!({"cmd":"archive_item","item_id":f.login,"source":"popup","archived":true}));
    assert_eq!(arch_res["ok"], true);
    assert_eq!(arch_res["archived"], true);

    // list_archived returns metadata only (no secrets)
    let arch_list = ok(&mut f.host, json!({"cmd":"list_archived"}));
    let arch_items = arch_list["items"].as_array().unwrap();
    assert_eq!(arch_items.len(), 1);
    assert_eq!(arch_items[0]["id"], f.login);
    assert_eq!(arch_items[0]["archived"], true);
    let arch_json = arch_list.to_string();
    assert!(!arch_json.contains("FAKE-gh-pw"), "secrets must never leak in archived metadata");

    // Archived item leaves default list_items and match (suggestions)
    assert!(!titles(&mut f.host).iter().any(|t| t == "GitHub"));
    let m = ok(&mut f.host, json!({"cmd":"match","origin":"https://github.com"}));
    assert!(m["items"].as_array().unwrap().is_empty(), "suggestions must not include archived items");

    // list_items with state: "archived" includes it
    let state_arch = ok(&mut f.host, json!({"cmd":"list_items","state":"archived"}));
    assert_eq!(state_arch["items"].as_array().unwrap().len(), 1);

    // Unarchive permissions and execution
    assert_eq!(err(&mut f.host, json!({"cmd":"unarchive_item","item_id":f.login})).0, "denied");
    let unarch_res = ok(&mut f.host, json!({"cmd":"unarchive_item","item_id":f.login,"source":"popup"}));
    assert_eq!(unarch_res["ok"], true);
    assert_eq!(unarch_res["archived"], false);

    // list_archived is now empty, item returns to default list_items
    let arch_empty = ok(&mut f.host, json!({"cmd":"list_archived"}));
    assert!(arch_empty["items"].as_array().unwrap().is_empty());
    assert!(titles(&mut f.host).iter().any(|t| t == "GitHub"));

    // Trash item and test list_deleted and get_item(include_deleted)
    ok(&mut f.host, json!({"cmd":"trash_item","item_id":f.login,"source":"popup"}));
    let del_list = ok(&mut f.host, json!({"cmd":"list_deleted"}));
    let del_items = del_list["items"].as_array().unwrap();
    assert_eq!(del_items.len(), 1);
    assert_eq!(del_items[0]["id"], f.login);
    assert!(del_items[0]["deleted_at"].is_string());
    let del_json = del_list.to_string();
    assert!(!del_json.contains("FAKE-gh-pw"), "secrets must never leak in deleted metadata");

    // get_item without include_deleted is not_found; with include_deleted returns detail
    assert_eq!(err(&mut f.host, json!({"cmd":"get_item","item_id":f.login})).0, "not_found");
    assert_eq!(err(&mut f.host, json!({"cmd":"get_item","item_id":f.login,"include_deleted":false})).0, "not_found");
    let del_detail = ok(&mut f.host, json!({"cmd":"get_item","item_id":f.login,"include_deleted":true}))["item"].clone();
    assert_eq!(del_detail["id"], f.login);
    assert!(del_detail["deleted_at"].is_string());
    let pw_fld = del_detail["fields"].as_array().unwrap().iter().find(|x| x["name"] == "password").unwrap();
    assert_eq!(pw_fld["concealed"], true);
    assert!(pw_fld.get("value").is_none());

    // list_items with state: "deleted" includes it
    let state_del = ok(&mut f.host, json!({"cmd":"list_items","state":"deleted"}));
    assert_eq!(state_del["items"].as_array().unwrap().len(), 1);

    // Cannot archive a deleted item
    assert_eq!(err(&mut f.host, json!({"cmd":"archive_item","item_id":f.login,"source":"popup"})).0, "not_found");

    // Restore item
    ok(&mut f.host, json!({"cmd":"restore_item","item_id":f.login,"source":"popup"}));
    assert!(ok(&mut f.host, json!({"cmd":"list_deleted"}))["items"].as_array().unwrap().is_empty());
}
