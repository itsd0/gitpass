//! v0.6 agent server: the desktop app serves the unlocked session to `gitpass-host` (and through it the
//! browser extension) over a UNIX socket. See `/workspace/ext-contract.md` (shared contract).
//!
//! * Socket `~/Library/Application Support/gitpass/agent.sock` (Linux: `$XDG_RUNTIME_DIR/gitpass/agent.sock`),
//!   dir 0700, socket 0600. Framing: 4-byte LE u32 length + UTF-8 JSON, max 4 MiB.
//! * Auth: peer uid must equal our uid (getpeereid / SO_PEERCRED), AND the first message must be
//!   `hello` carrying the 32-byte pairing token from `agent.token` (0600, hex). The code-signature
//!   check (SecCode) is not implemented, so the contract's pairing-token path is mandatory.
//! * Events `{"event":"lock"}` / `{"event":"unlock"}` are pushed to every authenticated connection.
use crate::api::App;
use anyhow::{bail, Result};
use gitpass_core::item::Category;
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::Item;
use serde_json::{json, Value};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::BTreeSet;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

fn clean_host(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() { return None; }
    let u = if s.contains("://") { url::Url::parse(s).ok()? } else { url::Url::parse(&format!("https://{s}")).ok()? };
    u.host_str().map(|h| h.trim_start_matches("www.").to_string()).filter(|h| !h.is_empty())
}


pub const MAX_MSG: usize = 4 * 1024 * 1024;
pub const UNLOCK_TIMEOUT: Duration = Duration::from_secs(120);

