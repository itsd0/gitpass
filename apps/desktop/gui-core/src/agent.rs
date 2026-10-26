//! v0.6 agent server: the desktop app serves the unlocked session to `gitpass-host` (and through it the
//! browser extension) over a UNIX socket. See `/workspace/ext-contract.md` (shared contract).
//!
//! * Socket `~/Library/Application Support/gitpass/agent.sock` (Linux: `$XDG_RUNTIME_DIR/gitpass/agent.sock`),
//!   dir 0700, socket 0600. Framing: 4-byte LE u32 length + UTF-8 JSON, max 4 MiB.
//! * Auth: peer uid must equal our uid (getpeereid / SO_PEERCRED), AND the first message must be
//!   `hello` carrying the 32-byte pairing token from `agent.token` (0600, hex). The code-signature
//!   check (SecCode) is not implemented, so the contract's pairing-token path is mandatory.
//! * Events `{"event":"lock"}` / `{"event":"unlock"}` are pushed to every authenticated connection.
use crate::api::{self, App};
use anyhow::{bail, Result};
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::Item;
use serde_json::{json, Value};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub const MAX_MSG: usize = 4 * 1024 * 1024;
pub const UNLOCK_TIMEOUT: Duration = Duration::from_secs(120);

/// UI side effects the Tauri shell provides (window focus, Touch ID prompt, navigation).
pub trait AgentUi: Send + Sync {
    /// Bring the main window forward and start Touch ID or show the password screen. Must not block
    /// until the user finishes; the server waits for the session to become unlocked.
    fn request_unlock(&self, reason: &str);
    /// Focus the app on an item (`None` = just bring the window forward).
    fn open_item(&self, vault_id: Option<&str>, item_id: Option<&str>);
    /// Ask the user to allow a sensitive action (SSH signature, passkey). Blocks until answered.
    /// Default: deny (headless).
    fn approve(&self, _title: &str, _message: &str) -> bool { false }
}
pub struct NoUi;
impl AgentUi for NoUi { fn request_unlock(&self, _: &str) {} fn open_item(&self, _: Option<&str>, _: Option<&str>) {} }

/// Default socket directory.
pub fn default_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    { dirs::home_dir().unwrap_or_default().join("Library/Application Support/gitpass") }
    #[cfg(not(target_os = "macos"))]
    { std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("gitpass") }
}

// ------------------------------------------------------------------ framing
pub fn read_frame<R: Read>(r: &mut R) -> Result<Option<Value>> {
    let mut len = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match r.read(&mut len[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => bail!("truncated length header"),
            Ok(n) => got += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    let n = u32::from_le_bytes(len) as usize;
    if n > MAX_MSG { bail!("message too large: {n} bytes"); }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(Some(serde_json::from_slice(&buf)?))
}
pub fn write_frame<W: Write>(w: &mut W, v: &Value) -> Result<()> {
    let b = serde_json::to_vec(v)?;
    if b.len() > MAX_MSG { bail!("message too large"); }
    let mut out = Vec::with_capacity(b.len() + 4);
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(&b);
    w.write_all(&out)?;
    w.flush()?;
    Ok(())
}

// ------------------------------------------------------------------ peer auth
pub fn peer_uid(s: &UnixStream) -> Result<u32> {
    use std::os::unix::io::AsRawFd;
    let fd = s.as_raw_fd();
    #[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd"))]
    {
        let (mut uid, mut gid): (libc::uid_t, libc::gid_t) = (0, 0);
        if unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } != 0 { bail!("getpeereid failed: {}", std::io::Error::last_os_error()); }
        Ok(uid)
    }
    #[cfg(target_os = "linux")]
    {
        let mut c: libc::ucred = unsafe { std::mem::zeroed() };
        let mut l = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        if unsafe { libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_PEERCRED, &mut c as *mut _ as *mut libc::c_void, &mut l) } != 0 {
            bail!("SO_PEERCRED failed: {}", std::io::Error::last_os_error());
        }
        Ok(c.uid)
    }
}
fn my_uid() -> u32 { unsafe { libc::getuid() } }
fn ct_eq(a: &[u8], b: &[u8]) -> bool { a.len() == b.len() && a.iter().zip(b).fold(0u8, |x, (p, q)| x | (p ^ q)) == 0 }

