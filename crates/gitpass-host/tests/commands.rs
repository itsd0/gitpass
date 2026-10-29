use gitpass_core::crypto::KdfParams;
use gitpass_core::store::{self, Item, Session, UnlockOpts};
use gitpass_host::handler::{Host, HostConfig};
use serde_json::{json, Value};
use std::time::Duration;

fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
const PW: &str = "correct horse battery";

struct Fx { _d: tempfile::TempDir, host: Host, paypal: String, gh: String }

fn item(t: &str, u: &str, url: &str, totp: Option<&str>) -> Item {
    Item { title: t.into(), username: Some(u.into()), password: Some(format!("{t}-pw")), url: Some(url.into()),
        notes: None, totp: totp.map(Into::into), updated_at: store::now() }
}

fn setup() -> Fx {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    std::fs::create_dir_all(&root).unwrap();
    let sk = store::init(&root, PW, fast()).unwrap();
    let skf = d.path().join("secret.key");
    std::fs::write(&skf, format!("{}\n", sk.encode())).unwrap();
    let s = Session::unlock_with(&root, PW, &sk, &UnlockOpts { accept_rollback: false, target_kdf: fast() }).unwrap();
    let vid = s.create_vault("personal").unwrap();
    let paypal = format!("{vid}:{}", s.add_item(&vid, &item("PayPal", "leeky@pp", "https://www.paypal.com/signin", None)).unwrap());
    let gh = format!("{vid}:{}", s.add_item(&vid, &item("GitHub", "leeky", "github.com", Some("JBSWY3DPEHPK3PXP"))).unwrap());
    s.add_item(&vid, &item("Evil", "x", "https://paypal.com.evil.io", None)).unwrap();
    drop(s);
    let host = Host::new(HostConfig { vault_path: root, secret_key_path: skf, idle_timeout: Duration::from_secs(300), target_kdf: fast() });
    Fx { _d: d, host, paypal, gh }
}

fn call(h: &mut Host, v: Value) -> Value { h.handle(&v) }
fn ok(h: &mut Host, v: Value) -> Value {
    let r = call(h, v.clone());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r["result"].clone()
}
fn err(h: &mut Host, v: Value) -> String {
    let r = call(h, v.clone());
    assert_eq!(r["ok"], false, "{v} -> {r}");
    format!("{}: {}", r["error"].as_str().unwrap(), r["message"].as_str().unwrap())
}

#[test]
fn locked_by_default_and_wrong_password() {
    let mut f = setup();
    let r = call(&mut f.host, json!({"id": 7, "cmd": "status"}));
    assert_eq!(r["id"], 7);
    assert_eq!(r["result"]["unlocked"], false);
    assert!(err(&mut f.host, json!({"cmd":"match","url":"https://paypal.com"})).contains("locked"));
    assert!(err(&mut f.host, json!({"cmd":"unlock","password":"nope"})).contains("unlock failed"));
    assert!(err(&mut f.host, json!({"cmd":"bogus"})).contains("unknown command"));
}

