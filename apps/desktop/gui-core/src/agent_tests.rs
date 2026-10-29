//! Agent socket tests: a real UNIX-socket client against a temp vault. All data is FAKE.
use crate::agent::*;
use crate::api::*;
use gitpass_core::crypto::KdfParams;
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::store::{self, UnlockOpts};
use serde_json::{json, Value};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }

/// Test UI: request_unlock unlocks the vault from another thread (stands in for Touch ID / password).
struct TestUi { app: Mutex<Option<(Arc<App>, String, String)>>, opened: Mutex<Vec<(Option<String>, Option<String>)>> }
impl AgentUi for TestUi {
    fn request_unlock(&self, _reason: &str) {
        if let Some((app, dir, sk)) = self.app.lock().unwrap().clone() {
            std::thread::spawn(move || { std::thread::sleep(Duration::from_millis(300)); app.unlock(&dir, "correct horse", Some(&sk)).unwrap(); });
        }
    }
    fn open_item(&self, v: Option<&str>, i: Option<&str>) { self.opened.lock().unwrap().push((v.map(Into::into), i.map(Into::into))); }
}

struct Env { _vault: tempfile::TempDir, _cfg: tempfile::TempDir, _sock: tempfile::TempDir, app: Arc<App>, agent: Arc<Agent>, ui: Arc<TestUi>, vault: String }

fn env() -> Env {
    let d = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let sockdir = tempfile::Builder::new().prefix("gpa").tempdir_in("/tmp").unwrap(); // short path (sun_path limit)
    let sk = store::init(d.path(), "correct horse", fast()).unwrap().encode();
    let app = Arc::new(App::new(cfg.path().to_path_buf(), Arc::new(MemClipboard::default())));
    app.set_unlock_opts(UnlockOpts { accept_rollback: false, target_kdf: fast() });
    app.unlock(&d.path().display().to_string(), "correct horse", Some(&sk)).unwrap();
    let vault = app.list_vaults().unwrap()[0].id.clone();
    let ui = Arc::new(TestUi { app: Mutex::new(Some((app.clone(), d.path().display().to_string(), sk))), opened: Mutex::new(vec![]) });
    let agent = Agent::start(app.clone(), ui.clone(), &sockdir.path().join("gitpass")).unwrap();
    Env { _vault: d, _cfg: cfg, _sock: sockdir, app, agent, ui, vault }
}

struct Client { s: UnixStream, n: u64, events: Vec<Value> }
impl Client {
    fn raw(e: &Env) -> Client {
        let s = UnixStream::connect(&e.agent.socket_path).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        Client { s, n: 0, events: vec![] }
    }
    fn connect(e: &Env) -> (Client, Value) {
        let mut c = Client::raw(e);
        let token = std::fs::read_to_string(e.agent.socket_path.with_file_name("agent.token")).unwrap();
        let r = c.call("hello", json!({"client": "gitpass-host", "version": "0.6.0", "token": token}));
        (c, r)
    }
    fn call(&mut self, cmd: &str, args: Value) -> Value {
        self.n += 1;
        let mut q = args.as_object().cloned().unwrap_or_default();
        q.insert("id".into(), json!(self.n)); q.insert("cmd".into(), json!(cmd));
        write_frame(&mut self.s, &Value::Object(q)).unwrap();
        loop {
            let v = read_frame(&mut self.s).unwrap().expect("connection closed");
            if v.get("event").is_some() { self.events.push(v); continue; }
            assert_eq!(v["id"], json!(self.n), "{v}");
            return v;
        }
    }
    fn ok(&mut self, cmd: &str, args: Value) -> Value { let r = self.call(cmd, args); assert_eq!(r["ok"], json!(true), "{cmd}: {r}"); r["result"].clone() }
    fn err(&mut self, cmd: &str, args: Value) -> String { let r = self.call(cmd, args); assert_eq!(r["ok"], json!(false), "{cmd}: {r}"); r["error"].as_str().unwrap().to_string() }
    fn wait_event(&mut self, name: &str) {
        if let Some(i) = self.events.iter().position(|e| e["event"] == json!(name)) { self.events.remove(i); return; }
        loop { let v = read_frame(&mut self.s).unwrap().expect("closed"); if v["event"] == json!(name) { return; } }
    }
}

