//! Standalone (fallback) command handling, used when the gitpass desktop app is not running.
//! Implements the v0.6 contract commands directly with gitpass-core (format v5 items).
//! Requests: `{"id":<any>,"cmd":"<name>", ...args}`.
//! Responses: `{"id":<same>,"ok":true,"result":{...}}` or
//! `{"id":<same>,"ok":false,"error":"<code>","message":"..."}`.
use crate::origin::{origin_matches, parse_request_origin};
use anyhow::Result;
use gitpass_core::crypto::{KdfParams, SecretKey};
use gitpass_core::item::{Category, Field, FieldKind, Section};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{self, Item, Session};
use gitpass_core::audit::Finding;
use std::sync::Arc;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};
use zeroize::Zeroizing;

/// Contract error codes.
pub mod code {
    pub const LOCKED: &str = "locked";
    pub const DENIED: &str = "denied";
    pub const NOT_FOUND: &str = "not_found";
    pub const ORIGIN_MISMATCH: &str = "origin_mismatch";
    pub const BAD_REQUEST: &str = "bad_request";
    pub const APP_NOT_RUNNING: &str = "app_not_running";
    pub const INTERNAL: &str = "internal";
}

/// An error carrying a contract error code.
#[derive(Debug)]
pub struct CodedError { pub code: &'static str, pub message: String }
impl std::fmt::Display for CodedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}
impl std::error::Error for CodedError {}
pub fn coded(code: &'static str, message: impl Into<String>) -> anyhow::Error { CodedError { code, message: message.into() }.into() }

/// Build an error response frame.
pub fn error_frame(id: Value, code: &str, message: &str) -> Value {
    json!({"id": id, "ok": false, "error": code, "message": message})
}

/// Validate a requesting page origin (https, or http on loopback). Used in both proxy and fallback mode.
pub fn check_request_origin(origin: &str) -> Result<()> {
    if parse_request_origin(origin).is_none() {
        return Err(coded(code::ORIGIN_MISMATCH, format!("refused: origin must be https (or http://localhost): {origin}")));
    }
    Ok(())
}

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
}

fn arg_str<'a>(req: &'a Value, k: &str) -> Result<&'a str> {
    req.get(k).and_then(Value::as_str).ok_or_else(|| coded(code::BAD_REQUEST, format!("missing string argument '{k}'")))
}

fn split_id(id: &str) -> Result<(&str, &str)> {
    id.split_once(':').filter(|(v, i)| !v.is_empty() && !i.is_empty())
        .ok_or_else(|| coded(code::BAD_REQUEST, "bad item_id (expected <vault>:<item>)"))
}

fn nonempty(v: &Option<String>) -> Option<&str> { v.as_deref().filter(|s| !s.is_empty()) }

fn url_host(u: &str) -> Option<String> {
    let u = u.trim();
    let p = if u.contains("://") { url::Url::parse(u).ok()? } else { url::Url::parse(&format!("https://{u}")).ok()? };
    p.host_str().map(str::to_string)
}

/// Contract field kind for a gitpass-core field.
fn kind_str(cat: Category, f: &Field) -> &'static str {
    match f.kind {
        FieldKind::Concealed if cat == Category::CreditCard && is_card_number(f) => "card_number",
        FieldKind::Text if cat == Category::CreditCard && is_card_number(f) => "card_number",
        FieldKind::Concealed => "password",
        FieldKind::Totp => "totp",
        FieldKind::Url => "url",
        FieldKind::Email => "email",
        FieldKind::Phone => "phone",
        FieldKind::Date | FieldKind::MonthYear => "date",
        FieldKind::Text | FieldKind::Multiline => "text",
    }
}
fn is_card_number(f: &Field) -> bool { matches!(f.id.as_str(), "number" | "ccnum" | "card_number") }

/// Preview of a long API secret: first 4 and last 4 characters (`abcd••••wxyz`). Secrets shorter than
/// 12 characters get no preview, so a short token is never mostly revealed.
fn secret_preview(v: &str) -> Option<String> {
    let c: Vec<char> = v.chars().collect();
    (c.len() >= 12).then(|| format!("{}••••{}", c[..4].iter().collect::<String>(), c[c.len() - 4..].iter().collect::<String>()))
}

