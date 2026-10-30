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
    // Leak check uses the full card number (spaced and unspaced): the short
    // "4111" prefix also matches random ids/timestamps in the list output,
    // which made this assertion fail at random (see #206).
    let list = all.to_string();
    assert!(
        !list.contains("FAKE-gh-pw")
            && !list.contains("4111 1111 1111 1234")
            && !list.contains("4111111111111234")
    );

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
    assert!(
        !d.to_string().contains("4111 1111 1111 1234")
            && !d.to_string().contains("4111111111111234")
            && !d.to_string().contains("\"123\"")
    );
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
    assert_eq!(err(&mut f.host, json!({"cmd":"fill","item_id":f.login,"origin":"https://another-domain.example"})).0, "origin_mismatch");
    assert_eq!(err(&mut f.host, json!({"cmd":"fill","item_id":f.login,"origin":"https://another-domain.example","confirm":true})).0, "origin_mismatch");
    assert_eq!(err(&mut f.host, json!({"cmd":"fill","item_id":f.login,"origin":"https://another-domain.example","user_confirm":"true"})).0, "origin_mismatch");
    let confirmed_fill = ok(&mut f.host, json!({"cmd":"fill","item_id":f.login,"origin":"https://another-domain.example","user_confirm":true}));
    assert_eq!(confirmed_fill["username"], "leeky");
    assert_eq!(confirmed_fill["password"], "FAKE-gh-pw");

    // add_url adds origin to item, allowing subsequent match and fill without user_confirm
    ok(&mut f.host, json!({"cmd":"add_url","item_id":f.login,"origin":"https://another-domain.example","url":"https://another-domain.example"}));
    let m_added = ok(&mut f.host, json!({"cmd":"match","origin":"https://another-domain.example"}));
    assert_eq!(m_added["items"].as_array().unwrap().len(), 1);
    assert_eq!(m_added["items"][0]["title"], "GitHub");
    let fill_added = ok(&mut f.host, json!({"cmd":"fill","item_id":f.login,"origin":"https://another-domain.example"}));
    assert_eq!(fill_added["username"], "leeky");
    assert_eq!(fill_added["password"], "FAKE-gh-pw");

    // add_url creates standard url1 field in urls section, which survives update_login
    let item_detail = ok(&mut f.host, json!({"cmd":"get_item","item_id":f.login}))["item"].clone();
    let fields = item_detail["fields"].as_array().unwrap();
    let added_url_field = fields.iter().find(|x| x["name"] == "url1").expect("url1 field");
    assert_eq!(added_url_field["value"], "https://another-domain.example");

    ok(&mut f.host, json!({
        "cmd": "update_login", "item_id": f.login, "source": "popup",
        "title": "GitHub", "username": "leeky",
        "urls": ["https://github.com", "https://another-domain.example"]
    }));
    let item_after = ok(&mut f.host, json!({"cmd":"get_item","item_id":f.login}))["item"].clone();
    let fields_after = item_after["fields"].as_array().unwrap();
    let preserved = fields_after.iter().find(|x| x["name"] == "url1").expect("url1 preserved");
    assert_eq!(preserved["value"], "https://another-domain.example");

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

    // The banner's vault picker: save_login honors a non-default vault and rejects an unknown one.
    let s2 = ok(&mut f.host, json!({"cmd":"save_login","origin":"https://work.example.com","title":"Work login","username":"w","password":"FAKE-work","vault_id":f.vid2}));
    let work_id = s2["item_id"].as_str().unwrap().to_string();
    assert!(work_id.starts_with(&format!("{}:", f.vid2)), "{work_id}");
    let work = ok(&mut f.host, json!({"cmd":"list_items","vault_id":f.vid2}));
    assert_eq!(work["items"].as_array().unwrap().len(), 1);
    assert_eq!(work["items"][0]["title"], "Work login");
    let personal = ok(&mut f.host, json!({"cmd":"list_items","vault_id":f.vid}));
    assert!(!personal["items"].as_array().unwrap().iter().any(|i| i["title"] == "Work login"));
    assert_eq!(err(&mut f.host, json!({"cmd":"save_login","origin":"https://x.example.com","username":"u","password":"p","vault_id":"nope"})).0, "not_found");

    let v = ok(&mut f.host, json!({"cmd":"list_vaults"}));
    assert_eq!(v["vaults"][0]["name"], "Personal");
    assert_eq!(v["vaults"][0]["item_count"], 5);

    let g = ok(&mut f.host, json!({"cmd":"generate","len":40,"digits":false,"symbols":false}))["password"].as_str().unwrap().to_string();
    assert_eq!(g.len(), 40);
    assert!(g.chars().all(|c| c.is_ascii_alphabetic()));
    let g = ok(&mut f.host, json!({"cmd":"generate","len":12,"digits":true,"symbols":true}))["password"].as_str().unwrap().to_string();
    assert!(g.chars().any(|c| c.is_ascii_digit()) && g.chars().any(|c| !c.is_ascii_alphanumeric()));

    // Passphrase and PIN modes share gitpass-core's generator with the CLI and desktop app.
    let g = ok(&mut f.host, json!({"cmd":"generate","len":5,"words":true}))["password"].as_str().unwrap().to_string();
    assert_eq!(g.split('-').count(), 5, "{g}");
    let g = ok(&mut f.host, json!({"cmd":"generate","len":6,"pin":true,"digits":false,"symbols":false}))["password"].as_str().unwrap().to_string();
    assert_eq!(g.len(), 6, "{g}");
    assert!(g.chars().all(|c| c.is_ascii_digit()), "{g}");
    assert_eq!(err(&mut f.host, json!({"cmd":"generate","len":2,"pin":true})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"generate","len":25,"words":true})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"generate","len":7})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"generate","len":129})).0, "bad_request");

    // Parity with the desktop app and CLI: a passphrase with symbols off joins words with '.', not '-'.
    let g = ok(&mut f.host, json!({"cmd":"generate","len":4,"words":true,"symbols":false,"digits":false}))["password"].as_str().unwrap().to_string();
    assert_eq!(g.split('.').count(), 4, "{g}");
    assert!(!g.contains('-'), "{g}");
    // Defaults per mode when `len` is absent.
    assert_eq!(ok(&mut f.host, json!({"cmd":"generate"}))["password"].as_str().unwrap().len(), 20);
    assert_eq!(ok(&mut f.host, json!({"cmd":"generate","pin":true}))["password"].as_str().unwrap().len(), 6);
    assert_eq!(ok(&mut f.host, json!({"cmd":"generate","words":true}))["password"].as_str().unwrap().split('-').count(), 5);

    // Memorable mode: separator and capitalize options (issue #326).
    let g = ok(&mut f.host, json!({"cmd":"generate","len":4,"words":true,"separator":"_","capitalize":true}))["password"].as_str().unwrap().to_string();
    assert_eq!(g.split('_').count(), 4, "{g}");
    for w in g.split('_') {
        assert!(w.chars().next().unwrap().is_ascii_uppercase(), "{w} in {g}");
    }
    let g = ok(&mut f.host, json!({"cmd":"generate","len":3,"words":true,"separator":" "}))["password"].as_str().unwrap().to_string();
    assert_eq!(g.split(' ').count(), 3, "{g}");

    // Invalid separator is rejected with bad_request.
    assert_eq!(err(&mut f.host, json!({"cmd":"generate","words":true,"separator":"/"})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"generate","words":true,"separator":"invalid"})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"generate","words":true,"separator":""})).0, "bad_request");
    assert_eq!(err(&mut f.host, json!({"cmd":"generate","words":true,"separator":123})).0, "bad_request");

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