fn fld(id: &str, kind: FieldKind, v: &str) -> Field { Field { id: id.into(), label: id.into(), kind, value: Some(v.into()), masked: None, diff: None } }
fn add_login(e: &Env, title: &str, url: &str, user: &str, pw: &str) -> String {
    let s = e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::Login, title: title.into(), url: Some(url.into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, user), fld("password", FieldKind::Concealed, pw)] }],
        notes: None, tags: None, favorite: None }).unwrap();
    format!("{}:{}", e.vault, s.id)
}
fn add_card(e: &Env) -> String {
    let s = e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::CreditCard, title: "Example Bank".into(), url: None,
        sections: vec![Section { id: "card".into(), title: Some("Card".into()), fields: vec![fld("number", FieldKind::Concealed, "4111 1111 1111 1111"), fld("cvv", FieldKind::Concealed, "123")] }],
        notes: None, tags: None, favorite: Some(true) }).unwrap();
    format!("{}:{}", e.vault, s.id)
}

#[test]
fn socket_and_token_permissions_and_peer_uid() {
    use std::os::unix::fs::PermissionsExt;
    let e = env();
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&e.agent.socket_path), 0o600);
    assert_eq!(mode(e.agent.socket_path.parent().unwrap()), 0o700);
    assert_eq!(mode(&e.agent.socket_path.with_file_name("agent.token")), 0o600);
    assert_eq!(e.agent.token().len(), 64);
    let c = Client::raw(&e);
    assert_eq!(peer_uid(&c.s).unwrap(), unsafe { libc::getuid() });
}

#[test]
fn auth_rejects_bad_token_and_commands_before_hello() {
    let e = env();
    let mut c = Client::raw(&e);
    assert_eq!(c.err("hello", json!({"client": "gitpass-host", "token": "00".repeat(32)})), "denied");
    assert!(read_frame(&mut c.s).unwrap().is_none(), "connection closed after reject");
    let mut c = Client::raw(&e);
    assert_eq!(c.err("hello", json!({"client": "gitpass-host"})), "denied");
    let mut c = Client::raw(&e);
    assert_eq!(c.err("list_items", json!({"token": e.agent.token()})), "denied");
    assert!(read_frame(&mut c.s).unwrap().is_none());
    let (_, h) = Client::connect(&e);
    assert_eq!(h["ok"], json!(true), "{h}");
    assert_eq!(h["result"]["unlocked"], json!(true));
    assert_eq!(h["result"]["account"]["vault_count"], json!(1));
}

#[test]
fn locked_responses() {
    let e = env();
    add_login(&e, "GitHub", "https://github.com", "octo", "pw1");
    e.app.lock();
    let (mut c, h) = Client::connect(&e);
    assert_eq!(h["result"]["unlocked"], json!(false));
    let st = c.ok("status", json!({}));
    assert_eq!((st["unlocked"].clone(), st["app_running"].clone(), st["idle_timeout_secs"].clone()), (json!(false), json!(true), json!(300)));
    for (cmd, a) in [("list_items", json!({})), ("match", json!({"origin": "https://github.com"})), ("list_vaults", json!({})),
        ("fill", json!({"item_id": "a:b", "origin": "https://github.com"})), ("save_login", json!({"origin": "https://x.com", "username": "u", "password": "p"}))] {
        assert_eq!(c.err(cmd, a), "locked", "{cmd}");
    }
    assert_eq!(c.err("nope", json!({})), "bad_request");
    assert!(c.ok("generate", json!({"len": 24, "digits": true, "symbols": false}))["password"].as_str().unwrap().len() == 24);
    assert_eq!(c.err("generate", json!({"len": 4})), "bad_request");
    assert_eq!(c.ok("generate", json!({"pin": true}))["password"].as_str().unwrap().len(), 6);
}