// ------------------------------------------------------------------ errors
#[derive(Debug)]
pub struct AgentError { pub code: &'static str, pub message: String }
impl std::fmt::Display for AgentError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}: {}", self.code, self.message) } }
impl std::error::Error for AgentError {}
fn err(code: &'static str, m: impl Into<String>) -> anyhow::Error { anyhow::Error::new(AgentError { code, message: m.into() }) }
fn classify(e: &anyhow::Error) -> (&'static str, String) {
    if let Some(a) = e.downcast_ref::<AgentError>() { return (a.code, a.message.clone()); }
    let m = e.to_string();
    let l = m.to_lowercase();
    let code = if l.contains("vault is locked") { "locked" } else if l.contains("not found") || l.contains("no such") || l.contains("unknown vault") { "not_found" }
        else if l.contains("revoked") { "denied" } else { "internal" };
    (code, m)
}

// ------------------------------------------------------------------ server
struct Conn { w: Mutex<UnixStream>, alive: AtomicBool }
impl Conn {
    fn send(&self, v: &Value) -> bool {
        if !self.alive.load(Ordering::SeqCst) { return false; }
        let ok = self.w.lock().map(|mut w| write_frame(&mut *w, v).is_ok()).unwrap_or(false);
        if !ok { self.alive.store(false, Ordering::SeqCst); }
        ok
    }
}

pub struct Agent {
    pub app: Arc<App>,
    ui: Arc<dyn AgentUi>,
    token: Zeroizing<String>,
    conns: Mutex<Vec<Arc<Conn>>>,
    /// last observed unlock state + condvar for request_unlock waiters
    state: Mutex<bool>,
    cv: Condvar,
    pub socket_path: PathBuf,
    unlock_timeout: Mutex<Duration>,
}

/// Create the dir (0700), write a fresh token (0600), bind the socket (0600).
pub fn bind(dir: &Path) -> Result<(UnixListener, PathBuf, Zeroizing<String>)> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let mut raw = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut raw);
    let token = Zeroizing::new(raw.iter().map(|b| format!("{b:02x}")).collect::<String>());
    let tp = dir.join("agent.token");
    let tmp = dir.join("agent.token.tmp");
    let _ = std::fs::remove_file(&tmp);
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        f.write_all(token.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &tp)?;
    let sp = dir.join("agent.sock");
    if sp.exists() {
        if UnixStream::connect(&sp).is_ok() { bail!("another gitpass agent is already listening on {}", sp.display()); }
        std::fs::remove_file(&sp)?;
    }
    // No umask change (process-global, would race other threads); the parent dir is already 0700.
    let l = UnixListener::bind(&sp)?;
    std::fs::set_permissions(&sp, std::fs::Permissions::from_mode(0o600))?;
    Ok((l, sp, token))
}