#[test]
fn host_generate_separator_and_capitalize_validation() {
    use gitpass_core::generate::{Mode, Request};
    use gitpass_host::handler::generate;

    // Allowed separators are honored.
    for sep in ['-', '.', '_', ' '] {
        let r = Request {
            mode: Mode::Words,
            length: Some(4),
            symbols: true,
            digits: false,
            separator: Some(sep),
            capitalize: true,
        };
        let pw = generate(&r).unwrap();
        assert_eq!(pw.split(sep).count(), 4, "{pw}");
        for w in pw.split(sep) {
            assert!(w.chars().next().unwrap().is_ascii_uppercase(), "{w} in {pw}");
        }
    }

    // Invalid separators rejected.
    for bad in ['/', 'a', '0', ':', '\n'] {
        let r = Request {
            mode: Mode::Words,
            length: Some(4),
            symbols: true,
            digits: false,
            separator: Some(bad),
            capitalize: false,
        };
        assert!(generate(&r).is_err());
    }

    // Defaults unchanged when omitted.
    let r = Request {
        mode: Mode::Words,
        length: Some(5),
        symbols: true,
        digits: true,
        separator: None,
        capitalize: false,
    };
    let pw = generate(&r).unwrap();
    assert_eq!(pw.split('-').count(), 5, "{pw}");
}
