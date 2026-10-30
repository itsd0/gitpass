//! Shared extension-protocol logic for `gitpass-host` and `gitpass-gui-core`'s agent.
//!
//! Pure request filtering, shaping and response generation for browser-extension commands:
//! `list_items`, `match`, `get_item`, `fill_card` / `fill_identity`, and `item_findings`.

use crate::handler::{code, coded};
use crate::origin::{origin_matches, parse_request_origin};
use crate::passkey::record_matches_origin;
use anyhow::Result;
use gitpass_core::audit::{Finding, FINDING_KINDS};
use gitpass_core::item::{Category, Field, FieldKind};
use gitpass_core::merge::ItemRecord;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub fn nonempty(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|s| !s.is_empty())
}

pub fn url_host(u: &str) -> Option<String> {
    let u = u.trim();
    let p = if u.contains("://") {
        url::Url::parse(u).ok()?
    } else {
        url::Url::parse(&format!("https://{u}")).ok()?
    };
    p.host_str().map(str::to_string)
}

pub fn is_card_number(f: &Field) -> bool {
    matches!(f.id.as_str(), "number" | "ccnum" | "card_number")
}

pub fn card_digits(v: &str) -> Option<String> {
    let d: String = v.chars().filter(|c| c.is_ascii_digit()).collect();
    (d.len() >= 8).then_some(d)
}

pub fn card_mask(d: &str) -> String {
    format!("•••• •••• •••• {}", &d[d.len() - 4..])
}

/// Preview of a long API secret: first 4 and last 4 characters (`abcd••••wxyz`). Secrets shorter than
/// 12 characters get no preview, so a short token is never mostly revealed.
pub fn secret_preview(v: &str) -> Option<String> {
    let c: Vec<char> = v.chars().collect();
    (c.len() >= 12).then(|| {
        format!(
            "{}••••{}",
            c[..4].iter().collect::<String>(),
            c[c.len() - 4..].iter().collect::<String>()
        )
    })
}

pub fn fields_of(r: &ItemRecord) -> Vec<Field> {
    r.view_sections().into_iter().flat_map(|s| s.fields).collect()
}

pub fn field_val(r: &ItemRecord, ids: &[&str]) -> Option<String> {
    let fs = fields_of(r);
    ids.iter().find_map(|id| {
        fs.iter()
            .find(|f| f.id == *id)
            .and_then(|f| f.value.clone())
            .filter(|v| !v.is_empty())
    })
}

pub fn kind_str(cat: Category, f: &Field) -> &'static str {
    match f.kind {
        FieldKind::Concealed | FieldKind::Text if cat == Category::CreditCard && is_card_number(f) => {
            "card_number"
        }
        FieldKind::Concealed => "password",
        FieldKind::Totp => "totp",
        FieldKind::Url => "url",
        FieldKind::Email => "email",
        FieldKind::Phone => "phone",
        FieldKind::Date | FieldKind::MonthYear => "date",
        FieldKind::Text | FieldKind::Multiline => "text",
    }
}

pub fn has_totp(r: &ItemRecord) -> bool {
    nonempty(&r.item.totp).is_some()
        || fields_of(r)
            .iter()
            .any(|f| f.kind == FieldKind::Totp && f.value.as_deref().is_some_and(|v| !v.is_empty()))
}