fn fields_of(r: &ItemRecord) -> Vec<Field> { r.view_sections().into_iter().flat_map(|s| s.fields).collect() }

fn field_val(r: &ItemRecord, ids: &[&str]) -> Option<String> {
    let fs = fields_of(r);
    ids.iter().find_map(|id| fs.iter().find(|f| f.id == *id).and_then(|f| f.value.clone()).filter(|v| !v.is_empty()))
}

fn subtitle(r: &ItemRecord) -> String {
    match r.category {
        Category::Login => r.item.username.clone().unwrap_or_default(),
        Category::CreditCard => {
            let n: String = field_val(r, &["number", "ccnum", "card_number"]).unwrap_or_default().chars().filter(|c| c.is_ascii_digit()).collect();
            if n.len() >= 4 { format!("•••• {}", &n[n.len() - 4..]) } else { String::new() }
        }
        Category::Identity => field_val(r, &["name", "full_name"]).unwrap_or_else(|| {
            [field_val(r, &["first", "first_name", "firstname"]), field_val(r, &["last", "last_name", "lastname"])]
                .into_iter().flatten().collect::<Vec<_>>().join(" ")
        }),
        Category::Wifi => field_val(r, &["ssid"]).unwrap_or_default(),
        _ => fields_of(r).into_iter().find(|f| !f.kind.is_secret() && f.value.as_deref().is_some_and(|v| !v.is_empty()))
            .and_then(|f| f.value).map(|v| v.lines().next().unwrap_or("").chars().take(80).collect()).unwrap_or_default(),
    }
}

fn has_totp(r: &ItemRecord) -> bool {
    nonempty(&r.item.totp).is_some() || fields_of(r).iter().any(|f| f.kind == FieldKind::Totp && f.value.as_deref().is_some_and(|v| !v.is_empty()))
}

/// ItemSummary (contract).
pub fn summary(vid: &str, vname: &str, iid: &str, r: &ItemRecord) -> Value {
    let url = nonempty(&r.item.url);
    json!({
        "id": format!("{vid}:{iid}"), "vault_id": vid, "vault_name": vname,
        "category": r.category.as_str(), "title": r.item.title, "subtitle": subtitle(r),
        "url": url, "favicon_host": url.and_then(url_host), "has_totp": has_totp(r),
        "favorite": r.favorite, "updated_at": r.item.updated_at, "created_at": r.created_at,
    })
}

/// ItemDetail (contract): summary + fields with secrets omitted (`concealed:true`, no value).
pub fn detail(vid: &str, vname: &str, iid: &str, r: &ItemRecord) -> Value {
    let mut v = summary(vid, vname, iid, r);
    let mut fields = vec![];
    if let Some(u) = nonempty(&r.item.url) {
        fields.push(json!({"name": "url", "label": "website", "value": u, "concealed": false, "kind": "url"}));
    }
    for f in fields_of(r) {
        let secret = f.kind.is_secret();
        let mut o = json!({"name": f.id, "label": f.label, "concealed": secret, "kind": kind_str(r.category, &f)});
        if !secret { o["value"] = json!(f.value); }
        else if r.category == Category::ApiCredential {
            if let Some(m) = f.value.as_deref().and_then(secret_preview) { o["masked"] = json!(m); }
        }
        fields.push(o);
    }
    let m = v.as_object_mut().unwrap();
    m.insert("fields".into(), json!(fields));
    m.insert("notes".into(), json!(nonempty(&r.item.notes)));
    m.insert("tags".into(), json!(r.tags));
    m.insert("created_at".into(), json!(r.created_at));
    v
}

fn visible(r: &ItemRecord) -> bool { r.deleted_at.is_none() && !r.archived }