/// UI side effects the Tauri shell provides (window focus, Touch ID prompt, navigation).
pub trait AgentUi: Send + Sync {
    /// Bring the main window forward and start Touch ID or show the password screen. Must not block
    /// until the user finishes; the server waits for the session to become unlocked.
    fn request_unlock(&self, reason: &str);
    /// Focus the app on an item (`None` = just bring the window forward).
    fn open_item(&self, vault_id: Option<&str>, item_id: Option<&str>);
    /// Focus the app on a specific view ("generator_history" | "watchtower" | "settings") with an optional section.
    fn open_view(&self, _view: &str, _section: Option<&str>) {}
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
    if let Some(c) = e.downcast_ref::<gitpass_host::handler::CodedError>() { return (c.code, c.message.clone()); }
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
    /// last observed vault change stamp (see `GuiState::change_stamp`)
    stamp: Mutex<Option<u64>>,
    cv: Condvar,
    pub socket_path: PathBuf,
    unlock_timeout: Mutex<Duration>,
    audit_cache: Mutex<Option<(Option<u64>, Arc<gitpass_core::audit::AuditReport>)>>,
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
        let a = Arc::new(Agent { state: Mutex::new(app.state.is_unlocked()), stamp: Mutex::new(app.state.change_stamp()), app, ui, token, conns: Mutex::new(vec![]), cv: Condvar::new(), socket_path: sp, unlock_timeout: Mutex::new(UNLOCK_TIMEOUT), audit_cache: Mutex::new(None) });
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
        } else { drop(s); }
        // Vault content changed (any writer: GUI, this agent, CLI, git sync): tell connected extensions.
        let st = self.app.state.change_stamp();
        let mut last = self.stamp.lock().unwrap();
        if *last != st {
            let changed = last.is_some() && st.is_some();
            *last = st;
            drop(last);
            if changed { self.broadcast(&json!({"event": "changed"})); }
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
                let usage = self.app.state.with(|x| Ok(x.session.all_last_used()))?;
                let recs = if s("state").as_deref() == Some("archived") || s("filter").as_deref() == Some("archive") || q.get("archived").and_then(|v| v.as_bool()) == Some(true) {
                    self.archived_records()?
                } else if s("state").as_deref() == Some("deleted") || s("filter").as_deref() == Some("deleted") || q.get("deleted").and_then(|v| v.as_bool()) == Some(true) {
                    self.deleted_records()?
                } else {
                    self.records()?
                };
                let iter = recs.iter().map(|(v, vn, i, r)| (v.as_str(), vn.as_str(), i.as_str(), r));
                gitpass_host::protocol::list_items(iter, q, &usage).map_err(host_err)
            }
            "list_archived" => {
                let vault = s("vault_id").filter(|v| !v.is_empty());
                let usage = self.app.state.with(|x| Ok(x.session.all_last_used()))?;
                let recs = self.archived_records()?;
                let iter = recs.iter().map(|(v, vn, i, r)| (v.as_str(), vn.as_str(), i.as_str(), r));
                Ok(gitpass_host::protocol::list_archived(iter, vault.as_deref(), &usage))
            }
            "list_deleted" => {
                let vault = s("vault_id").filter(|v| !v.is_empty());
                let recs = self.deleted_records()?;
                let iter = recs.iter().map(|(v, vn, i, r)| (v.as_str(), vn.as_str(), i.as_str(), r));
                Ok(gitpass_host::protocol::list_deleted(iter, vault.as_deref()))
            }
            "match" => {
                let records = self.records()?;
                let iter = records.iter().map(|(v, vn, i, r)| (v.as_str(), vn.as_str(), i.as_str(), r));
                let usage = self.app.state.with(|x| Ok(x.session.all_last_used()))?;
                gitpass_host::protocol::match_items(iter, q, &usage)
            }
            "get_item" => {
                let (vid, iid) = split_id(&req("item_id")?)?;
                let include_deleted = q.get("include_deleted").and_then(|v| v.as_bool()).unwrap_or(false);
                let (vname, r) = self.app.state.with(|u| {
                    let name = u.session.list_vaults()?.into_iter().find(|(v, _)| v == &vid).map(|(_, m)| m.name.clone()).ok_or_else(|| err("not_found", "no such vault"))?;
                    let (_, r) = u.session.read_record(&vid, &iid).map_err(|e| err("not_found", e.to_string()))?;
                    if r.deleted_at.is_some() && !include_deleted { return Err(err("not_found", "item is deleted")); }
                    Ok((name, r))
                })?;
                let lu = self.app.state.with(|x| Ok(x.session.last_used(&iid))).ok().flatten();
                Ok(gitpass_host::protocol::get_item(&vid, &vname, &iid, &r, lu))
            }
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
                let matches_url = |u: &str| gitpass_host::origin::origin_matches(u, &origin);
                let confirmed = q.get("user_confirm").and_then(|v| v.as_bool()).unwrap_or(false);
                let ok = r.item.url.as_deref().is_some_and(matches_url)
                    || r.sections.iter().flat_map(|s| &s.fields).any(|f| f.kind == gitpass_core::item::FieldKind::Url && f.value.as_deref().is_some_and(matches_url))
                    || confirmed;
                if !ok {
                    return Err(err("origin_mismatch", "the item's website does not match this page"));
                }
                let _ = self.app.state.with(|x| Ok(x.session.touch_item_used(&iid)));
                let mut v = json!({"username": r.item.username.clone().unwrap_or_default(), "password": r.item.password.clone().unwrap_or_default()});
                if let Some(t) = r.item.totp.as_deref().filter(|t| !t.is_empty()) { v["totp"] = json!(gitpass_core::totp::code_now(&gitpass_core::totp::parse(t)?).0); }
                Ok(v)
            }
            "fill_card" | "fill_identity" => {
                if source != "popup" && source != "inpage_click" { return Err(err("denied", "card/identity fill must be popup-initiated or follow an in-page click")); }
                let (vid, iid) = split_id(&req("item_id")?)?;
                let (_, r) = self.record(&vid, &iid)?;
                let _ = self.app.state.with(|x| Ok(x.session.touch_item_used(&iid)));
                gitpass_host::protocol::fill_card_or_identity(&r, cmd == "fill_card")
            }
            "totp" => { let (vid, iid) = split_id(&req("item_id")?)?; let t = self.app.totp_now(&vid, &iid)?; Ok(json!({"code": t.code, "remaining": t.remaining, "period": t.period})) }
            "save_login" => {
                let origin = req("origin")?;
                let u = gitpass_host::origin::parse_request_origin(&origin).ok_or_else(|| err("origin_mismatch", "origin must be https:// (or http:// on loopback)"))?;
                let password = Zeroizing::new(req("password")?);
                let username = s("username").unwrap_or_default();
                let host = u.host_str().unwrap_or_default().to_string();
                let title = s("title").filter(|t| !t.trim().is_empty()).unwrap_or_else(|| host.trim_start_matches("www.").to_string());
                let vid = match s("vault_id").filter(|v| !v.is_empty()) {
                    Some(v) => {
                        if !self.app.list_vaults()?.iter().any(|x| x.id == v) { return Err(err("not_found", "vault not found")); }
                        v
                    }
                    None => self.app.list_vaults()?.into_iter().next().map(|v| v.id).ok_or_else(|| err("not_found", "no vault"))?,
                };
                let url = u.origin().ascii_serialization();
                let iid = self.app.state.with(|x| {
                    crate::GuiState::writable(x).map_err(|e| err("denied", e.to_string()))?;
                    let mut it = Item::default();
                    it.title = title; it.username = Some(username).filter(|u| !u.is_empty()); it.password = Some(password.to_string()); it.url = Some(url);
                    let rec = ItemRecord::new(it);
                    x.session.add_record(&vid, rec)
                })?;
                self.poll_state();
                Ok(json!({"item_id": format!("{vid}:{iid}")}))
            }
            "create_item" => {
                // Any category, same model as the GUI's New Item (gui-core api::ItemInput); vault defaults to the first.
                let mut v = q.get("item").cloned().ok_or_else(|| err("bad_request", "missing 'item'"))?;
                if v.get("vaultId").and_then(|x| x.as_str()).filter(|x| !x.is_empty()).is_none() {
                    let vid = self.app.list_vaults()?.into_iter().next().map(|v| v.id).ok_or_else(|| err("not_found", "no vault"))?;
                    v["vaultId"] = json!(vid);
                }
                let input: crate::api::ItemInput = serde_json::from_value(v).map_err(|e| err("bad_request", e.to_string()))?;
                if input.category == Category::Passkey { return Err(err("bad_request", "passkeys are created by websites")); }
                if input.category == Category::Other { return Err(err("bad_request", "invalid category")); }
                let r = self.app.create_item(&input).map_err(|e| { let m = format!("{e:#}"); if m.contains("revoked") { err("denied", m) } else if m.contains("locked") { e } else { err("bad_request", m) } })?;
                self.poll_state();
                Ok(json!({"item_id": format!("{}:{}", input.vault_id, r.id), "title": r.title}))
            }
            "update_login" => {
                if source != "popup" { return Err(err("denied", "update_login is allowed only from the popup")); }
                let (vid, iid) = split_id(&req("item_id")?)?;
                self.app.state.with(|x| {
                    crate::GuiState::writable(x).map_err(|e| err("denied", e.to_string()))?;
                    let (_, mut r) = x.session.read_record(&vid, &iid).map_err(|e| err("not_found", e.to_string()))?;
                    if r.deleted_at.is_some() { return Err(err("not_found", "item not found")); }
                    gitpass_host::handler::apply_login_update(&mut r, q).map_err(host_err)?;
                    x.session.update_record(&vid, &iid, r)
                })?;
                self.poll_state();
                Ok(json!({"ok": true}))
            }
            "update_password" => {
                let (vid, iid) = split_id(&req("item_id")?)?;
                let pw = Zeroizing::new(req("password")?);
                let username = s("username").filter(|u| !u.trim().is_empty());
                self.app.state.with(|x| {
                    crate::GuiState::writable(x).map_err(|e| err("denied", e.to_string()))?;
                    let (_, mut r) = x.session.read_record(&vid, &iid)?;
                    if r.category != Category::Login { return Err(err("bad_request", "update_password needs a login item")); }
                    r.item.password = Some(pw.to_string());
                    if let Some(u) = username {
                        if r.item.username.as_deref().unwrap_or("").trim().is_empty() {
                            r.item.username = Some(u);
                        }
                    }
                    x.session.update_record(&vid, &iid, r)
                })?;
                self.poll_state();
                Ok(json!({"ok": true}))
            }
            "add_url" => {
                let (vid, iid) = split_id(&req("item_id")?)?;
                let url = req("url")?;
                gitpass_host::handler::check_request_origin(&url)?;
                self.app.state.with(|x| {
                    crate::GuiState::writable(x).map_err(|e| err("denied", e.to_string()))?;
                    let (_, mut r) = x.session.read_record(&vid, &iid)?;
                    gitpass_host::handler::add_url_to_record(&mut r, &url);
                    x.session.update_record(&vid, &iid, r)
                })?;
                self.poll_state();
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
                self.poll_state();
                Ok(json!({"favorite": fav}))
            }
            // Same soft delete as the GUI's Recently Deleted (deleted_at + 30-day purge).
            "trash_item" | "restore_item" => {
                if source != "popup" { return Err(err("denied", format!("{cmd} is allowed only from the popup"))); }
                let (vid, iid) = split_id(&req("item_id")?)?;
                if cmd == "trash_item" {
                    self.record(&vid, &iid)?; // live items only; already-deleted ones are not_found
                    self.app.delete_item(&vid, &iid).map_err(host_err)?;
                } else {
                    // The deleted record must still exist; anyhow's Display hides the io error, so map it here.
                    self.app.state.with(|x| x.session.read_record(&vid, &iid).map(|_| ())
                        .map_err(|e| err("not_found", format!("item not found: {e:#}"))))?;
                    self.app.restore_item(&vid, &iid).map_err(host_err)?;
                }
                self.poll_state();
                Ok(json!({"ok": true}))
            }
            "archive_item" | "unarchive_item" => {
                if source != "popup" { return Err(err("denied", format!("{cmd} is allowed only from the popup"))); }
                let (vid, iid) = split_id(&req("item_id")?)?;
                let archived = if cmd == "unarchive_item" { false } else { q.get("archived").and_then(|v| v.as_bool()).unwrap_or(true) };
                self.app.state.with(|x| {
                    let (_, r) = x.session.read_record(&vid, &iid).map_err(|e| err("not_found", format!("item not found: {e:#}")))?;
                    if r.deleted_at.is_some() { return Err(err("not_found", "item is deleted")); }
                    Ok(())
                })?;
                self.app.archive_item(&vid, &iid, archived).map_err(host_err)?;
                self.poll_state();
                Ok(json!({"ok": true, "archived": archived}))
            }
            "generate" => gitpass_host::protocol::generate(q).map_err(host_err),
            "record_generated_password" => {
                let pw = q.get("password").and_then(|v| v.as_str()).ok_or_else(|| err("bad_request", "password is required"))?;
                let source = q.get("source").and_then(|v| v.as_str()).unwrap_or("extension");
                let origin_or_host = q.get("origin").or_else(|| q.get("host")).and_then(|v| v.as_str());
                let host = origin_or_host.and_then(clean_host);
                let recorded = self.app.record_generated_password(pw, source, host).map_err(host_err)?.is_some();
                Ok(json!({"ok": true, "recorded": recorded}))
            }
            "generator_history" => {
                // Read-only, popup-facing: newest first, never logged. Refused while locked.
                let limit = gitpass_host::handler::history_limit(q);
                let mut entries = self.app.get_generator_history().map_err(host_err)?; // errors while locked
                if !self.app.get_settings().remember_generated_passwords {
                    return Ok(json!({"enabled": false, "entries": []}));
                }
                entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                let out: Vec<Value> = entries.iter().take(limit).map(|e| json!({
                    "id": e.id, "value": e.value, "created_at": e.created_at, "source": e.source, "host": e.host,
                })).collect();
                Ok(json!({"enabled": true, "entries": out}))
            }
            "passkey_query" => {
                self.app.state.with(|x| {
                    gitpass_host::passkey::query(&x.session, q)
                }).map_err(host_err)
            }
            "passkey_create" | "passkey_get" => {
                let origin = req("origin")?;
                let approved = gitpass_host::passkey::popup_approved(q) || {
                    let what = if cmd == "passkey_create" { "Save a passkey for" } else { "Sign in with a passkey on" };
                    self.ui.approve("Passkey", &format!("{what} {origin}?"))
                };
                if !approved { return Err(err("denied", "passkey use was not approved")); }
                let res = self.app.state.with(|x| {
                    crate::GuiState::writable(x).map_err(|e| err("denied", e.to_string()))?;
                    if cmd == "passkey_create" { gitpass_host::passkey::create(&x.session, q) } else { gitpass_host::passkey::get(&x.session, q) }
                }).map_err(host_err)?;
                if cmd == "passkey_create" { self.poll_state(); }
                Ok(res)
            }
            "item_findings" => {
                let id = req("item_id")?;
                let (vid, iid) = split_id(&id)?;
                let (_, r) = self.record(&vid, &iid)?;
                // Read the vault stamp *before* computing, so a write during the audit
                // leaves the cached report under the old stamp and invalidates it.
                let stamp = self.app.state.change_stamp();
                let rep = cached_audit(&self.audit_cache, stamp, || {
                    self.app.state.with(|x| {
                        let s = &x.session;
                        let records = App::all_records(s)?;
                        let passkey_sites: BTreeSet<String> = records.iter()
                            .filter(|(_, _, r)| r.deleted_at.is_none() && !r.archived && r.category == Category::Passkey)
                            .filter_map(|(_, _, r)| r.find_field(gitpass_core::passkey::F_RP_ID).and_then(|f| f.value))
                            .collect();
                        let items: Vec<(String, String, ItemRecord)> = records.into_iter()
                            .filter(|(_, _, r)| r.deleted_at.is_none() && !r.archived && r.category != Category::Passkey)
                            .collect();
                        gitpass_core::audit::audit_with_passkeys(&items, &passkey_sites, &gitpass_core::audit::AuditOpts::default(), None)
                    })
                })?;
                Ok(gitpass_host::protocol::item_findings(&vid, &iid, &r, &rep))
            }
            "site_icon" => {
                let host = req("host")?.trim().to_lowercase();
                if host.is_empty() { return Err(err("bad_request", "empty host")); }
                if !self.app.get_settings().show_website_icons && !self.app.read_local().allow_favicons {
                    return Err(err("denied", "website icons are disabled"));
                }
                let live_records = self.records()?;
                let allowed = live_records.iter().any(|(_, _, _, r)| {
                    if let Some(h) = r.item.url.as_deref().and_then(gitpass_host::protocol::url_host) {
                        if h.eq_ignore_ascii_case(&host) { return true; }
                    }
                    r.sections.iter().flat_map(|s| &s.fields).any(|f| {
                        f.kind == gitpass_core::item::FieldKind::Url
                            && f.value.as_deref().and_then(gitpass_host::protocol::url_host).is_some_and(|h| h.eq_ignore_ascii_case(&host))
                    })
                });
                if !allowed {
                    return Err(err("denied", format!("refusing host '{host}' not in any item")));
                }
                let cache_dir = self.app.favicon_cache_dir();
                match crate::favicon::get_cached_icon(&cache_dir, &host) {
                    Some(data_url) => Ok(json!({ "data_url": data_url })),
                    None => Err(err("not_found", "site icon not cached")),
                }
            }
            "open_in_app" => {
                let view_opt = s("view").filter(|v| !v.is_empty());
                if let Some(ref v) = view_opt {
                    if !matches!(v.as_str(), "generator_history" | "watchtower" | "settings") {
                        return Err(err("bad_request", format!("unknown view '{v}'")));
                    }
                }
                match s("item_id").filter(|v| !v.is_empty()) {
                    Some(id) => {
                        let (v, i) = split_id(&id)?;
                        self.ui.open_item(Some(&v), Some(&i));
                    }
                    None => {
                        if let Some(v) = view_opt {
                            let sec = s("section").filter(|s| !s.is_empty());
                            self.ui.open_view(&v, sec.as_deref());
                        } else {
                            self.ui.open_item(None, None);
                        }
                    }
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
    fn archived_records(&self) -> Result<Vec<(String, String, String, ItemRecord)>> {
        self.app.state.with(|u| {
            let mut out = vec![];
            for (vid, m) in u.session.list_vaults()? {
                for (iid, _, r) in u.session.list_records(&vid)? { if r.deleted_at.is_none() && r.archived { out.push((vid.clone(), m.name.clone(), iid, r)); } }
            }
            Ok(out)
        })
    }
    fn deleted_records(&self) -> Result<Vec<(String, String, String, ItemRecord)>> {
        self.app.state.with(|u| {
            let mut out = vec![];
            for (vid, m) in u.session.list_vaults()? {
                for (iid, _, r) in u.session.list_records(&vid)? { if r.deleted_at.is_some() { out.push((vid.clone(), m.name.clone(), iid, r)); } }
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


/// Keep contract codes from gitpass-host's `CodedError` (e.g. `origin_mismatch`, `exists`).
fn host_err(e: anyhow::Error) -> anyhow::Error {
    if let Some(c) = e.downcast_ref::<gitpass_host::handler::CodedError>() { return err(c.code, c.message.clone()); }
    e
}

/// Return the cached audit report for `stamp`, or compute it and cache it under `stamp`.
/// `stamp` must be read before `compute` runs; a `None` stamp (locked/unreadable) is never cached.
pub(crate) fn cached_audit<T, E>(
    cache: &Mutex<Option<(Option<u64>, Arc<T>)>>,
    stamp: Option<u64>,
    compute: impl FnOnce() -> Result<T, E>,
) -> Result<Arc<T>, E> {
    if stamp.is_some() {
        if let Some((s, rep)) = &*cache.lock().unwrap() {
            if *s == stamp {
                return Ok(Arc::clone(rep));
            }
        }
    }
    let fresh = Arc::new(compute()?);
    if stamp.is_some() {
        *cache.lock().unwrap() = Some((stamp, Arc::clone(&fresh)));
    }
    Ok(fresh)
}

#[cfg(test)]
mod cached_audit_tests {
    use super::cached_audit;
    use std::sync::{Arc, Mutex};

    #[test]
    fn write_during_compute_invalidates_cached_report() {
        let cache: Mutex<Option<(Option<u64>, Arc<u32>)>> = Mutex::new(None);
        let mut vault_stamp = 1u64;
        // Stamp read before compute; a write lands while computing.
        let before = Some(vault_stamp);
        let r = cached_audit(&cache, before, || { vault_stamp = 2; Ok::<_, ()>(10) }).unwrap();
        assert_eq!(*r, 10);
        // Next lookup sees the new stamp and must recompute, not reuse the stale report.
        let mut recomputed = false;
        let r2 = cached_audit(&cache, Some(vault_stamp), || { recomputed = true; Ok::<_, ()>(20) }).unwrap();
        assert!(recomputed);
        assert_eq!(*r2, 20);
        // Same stamp again hits the cache.
        let r3 = cached_audit(&cache, Some(vault_stamp), || -> Result<u32, ()> { panic!("should be cached") }).unwrap();
        assert_eq!(*r3, 20);
    }

    #[test]
    fn unknown_stamp_and_errors_are_not_cached() {
        let cache: Mutex<Option<(Option<u64>, Arc<u32>)>> = Mutex::new(None);
        cached_audit(&cache, None, || Ok::<_, ()>(1)).unwrap();
        assert!(cache.lock().unwrap().is_none());
        assert!(cached_audit(&cache, Some(5), || Err::<u32, _>("boom")).is_err());
        assert!(cache.lock().unwrap().is_none());
    }
}