#[test]
fn match_fill_totp_generate_lock() {
    let mut f = setup();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    assert_eq!(ok(&mut f.host, json!({"cmd":"status"}))["unlocked"], true);

    let m = ok(&mut f.host, json!({"cmd":"match","url":"https://paypal.com/login"}));
    let items = m["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{m}");
    assert_eq!(items[0]["title"], "PayPal");
    assert!(items[0].get("password").is_none(), "match must not leak passwords");
    assert!(!m.to_string().contains("-pw"));

    // phishing origin only sees the attacker's own item, never PayPal
    let m = ok(&mut f.host, json!({"cmd":"match","url":"https://paypal.com.evil.io"}));
    assert!(m["items"].as_array().unwrap().iter().all(|i| i["title"] == "Evil"));

    let r = ok(&mut f.host, json!({"cmd":"fill","item_id":f.paypal,"origin":"https://www.paypal.com"}));
    assert_eq!(r["username"], "leeky@pp");
    assert_eq!(r["password"], "PayPal-pw");
    let item_resp = ok(&mut f.host, json!({"cmd":"get_item","item_id":f.paypal}));
    assert!(item_resp["item"]["last_used"].as_u64().is_some(), "fill must update last_used");
    assert!(err(&mut f.host, json!({"cmd":"fill","item_id":f.paypal,"origin":"https://paypal.com.evil.io"})).contains("refused"));
    assert!(err(&mut f.host, json!({"cmd":"fill","item_id":f.paypal,"origin":"http://paypal.com"})).contains("refused"));
    assert!(err(&mut f.host, json!({"cmd":"match","url":"http://paypal.com"})).contains("refused"));
    assert!(err(&mut f.host, json!({"cmd":"fill","item_id":"nocolon","origin":"https://paypal.com"})).contains("bad item_id"));

    let t = ok(&mut f.host, json!({"cmd":"totp","item_id":f.gh}));
    assert_eq!(t["code"].as_str().unwrap().len(), 6);
    assert!(err(&mut f.host, json!({"cmd":"totp","item_id":f.paypal})).contains("no TOTP"));

    let s = ok(&mut f.host, json!({"cmd":"search","query":"git"}));
    assert_eq!(s["items"][0]["title"], "GitHub");
    assert!(!s.to_string().contains("-pw"));

    let g = ok(&mut f.host, json!({"cmd":"generate","len":32}));
    assert_eq!(g["password"].as_str().unwrap().chars().count(), 32);
    err(&mut f.host, json!({"cmd":"generate","len":4}));

    // Site password rules (extension `page:generate`): required classes present, only allowed symbols.
    let rules = json!({"classes":[
        {"kind":"lower","required":true},{"kind":"upper","required":true},{"kind":"digit","required":true},
        {"kind":"special","required":false,"chars":"-_"}]});
    for _ in 0..30 {
        let g = ok(&mut f.host, json!({"cmd":"generate","len":16,"rules":rules}));
        let p = g["password"].as_str().unwrap();
        assert_eq!(p.chars().count(), 16, "{p}");
        assert!(p.chars().any(|c| c.is_ascii_lowercase()) && p.chars().any(|c| c.is_ascii_uppercase()) && p.chars().any(|c| c.is_ascii_digit()), "{p}");
        assert!(p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "{p}");
    }
    for bad in [
        json!({"classes":[]}),
        json!({"classes":[{"kind":"emoji","required":true}]}),
        json!({"classes":[{"kind":"special","chars":"a1 "}]}),
        json!({"classes":[{"kind":"special","chars":"é"}]}),
    ] {
        err(&mut f.host, json!({"cmd":"generate","len":16,"rules":bad}));
    }
    err(&mut f.host, json!({"cmd":"generate","len":16,"words":true,"rules":rules}));

    ok(&mut f.host, json!({"cmd":"lock"}));
    assert!(err(&mut f.host, json!({"cmd":"fill","item_id":f.paypal,"origin":"https://paypal.com"})).contains("locked"));
}

#[test]
fn idle_auto_lock() {
    let mut f = setup();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    f.host.backdate_activity(Duration::from_secs(299));
    assert!(!f.host.expire_if_idle());
    f.host.backdate_activity(Duration::from_secs(301));
    assert!(f.host.expire_if_idle());
    assert!(!f.host.is_unlocked());
    // and lazily on the next request too
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    f.host.backdate_activity(Duration::from_secs(400));
    assert!(err(&mut f.host, json!({"cmd":"match","url":"https://github.com"})).contains("locked"));
}

#[test]
fn idle_auto_lock_after_sleep_wall_clock() {
    let mut f = setup();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    assert!(f.host.is_unlocked());
    // Monotonic Instant remains frozen as if asleep, but wall clock advanced past timeout
    f.host.backdate_wall_activity(Duration::from_secs(305));
    assert!(f.host.expire_if_idle(), "must expire when wall clock shows elapsed > timeout despite paused instant");
    assert!(!f.host.is_unlocked());
}

#[test]
fn missing_secret_key_file() {
    let mut f = setup();
    std::fs::remove_file(&f.host.cfg.secret_key_path).unwrap();
    assert!(err(&mut f.host, json!({"cmd":"unlock","password":PW})).contains("Secret Key"));
}

