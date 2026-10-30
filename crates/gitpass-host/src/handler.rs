//! Standalone (fallback) command handling, used when the gitpass desktop app is not running.
//! Implements the v0.6 contract commands directly with gitpass-core (format v5 items).
//! Requests: `{"id":<any>,"cmd":"<name>", ...args}`.
//! Responses: `{"id":<same>,"ok":true,"result":{...}}` or
//! `{"id":<same>,"ok":false,"error":"<code>","message":"..."}`.
use crate::origin::origin_matches;
use anyhow::Result;
use gitpass_core::crypto::{KdfParams, SecretKey};
use gitpass_core::item::{Category, Field, FieldKind, Section};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{self, Item, Session};
use std::sync::Arc;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};
use zeroize::Zeroizing;

fn clean_host(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() { return None; }
    let u = if s.contains("://") { url::Url::parse(s).ok()? } else { url::Url::parse(&format!("https://{s}")).ok()? };
    u.host_str().map(|h| h.trim_start_matches("www.").to_string()).filter(|h| !h.is_empty())
}

/// Read the `rememberGeneratedPasswords` preference from `.gitpass-settings.json`.
/// Defaults to `true` when missing, matching desktop settings.
fn remember_generated_passwords(vault_path: &std::path::Path) -> bool {
    let settings_path = vault_path.join(".gitpass-settings.json");
    std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|str| serde_json::from_str::<Value>(&str).ok())
        .map(|v| {
            v.get("settings")
                .and_then(|s| s.get("rememberGeneratedPasswords").or_else(|| s.get("remember_generated_passwords")))
                .and_then(Value::as_bool)
                .unwrap_or(true)
        })
        .unwrap_or(true)
}


/// `limit` of the `generator_history` command: default 10, clamped to 1..=100.
pub fn history_limit(req: &Value) -> usize {
    req.get("limit").and_then(Value::as_u64).map(|n| n.clamp(1, 100) as usize).unwrap_or(10)
}

/// Contract error codes.
pub mod code {
    pub const LOCKED: &str = "locked";
    pub const DENIED: &str = "denied";
    pub const NOT_FOUND: &str = "not_found";
    pub const ORIGIN_MISMATCH: &str = "origin_mismatch";
    pub const BAD_REQUEST: &str = "bad_request";
    pub const APP_NOT_RUNNING: &str = "app_not_running";
    pub const INTERNAL: &str = "internal";
    pub const THROTTLED: &str = "throttled";
}

/// An error carrying a contract error code.
#[derive(Debug)]
pub struct CodedError { pub code: &'static str, pub message: String, pub retry_after_secs: Option<u64> }
impl std::fmt::Display for CodedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for CodedError {}
pub fn coded(code: &'static str, message: impl Into<String>) -> anyhow::Error { CodedError { code, message: message.into(), retry_after_secs: None }.into() }
pub fn coded_throttled(secs: u64) -> anyhow::Error {
    CodedError {
        code: code::THROTTLED,
        message: format!("Too many attempts. Try again in {secs} s"),
        retry_after_secs: Some(secs),
    }.into()
}
/// Build an error response frame.
pub fn error_frame(id: Value, code: &str, message: &str) -> Value {
    json!({"id": id, "ok": false, "error": code, "message": message})
}

pub use crate::protocol::check_request_origin;

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
    last_activity_wall: SystemTime,
    audit_cache: Option<Arc<gitpass_core::audit::AuditReport>>,
    pub favicon_cache_dir: Option<PathBuf>,
}

fn arg_str<'a>(req: &'a Value, k: &str) -> Result<&'a str> {
    req.get(k).and_then(Value::as_str).ok_or_else(|| coded(code::BAD_REQUEST, format!("missing string argument '{k}'")))
}

fn split_id(id: &str) -> Result<(&str, &str)> {
    id.split_once(':').filter(|(v, i)| !v.is_empty() && !i.is_empty())
        .ok_or_else(|| coded(code::BAD_REQUEST, "bad item_id (expected <vault>:<item>)"))
}

pub use crate::protocol::{
    detail, field_val, fields_of, has_totp, is_card_number, kind_str, nonempty, secret_preview,
    subtitle, summary, url_host, WORD_SEPARATORS,
};

fn visible(r: &ItemRecord) -> bool { r.deleted_at.is_none() && !r.archived }

/// Password generator: characters (`digits`, `symbols`), words (`words`) or a PIN (`pin`).
/// The defaults, limits and separator come from gitpass-core, so the host, CLI and desktop app agree.
pub fn generate(r: &gitpass_core::generate::Request) -> Result<String> {
    if let Some(c) = r.separator {
        if !WORD_SEPARATORS.contains(&c) {
            return Err(coded(code::BAD_REQUEST, format!("invalid word separator {c:?}: use one of -, ., _, space")));
        }
    }
    let opts = gitpass_core::generate::Options::for_request(r).map_err(|m| coded(code::BAD_REQUEST, m))?;
    Ok(gitpass_core::generate::generate(&opts))
}