impl Agent {
    /// Bind in `dir` and start the accept loop + lock-state watcher threads.
    pub fn start(app: Arc<App>, ui: Arc<dyn AgentUi>, dir: &Path) -> Result<Arc<Agent>> {
        let (l, sp, token) = bind(dir)?;
        let a = Arc::new(Agent { state: Mutex::new(app.state.is_unlocked()), app, ui, token, conns: Mutex::new(vec![]), cv: Condvar::new(), socket_path: sp, unlock_timeout: Mutex::new(UNLOCK_TIMEOUT) });
        let a2 = a.clone();
        std::thread::spawn(move || for s in l.incoming() { match s { Ok(s) => { let a = a2.clone(); std::thread::spawn(move || a.serve(s)); } Err(_) => continue } });
        let w = Arc::downgrade(&a);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(250));
            match w.upgrade() { Some(a) => a.poll_state(), None => break }
        });
        Ok(a)
    }
    #[doc(hidden)]
    pub fn set_unlock_timeout(&self, d: Duration) { *self.unlock_timeout.lock().unwrap() = d; }
    pub fn token(&self) -> &str { &self.token }

    /// Detect lock/unlock (any path: GUI, Touch ID, idle auto-lock, agent `lock`) and push events.
    pub fn poll_state(&self) {
        self.app.state.check_idle();
        let now = self.app.state.is_unlocked();
        let mut s = self.state.lock().unwrap();
        if *s != now {
            *s = now;
            self.cv.notify_all();
            drop(s);
            self.broadcast(&json!({"event": if now { "unlock" } else { "lock" }}));
        }
    }
    fn broadcast(&self, v: &Value) {
        let mut c = self.conns.lock().unwrap();
        c.retain(|x| x.send(v));
    }

    fn serve(self: Arc<Self>, s: UnixStream) {
        let reject = |mut s: &UnixStream, id: Value, m: &str| { let _ = write_frame(&mut s, &json!({"id": id, "ok": false, "error": "denied", "message": m})); };
        match peer_uid(&s) { Ok(u) if u == my_uid() => {}, _ => { reject(&s, Value::Null, "peer uid mismatch"); return; } }
        let Ok(w) = s.try_clone() else { return };
        let conn = Arc::new(Conn { w: Mutex::new(w), alive: AtomicBool::new(true) });
        let mut r = s;
        // hello + token first
        let first = match read_frame(&mut r) { Ok(Some(v)) => v, _ => return };
        let id = first.get("id").cloned().unwrap_or(Value::Null);
        let tok = first.get("token").and_then(|t| t.as_str()).unwrap_or("");
        if first.get("cmd").and_then(|c| c.as_str()) != Some("hello") || !ct_eq(tok.as_bytes(), self.token.as_bytes()) {
            reject(&r, id, "hello with a valid pairing token is required");
            return;
        }
        conn.send(&self.respond(id, &first));
        self.conns.lock().unwrap().push(conn.clone());
        loop {
            let req = match read_frame(&mut r) { Ok(Some(v)) => v, Ok(None) => break, Err(e) => {
                conn.send(&json!({"id": Value::Null, "ok": false, "error": "bad_request", "message": e.to_string()})); break } };
            let (a, c) = (self.clone(), conn.clone());
            std::thread::spawn(move || { let id = req.get("id").cloned().unwrap_or(Value::Null); c.send(&a.respond(id, &req)); });
        }
        conn.alive.store(false, Ordering::SeqCst);
    }

    /// Handle one (already authenticated) request and build the response envelope.
    pub fn respond(&self, id: Value, req: &Value) -> Value {
        match self.dispatch(req) {
            Ok(result) => json!({"id": id, "ok": true, "result": result}),
            Err(e) => { let (code, m) = classify(&e); json!({"id": id, "ok": false, "error": code, "message": m}) }
        }
    }

    fn dispatch(&self, q: &Value) -> Result<Value> {
        let cmd = q.get("cmd").and_then(|c| c.as_str()).ok_or_else(|| err("bad_request", "missing cmd"))?;
        let s = |k: &str| q.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let req = |k: &str| s(k).filter(|v| !v.is_empty()).ok_or_else(|| err("bad_request", format!("missing '{k}'")));
        let source = s("source").unwrap_or_default();
        match cmd {
            "hello" => {
                let unlocked = self.app.state.is_unlocked();
                Ok(json!({"app_version": env!("CARGO_PKG_VERSION"), "unlocked": unlocked, "account": self.account()}))
            }
            "status" => {
                let st = self.app.status()?;
                let mut v = json!({"unlocked": st.unlocked, "app_running": true, "idle_timeout_secs": self.app.get_settings().auto_lock_minutes * 60});
                if let Some(a) = self.account() { v["account"] = a; }
                Ok(v)
            }
            "request_unlock" => {
                if self.app.state.is_unlocked() { return Ok(json!({"unlocked": true})); }
                self.ui.request_unlock(&s("reason").unwrap_or_else(|| "unlock gitpass for the browser".into()));
                let deadline = Instant::now() + *self.unlock_timeout.lock().unwrap();
                let mut g = self.state.lock().unwrap();
                loop {
                    if self.app.state.is_unlocked() { break; }
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() { break; }
                    g = self.cv.wait_timeout(g, left.min(Duration::from_millis(250))).unwrap().0;
                }
                drop(g);
                self.poll_state();
                Ok(json!({"unlocked": self.app.state.is_unlocked()}))
            }
            "lock" => { self.app.lock(); self.poll_state(); Ok(json!({"unlocked": false})) }
            "list_vaults" => {
                let v = self.app.list_vaults()?;
                Ok(json!({"vaults": v.iter().map(|x| json!({"id": x.id, "name": x.name, "item_count": x.item_count})).collect::<Vec<_>>()}))
            }
            "list_items" => {
                let cat = match s("category") { Some(c) if !c.is_empty() => Some(c.parse::<Category>().map_err(|e| err("bad_request", e.to_string()))?), _ => None };
                let query = s("query").unwrap_or_default().trim().to_lowercase();
                let vault = s("vault_id").filter(|v| !v.is_empty());
                let limit = q.get("limit").and_then(|l| l.as_u64()).map(|l| l as usize).unwrap_or(usize::MAX);
                let mut items: Vec<(Value, String)> = self.records()?.into_iter()
                    .filter(|(vid, _, _, r)| vault.as_ref().is_none_or(|v| v == vid) && cat.is_none_or(|c| c == r.category))
                    .map(|(vid, vname, iid, r)| (summary(&vid, &vname, &iid, &r), r.item.title.to_lowercase()))
                    .filter(|(v, _)| query.is_empty() || ["title", "subtitle", "url"].iter().any(|k| v[*k].as_str().unwrap_or("").to_lowercase().contains(&query)))
                    .collect();
                items.sort_by(|a, b| a.1.cmp(&b.1));
                Ok(json!({"items": items.into_iter().take(limit).map(|x| x.0).collect::<Vec<_>>()}))
            }
            "match" => {
                let origin = req("origin")?;
                if gitpass_host::origin::parse_request_origin(&origin).is_none() { return Err(err("origin_mismatch", "origin must be https:// (or http:// on loopback)")); }
                let items: Vec<Value> = self.records()?.into_iter()
                    .filter(|(_, _, _, r)| r.category == Category::Login && r.item.url.as_deref().is_some_and(|u| gitpass_host::origin::origin_matches(u, &origin)))
                    .map(|(vid, vname, iid, r)| summary(&vid, &vname, &iid, &r)).collect();
                Ok(json!({"items": items}))
            }
            "get_item" => { let (vid, iid) = split_id(&req("item_id")?)?; let (vname, r) = self.record(&vid, &iid)?; Ok(json!({"item": detail(&vid, &vname, &iid, &r)})) }
            "reveal" => {
                if source != "popup" { return Err(err("denied", "reveal is allowed only from the popup")); }
                let (vid, iid) = split_id(&req("item_id")?)?;
                let field = req("field")?;
                let v = self.app.reveal_field(&vid, &iid, &field).map_err(|e| if e.to_string().contains("has no field") { err("not_found", e.to_string()) } else { e })?;
                Ok(json!({"value": v.as_str()}))
            }
            "fill" => {
                let (vid, iid) = split_id(&req("item_id")?)?;
                let origin = req("origin")?;
                let (_, r) = self.record(&vid, &iid)?;
                if r.category != Category::Login { return Err(err("bad_request", "fill needs a login item")); }
                if !r.item.url.as_deref().is_some_and(|u| gitpass_host::origin::origin_matches(u, &origin)) {
                    return Err(err("origin_mismatch", "the item's website does not match this page"));
                }
                let mut v = json!({"username": r.item.username.clone().unwrap_or_default(), "password": r.item.password.clone().unwrap_or_default()});
                if let Some(t) = r.item.totp.as_deref().filter(|t| !t.is_empty()) { v["totp"] = json!(gitpass_core::totp::code_now(&gitpass_core::totp::parse(t)?).0); }
                Ok(v)
            }
            "fill_card" | "fill_identity" => {
                if source != "popup" && source != "inpage_click" { return Err(err("denied", "card/identity fill must be popup-initiated or follow an in-page click")); }
                let (vid, iid) = split_id(&req("item_id")?)?;
                let (_, r) = self.record(&vid, &iid)?;
                let want = if cmd == "fill_card" { Category::CreditCard } else { Category::Identity };
                if r.category != want { return Err(err("bad_request", format!("item is not a {}", want.as_str()))); }
                let mut m = serde_json::Map::new();
                for f in r.view_sections().into_iter().flat_map(|s| s.fields) { if let Some(v) = f.value { m.insert(f.id, json!(v)); } }
                Ok(Value::Object(m))
            }
            "totp" => { let (vid, iid) = split_id(&req("item_id")?)?; let t = self.app.totp_now(&vid, &iid)?; Ok(json!({"code": t.code, "remaining": t.remaining, "period": t.period})) }
            "save_login" => {
                let origin = req("origin")?;
                let u = gitpass_host::origin::parse_request_origin(&origin).ok_or_else(|| err("origin_mismatch", "origin must be https:// (or http:// on loopback)"))?;
                let password = Zeroizing::new(req("password")?);
                let username = s("username").unwrap_or_default();
                let host = u.host_str().unwrap_or_default().to_string();
                let title = s("title").filter(|t| !t.trim().is_empty()).unwrap_or_else(|| host.trim_start_matches("www.").to_string());
                let vid = match s("vault_id").filter(|v| !v.is_empty()) { Some(v) => v, None => self.app.list_vaults()?.into_iter().next().map(|v| v.id).ok_or_else(|| err("not_found", "no vault"))? };
                let url = u.origin().ascii_serialization();
                let iid = self.app.state.with(|x| {
                    crate::GuiState::writable(x).map_err(|e| err("denied", e.to_string()))?;
                    let mut it = Item::default();
                    it.title = title; it.username = Some(username).filter(|u| !u.is_empty()); it.password = Some(password.to_string()); it.url = Some(url);
                    let rec = ItemRecord::new(it);
                    x.session.add_record(&vid, rec)
                })?;
                Ok(json!({"item_id": format!("{vid}:{iid}")}))
            }
            "update_password" => {
                let (vid, iid) = split_id(&req("item_id")?)?;
                let pw = Zeroizing::new(req("password")?);
                self.app.state.with(|x| {
                    crate::GuiState::writable(x).map_err(|e| err("denied", e.to_string()))?;
                    let (_, mut r) = x.session.read_record(&vid, &iid)?;
                    if r.category != Category::Login { return Err(err("bad_request", "update_password needs a login item")); }
                    r.item.password = Some(pw.to_string());
                    x.session.update_record(&vid, &iid, r)
                })?;
                Ok(json!({"ok": true}))
            }
            "set_favorite" => {
                let (vid, iid) = split_id(&req("item_id")?)?;
                let fav = q.get("favorite").and_then(|v| v.as_bool()).ok_or_else(|| err("bad_request", "favorite must be a bool"))?;
                self.app.state.with(|x| {
                    crate::GuiState::writable(x).map_err(|e| err("denied", e.to_string()))?;
                    let (_, mut r) = x.session.read_record(&vid, &iid)?;
                    r.favorite = fav;
                    x.session.update_record(&vid, &iid, r)
                })?;
                Ok(json!({"favorite": fav}))
            }
            "generate" => {
                let len = q.get("len").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
                let b = |k: &str| q.get(k).and_then(|v| v.as_bool()).unwrap_or(true);
                Ok(json!({"password": api::generate(len, b("symbols"), b("digits"), false)}))
            }
            "passkey_create" | "passkey_get" => {
                let origin = req("origin")?;
                let approved = gitpass_host::passkey::popup_approved(q) || {
                    let what = if cmd == "passkey_create" { "Save a passkey for" } else { "Sign in with a passkey on" };
                    self.ui.approve("Passkey", &format!("{what} {origin}?"))
                };
                if !approved { return Err(err("denied", "passkey use was not approved")); }
                self.app.state.with(|x| {
                    crate::GuiState::writable(x).map_err(|e| err("denied", e.to_string()))?;
                    if cmd == "passkey_create" { gitpass_host::passkey::create(&x.session, q) } else { gitpass_host::passkey::get(&x.session, q) }
                }).map_err(host_err)
            }
            "open_in_app" => {
                match s("item_id").filter(|v| !v.is_empty()) {
                    Some(id) => { let (v, i) = split_id(&id)?; self.ui.open_item(Some(&v), Some(&i)); }
                    None => self.ui.open_item(None, None),
                }
                Ok(json!({"ok": true}))
            }
            other => Err(err("bad_request", format!("unknown command '{other}'"))),
        }
    }

    /// Friendly name (git config gitpass.displayName / remote owner / OS full name / "gitpass"), never the
    /// account UUID. Available while locked too (from the vault path); `vault_count` only when unlocked.
    fn account(&self) -> Option<Value> {
        let unlocked = self.app.state.with(|u| {
            let n = u.session.list_vaults()?.len();
            Ok(json!({"name": gitpass_core::remote::display_name(&u.session.root), "vault_count": n}))
        }).ok();
        unlocked.or_else(|| {
            let st = self.app.status().ok()?;
            (st.vault_exists && !st.vault_path.is_empty())
                .then(|| json!({"name": gitpass_core::remote::display_name(std::path::Path::new(&st.vault_path))}))
        })
    }
    /// Live (not deleted, not archived) records of every vault: (vault id, vault name, item id, record).
    fn records(&self) -> Result<Vec<(String, String, String, ItemRecord)>> {
        self.app.state.with(|u| {
            let mut out = vec![];
            for (vid, m) in u.session.list_vaults()? {
                for (iid, _, r) in u.session.list_records(&vid)? { if r.deleted_at.is_none() && !r.archived { out.push((vid.clone(), m.name.clone(), iid, r)); } }
            }
            Ok(out)
        })
    }
    fn record(&self, vid: &str, iid: &str) -> Result<(String, ItemRecord)> {
        self.app.state.with(|u| {
            let name = u.session.list_vaults()?.into_iter().find(|(v, _)| v == vid).map(|(_, m)| m.name.clone()).ok_or_else(|| err("not_found", "no such vault"))?;
            let (_, r) = u.session.read_record(vid, iid).map_err(|e| err("not_found", e.to_string()))?;
            if r.deleted_at.is_some() { return Err(err("not_found", "item is deleted")); }
            Ok((name, r))
        })
    }
}