pub fn subtitle(r: &ItemRecord) -> String {
    match r.category {
        Category::Login => r.item.username.clone().unwrap_or_default(),
        Category::CreditCard => {
            let n: String = field_val(r, &["number", "ccnum", "card_number"])
                .unwrap_or_default()
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            if n.len() >= 4 {
                let l4 = &n[n.len() - 4..];
                match gitpass_core::item::card_brand(&n) {
                    Some(b) => format!("{b} •••• {l4}"),
                    None => format!("•••• {l4}"),
                }
            } else {
                String::new()
            }
        }
        Category::Identity => field_val(r, &["name", "full_name"]).unwrap_or_else(|| {
            [
                field_val(r, &["first", "first_name", "firstname"]),
                field_val(r, &["last", "last_name", "lastname"]),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ")
        }),
        Category::Wifi => field_val(r, &["ssid"]).unwrap_or_default(),
        Category::BankAccount => {
            let acct = field_val(r, &["account_number", "account_no", "account"]);
            let holder = field_val(r, &["account_holder", "holder", "owner"]);
            let bank = field_val(r, &["bank_name", "bank"]);
            let acct_type = field_val(r, &["account_type", "type"]);
            let l4 = acct
                .as_deref()
                .map(|s| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>())
                .filter(|d| d.len() >= 4)
                .map(|d| format!("•••• {}", &d[d.len() - 4..]));
            match (acct_type, l4, bank, holder) {
                (Some(t), Some(m), _, _) => format!("{t} {m}"),
                (_, Some(m), Some(b), _) => format!("{b} {m}"),
                (_, Some(m), _, _) => m,
                (_, _, Some(b), _) => b,
                (_, _, _, Some(h)) => h,
                _ => String::new(),
            }
        }
        Category::Server => {
            let u = nonempty(&r.item.username).map(String::from).or_else(|| field_val(r, &["username"]));
            let h = nonempty(&r.item.url).map(String::from).or_else(|| field_val(r, &["url", "hostname"]));
            match (u, h) {
                (Some(u), Some(h)) => format!("{u}@{h}"),
                (Some(u), _) => u,
                (_, Some(h)) => h,
                _ => String::new(),
            }
        }
        Category::ApiCredential => gitpass_core::mask::api_credential_secret(r)
            .map(gitpass_core::mask::secret_hint)
            .or_else(|| fields_of(r).into_iter().find(|f| !f.kind.is_secret() && f.value.as_deref().is_some_and(|v| !v.is_empty())).and_then(|f| f.value))
            .unwrap_or_default(),
        _ => fields_of(r)
            .into_iter()
            .find(|f| !f.kind.is_secret() && f.value.as_deref().is_some_and(|v| !v.is_empty()))
            .and_then(|f| f.value)
            .map(|v| v.lines().next().unwrap_or("").chars().take(80).collect())
            .unwrap_or_default(),
    }
}

/// ItemSummary (contract): summary of an item.
pub fn summary(vid: &str, vname: &str, iid: &str, r: &ItemRecord) -> Value {
    let url = nonempty(&r.item.url);
    let mut s = json!({
        "id": format!("{vid}:{iid}"),
        "vault_id": vid,
        "vault_name": vname,
        "category": r.category.as_str(),
        "title": r.item.title,
        "subtitle": subtitle(r),
        "url": url,
        "favicon_host": url.and_then(url_host),
        "has_totp": has_totp(r),
        "has_attachments": !r.attachments.is_empty(),
        "favorite": r.favorite,
        "updated_at": r.item.updated_at,
        "created_at": r.created_at,
        "archived": r.archived,
    });
    if let Some(d) = r.deleted_at {
        s["deleted_at"] = json!(gitpass_core::device::iso8601(d as i64));
    }
    s
}

/// ItemDetail (contract): summary + fields with secrets omitted (`concealed:true`, no value).
pub fn detail(vid: &str, vname: &str, iid: &str, r: &ItemRecord, last_used: Option<u64>) -> Value {
    let mut v = summary(vid, vname, iid, r);
    if let Some(lu) = last_used {
        v["last_used"] = json!(lu);
    }
    let mut fields = vec![];
    if let Some(u) = nonempty(&r.item.url) {
        fields.push(json!({"name": "url", "label": "website", "value": u, "concealed": false, "kind": "url"}));
    }
    for f in fields_of(r) {
        let secret = f.kind.is_secret();
        let k = kind_str(r.category, &f);
        let mut o = json!({"name": f.id, "label": f.label, "concealed": secret, "kind": k});
        if !secret {
            o["value"] = json!(f.value);
        } else if r.category == Category::ApiCredential {
            if let Some(m) = f.value.as_deref().map(gitpass_core::mask::secret_hint) {
                o["masked"] = json!(m);
            }
        } else if k == "card_number" {
            if let Some(d) = f.value.as_deref().and_then(card_digits) {
                o["masked"] = json!(card_mask(&d));
            }
        } else if r.category == Category::BankAccount && (f.id == "account_number" || f.id == "account_no") {
            if let Some(v) = &f.value {
                let d: String = v.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
                if d.len() >= 4 {
                    o["masked"] = json!(format!("•••• •••• •••• {}", &d[d.len() - 4..]));
                }
            }
        }
        fields.push(o);
    }
    let m = v.as_object_mut().unwrap();
    m.insert("fields".into(), json!(fields));
    let attachments: Vec<Value> = r.attachments.iter().map(|a| {
        json!({
            "id": a.id,
            "name": a.name,
            "size": a.size,
            "mime": a.mime,
        })
    }).collect();
    m.insert("attachments".into(), json!(attachments));
    m.insert("notes".into(), json!(nonempty(&r.item.notes)));
    m.insert("tags".into(), json!(r.tags));
    m.insert("created_at".into(), json!(r.created_at));
    if r.category == Category::Login && !r.password_history.is_empty() {
        let history: Vec<Value> = r.password_history.iter().map(|h| {
            json!({
                "replaced_at": h.replaced_at,
            })
        }).collect();
        m.insert("password_history".into(), json!(history));
    }
    v
}

/// Shared pure logic for `list_items` command.
pub fn list_items<'a, I>(
    records: I,
    req: &Value,
    usage: &BTreeMap<String, u64>,
) -> Result<Value>
where
    I: IntoIterator<Item = (&'a str, &'a str, &'a str, &'a ItemRecord)>,
{
    let vault = req.get("vault_id").and_then(Value::as_str).filter(|v| !v.is_empty());
    let cat = req.get("category").and_then(Value::as_str).filter(|c| !c.is_empty());
    let parsed_cat = match cat {
        Some(c) => Some(c.parse::<Category>().map_err(|e| coded(code::BAD_REQUEST, e.to_string()))?),
        None => None,
    };
    let q_raw = req.get("query").and_then(Value::as_str).map(str::trim).filter(|q| !q.is_empty());
    let parsed_q = q_raw.map(gitpass_core::search::parse_query);
    let limit = req.get("limit").and_then(Value::as_u64).map(|n| n as usize);
    let offset = req.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;

    let mut out = vec![];
    for (vid, vname, iid, r) in records {
        if let Some(v) = vault {
            if v != vid {
                continue;
            }
        }
        if let Some(c) = parsed_cat {
            if r.category != c {
                continue;
            }
        }
        if let Some(pq) = &parsed_q {
            let cand = gitpass_core::search::CandidateInfo::from_record(r, Some(vname));
            if !pq.matches(&cand) {
                continue;
            }
        }
        let mut s = summary(vid, vname, iid, r);
        if let Some(lu) = usage.get(iid) {
            s["last_used"] = json!(lu);
        }
        if let Some(pq) = &parsed_q {
            if !pq.terms.is_empty() {
                let term = pq.terms.to_lowercase();
                let hay = format!(
                    "{} {} {}",
                    r.item.title,
                    s["subtitle"].as_str().unwrap_or(""),
                    r.item.url.as_deref().unwrap_or("")
                )
                .to_lowercase();
                if !hay.contains(term.as_str()) {
                    continue;
                }
            }
        }
        out.push(s);
    }
    out.sort_by(|a, b| {
        a["title"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .cmp(&b["title"].as_str().unwrap_or("").to_lowercase())
    });
    let total = out.len();
    let out: Vec<Value> = out.into_iter().skip(offset).take(limit.unwrap_or(usize::MAX)).collect();
    let end = offset.saturating_add(out.len());
    let next_offset = (end < total).then_some(end);
    Ok(json!({"items": out, "total": total, "next_offset": next_offset}))
}

/// Shared pure logic for `list_archived` command.
pub fn list_archived<'a, I>(
    records: I,
    vault: Option<&str>,
    usage: &BTreeMap<String, u64>,
) -> Value
where
    I: IntoIterator<Item = (&'a str, &'a str, &'a str, &'a ItemRecord)>,
{
    let mut items: Vec<(Value, String)> = records
        .into_iter()
        .filter(|(vid, _, _, _)| vault.as_ref().is_none_or(|v| v == vid))
        .map(|(vid, vname, iid, r)| {
            let mut s = summary(vid, vname, iid, r);
            if let Some(lu) = usage.get(iid) {
                s["last_used"] = json!(lu);
            }
            let title = r.item.title.to_lowercase();
            (s, title)
        })
        .collect();
    items.sort_by(|a, b| a.1.cmp(&b.1));
    json!({ "items": items.into_iter().map(|x| x.0).collect::<Vec<_>>() })
}

