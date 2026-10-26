//! Bridge tests against a mock desktop-app agent socket.
mod common;
use common::*;
use gitpass_host::agent::AgentPaths;
use gitpass_host::bridge::{caller_allowed, Bridge};
use gitpass_host::framing::{encode_message, read_message, write_message};
use serde_json::{json, Value};
use std::io::{Cursor, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);
impl Write for Buf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> { self.0.lock().unwrap().extend_from_slice(b); Ok(b.len()) }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}
impl Buf {
    fn frames(&self) -> Vec<Value> {
        let mut c = Cursor::new(self.0.lock().unwrap().clone());
        let mut v = vec![];
        while let Some(m) = read_message(&mut c).unwrap() { v.push(m); }
        v
    }
    fn wait_for(&self, pred: impl Fn(&Value) -> bool) -> Value {
        let t = Instant::now();
        loop {
            if let Some(v) = self.frames().into_iter().find(|f| pred(f)) { return v; }
            assert!(t.elapsed() < Duration::from_secs(10), "timed out; frames: {:?}", self.frames());
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Mock { _d: tempfile::TempDir, paths: AgentPaths, seen: Arc<Mutex<Vec<Value>>>, conns: Arc<Mutex<Vec<UnixStream>>> }

/// A fake desktop app: checks the hello token, answers commands, pushes events.
fn mock_app() -> Mock {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("gitpass");
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let paths = AgentPaths { socket: dir.join("agent.sock"), token: dir.join("agent.token") };
    std::fs::write(&paths.token, TOKEN).unwrap();
    let l = UnixListener::bind(&paths.socket).unwrap();
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let conns: Arc<Mutex<Vec<UnixStream>>> = Arc::default();
    let (seen2, conns2) = (seen.clone(), conns.clone());
    std::thread::spawn(move || for s in l.incoming() {
        let s = s.unwrap();
        conns2.lock().unwrap().push(s.try_clone().unwrap());
        let seen = seen2.clone();
        std::thread::spawn(move || {
            let mut r = s.try_clone().unwrap();
            let w = Arc::new(Mutex::new(s));
            let mut authed = false;
            while let Ok(Some(req)) = read_message(&mut r) {
                seen.lock().unwrap().push(req.clone());
                let id = req["id"].clone();
                let cmd = req["cmd"].as_str().unwrap_or("").to_string();
                let send = |v: Value| { let mut g = w.lock().unwrap(); write_message(&mut *g, &v).unwrap(); };
                if cmd == "hello" {
                    if req["token"] != TOKEN { send(json!({"id":id,"ok":false,"error":"denied","message":"bad token"})); continue; }
                    authed = true;
                    send(json!({"id":id,"ok":true,"result":{"app_version":"0.6.0","unlocked":false,"account":{"name":"Leeky","vault_count":1}}}));
                    continue;
                }
                if !authed { send(json!({"id":id,"ok":false,"error":"denied","message":"hello first"})); continue; }
                match cmd.as_str() {
                    "status" => send(json!({"id":id,"ok":true,"result":{"unlocked":true,"app_running":true,"idle_timeout_secs":600}})),
                    "match" => send(json!({"id":id,"ok":true,"result":{"items":[{"id":"v:i","category":"login","title":"From app","origin":req["origin"]}]}})),
                    "request_unlock" => {
                        let w2 = w.clone();
                        // reply later, from another thread, after pushing an unlock event
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(300));
                            let mut g = w2.lock().unwrap();
                            write_message(&mut *g, &json!({"event":"unlock"})).unwrap();
                            write_message(&mut *g, &json!({"id":id,"ok":true,"result":{"unlocked":true}})).unwrap();
                        });
                    }
                    "lock" => { send(json!({"event":"lock"})); send(json!({"id":id,"ok":true,"result":{"unlocked":false}})); }
                    _ => send(json!({"id":id,"ok":false,"error":"not_found","message":format!("mock: {cmd}")})),
                }
            }
        });
    });
    Mock { _d: d, paths, seen, conns }
}

fn frames(reqs: &[Value]) -> Cursor<Vec<u8>> {
    let mut b = vec![];
    for r in reqs { b.extend(encode_message(r).unwrap()); }
    Cursor::new(b)
}

#[test]
fn proxies_with_hello_token_and_preserves_ids() {
    let m = mock_app();
    let f = v5_vault();
    let out = Buf::default();
    let b = Bridge::new(Some(m.paths.clone()), f.host, Box::new(out.clone()));
    b.run(frames(&[
        json!({"id":"a","cmd":"status"}),
        json!({"id":7,"cmd":"match","origin":"https://github.com"}),
        json!({"id":8,"cmd":"get_item","item_id":"v:x"}),
    ])).unwrap();
    let st = out.wait_for(|v| v["id"] == "a");
    assert_eq!(st["result"]["app_running"], true, "{st}");
    let mt = out.wait_for(|v| v["id"] == 7);
    assert_eq!(mt["result"]["items"][0]["title"], "From app");
    let gi = out.wait_for(|v| v["id"] == 8);
    assert_eq!(gi["error"], "not_found");
    let seen = m.seen.lock().unwrap().clone();
    assert_eq!(seen[0]["cmd"], "hello");
    assert_eq!(seen[0]["client"], "gitpass-host");
    assert_eq!(seen[0]["token"], TOKEN);
    assert!(seen[0]["version"].is_string());
    let fwd = seen.iter().find(|s| s["cmd"] == "match").unwrap();
    assert_eq!(fwd["origin"], "https://github.com");
    assert!(fwd["id"].is_u64(), "host uses its own numeric ids on the socket");
}