/// Site password rules sent by the extension (already parsed from `passwordrules` / `maxlength`):
/// `{"classes": [{"kind": "lower|upper|digit|special", "required": bool, "chars": "-_"}]}`. `chars` only
/// narrows a `special` class and must be printable ASCII symbols. Fails with `bad_request` on anything else
/// or when the rules cannot be met, so the extension never gets a password the site would reject.
pub fn generate_for_rules(len: usize, rules: &Value) -> Result<String> {
    use gitpass_core::generate::{generate_with_classes, CharClass, DEFAULT_DIGITS, DEFAULT_SYMBOLS, LOWER, UPPER};
    let list = rules.get("classes").and_then(Value::as_array)
        .filter(|l| !l.is_empty() && l.len() <= 8)
        .ok_or_else(|| coded(code::BAD_REQUEST, "rules.classes must hold 1..=8 entries"))?;
    let mut classes = Vec::with_capacity(list.len());
    for c in list {
        let required = c.get("required").and_then(Value::as_bool).unwrap_or(false);
        let kind = c.get("kind").and_then(Value::as_str).unwrap_or("");
        let chars: Vec<u8> = match kind {
            "lower" => LOWER.to_vec(),
            "upper" => UPPER.to_vec(),
            "digit" => DEFAULT_DIGITS.to_vec(),
            "special" => match c.get("chars").and_then(Value::as_str) {
                None => DEFAULT_SYMBOLS.to_vec(),
                Some(s) => {
                    let mut v: Vec<u8> = s.bytes().collect();
                    if v.is_empty() || v.len() > 32 || !v.iter().all(|b| b.is_ascii_graphic() && !b.is_ascii_alphanumeric()) {
                        return Err(coded(code::BAD_REQUEST, "rules special chars must be 1..=32 printable ASCII symbols"));
                    }
                    v.sort_unstable();
                    v.dedup();
                    v
                }
            },
            _ => return Err(coded(code::BAD_REQUEST, "unknown rules class")),
        };
        classes.push(CharClass { chars, required });
    }
    generate_with_classes(len, &classes).ok_or_else(|| coded(code::BAD_REQUEST, "password rules cannot be satisfied"))
}

/// Apply an `update_login` request to a login record. Absent keys keep their stored value; an empty
/// username/password/notes clears it. `urls` replaces the websites: the first becomes the item URL and
/// the rest go to the importers' `urls` section (fields `url1`, `url2`, …).
pub fn apply_login_update(r: &mut ItemRecord, req: &Value) -> Result<()> {
    if r.category != Category::Login {
        return Err(coded(code::BAD_REQUEST, "update_login needs a login item"));
    }
    if let Some(v) = req.get("title") {
        let title = v.as_str().ok_or_else(|| coded(code::BAD_REQUEST, "title must be a string"))?;
        if title.trim().is_empty() { return Err(coded(code::BAD_REQUEST, "title is required")); }
        r.item.title = title.trim().to_string();
    }
    for (key, target) in [("username", &mut r.item.username), ("password", &mut r.item.password)] {
        if let Some(v) = req.get(key) {
            let s = v.as_str().ok_or_else(|| coded(code::BAD_REQUEST, format!("{key} must be a string")))?;
            *target = Some(s.to_string()).filter(|s| !s.is_empty());
        }
    }
    if let Some(v) = req.get("notes") {
        let notes = v.as_str().ok_or_else(|| coded(code::BAD_REQUEST, "notes must be a string"))?;
        r.item.notes = Some(notes.to_string()).filter(|s| !s.trim().is_empty());
    }
    if let Some(v) = req.get("urls") {
        apply_websites(r, parse_websites(v)?);
    }
    Ok(())
}

fn parse_websites(v: &Value) -> Result<Vec<String>> {
    let list: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::String(_) => vec![v],
        _ => return Err(coded(code::BAD_REQUEST, "urls must be an array of strings")),
    };
    let mut out: Vec<String> = vec![];
    for item in list {
        let s = item.as_str().ok_or_else(|| coded(code::BAD_REQUEST, "urls must be an array of strings"))?.trim();
        if s.is_empty() { continue; }
        let url = if s.contains("://") { s.to_string() } else { format!("https://{s}") };
        let valid = url::Url::parse(&url).ok()
            .is_some_and(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some_and(|h| !h.is_empty()));
        if !valid { return Err(coded(code::BAD_REQUEST, format!("website must be an http(s) URL: {s}"))); }
        if !out.contains(&url) { out.push(url); }
    }
    Ok(out)
}

pub fn apply_websites(r: &mut ItemRecord, urls: Vec<String>) {
    r.item.url = urls.first().cloned();
    let mut fields: Vec<Field> = urls.iter().skip(1).enumerate().map(|(n, u)| Field {
        id: format!("url{}", n + 1), label: "website".into(), kind: FieldKind::Url, value: Some(u.clone()), removed: false,
    }).collect();
    let old = r.sections.iter().find(|s| s.id == "urls").cloned();
    let Some(old) = old else {
        if !fields.is_empty() { r.sections.push(Section { id: "urls".into(), title: Some("Websites".into()), fields }); }
        return;
    };
    // Keep tombstones so a removed extra website also disappears after a git merge.
    let gone: Vec<Field> = old.fields.iter().filter(|f| !fields.iter().any(|x| x.id == f.id))
        .map(|f| Field { id: f.id.clone(), label: f.label.clone(), kind: f.kind, value: None, removed: true }).collect();
    fields.extend(gone);
    if fields.is_empty() && old.title.is_none() {
        r.sections.retain(|s| s.id != "urls");
    } else if let Some(s) = r.sections.iter_mut().find(|s| s.id == "urls") {
        s.fields = fields;
    }
}