/// Shared pure logic for `list_deleted` command.
pub fn list_deleted<'a, I>(
    records: I,
    vault: Option<&str>,
) -> Value
where
    I: IntoIterator<Item = (&'a str, &'a str, &'a str, &'a ItemRecord)>,
{
    let mut items: Vec<Value> = records
        .into_iter()
        .filter(|(vid, _, _, _)| vault.as_ref().is_none_or(|v| v == vid))
        .map(|(vid, vname, iid, r)| summary(vid, vname, iid, r))
        .collect();
    items.sort_by(|a, b| {
        let da = a.get("deleted_at").and_then(|v| v.as_str()).unwrap_or("");
        let db = b.get("deleted_at").and_then(|v| v.as_str()).unwrap_or("");
        db.cmp(da).then_with(|| {
            a["title"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .cmp(&b["title"].as_str().unwrap_or("").to_lowercase())
        })
    });
    json!({ "items": items })
}

/// Validate a requesting page origin (https, or http on loopback). Used in both proxy and fallback mode.
pub fn check_request_origin(origin: &str) -> Result<()> {
    if parse_request_origin(origin).is_none() {
        return Err(coded(
            code::ORIGIN_MISMATCH,
            format!("refused: origin must be https (or http://localhost): {origin}"),
        ));
    }
    Ok(())
}

/// Shared pure logic for `match` command.
pub fn match_items<'a, I>(
    records: I,
    req: &Value,
    usage: &BTreeMap<String, u64>,
) -> Result<Value>
where
    I: IntoIterator<Item = (&'a str, &'a str, &'a str, &'a ItemRecord)>,
{
    let origin = req
        .get("origin")
        .or_else(|| req.get("url"))
        .and_then(Value::as_str)
        .ok_or_else(|| coded(code::BAD_REQUEST, "missing string argument 'origin'"))?;
    check_request_origin(origin)?;
    let passkeys = req.get("include_passkeys").and_then(Value::as_bool).unwrap_or(false);
    let matches_url = |u: &str| origin_matches(u, origin);
    let mut out = vec![];
    for (vid, vname, iid, r) in records {
        if r.deleted_at.is_some() || r.archived {
            continue;
        }
        let login = r.category == Category::Login
            && (r.item.url.as_deref().is_some_and(matches_url)
                || r.sections.iter().flat_map(|s| &s.fields).any(|f| f.kind == FieldKind::Url && f.value.as_deref().is_some_and(matches_url)));
        if login || (passkeys && record_matches_origin(r, origin)) {
            let mut s = summary(vid, vname, iid, r);
            if let Some(lu) = usage.get(iid) {
                s["last_used"] = json!(lu);
            }
            out.push(s);
        }
    }
    Ok(json!({"items": out}))
}