pub fn split_id(id: &str) -> Result<(String, String)> {
    let (v, i) = id.split_once(':').ok_or_else(|| err("bad_request", "item_id must be '<vault_id>:<item_id>'"))?;
    if v.is_empty() || i.is_empty() { return Err(err("bad_request", "item_id must be '<vault_id>:<item_id>'")); }
    Ok((v.into(), i.into()))
}

fn has_totp(r: &ItemRecord) -> bool { r.view_sections().iter().flat_map(|s| s.fields.iter()).any(|f| f.kind == FieldKind::Totp && f.value.as_deref().is_some_and(|v| !v.is_empty())) }

fn summary(vid: &str, vname: &str, iid: &str, r: &ItemRecord) -> Value {
    let subtitle = api::subtitle(r);
    let favicon_host = r.item.url.as_deref().and_then(|u| url_host(u));
    let mut v = json!({"id": format!("{vid}:{iid}"), "vault_id": vid, "vault_name": vname, "category": r.category.as_str(), "title": r.item.title,
        "subtitle": subtitle, "has_totp": has_totp(r), "favorite": r.favorite, "updated_at": ts(&r.item.updated_at)});
    if let Some(u) = &r.item.url { v["url"] = json!(u); }
    if let Some(h) = favicon_host { v["favicon_host"] = json!(h); }
    v
}
fn url_host(u: &str) -> Option<String> {
    let u = u.trim();
    let p = if u.contains("://") { url::Url::parse(u).ok()? } else { url::Url::parse(&format!("https://{u}")).ok()? };
    p.host_str().map(str::to_string)
}
fn ts(s: &str) -> String { s.trim().parse::<i64>().map(gitpass_core::device::iso8601).unwrap_or_else(|_| s.to_string()) }