/// Password generator: characters (`digits`, `symbols`), words (`words`) or a PIN (`pin`).
/// Delegates to gitpass-core so the host, CLI and desktop app produce the same shapes.
pub fn generate(len: usize, digits: bool, symbols: bool, words: bool, pin: bool) -> String {
    gitpass_core::generate::generate(&gitpass_core::generate::Options {
        length: len, symbols, digits, words, pin, separator: '-', capitalize: false,
    })
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
    pub fn new(cfg: HostConfig) -> Self { Host { cfg, session: None, last_activity: Instant::now(), last_activity_wall: SystemTime::now(), audit_cache: None } }

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
            Err(e) => match e.downcast_ref::<CodedError>() {
                Some(c) => error_frame(id, c.code, &c.message),
                None => error_frame(id, code::INTERNAL, &format!("{e:#}")),
            },
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
                // Verify-only unlock: keeps the vault's KDF params and writes nothing on unlock.
                let s = Session::reopen_verify(root, &pw, &sk, false).map_err(|e| coded(code::DENIED, format!("{e:#}")))?;
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
                let cat = req.get("category").and_then(Value::as_str);
                if let Some(c) = cat { c.parse::<Category>().map_err(|e| coded(code::BAD_REQUEST, e.to_string()))?; }
                let q = req.get("query").and_then(Value::as_str).map(str::to_lowercase).filter(|q| !q.is_empty());
                // No cap unless the caller asks for one: the popup lists every item (a silent default cap dropped items past the 500th title).
                let limit = req.get("limit").and_then(Value::as_u64).map(|n| n as usize);
                // Paging: a reply is capped at 1 MiB (framing::MAX_OUT), so big vaults are fetched in pages via `offset`.
                let offset = req.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
                let usage = self.sess()?.all_last_used();
                let mut out = vec![];
                for (vid, vname, iid, r) in self.all_records(vault)? {
                    if cat.is_some_and(|c| c != r.category.as_str()) { continue; }
                    let mut s = summary(&vid, &vname, &iid, &r);
                    if let Some(lu) = usage.get(&iid) { s["last_used"] = json!(lu); }
                    if let Some(q) = &q {
                        let hay = format!("{} {} {}", r.item.title, s["subtitle"].as_str().unwrap_or(""), r.item.url.as_deref().unwrap_or("")).to_lowercase();
                        if !hay.contains(q.as_str()) { continue; }
                    }
                    out.push(s);
                }
                out.sort_by(|a, b| a["title"].as_str().unwrap_or("").to_lowercase().cmp(&b["title"].as_str().unwrap_or("").to_lowercase()));
                let total = out.len();
                let out: Vec<Value> = out.into_iter().skip(offset).take(limit.unwrap_or(usize::MAX)).collect();
                let end = offset.saturating_add(out.len());
                let next_offset = (end < total).then_some(end);
                Ok(json!({"items": out, "total": total, "next_offset": next_offset}))
            }
            "match" => {
                // `url` accepted for 0.5 extensions.
                let origin = req.get("origin").or_else(|| req.get("url")).and_then(Value::as_str)
                    .ok_or_else(|| coded(code::BAD_REQUEST, "missing string argument 'origin'"))?;
                check_request_origin(origin)?;
                let usage = self.sess()?.all_last_used();
                let matches_url = |u: &str| origin_matches(u, origin);
                let mut out = vec![];
                for (vid, vname, iid, r) in self.all_records(None)? {
                    if r.category == Category::Login
                        && (r.item.url.as_deref().is_some_and(matches_url)
                            || r.sections.iter().flat_map(|s| &s.fields).any(|f| f.kind == FieldKind::Url && f.value.as_deref().is_some_and(matches_url)))
                    {
                        let mut s = summary(&vid, &vname, &iid, &r);
                        if let Some(lu) = usage.get(&iid) { s["last_used"] = json!(lu); }
                        out.push(s);
                    }
                }
                Ok(json!({"items": out}))
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
                let (vid, iid, r) = self.record(arg_str(req, "item_id")?)?;
                let mut d = detail(&vid, &self.vault_name(&vid)?, &iid, &r);
                if let Some(lu) = self.sess()?.last_used(&iid) {
                    d["last_used"] = json!(lu);
                }
                Ok(json!({"item": d}))
            }
            "reveal" => {
                if req.get("source").and_then(Value::as_str) != Some("popup") {
                    return Err(coded(code::DENIED, "reveal is allowed only from the popup"));
                }
                let (_, iid, r) = self.record(arg_str(req, "item_id")?)?;
                let field = arg_str(req, "field")?;
                let value = match field {
                    "notes" => r.item.notes.clone(),
                    "url" => r.item.url.clone(),
                    _ => r.find_field(field).and_then(|f| f.value),
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
                let want = if cmd == "fill_card" { Category::CreditCard } else { Category::Identity };
                let (_, iid, r) = self.record(arg_str(req, "item_id")?)?;
                if r.category != want { return Err(coded(code::BAD_REQUEST, format!("item is a {}, not a {}", r.category.as_str(), want.as_str()))); }
                let _ = self.sess()?.touch_item_used(&iid);
                let mut m = Map::new();
                for f in fields_of(&r) {
                    if let Some(v) = f.value.filter(|v| !v.is_empty()) { m.entry(f.id).or_insert(json!(v)); }
                }
                Ok(Value::Object(m))
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
            "generate" => {
                use gitpass_core::generate::{MAX_LENGTH, MAX_PIN, MAX_WORDS, MIN_PIN, MIN_WORDS};
                let words = req.get("words").and_then(Value::as_bool).unwrap_or(false);
                let pin = req.get("pin").and_then(Value::as_bool).unwrap_or(false);
                let (min, max, default) = if pin { (MIN_PIN, MAX_PIN, 6) } else if words { (MIN_WORDS, MAX_WORDS, 5) } else { (8, MAX_LENGTH, 20) };
                let len = req.get("len").and_then(Value::as_u64).map_or(default, |v| v as usize);
                if !(min..=max).contains(&len) { return Err(coded(code::BAD_REQUEST, format!("len must be {min}..={max}"))); }
                let digits = req.get("digits").and_then(Value::as_bool).unwrap_or(true);
                let symbols = req.get("symbols").and_then(Value::as_bool).unwrap_or(true);
                if let Some(rules) = req.get("rules").filter(|r| !r.is_null()) {
                    if words || pin { return Err(coded(code::BAD_REQUEST, "rules apply to random passwords only")); }
                    return Ok(json!({"password": generate_for_rules(len, rules)?}));
                }
                Ok(json!({"password": generate(len, digits, symbols, words, pin)}))
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
                let item_finding = report.per_item.iter().find(|f| f.vault == vid && f.id == iid);
                let change_pw_url = rec.item.url.as_deref().and_then(gitpass_core::audit::change_password_url);
                let mut kinds = vec![];
                let mut reused_count = None;
                let mut pwned_count = None;
                let mut old_days = None;
                let mut expiring_days = None;
                let mut insecure_url_host = None;

                if let Some(f) = item_finding {
                    for finding in &f.findings {
                        kinds.push(finding.kind().to_string());
                        match finding {
                            Finding::Reused { others, .. } => reused_count = Some(others + 1),
                            Finding::Pwned { count } => pwned_count = Some(*count),
                            Finding::Old { days } => old_days = Some(*days),
                            Finding::ExpiringSoon { days, .. } => expiring_days = Some(*days),
                            Finding::InsecureUrl { url_host } => insecure_url_host = Some(url_host.clone()),
                            _ => {}
                        }
                    }
                }
                let mut out = json!({ "kinds": kinds });
                if let Some(c) = reused_count { out["reused_count"] = json!(c); }
                if let Some(c) = pwned_count { out["pwned_count"] = json!(c); }
                if let Some(d) = old_days { out["old_days"] = json!(d); }
                if let Some(d) = expiring_days { out["expiring_days"] = json!(d); }
                if let Some(h) = insecure_url_host { out["insecure_url_host"] = json!(h); }
                if let Some(u) = change_pw_url { out["change_password_url"] = json!(u); }
                Ok(out)
            }
            "open_in_app" => {
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