pub fn add_url_to_record(r: &mut ItemRecord, url: &str) {
    let matches_url = |u: &str| origin_matches(u, url);
    let mut urls: Vec<String> = Vec::new();
    if let Some(u) = &r.item.url {
        urls.push(u.clone());
    }
    for s in &r.sections {
        for f in &s.fields {
            if !f.removed && f.kind == FieldKind::Url {
                if let Some(v) = &f.value {
                    if !urls.contains(v) {
                        urls.push(v.clone());
                    }
                }
            }
        }
    }
    if !urls.iter().any(|u| matches_url(u)) {
        urls.push(url.to_string());
        apply_websites(r, urls);
    }
}

impl Host {
    pub fn new(cfg: HostConfig) -> Self { Host { cfg, session: None, last_activity: Instant::now(), last_activity_wall: SystemTime::now(), audit_cache: None, favicon_cache_dir: None } }

    pub fn set_favicon_cache_dir(&mut self, dir: PathBuf) { self.favicon_cache_dir = Some(dir); }

    pub fn is_unlocked(&self) -> bool { self.session.is_some() }

    /// Drop the session if idle for longer than the timeout. Returns true if it locked.
    pub fn expire_if_idle(&mut self) -> bool {
        let idle_by_instant = self.last_activity.elapsed() >= self.cfg.idle_timeout;
        let idle_by_wall = SystemTime::now().duration_since(self.last_activity_wall).map(|d| d >= self.cfg.idle_timeout).unwrap_or(false);
        if self.session.is_some() && (idle_by_instant || idle_by_wall) {
            self.session = None;
            self.audit_cache = None;
            return true;
        }
        false
    }

    /// Test hook: pretend the last activity was `d` ago.
    pub fn backdate_activity(&mut self, d: Duration) {
        self.last_activity = Instant::now() - d;
        self.last_activity_wall = SystemTime::now().checked_sub(d).unwrap_or_else(SystemTime::now);
    }

    #[doc(hidden)]
    pub fn backdate_wall_activity(&mut self, d: Duration) {
        self.last_activity_wall = SystemTime::now().checked_sub(d).unwrap_or_else(SystemTime::now);
    }

    /// Drop the in-memory session (e.g. when the desktop app takes over).
    pub fn lock(&mut self) { self.session = None; self.audit_cache = None; }

    pub fn handle(&mut self, req: &Value) -> Value {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        self.expire_if_idle();
        match self.dispatch(req) {
            Ok(r) => json!({"id": id, "ok": true, "result": r}),
            Err(e) => {
                if let Some(t) = e.downcast_ref::<gitpass_core::unlock_throttle::ThrottledError>() {
                    let mut frame = error_frame(id, code::THROTTLED, &format!("Too many attempts. Try again in {} s", t.seconds_remaining));
                    frame["seconds_remaining"] = json!(t.seconds_remaining);
                    frame["retry_after_secs"] = json!(t.seconds_remaining);
                    frame
                } else if let Some(c) = e.downcast_ref::<CodedError>() {
                    let mut frame = error_frame(id, c.code, &c.message);
                    if let Some(secs) = c.retry_after_secs {
                        frame["seconds_remaining"] = json!(secs);
                        frame["retry_after_secs"] = json!(secs);
                    }
                    frame
                } else {
                    error_frame(id, code::INTERNAL, &format!("{e:#}"))
                }
            }
        }
    }

    fn sess(&self) -> Result<&Session> {
        self.session.as_ref().ok_or_else(|| coded(code::LOCKED, "locked"))
    }

    fn vault_name(&self, vid: &str) -> Result<String> { Ok(self.sess()?.vault_meta(vid)?.name.clone()) }

    fn record(&self, item_id: &str) -> Result<(String, String, ItemRecord)> {
        let (vid, iid) = split_id(item_id)?;
        let s = self.sess()?;
        let (_, r) = s.read_record(vid, iid).map_err(|e| coded(code::NOT_FOUND, format!("item not found: {e:#}")))?;
        if r.deleted_at.is_some() { return Err(coded(code::NOT_FOUND, "item not found")); }
        Ok((vid.to_string(), iid.to_string(), r))
    }

    /// Every visible record: (vault id, vault name, item id, record).
    fn all_records(&self, only_vault: Option<&str>) -> Result<Vec<(String, String, String, ItemRecord)>> {
        let s = self.sess()?;
        let mut out = vec![];
        for (vid, meta) in s.list_vaults()? {
            if only_vault.is_some_and(|v| v != vid) { continue; }
            for (iid, _, r) in s.list_records(&vid)? {
                if visible(&r) { out.push((vid.clone(), meta.name.clone(), iid, r)); }
            }
        }
        Ok(out)
    }

    /// Every archived record: (vault id, vault name, item id, record).
    fn archived_records(&self, only_vault: Option<&str>) -> Result<Vec<(String, String, String, ItemRecord)>> {
        let s = self.sess()?;
        let mut out = vec![];
        for (vid, meta) in s.list_vaults()? {
            if only_vault.is_some_and(|v| v != vid) { continue; }
            for (iid, _, r) in s.list_records(&vid)? {
                if r.deleted_at.is_none() && r.archived { out.push((vid.clone(), meta.name.clone(), iid, r)); }
            }
        }
        Ok(out)
    }