fn kind_name(cat: Category, f: &gitpass_core::item::Field) -> &'static str {
    match f.kind {
        FieldKind::Concealed | FieldKind::Text if cat == Category::CreditCard && api::is_card_number(f) => "card_number",
        FieldKind::Concealed => "password", FieldKind::Totp => "totp", FieldKind::Url => "url", FieldKind::Email => "email",
        FieldKind::Phone => "phone", FieldKind::Date | FieldKind::MonthYear => "date", FieldKind::Text | FieldKind::Multiline => "text",
    }
}
fn detail(vid: &str, vname: &str, iid: &str, r: &ItemRecord) -> Value {
    let mut v = summary(vid, vname, iid, r);
    let mut fields: Vec<Value> = vec![];
    if r.category == Category::Login { if let Some(u) = &r.item.url { fields.push(json!({"name": "url", "label": "website", "value": u, "concealed": false, "kind": "url"})); } }
    for f in r.view_sections().into_iter().flat_map(|s| s.fields) {
        let concealed = f.kind.is_secret();
        let mut x = json!({"name": f.id, "label": f.label, "concealed": concealed, "kind": kind_name(r.category, &f)});
        if !concealed { if let Some(val) = &f.value { x["value"] = json!(val); } }
        else if x["kind"] == json!("card_number") { if let Some(d) = f.value.as_deref().and_then(api::card_digits_of) { x["masked"] = json!(api::card_mask(&d)); } }
        fields.push(x);
    }
    v["fields"] = json!(fields);
    if let Some(n) = &r.item.notes { v["notes"] = json!(n); }
    v["tags"] = json!(r.tags);
    v["created_at"] = json!(r.created_at.map(|c| gitpass_core::device::iso8601(c as i64)).unwrap_or_else(|| ts(&r.item.updated_at)));
    v
}

/// Keep contract codes from gitpass-host's `CodedError` (e.g. `origin_mismatch`, `exists`).
fn host_err(e: anyhow::Error) -> anyhow::Error {
    if let Some(c) = e.downcast_ref::<gitpass_host::handler::CodedError>() { return err(c.code, c.message.clone()); }
    e
}
