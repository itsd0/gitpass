//! `move_item`: popup-only move to another vault, re-encrypted under the target Vault Key.
mod common;
use common::*;

/// Item summaries from `list_items` for one vault: (id, title).
fn vault_items(f: &mut gitpass_host::handler::Host, vid: &str) -> Vec<(String, String)> {
    ok(f, serde_json::json!({"cmd":"list_items","vault_id":vid}))["items"].as_array().unwrap().iter()
        .map(|i| (i["id"].as_str().unwrap().to_string(), i["title"].as_str().unwrap().to_string())).collect()
}

fn vault_counts(f: &mut gitpass_host::handler::Host) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = ok(f, serde_json::json!({"cmd":"list_vaults"}))["vaults"].as_array().unwrap().iter()
        .map(|v| (v["name"].as_str().unwrap().to_string(), v["item_count"].as_u64().unwrap() as usize)).collect();
    out.sort();
    out
}

#[test]
fn move_keeps_fields_and_history_and_moves_back() {
    let mut f = v5_vault();
    ok(&mut f.host, serde_json::json!({"cmd":"unlock","password":PW}));
    let (vid, iid) = f.login.split_once(':').unwrap();
    let (vid, iid) = (vid.to_string(), iid.to_string());
    assert_eq!(vault_counts(&mut f.host), vec![("Personal".to_string(), 4), ("Work".to_string(), 0)]);

    // Popup-only, like trash_item: no stamp (content script) or another source is refused.
    assert_eq!(err(&mut f.host, serde_json::json!({"cmd":"move_item","item_id":f.login,"to_vault_id":f.vid2})).0, "denied");
    assert_eq!(err(&mut f.host, serde_json::json!({"cmd":"move_item","item_id":f.login,"to_vault_id":f.vid2,"source":"page"})).0, "denied");

    // Same vault and unknown vault are bad_request, and a refused call must not write.
    assert_eq!(err(&mut f.host, serde_json::json!({"cmd":"move_item","item_id":f.login,"to_vault_id":vid,"source":"popup"})).0, "bad_request");
    assert_eq!(err(&mut f.host, serde_json::json!({"cmd":"move_item","item_id":f.login,"to_vault_id":"nope","source":"popup"})).0, "bad_request");
    assert_eq!(err(&mut f.host, serde_json::json!({"cmd":"move_item","item_id":format!("{vid}:missing"),"to_vault_id":f.vid2,"source":"popup"})).0, "not_found");
    assert_eq!(err(&mut f.host, serde_json::json!({"cmd":"move_item","item_id":"nocolon","to_vault_id":f.vid2,"source":"popup"})).0, "bad_request");
    assert!(vault_items(&mut f.host, &vid).iter().any(|(_, t)| t == "GitHub"));

    let res = ok(&mut f.host, serde_json::json!({"cmd":"move_item","item_id":f.login,"to_vault_id":f.vid2,"source":"popup"}));
    assert_eq!(res["ok"], true);
    let new_id = res["item_id"].as_str().unwrap().to_string();
    assert!(new_id.starts_with(&format!("{}:", f.vid2)), "{new_id}");
    assert_eq!(res["vault_id"], f.vid2);
    assert_eq!(res["vault_name"], "Work");

    // Gone from the source vault, present in the target with every field intact.
    assert!(!vault_items(&mut f.host, &vid).iter().any(|(_, t)| t == "GitHub"));
    assert!(vault_items(&mut f.host, &f.vid2).iter().any(|(id, t)| id == &new_id && t == "GitHub"));
    assert_eq!(vault_counts(&mut f.host), vec![("Personal".to_string(), 3), ("Work".to_string(), 1)]);
    assert_eq!(ok(&mut f.host, serde_json::json!({"cmd":"get_item","item_id":new_id}))["item"]["title"], "GitHub");
    assert_eq!(ok(&mut f.host, serde_json::json!({"cmd":"reveal","item_id":new_id,"field":"password","source":"popup"}))["value"], "FAKE-gh-pw");
    assert_eq!(ok(&mut f.host, serde_json::json!({"cmd":"totp","item_id":new_id}))["code"].as_str().unwrap().len(), 6);
    assert_eq!(err(&mut f.host, serde_json::json!({"cmd":"get_item","item_id":f.login})).0, "not_found");

    // Undo is just a move back: the item lands in the source vault again.
    let back = ok(&mut f.host, serde_json::json!({"cmd":"move_item","item_id":new_id,"to_vault_id":vid,"source":"popup"}));
    assert_eq!(back["vault_id"], vid);
    assert!(vault_items(&mut f.host, &vid).iter().any(|(_, t)| t == "GitHub"));
    assert_eq!(vault_counts(&mut f.host), vec![("Personal".to_string(), 4), ("Work".to_string(), 0)]);
    let _ = iid;
}

#[test]
fn move_needs_an_unlocked_vault_and_a_live_item() {
    let mut f = v5_vault();
    assert_eq!(err(&mut f.host, serde_json::json!({"cmd":"move_item","item_id":f.login,"to_vault_id":f.vid2,"source":"popup"})).0, "locked");

    ok(&mut f.host, serde_json::json!({"cmd":"unlock","password":PW}));
    ok(&mut f.host, serde_json::json!({"cmd":"trash_item","item_id":f.login,"source":"popup"}));
    assert_eq!(err(&mut f.host, serde_json::json!({"cmd":"move_item","item_id":f.login,"to_vault_id":f.vid2,"source":"popup"})).0, "not_found");
}
