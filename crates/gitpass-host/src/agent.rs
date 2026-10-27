//! Client for the gitpass desktop app's agent socket (contract v0.6).
//!
//! Socket: `~/Library/Application Support/gitpass/agent.sock` (macOS) or
//! `$XDG_RUNTIME_DIR/gitpass/agent.sock` (Linux). Framing: 4-byte LE length + JSON, max 4 MiB.
//! On connect the host sends `hello {client:"gitpass-host", version, token?}`; the token is read from
//! `agent.token` next to the socket if it exists. Responses are matched to requests by `id`;
//! frames with an `event` key (`lock` / `unlock`) are passed to the event callback.
use crate::framing::{encode_message_max, read_message_max, MAX_SOCKET};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Bundle id of the desktop app (`open -b`).
pub const APP_BUNDLE_ID: &str = "dev.gitpass.desktop";

/// Planned Windows agent endpoint (named pipe). STUB: neither side speaks it yet.
pub const WINDOWS_PIPE_NAME: &str = r"\\.\pipe\gitpass-agent";

#[derive(Clone, Debug)]
pub struct AgentPaths { pub socket: PathBuf, pub token: PathBuf }

impl AgentPaths {
    /// Default paths, overridable with `GITPASS_AGENT_SOCK` / `GITPASS_AGENT_TOKEN`.
    pub fn default_paths() -> Option<AgentPaths> {
        let dir = if cfg!(target_os = "macos") {
            dirs::home_dir()?.join("Library/Application Support/gitpass")
        } else if cfg!(windows) {
            // Windows: the app would listen on the named pipe `WINDOWS_PIPE_NAME`; the client side is not
            // implemented yet (STUB), so the host always runs standalone there.
            return None;
        } else {
            PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join("gitpass")
        };
        let socket = std::env::var_os("GITPASS_AGENT_SOCK").map(PathBuf::from).unwrap_or_else(|| dir.join("agent.sock"));
        let token = std::env::var_os("GITPASS_AGENT_TOKEN").map(PathBuf::from)
            .unwrap_or_else(|| socket.parent().map(|p| p.join("agent.token")).unwrap_or_else(|| dir.join("agent.token")));
        Some(AgentPaths { socket, token })
    }
}

/// Launch (or focus) the desktop app.
pub fn launch_app() -> Result<()> {
    if cfg!(target_os = "macos") {
        let st = std::process::Command::new("open").args(["-b", APP_BUNDLE_ID]).status().context("run `open`")?;
        if !st.success() { bail!("`open -b {APP_BUNDLE_ID}` failed ({st})"); }
        Ok(())
    } else {
        let st = std::process::Command::new("gitpass-desktop").spawn();
        st.map(|_| ()).context("start gitpass-desktop")
    }
}

/// Why a proxied call failed.
#[derive(Debug)]
pub enum CallError { Disconnected, Timeout, Io(anyhow::Error) }

type EventFn = Arc<dyn Fn(Value) + Send + Sync>;

/// A live connection to the desktop app.
#[cfg(unix)]
pub struct Agent {
    writer: Mutex<std::os::unix::net::UnixStream>,
    pending: Arc<Mutex<HashMap<u64, Sender<Value>>>>,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    /// The `hello` result.
    pub hello: Value,
}

#[cfg(unix)]
impl Agent {
    /// Connect, start the reader thread and send `hello`. `on_event` gets every pushed event frame;
    /// `on_close` runs once when an established connection closes.
    pub fn connect(paths: &AgentPaths, on_event: EventFn, on_close: Arc<dyn Fn() + Send + Sync>) -> Result<Agent> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        use std::os::unix::net::UnixStream;
        // The socket must live in a private directory owned by us (the app creates it 0700).
        let dir = paths.socket.parent().context("socket path has no parent")?;
        let md = std::fs::metadata(dir).with_context(|| format!("agent dir {}", dir.display()))?;
        if md.permissions().mode() & 0o077 != 0 { bail!("agent dir {} is accessible by other users", dir.display()); }
        let home_uid = dirs::home_dir().and_then(|h| std::fs::metadata(h).ok()).map(|m| m.uid());
        if home_uid.is_some_and(|u| u != md.uid()) { bail!("agent dir {} is not owned by this user", dir.display()); }

        let stream = UnixStream::connect(&paths.socket).with_context(|| format!("connect {}", paths.socket.display()))?;
        let mut reader = stream.try_clone()?;
        let pending: Arc<Mutex<HashMap<u64, Sender<Value>>>> = Arc::default();
        let alive = Arc::new(AtomicBool::new(true));
        // Set once hello succeeded; `on_close` only fires for an established connection.
        let ready = Arc::new(AtomicBool::new(false));
        {
            let pending = pending.clone();
            let alive = alive.clone();
            let ready = ready.clone();
            std::thread::spawn(move || {
                while let Ok(Some(v)) = read_message_max(&mut reader, MAX_SOCKET) {
                    if v.get("event").is_some() { on_event(v); continue; }
                    if let Some(id) = v.get("id").and_then(Value::as_u64) {
                        if let Some(tx) = pending.lock().unwrap().remove(&id) { let _ = tx.send(v); }
                    }
                }
                alive.store(false, Ordering::SeqCst);
                pending.lock().unwrap().clear(); // waiting callers see Disconnected
                if ready.load(Ordering::SeqCst) { on_close(); }
            });
        }
        let mut a = Agent { writer: Mutex::new(stream), pending, next_id: AtomicU64::new(1), alive, hello: Value::Null };
        let token = std::fs::read(&paths.token).ok().map(|b| String::from_utf8_lossy(&b).trim().to_string()).filter(|t| !t.is_empty());
        let mut hello = json!({"cmd": "hello", "client": "gitpass-host", "version": env!("CARGO_PKG_VERSION")});
        if let Some(t) = token { hello["token"] = json!(t); }
        let r = a.call(hello, Duration::from_secs(10)).map_err(|e| anyhow::anyhow!("hello failed: {e:?}"))?;
        if r.get("ok") != Some(&json!(true)) {
            a.shutdown();
            bail!("hello refused: {} {}", r["error"], r["message"]);
        }
        a.hello = r["result"].clone();
        ready.store(true, Ordering::SeqCst);
        Ok(a)
    }

    pub fn is_alive(&self) -> bool { self.alive.load(Ordering::SeqCst) }

    pub fn shutdown(&self) {
        if let Ok(w) = self.writer.lock() { let _ = w.shutdown(std::net::Shutdown::Both); }
    }

    /// Send `req` (its `id` is replaced by an internal one) and wait for the response frame.
    pub fn call(&self, mut req: Value, timeout: Duration) -> Result<Value, CallError> {
        use std::io::Write;
        if !self.is_alive() { return Err(CallError::Disconnected); }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        req["id"] = json!(id);
        let (tx, rx) = channel();
        self.pending.lock().unwrap().insert(id, tx);
        let frame = encode_message_max(&req, MAX_SOCKET).map_err(CallError::Io)?;
        {
            let mut w = self.writer.lock().unwrap();
            if let Err(e) = w.write_all(&frame).and_then(|_| w.flush()) {
                self.pending.lock().unwrap().remove(&id);
                return Err(CallError::Io(e.into()));
            }
        }
        match rx.recv_timeout(timeout) {
            Ok(v) => Ok(v),
            Err(RecvTimeoutError::Timeout) => { self.pending.lock().unwrap().remove(&id); Err(CallError::Timeout) }
            Err(RecvTimeoutError::Disconnected) => Err(CallError::Disconnected),
        }
    }
}