#[test]
fn forwards_lock_and_unlock_events() {
    let m = mock_app();
    let f = v5_vault();
    let out = Buf::default();
    let b = Bridge::new(Some(m.paths.clone()), f.host, Box::new(out.clone()));
    b.run(frames(&[json!({"id":1,"cmd":"request_unlock","reason":"popup"}), json!({"id":2,"cmd":"lock"})])).unwrap();
    assert_eq!(out.wait_for(|v| v["id"] == 1)["result"]["unlocked"], true);
    out.wait_for(|v| v["event"] == "unlock");
    out.wait_for(|v| v["event"] == "lock");
    assert_eq!(out.wait_for(|v| v["id"] == 2)["result"]["unlocked"], false);
    // app quits -> extension is told it is locked
    for c in m.conns.lock().unwrap().iter() { c.shutdown(std::net::Shutdown::Both).unwrap(); }
    out.wait_for(|v| v["event"] == "lock" && v["reason"] == "app_disconnected");
}

#[test]
fn falls_back_when_app_not_running() {
    let f = v5_vault();
    let login = f.login.clone();
    let out = Buf::default();
    let missing = AgentPaths { socket: PathBuf::from("/nonexistent/gitpass/agent.sock"), token: PathBuf::from("/nonexistent/t") };
    let b = Bridge::new(Some(missing), f.host, Box::new(out.clone()));
    b.run(frames(&[
        json!({"id":1,"cmd":"status"}),
        json!({"id":2,"cmd":"list_items"}),
        json!({"id":3,"cmd":"unlock","password":PW}),
        json!({"id":4,"cmd":"match","origin":"https://github.com"}),
        json!({"id":5,"cmd":"fill","item_id":login,"origin":"https://github.com"}),
    ])).unwrap();
    let fr = out.frames();
    assert_eq!(fr[0]["result"]["app_running"], false);
    assert_eq!(fr[1]["error"], "locked");
    assert_eq!(fr[2]["result"]["unlocked"], true);
    assert_eq!(fr[3]["result"]["items"][0]["title"], "GitHub");
    assert_eq!(fr[4]["result"]["password"], "FAKE-gh-pw");
}

#[test]
fn bad_token_falls_back() {
    let m = mock_app();
    std::fs::write(&m.paths.token, "wrong").unwrap();
    let f = v5_vault();
    let out = Buf::default();
    let b = Bridge::new(Some(m.paths.clone()), f.host, Box::new(out.clone()));
    b.run(frames(&[json!({"id":1,"cmd":"status"})])).unwrap();
    assert_eq!(out.frames()[0]["result"]["app_running"], false);
}

#[test]
fn origin_rules_enforced_before_proxying() {
    let m = mock_app();
    let f = v5_vault();
    let out = Buf::default();
    let b = Bridge::new(Some(m.paths.clone()), f.host, Box::new(out.clone()));
    b.run(frames(&[
        json!({"id":1,"cmd":"match","origin":"http://github.com"}),
        json!({"id":2,"cmd":"fill","item_id":"v:i","origin":"javascript:alert(1)"}),
        json!({"id":3,"cmd":"save_login","origin":"file:///etc","username":"u","password":"p"}),
        json!({"id":4,"cmd":"match","origin":"http://localhost:3000"}),
    ])).unwrap();
    for id in 1..=3 { assert_eq!(out.wait_for(|v| v["id"] == id)["error"], "origin_mismatch"); }
    assert_eq!(out.wait_for(|v| v["id"] == 4)["ok"], true);
    let seen = m.seen.lock().unwrap();
    assert_eq!(seen.iter().filter(|s| s["cmd"] == "match" || s["cmd"] == "fill" || s["cmd"] == "save_login").count(), 1, "only the loopback match reached the app");
}

#[test]
fn caller_validation() {
    let allowed = vec!["chrome-extension://abcdefghijklmnopabcdefghijklmnop/".to_string(), "gitpass@gitpass.dev".to_string()];
    assert!(caller_allowed(&["chrome-extension://abcdefghijklmnopabcdefghijklmnop/".into()], &allowed).is_ok());
    assert!(caller_allowed(&["/path/manifest.json".into(), "gitpass@gitpass.dev".into()], &allowed).is_ok());
    assert!(caller_allowed(&["chrome-extension://evilevilevilevilevilevilevilevil/".into()], &allowed).is_err());
    assert!(caller_allowed(&["/m.json".into(), "evil@x".into()], &allowed).is_err());
    assert!(caller_allowed(&["chrome-extension://abcdefghijklmnopabcdefghijklmnop/".into()], &[]).is_err(), "empty allow-list refuses everyone");
    assert!(caller_allowed(&[], &allowed).is_err());
}
