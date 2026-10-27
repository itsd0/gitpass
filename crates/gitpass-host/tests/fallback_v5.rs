//! Standalone mode (desktop app not running): v0.6 contract commands with real v5 categories.
mod common;
use common::*;
use serde_json::json;

#[test]
fn categories_summaries_and_detail() {
    let mut f = v5_vault();
    assert_eq!(err(&mut f.host, json!({"cmd":"list_items"})).0, "locked");
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    let all = ok(&mut f.host, json!({"cmd":"list_items"}));
    let items = all["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    let by = |t: &str| items.iter().find(|i| i["title"] == t).unwrap().clone();
    assert_eq!(by("GitHub")["category"], "login");
    assert_eq!(by("GitHub")["subtitle"], "leeky");
    assert_eq!(by("GitHub")["favicon_host"], "github.com");
    assert_eq!(by("GitHub")["has_totp"], true);
    assert_eq!(by("GitHub")["vault_name"], "Personal");
    assert_eq!(by("Visa")["category"], "credit_card");
    assert_eq!(by("Visa")["subtitle"], "•••• 1234");
    assert_eq!(by("Me")["category"], "identity");
    assert_eq!(by("Me")["subtitle"], "Test Person");
    assert_eq!(by("Wifi notes")["category"], "secure_note");
    assert!(!all.to_string().contains("FAKE-gh-pw") && !all.to_string().contains("4111"));

    let cards = ok(&mut f.host, json!({"cmd":"list_items","category":"credit_card"}));
    assert_eq!(cards["items"].as_array().unwrap().len(), 1);
    assert_eq!(err(&mut f.host, json!({"cmd":"list_items","category":"bogus"})).0, "bad_request");
    let q = ok(&mut f.host, json!({"cmd":"list_items","query":"git","limit":10}));
    assert_eq!(q["items"].as_array().unwrap().len(), 1);
    let v = ok(&mut f.host, json!({"cmd":"list_items","vault_id":"nope"}));
    assert_eq!(v["items"].as_array().unwrap().len(), 0);

    // get_item: secret fields are listed as concealed without value
    let d = ok(&mut f.host, json!({"cmd":"get_item","item_id":f.card}))["item"].clone();
    assert_eq!(d["category"], "credit_card");
    let num = d["fields"].as_array().unwrap().iter().find(|x| x["name"] == "number").unwrap();
    assert_eq!(num["concealed"], true);
    assert_eq!(num["kind"], "card_number");
    assert!(num.get("value").is_none());
    assert!(!d.to_string().contains("4111") && !d.to_string().contains("\"123\""));
    let exp = d["fields"].as_array().unwrap().iter().find(|x| x["name"] == "expiry").unwrap();
    assert_eq!(exp["value"], "12/30");
    assert_eq!(exp["kind"], "date");
    let gd = ok(&mut f.host, json!({"cmd":"get_item","item_id":f.login}))["item"].clone();
    assert_eq!(gd["notes"], "fake note");
    assert!(gd["created_at"].is_u64());
    assert!(!gd.to_string().contains("FAKE-gh-pw"));
    let kinds: Vec<_> = gd["fields"].as_array().unwrap().iter().map(|x| x["kind"].as_str().unwrap().to_string()).collect();
    assert!(kinds.contains(&"password".into()) && kinds.contains(&"totp".into()) && kinds.contains(&"url".into()));
    assert_eq!(err(&mut f.host, json!({"cmd":"get_item","item_id":format!("{}:nope", f.vid)})).0, "not_found");
    let _ = f.note;
}

#[test]
fn reveal_and_card_identity_need_user_action() {
    let mut f = v5_vault();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    assert_eq!(err(&mut f.host, json!({"cmd":"reveal","item_id":f.login,"field":"password"})).0, "denied");
    assert_eq!(err(&mut f.host, json!({"cmd":"reveal","item_id":f.login,"field":"password","source":"content"})).0, "denied");
    assert_eq!(ok(&mut f.host, json!({"cmd":"reveal","item_id":f.login,"field":"password","source":"popup"}))["value"], "FAKE-gh-pw");
    assert_eq!(ok(&mut f.host, json!({"cmd":"reveal","item_id":f.card,"field":"cvv","source":"popup"}))["value"], "123");
    assert_eq!(err(&mut f.host, json!({"cmd":"reveal","item_id":f.card,"field":"nope","source":"popup"})).0, "not_found");

    assert_eq!(err(&mut f.host, json!({"cmd":"fill_card","item_id":f.card})).0, "denied");
    let c = ok(&mut f.host, json!({"cmd":"fill_card","item_id":f.card,"source":"popup"}));
    assert_eq!(c["number"], "4111 1111 1111 1234");
    assert_eq!(c["expiry"], "12/30");
    assert_eq!(c["cardholder"], "Test Person");
    let c2 = ok(&mut f.host, json!({"cmd":"fill_card","item_id":f.card,"source":"inpage_click"}));
    assert_eq!(c2["cvv"], "123");
    assert_eq!(err(&mut f.host, json!({"cmd":"fill_card","item_id":f.ident,"source":"popup"})).0, "bad_request");
    let i = ok(&mut f.host, json!({"cmd":"fill_identity","item_id":f.ident,"source":"popup"}));
    assert_eq!(i["email"], "t@example.invalid");
    assert_eq!(i["name"], "Test Person");
    // v0.6: the old `user_gesture:true` flag is no longer accepted; `source:"inpage_click"` is the only in-page form.
    assert_eq!(err(&mut f.host, json!({"cmd":"fill_card","item_id":f.card,"user_gesture":true})).0, "denied");

    // set_favorite toggles the item favorite flag.
    assert_eq!(ok(&mut f.host, json!({"cmd":"set_favorite","item_id":f.login,"favorite":true}))["favorite"], true);
    assert_eq!(ok(&mut f.host, json!({"cmd":"get_item","item_id":f.login}))["item"]["favorite"], true);
    assert_eq!(ok(&mut f.host, json!({"cmd":"set_favorite","item_id":f.login,"favorite":false}))["favorite"], false);
    assert_eq!(ok(&mut f.host, json!({"cmd":"get_item","item_id":f.login}))["item"]["favorite"], false);
    assert_eq!(err(&mut f.host, json!({"cmd":"set_favorite","item_id":f.login})).0, "bad_request");
}

#[test]
fn fill_totp_save_update_vaults_generate() {
    let mut f = v5_vault();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    let r = ok(&mut f.host, json!({"cmd":"fill","item_id":f.login,"origin":"https://github.com"}));
    assert_eq!(r["username"], "leeky");
    assert_eq!(r["password"], "FAKE-gh-pw");
    assert_eq!(r["totp"].as_str().unwrap().len(), 6);
    assert_eq!(err(&mut f.host, json!({"cmd":"fill","item_id":f.login,"origin":"https://github.com.evil.io"})).0, "origin_mismatch");
    let t = ok(&mut f.host, json!({"cmd":"totp","item_id":f.login}));
    assert_eq!(t["period"], 30);
    assert!(t["remaining"].as_u64().unwrap() <= 30);
    assert_eq!(err(&mut f.host, json!({"cmd":"totp","item_id":f.card})).0, "not_found");

    // match only returns logins, using `origin`
    let m = ok(&mut f.host, json!({"cmd":"match","origin":"https://github.com"}));
    assert_eq!(m["items"].as_array().unwrap().len(), 1);
    assert_eq!(m["items"][0]["category"], "login");

    let s = ok(&mut f.host, json!({"cmd":"save_login","origin":"https://example.com","username":"u","password":"FAKE-new"}));
    let new_id = s["item_id"].as_str().unwrap().to_string();
    assert!(new_id.starts_with(&format!("{}:", f.vid)));
    let m = ok(&mut f.host, json!({"cmd":"match","origin":"https://www.example.com"}));
    assert_eq!(m["items"][0]["title"], "example.com");
    assert_eq!(err(&mut f.host, json!({"cmd":"save_login","origin":"http://example.com","username":"u","password":"p"})).0, "origin_mismatch");
    ok(&mut f.host, json!({"cmd":"update_password","item_id":new_id,"password":"FAKE-newer"}));
    assert_eq!(ok(&mut f.host, json!({"cmd":"fill","item_id":new_id,"origin":"https://example.com"}))["password"], "FAKE-newer");

    let v = ok(&mut f.host, json!({"cmd":"list_vaults"}));
    assert_eq!(v["vaults"][0]["name"], "Personal");
    assert_eq!(v["vaults"][0]["item_count"], 5);

    let g = ok(&mut f.host, json!({"cmd":"generate","len":40,"digits":false,"symbols":false}))["password"].as_str().unwrap().to_string();
    assert_eq!(g.len(), 40);
    assert!(g.chars().all(|c| c.is_ascii_alphabetic()));
    let g = ok(&mut f.host, json!({"cmd":"generate","len":12,"digits":true,"symbols":true}))["password"].as_str().unwrap().to_string();
    assert!(g.chars().any(|c| c.is_ascii_digit()) && g.chars().any(|c| !c.is_ascii_alphanumeric()));

    let st = ok(&mut f.host, json!({"cmd":"status"}));
    assert_eq!(st["app_running"], false);
    assert_eq!(err(&mut f.host, json!({"cmd":"request_unlock","reason":"x"})).0, "app_not_running");
}

#[test]
fn create_item_any_category_standalone() {
    let mut f = v5_vault();
    let item = json!({"category":"api_credential","title":"Stripe test key","url":"https://dashboard.stripe.com","notes":"FAKE",
        "sections":[{"id":"main","title":null,"fields":[{"id":"credential","label":"credential","kind":"concealed","value":"sk_test_FAKE"}]}]});
    assert_eq!(err(&mut f.host, json!({"cmd":"create_item","item":item})).0, "locked");
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    let r = ok(&mut f.host, json!({"cmd":"create_item","item":item}));
    assert_eq!(r["title"], "Stripe test key");
    let d = ok(&mut f.host, json!({"cmd":"get_item","item_id":r["item_id"]}))["item"].clone();
    assert_eq!(d["category"], "api_credential");
    assert!(!d.to_string().contains("sk_test_FAKE"), "secret must stay concealed in detail");
    let note = json!({"category":"secure_note","title":"N","sections":[{"id":"main","title":null,"fields":[{"id":"note","label":"note","kind":"multiline","value":"hello"}]}]});
    assert_eq!(ok(&mut f.host, json!({"cmd":"list_items","category":"secure_note"}))["items"].as_array().unwrap().len(), 1);
    ok(&mut f.host, json!({"cmd":"create_item","item":note}));
    assert_eq!(ok(&mut f.host, json!({"cmd":"list_items","category":"secure_note"}))["items"].as_array().unwrap().len(), 2);
    // validation (same rules as the desktop app)
    assert_eq!(err(&mut f.host, json!({"cmd":"create_item","item":{"category":"login","title":"  "}})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"create_item","item":{"category":"passkey","title":"x"}})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"create_item","item":{"category":"bogus","title":"x"}})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"create_item","item":{"category":"login","title":"x","sections":[{"id":"m","title":null,"fields":[
        {"id":"otp","label":"otp","kind":"totp","value":"not a secret!!"}]}]}})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"create_item","item":{"category":"login","title":"x","sections":[{"id":"m","title":null,"fields":[
        {"id":"a","label":"a","kind":"text","value":"1"},{"id":"a","label":"a","kind":"text","value":"2"}]}]}})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"create_item","item":{"vaultId":"nope","category":"secure_note","title":"x"}})).0, "not_found");
}
