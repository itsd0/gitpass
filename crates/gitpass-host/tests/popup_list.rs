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
    assert_eq!(capped["next_offset"], 7);
    assert_eq!(all["next_offset"], serde_json::Value::Null);
    // Paging (replies are capped at 1 MiB): walking next_offset returns every item exactly once, in order.
    let (mut got, mut off) = (vec![], 0u64);
    loop {
        let r = ok(&mut f.host, json!({"cmd":"list_items","limit":200,"offset":off}));
        assert_eq!(r["total"], 524);
        got.extend(r["items"].as_array().unwrap().iter().map(|i| i["id"].clone()));
        match r["next_offset"].as_u64() { Some(n) => off = n, None => break }
    }
    let ids: Vec<_> = all["items"].as_array().unwrap().iter().map(|i| i["id"].clone()).collect();
    assert_eq!(got, ids);
}

#[test]
fn api_secret_preview_first4_last4_only_for_long_values() {
    let mut f = v5_vault_with(|s, vid| {
        s.add_record(vid, rec(Category::ApiCredential, "Long key", vec![
            f("credential", FieldKind::Concealed, "sk-FAKE1234567890abcdWXYZ"), f("username", FieldKind::Text, "svc")])).unwrap();
        s.add_record(vid, rec(Category::ApiCredential, "Short key", vec![f("credential", FieldKind::Concealed, "short-12345")])).unwrap();
        s.add_record(vid, rec(Category::ApiCredential, "Tiny key", vec![f("credential", FieldKind::Concealed, "tiny")])).unwrap();
    });
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    let list = ok(&mut f.host, json!({"cmd":"list_items","category":"api_credential"}));
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert!(items.iter().all(|i| i["created_at"].is_u64() || i["created_at"].is_null()));
    let id_of = |t: &str| items.iter().find(|i| i["title"] == t).unwrap()["id"].as_str().unwrap().to_string();

    let long_item = items.iter().find(|i| i["title"] == "Long key").unwrap();
    assert_eq!(long_item["subtitle"], "sk-F••••••••WXYZ");
    let short_item = items.iter().find(|i| i["title"] == "Short key").unwrap();
    assert_eq!(short_item["subtitle"], "sh••••••••");
    let tiny_item = items.iter().find(|i| i["title"] == "Tiny key").unwrap();
    assert_eq!(tiny_item["subtitle"], "••••••••");

    let long = ok(&mut f.host, json!({"cmd":"get_item","item_id":id_of("Long key")}))["item"].clone();
    let cred = long["fields"].as_array().unwrap().iter().find(|x| x["name"] == "credential").unwrap();
    assert_eq!(cred["concealed"], true);
    assert_eq!(cred["masked"], "sk-F••••••••WXYZ");
    assert!(cred.get("value").is_none());
    assert!(!long.to_string().contains("1234567890abcd"), "middle of the secret never leaves the host");

    let short = ok(&mut f.host, json!({"cmd":"get_item","item_id":id_of("Short key")}))["item"].clone();
    let cred = short["fields"].as_array().unwrap().iter().find(|x| x["name"] == "credential").unwrap();
    assert_eq!(cred["masked"], "sh••••••••");
    assert!(cred.get("value").is_none());
    assert!(!short.to_string().contains("short-12345"));

    let tiny = ok(&mut f.host, json!({"cmd":"get_item","item_id":id_of("Tiny key")}))["item"].clone();
    let cred = tiny["fields"].as_array().unwrap().iter().find(|x| x["name"] == "credential").unwrap();
    assert_eq!(cred["masked"], "••••••••");
    assert!(cred.get("value").is_none());
    assert!(!tiny.to_string().contains("tiny"));

    // Ensure list serialization never contains the full secrets
    let list_str = list.to_string();
    assert!(!list_str.contains("1234567890abcd"));
    assert!(!list_str.contains("short-12345"));
    assert!(!list_str.contains("tiny"));
}
