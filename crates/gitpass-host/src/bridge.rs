//! The native-messaging side: reads extension requests, proxies them to the desktop app when it is
//! running, and otherwise answers them in standalone mode with [`Host`]. App events are forwarded.
use crate::agent::AgentPaths;
use crate::framing::{read_message, write_message};
use crate::handler::{check_request_origin, code, error_frame, CodedError, Host};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Timeout for `request_unlock` (the prompt itself times out after 120 s in the app).
pub const UNLOCK_TIMEOUT: Duration = Duration::from_secs(125);
/// Timeout for every other proxied command.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Check the browser-supplied caller (argv) against `allowed_callers`.
/// Chromium: argv[1] = `chrome-extension://<id>/`. Firefox: argv[1] = manifest path, argv[2] = extension id.
pub fn caller_from_args(args: &[String]) -> Option<String> {
    if args.len() >= 2 && !args[0].starts_with("chrome-extension://") { Some(args[1].clone()) } else { args.first().cloned() }
}
pub fn caller_allowed(args: &[String], allowed: &[String]) -> Result<String, String> {
    let caller = caller_from_args(args).ok_or_else(|| "no caller in argv".to_string())?;
    if allowed.is_empty() || !allowed.contains(&caller) { return Err(caller); }
    Ok(caller)
}

#[cfg(unix)]
type AgentT = crate::agent::Agent;

pub struct Bridge {
    paths: Option<AgentPaths>,
    #[cfg(unix)]
    agent: Mutex<Option<Arc<AgentT>>>,
    pub host: Arc<Mutex<Host>>,
    out: Arc<Mutex<Box<dyn Write + Send>>>,
    inflight: Mutex<Vec<JoinHandle<()>>>,
}

/// Commands that carry a page origin which must be https (or loopback http).
fn origin_arg(cmd: &str, req: &Value) -> Option<String> {
    match cmd {
        "match" => req.get("origin").or_else(|| req.get("url")).and_then(Value::as_str).map(str::to_string),
        "fill" | "save_login" | "passkey_create" | "passkey_get" | "passkey_query" | "add_url" => req.get("origin").or_else(|| req.get("url")).and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

impl Bridge {
    /// `paths = None` disables the desktop-app connection (standalone only).
    pub fn new(paths: Option<AgentPaths>, host: Host, out: Box<dyn Write + Send>) -> Arc<Bridge> {
        Arc::new(Bridge {
            paths,
            #[cfg(unix)]
            agent: Mutex::new(None),
            host: Arc::new(Mutex::new(host)),
            out: Arc::new(Mutex::new(out)),
            inflight: Mutex::new(vec![]),
        })
    }

    fn emit_to(out: &Arc<Mutex<Box<dyn Write + Send>>>, v: &Value) {
        if let Ok(mut o) = out.lock() {
            if let Err(e) = write_message(&mut *o, v) { eprintln!("gitpass-host: write to browser failed: {e:#}"); }
        }
    }
    pub fn emit(&self, v: &Value) { Self::emit_to(&self.out, v) }

    /// The live app connection, connecting (and sending hello) if needed.
    #[cfg(unix)]
    pub fn agent(&self) -> Option<Arc<AgentT>> {
        let paths = self.paths.as_ref()?;
        let mut g = self.agent.lock().unwrap();
        if let Some(a) = g.as_ref() { if a.is_alive() { return Some(a.clone()); } }
        *g = None;
        if !paths.socket.exists() { return None; }
        let out = self.out.clone();
        let out2 = self.out.clone();
        let on_event: Arc<dyn Fn(Value) + Send + Sync> = Arc::new(move |v: Value| Self::emit_to(&out, &v));
        // App went away: the extension no longer has access (standalone mode starts locked).
        let on_close: Arc<dyn Fn() + Send + Sync> = Arc::new(move || Self::emit_to(&out2, &json!({"event": "lock", "reason": "app_disconnected"})));
        match AgentT::connect(paths, on_event, on_close) {
            Ok(a) => {
                // The app owns the session now; drop any standalone session.
                self.host.lock().unwrap().lock();
                let a = Arc::new(a);
                *g = Some(a.clone());
                Some(a)
            }
            Err(e) => { eprintln!("gitpass-host: desktop app not available: {e:#}"); None }
        }
    }
    #[cfg(not(unix))]
    pub fn agent(&self) -> Option<()> { let _ = &self.paths; None }

    /// Handle one extension request (responses may be written asynchronously).
    pub fn handle_message(self: &Arc<Self>, req: Value) {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let Some(cmd) = req.get("cmd").and_then(Value::as_str).map(str::to_string) else {
            return self.emit(&error_frame(id, code::BAD_REQUEST, "missing string argument 'cmd'"));
        };
        if let Some(o) = origin_arg(&cmd, &req) {
            if let Err(e) = check_request_origin(&o) {
                let c = e.downcast_ref::<CodedError>().unwrap();
                return self.emit(&error_frame(id, c.code, &c.message));
            }
        }
        // `generate` needs no vault and `unlock` (password + Secret Key) is standalone-only.
        if !matches!(cmd.as_str(), "generate" | "unlock") {
            #[cfg(unix)]
            if let Some(agent) = self.agent() {
                let me = self.clone();
                let h = std::thread::spawn(move || {
                    let timeout = if cmd == "request_unlock" { UNLOCK_TIMEOUT } else { CALL_TIMEOUT };
                    let mut fwd = req;
                    if let Some(m) = fwd.as_object_mut() { m.remove("id"); }
                    let resp = match agent.call(fwd, timeout) {
                        Ok(mut r) => { r["id"] = id; r }
                        Err(crate::agent::CallError::Timeout) => error_frame(id, code::INTERNAL, "timed out waiting for the gitpass app"),
                        Err(e) => error_frame(id, code::APP_NOT_RUNNING, &format!("lost connection to the gitpass app: {e:?}")),
                    };
                    me.emit(&resp);
                });
                let mut inf = self.inflight.lock().unwrap();
                inf.retain(|h| !h.is_finished());
                inf.push(h);
                return;
            }
        }
        if cmd == "unlock" {
            #[cfg(unix)]
            if self.agent().is_some() {
                return self.emit(&error_frame(id, code::BAD_REQUEST, "the gitpass app is running: use request_unlock"));
            }
        }
        let resp = self.host.lock().unwrap().handle(&req);
        self.emit(&resp);
    }

    /// Read framed requests until EOF, then wait for in-flight proxied calls.
    pub fn run<R: Read>(self: &Arc<Self>, mut input: R) -> anyhow::Result<()> {
        while let Some(msg) = read_message(&mut input)? { self.handle_message(msg); }
        let hs: Vec<_> = std::mem::take(&mut *self.inflight.lock().unwrap());
        for h in hs { let _ = h.join(); }
        Ok(())
    }
}