#[test]
fn match_fill_reveal_origin_rules_and_categories() {
    let e = env();
    let gh = add_login(&e, "GitHub", "https://github.com/login", "octo", "pw-gh");
    let _other = add_login(&e, "Example", "https://example.org", "ex", "pw-ex");
    let card = add_card(&e);
    let (mut c, _) = Client::connect(&e);
    let m = c.ok("match", json!({"origin": "https://www.github.com"}));
    let ids: Vec<&str> = m["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec![gh.as_str()]);
    assert_eq!(m["items"][0]["subtitle"], json!("octo"));
    assert_eq!(m["items"][0]["vault_name"], json!("Personal"));
    assert_eq!(m["items"][0]["favicon_host"], json!("github.com"));
    assert!(c.ok("match", json!({"origin": "https://github.com.evil.io"}))["items"].as_array().unwrap().is_empty());
    assert_eq!(c.err("match", json!({"origin": "http://github.com"})), "origin_mismatch");

    let f = c.ok("fill", json!({"item_id": gh, "origin": "https://github.com"}));
    assert_eq!((f["username"].clone(), f["password"].clone()), (json!("octo"), json!("pw-gh")));
    assert!(c.ok("get_item", json!({"item_id": gh}))["item"]["last_used"].as_u64().is_some());
    assert_eq!(c.err("fill", json!({"item_id": gh, "origin": "https://github.com.evil.io"})), "origin_mismatch");
    assert_eq!(c.err("fill", json!({"item_id": gh, "origin": "https://example.org"})), "origin_mismatch");
    assert_eq!(c.err("fill", json!({"item_id": gh, "origin": "https://example.org", "confirm": true})), "origin_mismatch");
    assert_eq!(c.err("fill", json!({"item_id": gh, "origin": "https://example.org", "user_confirm": "true"})), "origin_mismatch");
    let f_confirmed = c.ok("fill", json!({"item_id": gh, "origin": "https://example.org", "user_confirm": true}));
    assert_eq!((f_confirmed["username"].clone(), f_confirmed["password"].clone()), (json!("octo"), json!("pw-gh")));
    assert_eq!(c.err("fill", json!({"item_id": format!("{}:nope", e.vault), "origin": "https://github.com"})), "not_found");

    // reveal: popup only
    assert_eq!(c.err("reveal", json!({"item_id": gh, "field": "password"})), "denied");
    assert_eq!(c.err("reveal", json!({"item_id": gh, "field": "password", "source": "content"})), "denied");
    assert_eq!(c.ok("reveal", json!({"item_id": gh, "field": "password", "source": "popup"}))["value"], json!("pw-gh"));

    // get_item: secrets omitted, flagged concealed
    let d = c.ok("get_item", json!({"item_id": gh}))["item"].clone();
    let pw = d["fields"].as_array().unwrap().iter().find(|f| f["name"] == json!("password")).unwrap().clone();
    assert_eq!(pw["concealed"], json!(true)); assert!(pw.get("value").is_none()); assert_eq!(pw["kind"], json!("password"));
    assert!(!d.to_string().contains("pw-gh"));

    // real v5 categories
    let all = c.ok("list_items", json!({}))["items"].clone();
    let cat = |id: &str| all.as_array().unwrap().iter().find(|i| i["id"] == json!(id)).unwrap()["category"].clone();
    assert_eq!(cat(&gh), json!("login")); assert_eq!(cat(&card), json!("credit_card"));
    let cards = c.ok("list_items", json!({"category": "credit_card"}))["items"].clone();
    assert_eq!(cards.as_array().unwrap().len(), 1);
    assert_eq!(cards[0]["subtitle"], json!("Visa •••• 1111"));
    let cd = c.ok("get_item", json!({"item_id": card}))["item"].clone();
    assert_eq!(cd["fields"][0]["kind"], json!("card_number"));
    assert!(!cd.to_string().contains("4111 1111"));
    assert_eq!(c.ok("list_items", json!({"query": "exam", "limit": 5}))["items"].as_array().unwrap().len(), 2);
    assert_eq!(c.ok("list_items", json!({"limit": 1}))["items"].as_array().unwrap().len(), 1);

    // fill_card: popup or in-page click only
    assert_eq!(c.err("fill_card", json!({"item_id": card})), "denied");
    assert_eq!(c.ok("fill_card", json!({"item_id": card, "source": "popup"}))["number"], json!("4111 1111 1111 1111"));
    assert_eq!(c.err("fill_identity", json!({"item_id": card, "source": "popup"})), "bad_request");

    let v = c.ok("list_vaults", json!({}));
    assert_eq!(v["vaults"][0]["item_count"], json!(3));
    c.ok("open_in_app", json!({"item_id": gh}));
    let (v, i) = gh.split_once(':').unwrap();
    assert_eq!(e.ui.opened.lock().unwrap()[0], (Some(v.to_string()), Some(i.to_string())));
}

#[test]
fn save_login_and_update_password_write_real_items() {
    let e = env();
    let (mut c, _) = Client::connect(&e);
    assert_eq!(c.err("save_login", json!({"origin": "http://evil.example", "username": "u", "password": "p"})), "origin_mismatch");
    let id = c.ok("save_login", json!({"origin": "https://accounts.example.com", "username": "alice", "password": "s3cret-1"}))["item_id"].as_str().unwrap().to_string();
    let (vid, iid) = id.split_once(':').unwrap();
    assert_eq!(vid, e.vault);
    // a real item in the vault, visible to the GUI API
    let d = e.app.get_item(vid, iid).unwrap();
    assert_eq!((d.summary.category, d.summary.title.as_str(), d.summary.subtitle.as_str(), d.summary.url.as_deref()),
        (Category::Login, "accounts.example.com", "alice", Some("https://accounts.example.com")));
    assert_eq!(e.app.reveal_field(vid, iid, "password").unwrap().as_str(), "s3cret-1");
    assert_eq!(c.ok("fill", json!({"item_id": id, "origin": "https://www.example.com"}))["password"], json!("s3cret-1"));
    assert_eq!(c.ok("match", json!({"origin": "https://example.com"}))["items"].as_array().unwrap().len(), 1);
    assert_eq!(c.ok("update_password", json!({"item_id": id, "password": "n3w-pass"}))["ok"], json!(true));
    assert_eq!(e.app.reveal_field(vid, iid, "password").unwrap().as_str(), "n3w-pass");
    assert!(!e.app.get_item(vid, iid).unwrap().history.is_empty(), "old version kept in history");
    let t = c.ok("save_login", json!({"origin": "https://x.example.net", "title": "My X", "username": "", "password": "p", "vault_id": e.vault}));
    assert!(t["item_id"].as_str().unwrap().starts_with(&e.vault));
    assert_eq!(c.err("save_login", json!({"origin": "https://x.example.net", "username": "u", "password": "p", "vault_id": "nope"})), "not_found");
}

#[test]
fn update_login_via_agent_edits_and_records_history() {
    let e = env();
    let id = add_login(&e, "GitHub", "https://github.com", "leeky", "FAKE-old-pw");
    let (mut c, _) = Client::connect(&e);
    assert_eq!(c.err("update_login", json!({"item_id": id, "source": "page", "title": "X"})), "denied");
    assert_eq!(c.ok("update_login", json!({"item_id": id, "source": "popup", "title": "GitHub (work)",
        "username": "octo", "password": "FAKE-new-pw", "urls": ["https://github.com/login", "https://github.com/enterprise"],
        "notes": "edited" }))["ok"], json!(true));
    let (vid, iid) = id.split_once(':').unwrap();
    let d = e.app.get_item(vid, iid).unwrap();
    assert_eq!((d.summary.title.as_str(), d.summary.subtitle.as_str(), d.summary.url.as_deref()),
        ("GitHub (work)", "octo", Some("https://github.com/login")));
    assert_eq!(d.notes.as_deref(), Some("edited"));
    assert_eq!(e.app.reveal_field(vid, iid, "password").unwrap().as_str(), "FAKE-new-pw");
    assert!(!d.history.is_empty(), "old version kept in history");
}

#[test]
fn lock_unlock_events_and_request_unlock() {
    let e = env();
    let (mut c, _) = Client::connect(&e);
    let (mut c2, _) = Client::connect(&e);
    // lock from the GUI side -> pushed to every connection
    e.app.lock();
    c.wait_event("lock");
    c2.wait_event("lock");
    // request_unlock: UI is asked, result arrives when unlocked
    let r = c.ok("request_unlock", json!({"reason": "fill github.com"}));
    assert_eq!(r["unlocked"], json!(true));
    c.wait_event("unlock");
    c2.wait_event("unlock");
    assert_eq!(c.ok("status", json!({}))["unlocked"], json!(true));
    // lock via the agent
    assert_eq!(c.ok("lock", json!({}))["unlocked"], json!(false));
    assert!(!e.app.state.is_unlocked());
    c2.wait_event("lock");
    // idle auto-lock (app's own setting) also emits lock
    let r = c.ok("request_unlock", json!({}));
    assert_eq!(r["unlocked"], json!(true));
    c2.wait_event("unlock");
    e.app.state.set_auto_lock_duration(Duration::from_millis(50));
    std::thread::sleep(Duration::from_millis(100));
    c2.wait_event("lock");
}

#[test]
fn request_unlock_times_out() {
    let e = env();
    *e.ui.app.lock().unwrap() = None; // user ignores the prompt
    e.app.lock();
    e.agent.set_unlock_timeout(Duration::from_millis(600));
    let (mut c, _) = Client::connect(&e);
    assert_eq!(c.ok("request_unlock", json!({"reason": "x"}))["unlocked"], json!(false));
}

#[test]
fn create_item_any_category_via_agent() {
    let e = env();
    let (mut c, _) = Client::connect(&e);
    let r = c.ok("create_item", json!({"item": {"category": "api_credential", "title": "OpenAI", "url": "https://platform.openai.com",
        "sections": [{"id": "main", "title": null, "fields": [{"id": "credential", "label": "credential", "kind": "concealed", "value": "sk-FAKE"}]}]}}));
    assert_eq!(r["title"], "OpenAI");
    let (vid, iid) = split_id(r["item_id"].as_str().unwrap()).unwrap();
    assert_eq!(vid, e.vault);
    let d = e.app.get_item(&vid, &iid).unwrap();
    assert_eq!(d.summary.category, Category::ApiCredential);
    assert_eq!(e.app.reveal_field(&vid, &iid, "credential").unwrap().as_str(), "sk-FAKE");
    assert_eq!(c.err("create_item", json!({"item": {"category": "login", "title": ""}})), "bad_request");
    assert_eq!(c.err("create_item", json!({"item": {"category": "passkey", "title": "x"}})), "bad_request");
    assert_eq!(c.err("create_item", json!({})), "bad_request");
    e.app.lock();
    assert_eq!(c.err("create_item", json!({"item": {"category": "secure_note", "title": "x"}})), "locked");
}

#[test]
fn changed_event_pushed_for_gui_and_agent_writes() {
    let e = env();
    let (mut c, _) = Client::connect(&e);
    std::thread::sleep(Duration::from_millis(400)); // watcher has seen the initial stamp
    let t = std::time::Instant::now();
    add_login(&e, "From GUI", "https://a.example", "u", "FAKE-pw"); // a GUI-side write
    c.wait_event("changed");
    assert!(t.elapsed() < Duration::from_millis(1500), "changed after {:?}", t.elapsed());
    // lock/unlock still pushed; a lock is not reported as a content change
    e.app.lock();
    c.wait_event("lock");
    assert!(c.events.iter().all(|v| v["event"] != json!("changed")));
}

#[test]
fn item_added_via_host_agent_socket_emits_changed_exactly_once() {
    let e = env();
    let (mut c_popup, _) = Client::connect(&e);
    let (mut c_watcher, _) = Client::connect(&e);
    std::thread::sleep(Duration::from_millis(400)); // allow background watcher to observe initial stamp
    c_popup.events.clear();
    c_watcher.events.clear();

    // 1. Add item via create_item with source: "popup" (as the extension does)
    let t = std::time::Instant::now();
    let r = c_popup.ok(
        "create_item",
        json!({
            "source": "popup",
            "item": {
                "category": "login",
                "title": "GitHub New",
                "url": "https://github.com",
                "sections": [{
                    "id": "login",
                    "title": null,
                    "fields": [
                        {"id": "username", "label": "username", "kind": "text", "value": "mona"},
                        {"id": "password", "label": "password", "kind": "concealed", "value": "FAKE-pw-123"}
                    ]
                }]
            }
        }),
    );
    assert!(r["item_id"].as_str().is_some());
    assert_eq!(r["title"], "GitHub New");

    // Both the popup connection and the watching connection receive the "changed" event
    c_watcher.wait_event("changed");
    c_popup.wait_event("changed");
    assert!(t.elapsed() < Duration::from_millis(1500), "changed event took too long: {:?}", t.elapsed());

    // Wait long enough for the background watcher loop (250 ms tick) to run
    std::thread::sleep(Duration::from_millis(400));

    // Drain and verify that "changed" was emitted exactly once across both connections
    c_watcher.s.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    while let Ok(Some(v)) = read_frame(&mut c_watcher.s) {
        c_watcher.events.push(v);
    }
    assert_eq!(
        c_watcher.events.iter().filter(|v| v["event"] == json!("changed")).count(),
        0,
        "watcher received duplicate changed events: {:?}",
        c_watcher.events
    );

    // 2. Also test save_login with source: "popup" emits "changed" exactly once
    c_watcher.events.clear();
    c_popup.events.clear();
    let r2 = c_popup.ok(
        "save_login",
        json!({
            "source": "popup",
            "origin": "https://gitlab.example.invalid",
            "username": "alice",
            "password": "FAKE-pw-456"
        }),
    );
    assert!(r2["item_id"].as_str().is_some());
    c_watcher.wait_event("changed");
    std::thread::sleep(Duration::from_millis(400));
    while let Ok(Some(v)) = read_frame(&mut c_watcher.s) {
        c_watcher.events.push(v);
    }
    assert_eq!(c_watcher.events.iter().filter(|v| v["event"] == json!("changed")).count(), 0);
}

#[test]
fn item_findings_agent_contract_and_security() {
    let e = env();
    let (mut c, _) = Client::connect(&e);

    // 1. Unknown id gives not_found
    assert_eq!(c.err("item_findings", json!({"item_id": format!("{}:nope", e.vault)})), "not_found");

    // 2. Bad item_id format gives bad_request
    assert_eq!(c.err("item_findings", json!({"item_id": "bad_id"})), "bad_request");

    // 3. Add two items with the same weak password ("123456")
    let secret = "123456";
    let r1 = c.ok("save_login", json!({
        "source": "popup",
        "origin": "https://accounts.example.org",
        "username": "alice",
        "password": secret
    }));
    let id1 = r1["item_id"].as_str().unwrap().to_string();

    let r2 = c.ok("save_login", json!({
        "source": "popup",
        "origin": "https://service.example.org",
        "username": "bob",
        "password": secret
    }));
    let id2 = r2["item_id"].as_str().unwrap().to_string();

    let f1 = c.ok("item_findings", json!({"item_id": id1}));
    let kinds: Vec<&str> = f1["kinds"].as_array().unwrap().iter().map(|k| k.as_str().unwrap()).collect();
    assert!(kinds.contains(&"weak"), "must flag weak password: {f1:?}");
    assert!(kinds.contains(&"reused"), "must flag reused password: {f1:?}");
    assert_eq!(f1["reused_count"], 2);
    assert_eq!(f1["change_password_url"], "https://accounts.example.org/.well-known/change-password");

    // 4. No secret values or other item IDs leak
    let json_str = f1.to_string();
    assert!(!json_str.contains(secret), "secret password must never leak: {json_str}");
    assert!(!json_str.contains(&id2), "other item ids must never leak: {json_str}");

    // 5. Ignored finding is hidden
    let (vid, iid) = id1.split_once(':').unwrap();
    e.app.ignore_finding(vid, iid, "weak").unwrap();

    let f1_after = c.ok("item_findings", json!({"item_id": id1}));
    let kinds_after: Vec<&str> = f1_after["kinds"].as_array().unwrap().iter().map(|k| k.as_str().unwrap()).collect();
    assert!(!kinds_after.contains(&"weak"), "ignored finding must be hidden: {f1_after:?}");
    assert!(kinds_after.contains(&"reused"), "non-ignored finding must still be visible");

    // 6. When locked gives locked
    e.app.lock();
    assert_eq!(c.err("item_findings", json!({"item_id": id1})), "locked");
}