    /// Every deleted record: (vault id, vault name, item id, record).
    fn deleted_records(&self, only_vault: Option<&str>) -> Result<Vec<(String, String, String, ItemRecord)>> {
        let s = self.sess()?;
        let mut out = vec![];
        for (vid, meta) in s.list_vaults()? {
            if only_vault.is_some_and(|v| v != vid) { continue; }
            for (iid, _, r) in s.list_records(&vid)? {
                if r.deleted_at.is_some() { out.push((vid.clone(), meta.name.clone(), iid, r)); }
            }
        }
        Ok(out)
    }

    fn get_audit_report(&mut self) -> Result<Arc<gitpass_core::audit::AuditReport>> {
        if let Some(cached) = &self.audit_cache {
            return Ok(Arc::clone(cached));
        }
        let records = self.all_records(None)?;
        let passkey_sites: std::collections::BTreeSet<String> = records.iter()
            .filter(|(_, _, _, r)| r.deleted_at.is_none() && !r.archived && r.category == Category::Passkey)
            .filter_map(|(_, _, _, r)| r.find_field(gitpass_core::passkey::F_RP_ID).and_then(|f| f.value))
            .collect();
        let items: Vec<(String, String, ItemRecord)> = records.into_iter()
            .filter(|(_, _, _, r)| r.deleted_at.is_none() && !r.archived && r.category != Category::Passkey)
            .map(|(vid, _, iid, r)| (vid, iid, r))
            .collect();
        let rep = gitpass_core::audit::audit_with_passkeys(&items, &passkey_sites, &gitpass_core::audit::AuditOpts::default(), None)?;
        let rep = Arc::new(rep);
        self.audit_cache = Some(Arc::clone(&rep));
        Ok(rep)
    }

    /// `{name, vault_count?}`; the name is never the account UUID (see `remote::display_name`).
    fn account(&self) -> Value {
        let mut a = json!({"name": gitpass_core::remote::display_name(&self.cfg.vault_path)});
        if let Some(n) = self.session.as_ref().and_then(|s| s.list_vaults().ok()).map(|v| v.len()) { a["vault_count"] = json!(n); }
        a
    }

    fn require_user_action(req: &Value, what: &str) -> Result<()> {
        let src = req.get("source").and_then(Value::as_str);
        if matches!(src, Some("popup") | Some("inpage_click")) { Ok(()) } else { Err(coded(code::DENIED, format!("{what} is allowed only from the popup or after an in-page click"))) }
    }