#[test]
fn unlock_with_provided_secret_key_no_file() {
    let mut f = setup();
    let sk = std::fs::read_to_string(&f.host.cfg.secret_key_path).unwrap();
    std::fs::remove_file(&f.host.cfg.secret_key_path).unwrap();
    assert_eq!(ok(&mut f.host, json!({"cmd":"status"}))["secret_key_file"], false);
    assert!(!err(&mut f.host, json!({"cmd":"unlock","password":PW,"secret_key":"A3-BOGUS"})).is_empty());
    assert!(!f.host.is_unlocked());
    ok(&mut f.host, json!({"cmd":"unlock","password":PW,"secret_key":sk.trim()}));
    let m = ok(&mut f.host, json!({"cmd":"match","url":"https://www.paypal.com"}));
    assert_eq!(m["items"].as_array().unwrap().len(), 1);
    // host never echoes the secret key back
    assert!(!m.to_string().contains(sk.trim()));
    ok(&mut f.host, json!({"cmd":"lock"}));
    assert!(err(&mut f.host, json!({"cmd":"match","url":"https://www.paypal.com"})).contains("locked"));
    // Not retained: unlocking again without it fails.
    assert!(err(&mut f.host, json!({"cmd":"unlock","password":PW})).contains("Secret Key required"));
}

#[test]
fn item_findings_contract_and_security() {
    let mut f = setup();
    // 1. Locked gives locked
    assert!(err(&mut f.host, json!({"cmd":"item_findings","item_id":f.paypal})).contains("locked"));

    // Unlock
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));

    // 2. Unknown id gives not_found
    let s_err = err(&mut f.host, json!({"cmd":"item_findings","item_id":"v1:nope"}));
    assert!(s_err.contains("not_found") || s_err.contains("not found"));

    // 3. Bad item_id format gives bad_request
    assert!(err(&mut f.host, json!({"cmd":"item_findings","item_id":"invalid_id"})).contains("bad item_id"));

    // Create 2 items with the same weak password
    let secret = "123456";
    let s1 = ok(&mut f.host, json!({
        "cmd": "save_login",
        "origin": "https://accounts.example.com",
        "username": "alice",
        "password": secret,
        "title": "Example Login 1"
    }));
    let id1 = s1["item_id"].as_str().unwrap().to_string();

    let s2 = ok(&mut f.host, json!({
        "cmd": "save_login",
        "origin": "https://service.example.org",
        "username": "bob",
        "password": secret,
        "title": "Example Login 2"
    }));
    let id2 = s2["item_id"].as_str().unwrap().to_string();

    // Query item 1 findings
    let f1 = ok(&mut f.host, json!({"cmd":"item_findings","item_id":id1}));
    let kinds1: Vec<&str> = f1["kinds"].as_array().unwrap().iter().map(|k| k.as_str().unwrap()).collect();
    assert!(kinds1.contains(&"weak"), "must flag weak password: {f1:?}");
    assert!(kinds1.contains(&"reused"), "must flag reused password: {f1:?}");
    assert_eq!(f1["reused_count"], 2);
    assert_eq!(f1["change_password_url"], "https://accounts.example.com/.well-known/change-password");

    // 4. No secret fields in JSON (no password value, no hashes, no other item IDs)
    let json_str = f1.to_string();
    assert!(!json_str.contains(secret), "secret password must never leak in findings JSON: {json_str}");
    assert!(!json_str.contains(&id2), "other item ids must never leak: {json_str}");

    // 5. Ignored finding is hidden
    let (vid, iid) = id1.split_once(':').unwrap();
    // Reopen session to mutate record and ignore "weak"
    let sk = gitpass_core::crypto::SecretKey::parse(&std::fs::read_to_string(&f.host.cfg.secret_key_path).unwrap()).unwrap();
    let s = Session::unlock_with(&f.host.cfg.vault_path, PW, &sk, &UnlockOpts { accept_rollback: false, target_kdf: fast() }).unwrap();
    s.ignore_finding(vid, iid, "weak").unwrap();
    drop(s);

    // Lock and re-unlock host to reload session
    ok(&mut f.host, json!({"cmd":"lock"}));
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));

    let f1_after = ok(&mut f.host, json!({"cmd":"item_findings","item_id":id1}));
    let kinds1_after: Vec<&str> = f1_after["kinds"].as_array().unwrap().iter().map(|k| k.as_str().unwrap()).collect();
    assert!(!kinds1_after.contains(&"weak"), "ignored finding must be hidden: {f1_after:?}");
    assert!(kinds1_after.contains(&"reused"), "non-ignored finding must still be visible");
}

