//! Command handling. Requests: `{"id":<any>,"cmd":"<name>", ...args}`.
//! Responses: `{"id":<same>,"ok":true,"result":{...}}` or `{"id":<same>,"ok":false,"error":"..."}`.
use crate::origin::{origin_matches, parse_request_origin};
use anyhow::{anyhow, bail, Context, Result};
use gitpass_core::crypto::{generate_password, KdfParams, SecretKey};
use gitpass_core::store::{self, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub struct HostConfig {
    pub vault_path: PathBuf,
    /// File holding the Secret Key (default `<vault>/secret.key`).
    pub secret_key_path: PathBuf,
    pub idle_timeout: Duration,
    /// KDF target passed to unlock (tests use fast params; default = production params).
    pub target_kdf: KdfParams,
}

pub struct Host {
    pub cfg: HostConfig,
    session: Option<Session>,
    last_activity: Instant,
}

fn arg_str<'a>(req: &'a Value, k: &str) -> Result<&'a str> {
    req.get(k).and_then(Value::as_str).ok_or_else(|| anyhow!("missing string argument '{k}'"))
}

fn split_id(id: &str) -> Result<(&str, &str)> {
    id.split_once(':').filter(|(v, i)| !v.is_empty() && !i.is_empty()).ok_or_else(|| anyhow!("bad item_id (expected <vault>:<item>)"))
}

impl Host {
    pub fn new(cfg: HostConfig) -> Self { Host { cfg, session: None, last_activity: Instant::now() } }

    pub fn is_unlocked(&self) -> bool { self.session.is_some() }

    /// Drop the session if idle for longer than the timeout. Returns true if it locked.
    pub fn expire_if_idle(&mut self) -> bool {
        if self.session.is_some() && self.last_activity.elapsed() >= self.cfg.idle_timeout {
            self.session = None;
            return true;
        }
        false
    }

    /// Test hook: pretend the last activity was `d` ago.
    pub fn backdate_activity(&mut self, d: Duration) { self.last_activity = Instant::now() - d; }

    pub fn handle(&mut self, req: &Value) -> Value {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        self.expire_if_idle();
        match self.dispatch(req) {
            Ok(r) => json!({"id": id, "ok": true, "result": r}),
            Err(e) => json!({"id": id, "ok": false, "error": format!("{e:#}")}),
        }
    }

    fn sess(&self) -> Result<&Session> {
        let s = self.session.as_ref().ok_or_else(|| anyhow!("locked"))?;
        Ok(s)
    }

    fn dispatch(&mut self, req: &Value) -> Result<Value> {
        let cmd = arg_str(req, "cmd")?;
        // status/generate don't count as vault activity, everything else refreshes the idle timer.
        if !matches!(cmd, "status" | "generate") { self.last_activity = Instant::now(); }
        match cmd {
            "status" => Ok(json!({
                "unlocked": self.is_unlocked(),
                "version": env!("CARGO_PKG_VERSION"),
                "idle_timeout_secs": self.cfg.idle_timeout.as_secs(),
                "secret_key_file": self.cfg.secret_key_path.is_file(),
            })),
            "unlock" => {
                let pw = Zeroizing::new(arg_str(req, "password")?.to_string());
                // Secret Key: provided by the popup (kept only for this call, zeroized), else the optional file.
                let sk_txt = match req.get("secret_key").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()) {
                    Some(v) => Zeroizing::new(v.to_string()),
                    None => Zeroizing::new(std::fs::read_to_string(&self.cfg.secret_key_path).map_err(|_| anyhow!(
                        "Secret Key required: enter it in the popup (no Secret Key file at {})", self.cfg.secret_key_path.display()))?),
                };
                let sk = SecretKey::parse(sk_txt.trim())?;
                let root = &self.cfg.vault_path;
                if store::read_format(root)?.version < store::MIN_READ_FORMAT {
                    bail!("vault uses an old format; run `gitpass migrate` first");
                }
                // Verify-only unlock: keeps the vault's KDF params and writes nothing to the vault repo.
                let s = Session::reopen_verify(root, &pw, &sk, false)?;
                if s.device_revoked { bail!("this device is revoked"); }
                self.session = Some(s);
                self.last_activity = Instant::now();
                Ok(json!({"unlocked": true}))
            }
            "lock" => { self.session = None; Ok(json!({"unlocked": false})) }
            "match" => {
                let url = arg_str(req, "url")?;
                if parse_request_origin(url).is_none() { bail!("refused: origin must be https (or http://localhost)"); }
                let s = self.sess()?;
                let mut out = vec![];
                for (vid, _) in s.list_vaults()? {
                    for (iid, it) in s.list_items(&vid)? {
                        if it.url.as_deref().is_some_and(|u| origin_matches(u, url)) {
                            out.push(json!({"id": format!("{vid}:{iid}"), "title": it.title, "username": it.username, "has_totp": it.totp.is_some()}));
                        }
                    }
                }
                Ok(json!({"items": out}))
            }
            "search" => {
                let q = arg_str(req, "query")?;
                let s = self.sess()?;
                let mut out = vec![];
                for (vid, _) in s.list_vaults()? {
                    for (_, iid, it) in gitpass_core::fuzzy_find(q, s.list_items(&vid)?) {
                        out.push(json!({"id": format!("{vid}:{iid}"), "title": it.title, "username": it.username, "url": it.url, "has_totp": it.totp.is_some()}));
                    }
                }
                out.truncate(50);
                Ok(json!({"items": out}))
            }
            "fill" => {
                let (vid, iid) = split_id(arg_str(req, "item_id")?)?;
                let origin = arg_str(req, "origin")?;
                let s = self.sess()?;
                let (_, it) = s.read_item(vid, iid)?;
                let ok = it.url.as_deref().is_some_and(|u| origin_matches(u, origin));
                if !ok { bail!("refused: item URL does not match origin {origin}"); }
                Ok(json!({"username": it.username, "password": it.password}))
            }
            "totp" => {
                let (vid, iid) = split_id(arg_str(req, "item_id")?)?;
                let s = self.sess()?;
                let (_, it) = s.read_item(vid, iid)?;
                let t = it.totp.as_deref().ok_or_else(|| anyhow!("item has no TOTP secret"))?;
                let (code, remaining) = gitpass_core::totp_now(&gitpass_core::totp::parse(t)?);
                Ok(json!({"code": code, "remaining": remaining}))
            }
            "generate" => {
                let len = req.get("len").and_then(Value::as_u64).unwrap_or(20) as usize;
                if !(8..=128).contains(&len) { bail!("len must be 8..=128"); }
                Ok(json!({"password": generate_password(len)}))
            }
            other => bail!("unknown command '{other}'"),
        }
    }
}