/// Shared pure logic for `get_item` command.
pub fn get_item(
    vid: &str,
    vname: &str,
    iid: &str,
    r: &ItemRecord,
    last_used: Option<u64>,
) -> Value {
    let d = detail(vid, vname, iid, r, last_used);
    json!({ "item": d })
}

/// Shared pure logic for `fill_card` and `fill_identity` commands.
pub fn fill_card_or_identity(r: &ItemRecord, is_card: bool) -> Result<Value> {
    let want = if is_card { Category::CreditCard } else { Category::Identity };
    if r.category != want {
        return Err(coded(code::BAD_REQUEST, format!("item is a {}, not a {}", r.category.as_str(), want.as_str())));
    }
    let mut m = Map::new();
    for f in fields_of(r) {
        if let Some(v) = f.value.filter(|v| !v.is_empty()) {
            m.entry(f.id).or_insert(json!(v));
        }
    }
    Ok(Value::Object(m))
}

/// Separators the generator offers for word passphrases; anything else is a bad request.
pub const WORD_SEPARATORS: [char; 4] = ['-', '.', '_', ' '];

/// Shared pure logic for `generate` command.
pub fn generate(req: &Value) -> Result<Value> {
    use gitpass_core::generate::{Mode, Request};
    let flag = |k: &str, default: bool| req.get(k).and_then(Value::as_bool).unwrap_or(default);
    let (words, pin) = (flag("words", false), flag("pin", false));
    let separator = match req.get("separator") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            let mut chars = s.chars();
            let c = chars.next();
            if c.is_none() || chars.next().is_some() || !WORD_SEPARATORS.contains(&c.unwrap()) {
                return Err(coded(code::BAD_REQUEST, format!("invalid word separator {s:?}: use one of -, ., _, space")));
            }
            c
        }
        Some(_) => return Err(coded(code::BAD_REQUEST, "separator must be a string")),
    };
    let capitalize = flag("capitalize", false);
    let r = Request {
        mode: if pin { Mode::Pin } else if words { Mode::Words } else { Mode::Random },
        length: req.get("len").and_then(Value::as_u64).map(|v| v as usize),
        symbols: flag("symbols", true),
        digits: flag("digits", true),
        separator,
        capitalize,
    };
    if let Some(rules) = req.get("rules").filter(|r| !r.is_null()) {
        if words || pin {
            return Err(coded(code::BAD_REQUEST, "rules apply to random passwords only"));
        }
        let len = gitpass_core::generate::Options::for_request(&r)
            .map_err(|m| coded(code::BAD_REQUEST, m))?
            .length;
        return Ok(json!({"password": crate::handler::generate_for_rules(len, rules)?}));
    }
    let opts = gitpass_core::generate::Options::for_request(&r).map_err(|m| coded(code::BAD_REQUEST, m))?;
    Ok(json!({"password": gitpass_core::generate::generate(&opts)}))
}