    fn dispatch(&mut self, req: &Value) -> Result<Value> {
        let cmd = arg_str(req, "cmd")?;
        // status/generate don't count as vault activity, everything else refreshes the idle timer.
        if !matches!(cmd, "status" | "generate" | "hello") {
            self.last_activity = Instant::now();
            self.last_activity_wall = SystemTime::now();
        }
        match cmd {
            "status" | "hello" => Ok(json!({
                "unlocked": self.is_unlocked(),
                "account": self.account(),
                "app_running": false,
                "mode": "standalone",
                "version": env!("CARGO_PKG_VERSION"),
                "idle_timeout_secs": self.cfg.idle_timeout.as_secs(),
                "secret_key_file": self.cfg.secret_key_path.is_file(),
                "throttled_secs": gitpass_core::unlock_throttle::seconds_remaining(&self.cfg.vault_path),
            })),
            // Fallback only: password + Secret Key, in memory only.
            "unlock" => {
                let pw = Zeroizing::new(arg_str(req, "password")?.to_string());
                // Secret Key: provided by the popup (kept only for this call, zeroized), else the optional file.
                let sk_txt = match req.get("secret_key").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()) {
                    Some(v) => Zeroizing::new(v.to_string()),
                    None => Zeroizing::new(std::fs::read_to_string(&self.cfg.secret_key_path).map_err(|_| coded(code::BAD_REQUEST, format!(
                        "Secret Key required: enter it in the popup (no Secret Key file at {})", self.cfg.secret_key_path.display())))?),
                };
                let sk = SecretKey::parse(sk_txt.trim()).map_err(|e| coded(code::DENIED, format!("unlock failed: {e:#}")))?;
                let root = &self.cfg.vault_path;
                if store::read_format(root)?.version < store::MIN_READ_FORMAT {
                    return Err(coded(code::INTERNAL, "vault uses an old format; run `gitpass migrate` first"));
                }
                gitpass_core::unlock_throttle::check_throttle(root).map_err(|t| coded_throttled(t.seconds_remaining))?;
                // Verify-only unlock: keeps the vault's KDF params and writes nothing on unlock.
                let s = Session::reopen_verify(root, &pw, &sk, false).map_err(|e| {
                    if let Some(t) = e.downcast_ref::<gitpass_core::unlock_throttle::ThrottledError>() {
                        coded_throttled(t.seconds_remaining)
                    } else {
                        coded(code::DENIED, format!("{e:#}"))
                    }
                })?;
                if s.device_revoked { return Err(coded(code::DENIED, "this device is revoked")); }
                self.session = Some(s);
                self.audit_cache = None;
                self.last_activity = Instant::now();
                self.last_activity_wall = SystemTime::now();
                Ok(json!({"unlocked": true}))
            }
            "request_unlock" => Err(coded(code::APP_NOT_RUNNING, "the gitpass app is not running; open it or unlock with password")),
            "lock" => { self.session = None; Ok(json!({"unlocked": false})) }
            "list_items" => {
                let vault = req.get("vault_id").and_then(Value::as_str);
                let usage = self.sess()?.all_last_used();
                let recs = if req.get("state").and_then(Value::as_str) == Some("archived")
                    || req.get("filter").and_then(Value::as_str) == Some("archive")
                    || req.get("archived").and_then(Value::as_bool) == Some(true) {
                    self.archived_records(vault)?
                } else if req.get("state").and_then(Value::as_str) == Some("deleted")
                    || req.get("filter").and_then(Value::as_str) == Some("deleted")
                    || req.get("deleted").and_then(Value::as_bool) == Some(true) {
                    self.deleted_records(vault)?
                } else {
                    self.all_records(vault)?
                };
                let iter = recs.iter().map(|(v, vn, i, r)| (v.as_str(), vn.as_str(), i.as_str(), r));
                crate::protocol::list_items(iter, req, &usage)
            }
            "list_archived" => {
                let vault = req.get("vault_id").and_then(Value::as_str);
                let usage = self.sess()?.all_last_used();
                let recs = self.archived_records(vault)?;
                let iter = recs.iter().map(|(v, vn, i, r)| (v.as_str(), vn.as_str(), i.as_str(), r));
                Ok(crate::protocol::list_archived(iter, vault, &usage))
            }
            "list_deleted" => {
                let vault = req.get("vault_id").and_then(Value::as_str);
                let recs = self.deleted_records(vault)?;
                let iter = recs.iter().map(|(v, vn, i, r)| (v.as_str(), vn.as_str(), i.as_str(), r));
                Ok(crate::protocol::list_deleted(iter, vault))
            }
            "match" => {
                let usage = self.sess()?.all_last_used();
                let records = self.all_records(None)?;
                let iter = records.iter().map(|(v, vn, i, r)| (v.as_str(), vn.as_str(), i.as_str(), r));
                crate::protocol::match_items(iter, req, &usage)
            }
            "search" => {
                // 0.5 compatibility.
                let q = arg_str(req, "query")?;
                let s = self.sess()?;
                let mut out = vec![];
                for (vid, meta) in s.list_vaults()? {
                    for (_, iid, it) in gitpass_core::fuzzy_find(q, s.list_items(&vid)?) {
                        out.push(json!({"id": format!("{vid}:{iid}"), "vault_name": meta.name, "title": it.title, "username": it.username, "url": it.url, "has_totp": it.totp.is_some()}));
                    }
                }
                out.truncate(50);
                Ok(json!({"items": out}))
            }
            "get_item" => {
                let (vid, iid) = split_id(arg_str(req, "item_id")?)?;
                let include_deleted = req.get("include_deleted").and_then(Value::as_bool).unwrap_or(false);
                let s = self.sess()?;
                let vname = self.vault_name(vid)?;
                let (_, r) = s.read_record(vid, iid).map_err(|e| coded(code::NOT_FOUND, format!("item not found: {e:#}")))?;
                if r.deleted_at.is_some() && !include_deleted {
                    return Err(coded(code::NOT_FOUND, "item not found"));
                }
                let lu = s.last_used(iid);
                Ok(crate::protocol::get_item(vid, &vname, iid, &r, lu))
            }
            "reveal" => {
                if req.get("source").and_then(Value::as_str) != Some("popup") {
                    return Err(coded(code::DENIED, "reveal is allowed only from the popup"));
                }
                let (_, iid, r) = self.record(arg_str(req, "item_id")?)?;
                let field = arg_str(req, "field")?;
                let value = if let Some(idx_str) = field.strip_prefix("password_history:") {
                    let idx: usize = idx_str.parse().map_err(|_| coded(code::BAD_REQUEST, "invalid password history index"))?;
                    r.password_history.get(idx).map(|h| h.value.clone())
                } else {
                    match field {
                        "notes" => r.item.notes.clone(),
                        "url" => r.item.url.clone(),
                        _ => r.find_field(field).and_then(|f| f.value),
                    }
                }.ok_or_else(|| coded(code::NOT_FOUND, format!("no field '{field}'")))?;
                let _ = self.sess()?.touch_item_used(&iid);
                Ok(json!({"value": value}))
            }
            "fill" => {
                let origin = arg_str(req, "origin")?;
                check_request_origin(origin)?;
                let (_, iid, r) = self.record(arg_str(req, "item_id")?)?;
                let matches_url = |u: &str| origin_matches(u, origin);
                let confirmed = req.get("user_confirm").and_then(Value::as_bool).unwrap_or(false);
                let ok = r.item.url.as_deref().is_some_and(matches_url)
                    || r.sections.iter().flat_map(|s| &s.fields).any(|f| f.kind == FieldKind::Url && f.value.as_deref().is_some_and(matches_url))
                    || confirmed;
                if !ok { return Err(coded(code::ORIGIN_MISMATCH, format!("refused: item URL does not match origin {origin}"))); }
                let _ = self.sess()?.touch_item_used(&iid);
                let totp = match nonempty(&r.item.totp) {
                    Some(t) => Some(gitpass_core::totp_now(&gitpass_core::totp::parse(t)?).0),
                    None => None,
                };
                Ok(json!({"username": r.item.username, "password": r.item.password, "totp": totp}))
            }
            "fill_card" | "fill_identity" => {
                Self::require_user_action(req, cmd)?;
                let (_, iid, r) = self.record(arg_str(req, "item_id")?)?;
                let _ = self.sess()?.touch_item_used(&iid);
                crate::protocol::fill_card_or_identity(&r, cmd == "fill_card")
            }
            "totp" => {
                let (_, _, r) = self.record(arg_str(req, "item_id")?)?;
                let t = nonempty(&r.item.totp).map(str::to_string)
                    .or_else(|| fields_of(&r).into_iter().find(|f| f.kind == FieldKind::Totp).and_then(|f| f.value))
                    .ok_or_else(|| coded(code::NOT_FOUND, "item has no TOTP secret"))?;
                let spec = gitpass_core::totp::parse(&t)?;
                let (code, remaining) = gitpass_core::totp_now(&spec);
                Ok(json!({"code": code, "remaining": remaining, "period": spec.period}))
            }
            "save_login" => {
                let origin = arg_str(req, "origin")?;
                check_request_origin(origin)?;
                let username = arg_str(req, "username")?;
                let password = arg_str(req, "password")?;
                let s = self.sess()?;
                let vid = match req.get("vault_id").and_then(Value::as_str) {
                    Some(v) => { s.vault_meta(v).map_err(|_| coded(code::NOT_FOUND, "vault not found"))?; v.to_string() }
                    None => s.list_vaults()?.into_iter().next().map(|(v, _)| v).ok_or_else(|| coded(code::NOT_FOUND, "no vault to save into"))?,
                };
                let host = url_host(origin).unwrap_or_else(|| origin.to_string());
                let title = req.get("title").and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string).unwrap_or(host);
                let item = Item { title, username: Some(username.into()), password: Some(password.into()),
                    url: Some(origin.into()), notes: None, totp: None, updated_at: String::new() };
                let iid = s.add_record(&vid, ItemRecord::new(item))?;
                self.audit_cache = None;
                Ok(json!({"item_id": format!("{vid}:{iid}")}))
            }
            "create_item" => {
                // Same item model as the desktop app's New Item (any category except passkey).
                let v = req.get("item").ok_or_else(|| coded(code::BAD_REQUEST, "missing 'item'"))?;
                let (want_vid, rec) = record_from_input(v)?;
                let s = self.sess()?;
                let vid = match want_vid {
                    Some(v) => { s.vault_meta(&v).map_err(|_| coded(code::NOT_FOUND, "vault not found"))?; v }
                    None => s.list_vaults()?.into_iter().next().map(|(v, _)| v).ok_or_else(|| coded(code::NOT_FOUND, "no vault to save into"))?,
                };
                let title = rec.item.title.clone();
                let iid = s.add_record(&vid, rec)?;
                self.audit_cache = None;
                Ok(json!({"item_id": format!("{vid}:{iid}"), "title": title}))
            }
            "update_login" => {
                // Extension pages only: background.js stamps `source` for popup/options routes, and
                // content scripts can never reach `popup:*` messages.
                if req.get("source").and_then(Value::as_str) != Some("popup") {
                    return Err(coded(code::DENIED, "update_login is allowed only from the popup"));
                }
                let (vid, iid, mut r) = self.record(arg_str(req, "item_id")?)?;
                apply_login_update(&mut r, req)?;
                self.sess()?.update_record(&vid, &iid, r)?;
                self.audit_cache = None;
                Ok(json!({"ok": true}))
            }
            "update_password" => {
                let password = arg_str(req, "password")?.to_string();
                let username = req.get("username").and_then(Value::as_str).filter(|u| !u.trim().is_empty());
                let (vid, iid, mut r) = self.record(arg_str(req, "item_id")?)?;
                r.item.password = Some(password);
                if let Some(u) = username {
                    if r.item.username.as_deref().unwrap_or("").trim().is_empty() {
                        r.item.username = Some(u.to_string());
                    }
                }
                self.sess()?.update_record(&vid, &iid, r)?;
                self.audit_cache = None;
                Ok(json!({"ok": true}))
            }
            "add_url" => {
                let url = arg_str(req, "url")?;
                check_request_origin(url)?;
                let (vid, iid, mut r) = self.record(arg_str(req, "item_id")?)?;
                add_url_to_record(&mut r, url);
                self.sess()?.update_record(&vid, &iid, r)?;
                Ok(json!({"ok": true}))
            }
            "set_favorite" => {
                let fav = req.get("favorite").and_then(Value::as_bool).ok_or_else(|| coded(code::BAD_REQUEST, "favorite must be a bool"))?;
                let (vid, iid, mut r) = self.record(arg_str(req, "item_id")?)?;
                r.favorite = fav;
                self.sess()?.update_record(&vid, &iid, r)?;
                self.audit_cache = None;
                Ok(json!({"favorite": fav}))
            }
            // Same soft delete as the desktop's Recently Deleted (deleted_at + 30-day purge).
            // Destructive write: popup only, never a content script.
            "trash_item" | "restore_item" => {
                if req.get("source").and_then(Value::as_str) != Some("popup") {
                    return Err(coded(code::DENIED, format!("{cmd} is allowed only from the popup")));
                }
                let (vid, iid) = split_id(arg_str(req, "item_id")?)?;
                let s = self.sess()?;
                // Restore must see the deleted record, so read it directly instead of `record()`.
                let (_, r) = s.read_record(vid, iid).map_err(|e| coded(code::NOT_FOUND, format!("item not found: {e:#}")))?;
                if cmd == "trash_item" {
                    if r.deleted_at.is_some() { return Err(coded(code::NOT_FOUND, "item not found")); }
                    s.soft_delete(vid, iid)?;
                } else {
                    s.restore(vid, iid)?;
                }
                self.audit_cache = None;
                Ok(json!({"ok": true}))
            }
            "archive_item" | "unarchive_item" => {
                if req.get("source").and_then(Value::as_str) != Some("popup") {
                    return Err(coded(code::DENIED, format!("{cmd} is allowed only from the popup")));
                }
                let (vid, iid) = split_id(arg_str(req, "item_id")?)?;
                let s = self.sess()?;
                let (_, r) = s.read_record(vid, iid).map_err(|e| coded(code::NOT_FOUND, format!("item not found: {e:#}")))?;
                if r.deleted_at.is_some() { return Err(coded(code::NOT_FOUND, "item not found")); }
                let archived = if cmd == "unarchive_item" { false } else { req.get("archived").and_then(Value::as_bool).unwrap_or(true) };
                s.set_archived(vid, iid, archived)?;
                self.audit_cache = None;
                Ok(json!({"ok": true, "archived": archived}))
            }
            "generate" => crate::protocol::generate(req),
            "record_generated_password" => {
                let pw = arg_str(req, "password")?;
                let source = req.get("source").and_then(Value::as_str).unwrap_or("extension");
                let origin_or_host = req.get("origin").or_else(|| req.get("host")).and_then(Value::as_str);
                let host = origin_or_host.and_then(clean_host);
                if !remember_generated_passwords(&self.cfg.vault_path) {
                    return Ok(json!({"ok": true, "recorded": false}));
                }
                self.sess()?.record_generated_password(pw, source, host)?;
                Ok(json!({"ok": true, "recorded": true}))
            }
            "generator_history" => {
                // Read-only, popup-facing: newest first, never logged. Refused while locked (`sess()`).
                let limit = history_limit(req);
                let sess = self.sess()?;
                // Same setting as `record_generated_password`: when off, the popup gets no values.
                if !remember_generated_passwords(&self.cfg.vault_path) {
                    return Ok(json!({"enabled": false, "entries": []}));
                }
                let mut entries = sess.read_generator_history()?.entries;
                entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                let out: Vec<Value> = entries.iter().take(limit).map(|e| json!({
                    "id": e.id, "value": e.value, "created_at": e.created_at, "source": e.source, "host": e.host,
                })).collect();
                Ok(json!({"enabled": true, "entries": out}))
            }
            "list_vaults" => {
                let s = self.sess()?;
                let mut out = vec![];
                for (vid, meta) in s.list_vaults()? {
                    let n = s.list_records(&vid)?.iter().filter(|(_, _, r)| visible(r)).count();
                    out.push(json!({"id": vid, "name": meta.name, "item_count": n}));
                }
                Ok(json!({"vaults": out}))
            }
            "passkey_query" => {
                let s = self.sess()?;
                crate::passkey::query(s, req)
            }
            "passkey_create" | "passkey_get" => {
                if !crate::passkey::popup_approved(req) { return Err(coded(code::DENIED, format!("{cmd} needs approval in the gitpass popup"))); }
                let s = self.sess()?;
                if cmd == "passkey_create" {
                    let res = crate::passkey::create(s, req);
                    // A new passkey changes Watchtower's passkey-available sites.
                    if res.is_ok() { self.audit_cache = None; }
                    res
                } else { crate::passkey::get(s, req) }
            }
            "item_findings" => {
                let item_id_str = arg_str(req, "item_id")?;
                let (vid, iid) = split_id(item_id_str)?;
                let (_, _, rec) = self.record(item_id_str)?;
                let report = self.get_audit_report()?;
                Ok(crate::protocol::item_findings(vid, iid, &rec, &report))
            }
            "audit_summary" => {
                let report = self.get_audit_report()?;
                let recs = self.all_records(None)?;
                let iter = recs.iter().map(|(v, vn, i, r)| (v.as_str(), vn.as_str(), i.as_str(), r));
                Ok(crate::protocol::audit_summary(&report, iter))
            }
            "site_icon" => {
                let s = self.sess()?;
                let host_arg = arg_str(req, "host")?.trim().to_lowercase();
                if host_arg.is_empty() { return Err(coded(code::BAD_REQUEST, "empty host")); }
                let settings_path = self.cfg.vault_path.join(".gitpass-settings.json");
                let show_icons = std::fs::read_to_string(&settings_path)
                    .ok()
                    .and_then(|str| serde_json::from_str::<serde_json::Value>(&str).ok())
                    .map(|v| {
                        // Desktop gui-core serializes LocalFile as camelCase:
                        // `settings.showWebsiteIcons` (user setting) and top-level `allowFavicons` (dev flag).
                        let nested = v.get("settings")
                            .and_then(|s| s.get("showWebsiteIcons").or_else(|| s.get("show_website_icons")))
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        let allow_favicons = v.get("allowFavicons")
                            .or_else(|| v.get("allow_favicons"))
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        let top_level = v.get("showWebsiteIcons")
                            .or_else(|| v.get("show_website_icons"))
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        nested || allow_favicons || top_level
                    })
                    .unwrap_or(false);
                if !show_icons {
                    return Err(coded(code::DENIED, "website icons are disabled"));
                }
                let mut host_found = false;
                for (vid, _) in s.list_vaults()? {
                    for (_iid, _, rec) in s.list_records(&vid)? {
                        if rec.deleted_at.is_some() || rec.archived { continue; }
                        if let Some(h) = rec.item.url.as_deref().and_then(url_host) {
                            if h.eq_ignore_ascii_case(&host_arg) {
                                host_found = true;
                                break;
                            }
                        }
                        for field in rec.view_sections().into_iter().flat_map(|sec| sec.fields) {
                            if field.kind == gitpass_core::item::FieldKind::Url && field.value.is_some() {
                                if let Some(h) = url_host(field.value.as_ref().unwrap()) {
                                    if h.eq_ignore_ascii_case(&host_arg) {
                                        host_found = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if host_found { break; }
                }
                if !host_found {
                    return Err(coded(code::DENIED, format!("refusing host '{host_arg}' not in any item")));
                }
                let cache_dir = self.favicon_cache_dir.clone().unwrap_or_else(|| {
                    dirs::cache_dir()
                        .unwrap_or_else(std::env::temp_dir)
                        .join("gitpass")
                        .join("favicons")
                });
                // Cache file schema and 30-day TTL are shared with `apps/desktop/gui-core/src/favicon.rs`.
                // Cache file name: <sha256(host)>.json; entry: {"ts": <unix_secs>, "data_url": <string or null>}.
                const FAVICON_TTL_SECS: u64 = 30 * 24 * 60 * 60;
                let host_hash = gitpass_core::store::hash_bytes(host_arg.as_bytes());
                let p = cache_dir.join(format!("{host_hash}.json"));
                if let Ok(bytes) = std::fs::read(&p) {
                    #[derive(serde::Deserialize)]
                    struct CachedFaviconEntry { ts: u64, data_url: Option<String> }
                    if let Ok(entry) = serde_json::from_slice::<CachedFaviconEntry>(&bytes) {
                        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                        if now.saturating_sub(entry.ts) <= FAVICON_TTL_SECS {
                            if let Some(data_url) = entry.data_url {
                                return Ok(json!({ "data_url": data_url }));
                            }
                        }
                    }
                }
                Err(coded(code::NOT_FOUND, "site icon not cached"))
            }
            "open_in_app" => {
                if let Some(v) = req.get("view").and_then(Value::as_str).filter(|v| !v.is_empty()) {
                    if !matches!(v, "generator_history" | "watchtower" | "settings") {
                        return Err(coded(code::BAD_REQUEST, format!("unknown view '{v}'")));
                    }
                }
                crate::agent::launch_app().map_err(|e| coded(code::APP_NOT_RUNNING, format!("could not launch the gitpass app: {e:#}")))?;
                Ok(json!({"launched": true}))
            }
            other => Err(coded(code::BAD_REQUEST, format!("unknown command '{other}'"))),
        }
    }
}


/// Build a record from the desktop app's `ItemInput` JSON shape
/// (`{vaultId?, category, title, sections:[{id,title,fields:[{id,label,kind,value}]}], notes?, url?, tags?}`).
/// Mirrors gui-core `App::create_item` validation: title required, unique non-empty field ids, valid TOTP.
pub fn record_from_input(v: &Value) -> Result<(Option<String>, ItemRecord)> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Input { #[serde(default)] vault_id: Option<String>, category: Category, title: String,
        #[serde(default)] sections: Vec<gitpass_core::item::Section>, #[serde(default)] notes: Option<String>,
        #[serde(default)] url: Option<String>, #[serde(default)] tags: Option<Vec<String>> }
    let i: Input = serde_json::from_value(v.clone()).map_err(|e| coded(code::BAD_REQUEST, format!("bad item: {e}")))?;
    if i.category == Category::Passkey { return Err(coded(code::BAD_REQUEST, "passkeys are created by websites")); }
    if i.category == Category::Other { return Err(coded(code::BAD_REQUEST, "invalid category")); }
    if i.title.trim().is_empty() { return Err(coded(code::BAD_REQUEST, "title is required")); }
    let mut ids = std::collections::BTreeSet::new();
    for s in &i.sections { for f in &s.fields {
        if f.id.trim().is_empty() { return Err(coded(code::BAD_REQUEST, "field id is empty")); }
        if !ids.insert(f.id.clone()) { return Err(coded(code::BAD_REQUEST, format!("duplicate field id '{}'", f.id))); }
        if f.kind == FieldKind::Totp { if let Some(t) = f.value.as_deref().filter(|t| !t.is_empty()) {
            gitpass_core::totp::parse(t).map_err(|e| coded(code::BAD_REQUEST, format!("invalid one-time password: {e}")))?; } }
    } }
    let ne = |o: Option<String>| o.filter(|x| !x.trim().is_empty());
    let mut r = ItemRecord::new(Item::default());
    r.category = i.category;
    r.item.title = i.title.trim().into();
    r.item.notes = ne(i.notes);
    r.item.url = ne(i.url);
    if let Some(t) = i.tags { let mut t: Vec<String> = t.iter().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(); t.dedup(); r.tags = t; }
    r.apply_sections(&i.sections);
    Ok((ne(i.vault_id), r))
}