#[test]
fn open_in_app_standalone_validation() {
    let mut f = setup();

    // Unknown view rejected with bad_request
    let e1 = err(&mut f.host, json!({"cmd": "open_in_app", "view": "invalid_screen"}));
    assert!(e1.starts_with("bad_request"), "unknown view must return bad_request: {e1}");

    // Valid views pass validation (not bad_request)
    for v in ["generator_history", "watchtower", "settings"] {
        let r = call(&mut f.host, json!({"cmd": "open_in_app", "view": v}));
        if r["ok"] == false {
            assert_eq!(r["error"], "app_not_running", "expected app_not_running if app uninstalled, got: {r}");
        } else {
            assert_eq!(r["result"]["launched"], true);
        }
    }
}

#[test]
fn record_generated_password_respects_settings_and_returns_recorded_flag() {
    let mut f = setup();
    let history_file = f.host.cfg.vault_path.join("generator_history.enc");
    let settings_file = f.host.cfg.vault_path.join(".gitpass-settings.json");

    // 1. Initially no generator history on disk
    assert!(!history_file.exists());

    // 2. Default (no settings file): records password and returns recorded: true
    ok(&mut f.host, json!({"cmd": "unlock", "password": PW}));
    let r1 = ok(&mut f.host, json!({
        "cmd": "record_generated_password",
        "password": "FAKE-generated-password-1",
        "source": "extension",
        "origin": "https://github.com/login"
    }));
    assert_eq!(r1["ok"], true);
    assert_eq!(r1["recorded"], true);
    assert!(history_file.exists(), "generator_history.enc must exist after recording");
    let bytes_after_r1 = std::fs::read(&history_file).unwrap();

    // 3. When settings.rememberGeneratedPasswords is false (real desktop LocalFile structure):
    // Skips recording, returns recorded: false, and generator_history.enc is unchanged.
    std::fs::write(&settings_file, serde_json::to_string(&json!({
        "settings": {
            "rememberGeneratedPasswords": false
        }
    })).unwrap()).unwrap();

    let r2 = ok(&mut f.host, json!({
        "cmd": "record_generated_password",
        "password": "FAKE-generated-password-2",
        "source": "extension",
        "origin": "https://example.com"
    }));
    assert_eq!(r2["ok"], true);
    assert_eq!(r2["recorded"], false);
    let bytes_after_r2 = std::fs::read(&history_file).unwrap();
    assert_eq!(bytes_after_r1, bytes_after_r2, "history file must not be modified when setting is off");

    // 4. When locked and setting is off: returns ok: true, recorded: false without needing unlock
    ok(&mut f.host, json!({"cmd": "lock"}));
    let r3 = ok(&mut f.host, json!({
        "cmd": "record_generated_password",
        "password": "FAKE-generated-password-3",
        "source": "extension"
    }));
    assert_eq!(r3["ok"], true);
    assert_eq!(r3["recorded"], false);

    // 5. When setting is restored to true: records again and updates history file
    std::fs::write(&settings_file, serde_json::to_string(&json!({
        "settings": {
            "rememberGeneratedPasswords": true
        }
    })).unwrap()).unwrap();

    // When locked and setting is true: requires unlock
    assert!(err(&mut f.host, json!({
        "cmd": "record_generated_password",
        "password": "FAKE-generated-password-4"
    })).contains("locked"));

    // Once unlocked: records and updates history
    ok(&mut f.host, json!({"cmd": "unlock", "password": PW}));
    let r4 = ok(&mut f.host, json!({
        "cmd": "record_generated_password",
        "password": "FAKE-generated-password-4",
        "source": "extension",
        "host": "google.com"
    }));
    assert_eq!(r4["ok"], true);
    assert_eq!(r4["recorded"], true);
    let bytes_after_r4 = std::fs::read(&history_file).unwrap();
    assert_ne!(bytes_after_r1, bytes_after_r4, "history file must be updated when setting is re-enabled");
}