/// Shared pure logic for `item_findings` command.
pub fn item_findings(
    vid: &str,
    iid: &str,
    r: &ItemRecord,
    report: &gitpass_core::audit::AuditReport,
) -> Value {
    let item_finding = report.per_item.iter().find(|f| f.vault == vid && f.id == iid);
    let change_pw_url = r.item.url.as_deref().and_then(gitpass_core::audit::change_password_url);
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
    out
}

/// Shared pure logic for `audit_summary` command.
pub fn audit_summary<'a, I>(
    report: &gitpass_core::audit::AuditReport,
    records: I,
) -> Value
where
    I: IntoIterator<Item = (&'a str, &'a str, &'a str, &'a ItemRecord)>,
{
    let recs: Vec<(&'a str, &'a str, &'a str, &'a ItemRecord)> = records.into_iter().collect();
    let with_pw = recs.iter().filter(|(_, _, _, r)| r.item.password.as_deref().is_some_and(|p| !p.is_empty())).count();

    let mut bad: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut old_count = 0;
    let mut missing_totp_count = 0;
    let mut reused_old_only_count = 0;

    for f in &report.per_item {
        let full_id = format!("{}:{}", f.vault, f.id);
        let mut has_bad = false;
        let mut has_old = false;
        let mut has_totp = false;
        let mut has_reused_old = false;
        for x in &f.findings {
            match x {
                Finding::Weak { .. } | Finding::Reused { .. } | Finding::Pwned { .. } => has_bad = true,
                Finding::ReusedOldPassword { .. } => has_reused_old = true,
                Finding::Old { .. } => has_old = true,
                Finding::MissingTotp | Finding::TotpAvailable { .. } => has_totp = true,
                _ => {}
            }
        }
        if has_bad { bad.insert(full_id); }
        if has_old { old_count += 1; }
        if has_totp { missing_totp_count += 1; }
        // Half the live-reuse weight, and only when the item is not already counted in `bad`.
        if has_reused_old && !has_bad { reused_old_only_count += 1; }
    }

    let score = if with_pw > 0 {
        let penalty = bad.len() as f64 / with_pw as f64 * 80.0
            + reused_old_only_count as f64 / with_pw as f64 * 40.0
            + old_count as f64 / with_pw as f64 * 10.0
            + missing_totp_count as f64 / report.items_checked.max(1) as f64 * 10.0;
        (100.0 - penalty).round().clamp(0.0, 100.0) as u8
    } else {
        100
    };

    let rec_map: std::collections::HashMap<(&str, &str), &'a ItemRecord> = recs
        .iter()
        .map(|(v, _, i, r)| ((*v, *i), *r))
        .collect();

    let mut findings_out = vec![];
    for kind in FINDING_KINDS {
        let mut kind_items = vec![];
        let mut seen = std::collections::BTreeSet::new();
        for f in &report.per_item {
            if f.findings.iter().any(|x| x.kind() == *kind) {
                let full_id = format!("{}:{}", f.vault, f.id);
                if seen.insert(full_id.clone()) {
                    let (username, url, category) = if let Some(r) = rec_map.get(&(f.vault.as_str(), f.id.as_str())) {
                        (
                            r.item.username.clone().unwrap_or_default(),
                            r.item.url.clone().unwrap_or_default(),
                            r.category.as_str(),
                        )
                    } else {
                        (String::new(), String::new(), "login")
                    };
                    let title = if f.title.is_empty() {
                        rec_map.get(&(f.vault.as_str(), f.id.as_str())).map(|r| r.item.title.as_str()).unwrap_or("").to_string()
                    } else {
                        f.title.clone()
                    };
                    kind_items.push(json!({
                        "id": full_id,
                        "title": title,
                        "username": username,
                        "url": url,
                        "category": category,
                    }));
                }
            }
        }
        if !kind_items.is_empty() {
            findings_out.push(json!({
                "kind": kind,
                "count": kind_items.len(),
                "items": kind_items,
            }));
        }
    }

    json!({
        "score": score,
        "items_checked": report.items_checked,
        "findings": findings_out,
    })
}
