//! Popup list contract (#165): no silent item cap, API secret previews, created_at in summaries.
mod common;
use common::*;
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::Item;
use serde_json::json;

#[test]
fn list_items_has_no_default_cap_but_honours_limit() {
    let mut f = v5_vault_with(|s, vid| {
        for i in 0..520 {
            let mut it = Item::default();
            it.title = format!("Fake item {i:04}");
            s.add_record(vid, ItemRecord::new(it)).unwrap();
        }
    });
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    let all = ok(&mut f.host, json!({"cmd":"list_items"}));
    assert_eq!(all["items"].as_array().unwrap().len(), 524, "4 base items + 520 extra, none dropped");
    // Titles sort A to Z, so the last one used to be cut off past the 500th.
    assert!(all["items"].as_array().unwrap().iter().any(|i| i["title"] == "Fake item 0519"));
    let capped = ok(&mut f.host, json!({"cmd":"list_items","limit":7}));
    assert_eq!(capped["items"].as_array().unwrap().len(), 7);
}

#[test]
fn api_secret_preview_first4_last4_only_for_long_values() {
    let mut f = v5_vault_with(|s, vid| {
        s.add_record(vid, rec(Category::ApiCredential, "Long key", vec![
            f("credential", FieldKind::Concealed, "sk-FAKE1234567890abcdWXYZ"), f("username", FieldKind::Text, "svc")])).unwrap();
        s.add_record(vid, rec(Category::ApiCredential, "Short key", vec![f("credential", FieldKind::Concealed, "short-12345")])).unwrap();
    });
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    let list = ok(&mut f.host, json!({"cmd":"list_items","category":"api_credential"}));
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i["created_at"].is_u64() || i["created_at"].is_null()));
    let id_of = |t: &str| items.iter().find(|i| i["title"] == t).unwrap()["id"].as_str().unwrap().to_string();

    let long = ok(&mut f.host, json!({"cmd":"get_item","item_id":id_of("Long key")}))["item"].clone();
    let cred = long["fields"].as_array().unwrap().iter().find(|x| x["name"] == "credential").unwrap();
    assert_eq!(cred["concealed"], true);
    assert_eq!(cred["masked"], "sk-F••••WXYZ");
    assert!(cred.get("value").is_none());
    assert!(!long.to_string().contains("1234567890abcd"), "middle of the secret never leaves the host");

    let short = ok(&mut f.host, json!({"cmd":"get_item","item_id":id_of("Short key")}))["item"].clone();
    let cred = short["fields"].as_array().unwrap().iter().find(|x| x["name"] == "credential").unwrap();
    assert!(cred.get("masked").is_none(), "under 12 chars: bullets only");
    assert!(!short.to_string().contains("short-12345"));
}
