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
    r["error"].as_str().unwrap().to_string()
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
fn missing_secret_key_file() {
    let mut f = setup();
    std::fs::remove_file(&f.host.cfg.secret_key_path).unwrap();
    assert!(err(&mut f.host, json!({"cmd":"unlock","password":PW})).contains("Secret Key"));
}
