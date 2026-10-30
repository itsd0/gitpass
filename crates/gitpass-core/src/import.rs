//! Data import / export.
//!
//! Importers parse vendor export files into [`ItemRecord`]s (category, typed sections, tags,
//! favorite/archived) and [`import_items`] writes them through the public [`Session`] API.
//! Column lookup in CSV formats is case-insensitive and accepts several aliases per field, because
//! vendors have changed their header names across versions. Any non-empty column that is not mapped
//! is appended to `notes` as `Column: value` so nothing is silently dropped. Folders / groups /
//! vendor vaults become tags (gitpass imports everything into the one vault picked by the user).
use crate::crypto::{open, random_bytes, seal, Envelope, KdfParams, Key32};
use crate::item::{Category, Field, FieldKind, Section};
use crate::merge::ItemRecord;
use crate::store::{now, Item, Session};
use aes::Aes256;
use anyhow::{anyhow, bail, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::collections::{BTreeMap, HashSet};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat { BitwardenJson, BitwardenCsv, GenericCsv, Pux, OnePif, KeepassCsv, ChromeCsv, ApplePasswordsCsv, GitpassJson, LastpassCsv, ProtonPassCsv }

impl std::str::FromStr for ImportFormat {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "bitwarden-json" => Self::BitwardenJson,
            "bitwarden-csv" => Self::BitwardenCsv,
            "generic-csv" | "1password-csv" => Self::GenericCsv,
            "1pux" | "1password-1pux" => Self::Pux,
            "1pif" | "onepif" => Self::OnePif,
            "keepass-csv" | "keepassxc-csv" => Self::KeepassCsv,
            "chrome-csv" => Self::ChromeCsv,
            "apple-passwords-csv" | "apple-passwords" | "apple-csv" | "apple" | "safari-csv" | "safari" => Self::ApplePasswordsCsv,
            "gitpass-json" => Self::GitpassJson,
            "lastpass-csv" | "lastpass" => Self::LastpassCsv,
            "protonpass-csv" | "proton-pass-csv" | "protonpass" | "proton-pass" => Self::ProtonPassCsv,
            o => bail!("unknown import format '{o}' (bitwarden-json, bitwarden-csv, generic-csv, 1pux, 1pif, keepass-csv, chrome-csv, apple-passwords-csv, gitpass-json, lastpass-csv, protonpass-csv)"),
        })
    }
}

/// Result of parsing a file: records plus human-readable warnings (skipped rows, unknown columns...).
#[derive(Default)]
pub struct Parsed { pub items: Vec<ItemRecord>, pub warnings: Vec<String> }

/// Parse text `data` in `format`. `passphrase` is needed for encrypted gitpass-json exports and for
/// password-protected Bitwarden JSON exports (it is the export password there).
/// (.1pux is a zip archive: use [`parse_bytes`].)
pub fn parse(format: ImportFormat, data: &str, passphrase: Option<&str>) -> Result<Parsed> {
    let data = data.strip_prefix('\u{feff}').unwrap_or(data); // tolerate UTF-8 BOM (Excel / other password managers)
    match format {
        ImportFormat::BitwardenJson => parse_bitwarden_json(data, passphrase),
        ImportFormat::GitpassJson => parse_gitpass_json(data, passphrase),
        ImportFormat::BitwardenCsv => parse_csv(data, &BITWARDEN_CSV),
        ImportFormat::GenericCsv => parse_csv(data, &GENERIC_CSV),
        ImportFormat::KeepassCsv => parse_csv(data, &KEEPASS_CSV),
        ImportFormat::ChromeCsv => parse_csv(data, &CHROME_CSV),
        ImportFormat::ApplePasswordsCsv => parse_csv(data, &APPLE_PASSWORDS_CSV),
        ImportFormat::Pux => parse_1pux_json(data),
        ImportFormat::OnePif => parse_onepif(data),
        ImportFormat::LastpassCsv => parse_csv(data, &LASTPASS_CSV),
        ImportFormat::ProtonPassCsv => parse_csv(data, &PROTONPASS_CSV),
    }
}

/// Parse raw file bytes (handles the .1pux zip container; other formats must be UTF-8 text).
pub fn parse_bytes(format: ImportFormat, data: &[u8], passphrase: Option<&str>) -> Result<Parsed> {
    if format == ImportFormat::Pux { return parse_1pux(data); }
    let s = std::str::from_utf8(data).map_err(|_| anyhow!("file is not UTF-8 text; is this the right format?"))?;
    parse(format, s, passphrase)
}

/// If `path` is a directory containing `data.1pif`, resolve to `data.1pif`, otherwise keep `path`.
pub fn resolve_import_path(path: &std::path::Path) -> std::path::PathBuf {
    if path.is_dir() && path.join("data.1pif").exists() {
        path.join("data.1pif")
    } else {
        path.to_path_buf()
    }
}

fn opt(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|v| !v.is_empty()).map(String::from)
}

fn append_note(notes: &mut Option<String>, line: String) {
    match notes { Some(n) => { n.push('\n'); n.push_str(&line); } None => *notes = Some(line) }
}

fn fld(id: &str, label: &str, kind: FieldKind, v: Option<String>) -> Option<Field> {
    v.filter(|x| !x.trim().is_empty()).map(|value| Field { id: id.into(), label: label.into(), kind, value: Some(value), removed: false })
}

fn push_section(rec: &mut ItemRecord, id: &str, title: Option<&str>, fields: Vec<Option<Field>>) {
    let fields: Vec<Field> = fields.into_iter().flatten().collect();
    if fields.is_empty() { return; }
    if let Some(s) = rec.sections.iter_mut().find(|s| s.id == id) { s.fields.extend(fields); return; }
    rec.sections.push(Section { id: id.into(), title: title.map(String::from), fields });
}

fn add_tag(rec: &mut ItemRecord, t: &str) {
    let t = t.trim();
    if !t.is_empty() && !rec.tags.iter().any(|x| x == t) { rec.tags.push(t.to_string()); }
}

fn truthy(v: &str) -> bool { matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "y") }

fn record(title: String, category: Category) -> ItemRecord {
    let mut it = Item::default(); it.title = title; it.updated_at = now();
    let mut r = ItemRecord::new(it);
    r.category = category;
    r
}

/// Unique, stable field id for custom fields ("f1", "f2", ...) within a record.
fn next_id(rec: &ItemRecord, prefix: &str) -> String {
    let n = rec.sections.iter().flat_map(|s| s.fields.iter()).filter(|f| f.id.starts_with(prefix)).count() + 1;
    format!("{prefix}{n}")
}

// ---------------------------------------------------------------- CSV

/// Column aliases (lower-case) per target field. Columns listed in `ignore` are dropped silently.
struct CsvSpec {
    name: &'static str,
    title: &'static [&'static str],
    username: &'static [&'static str],
    email: &'static [&'static str],
    password: &'static [&'static str],
    url: &'static [&'static str],
    notes: &'static [&'static str],
    totp: &'static [&'static str],
    created: &'static [&'static str],
    modified: &'static [&'static str],
    /// Folder / group / tags columns -> tags (split on `tag_sep` when non-empty).
    tags: &'static [&'static str],
    tag_sep: &'static [char],
    favorite: &'static [&'static str],
    archived: &'static [&'static str],
    ignore: &'static [&'static str],
    /// (column, value) — rows whose column equals value are not logins and are imported as secure notes.
    note_type: Option<(&'static str, &'static str)>,
    category: &'static [&'static str],
}

// Bitwarden: folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp
const BITWARDEN_CSV: CsvSpec = CsvSpec {
    name: "bitwarden-csv",
    title: &["name"], username: &["login_username"], email: &[], password: &["login_password"],
    url: &["login_uri"], notes: &["notes"], totp: &["login_totp"], created: &[], modified: &[],
    tags: &["folder"], tag_sep: &[], favorite: &["favorite"], archived: &[],
    ignore: &["reprompt", "type"], note_type: Some(("type", "note")), category: &["type"],
};
// Generic CSV: Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes
const GENERIC_CSV: CsvSpec = CsvSpec {
    name: "generic-csv",
    title: &["title", "name"], username: &["username", "user name", "login"], email: &[], password: &["password"],
    url: &["url", "website", "urls", "location"], notes: &["notes", "notesplain", "note"],
    totp: &["otpauth", "one-time password", "totp", "otp"], created: &[], modified: &[],
    tags: &["tags"], tag_sep: &[',', ';'], favorite: &["favorite"], archived: &["archived"],
    ignore: &[], note_type: None, category: &["type", "category"],
};
// KeePassXC: "Group","Title","Username","Password","URL","Notes","TOTP","Icon","Last Modified","Created"
// KeePass 2 (generic CSV): "Account","Login Name","Password","Web Site","Comments"
const KEEPASS_CSV: CsvSpec = CsvSpec {
    name: "keepass-csv",
    title: &["title", "account"], username: &["username", "login name", "user name"], email: &[], password: &["password"],
    url: &["url", "web site"], notes: &["notes", "comments"], totp: &["totp", "otp"],
    created: &[], modified: &["last modified"], tags: &["group"], tag_sep: &[], favorite: &[], archived: &[],
    ignore: &["icon", "created"], note_type: None, category: &[],
};
// Chrome / Edge / Brave: name,url,username,password[,note]
const CHROME_CSV: CsvSpec = CsvSpec {
    name: "chrome-csv",
    title: &["name", "origin"], username: &["username"], email: &[], password: &["password"],
    url: &["url", "origin_url"], notes: &["note", "notes"], totp: &[], created: &[], modified: &[],
    tags: &[], tag_sep: &[], favorite: &[], archived: &[], ignore: &[], note_type: None, category: &[],
};
// LastPass: url,username,password,totp,extra,name,grouping,fav
const LASTPASS_CSV: CsvSpec = CsvSpec {
    name: "lastpass-csv",
    title: &["name"], username: &["username"], email: &[], password: &["password"],
    url: &["url"], notes: &["extra", "notes", "note"], totp: &["totp"], created: &[], modified: &[],
    tags: &["grouping"], tag_sep: &[], favorite: &["fav", "favorite"], archived: &[],
    ignore: &[], note_type: None, category: &[],
};
// Proton Pass: type,name,url,email,username,password,note,totp,createTime,modifyTime,vault
const PROTONPASS_CSV: CsvSpec = CsvSpec {
    name: "protonpass-csv",
    title: &["name", "title"], username: &["username"], email: &["email"], password: &["password"],
    url: &["url"], notes: &["note", "notes"], totp: &["totp"],
    created: &["createtime", "create_time"], modified: &["modifytime", "modify_time"],
    tags: &["vault"], tag_sep: &[], favorite: &[], archived: &[],
    ignore: &["type"], note_type: Some(("type", "note")), category: &["type"],
};
// Apple Passwords / Safari CSV (macOS 15 Passwords app / Safari export): Title,URL,Username,Password,Notes,OTPAuth
const APPLE_PASSWORDS_CSV: CsvSpec = CsvSpec {
    name: "apple-passwords-csv",
    title: &["title"], username: &["username", "user name"], email: &[], password: &["password"],
    url: &["url", "website"], notes: &["notes", "note"],
    totp: &["otpauth", "totp", "otp"], created: &[], modified: &[],
    tags: &[], tag_sep: &[], favorite: &[], archived: &[],
    ignore: &[], note_type: None, category: &[],
};

/// KeePass group path "Root/Work/Mail" -> tag "Work/Mail" (the root group alone gives no tag).
fn group_tag(spec: &CsvSpec, v: &str) -> String {
    if spec.name == "lastpass-csv" {
        let v = v.trim();
        if v.eq_ignore_ascii_case("(none)") { return String::new(); }
        return v.to_string();
    }
    if spec.name != "keepass-csv" { return v.to_string(); }
    let v = v.trim().trim_matches('/');
    match v.split_once('/') { Some((root, rest)) if ["root", "database", "passwords"].contains(&root.to_ascii_lowercase().as_str()) => rest.to_string(),
        None if ["root", "database", "passwords"].contains(&v.to_ascii_lowercase().as_str()) => String::new(), _ => v.to_string() }
}

fn parse_csv(data: &str, spec: &CsvSpec) -> Result<Parsed> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).trim(csv::Trim::Headers).from_reader(data.as_bytes());
    let headers: Vec<String> = rdr.headers().with_context(|| format!("{}: reading header row", spec.name))?
        .iter().map(|h| h.trim().to_string()).collect();
    let lower: Vec<String> = headers.iter().map(|h| h.to_ascii_lowercase()).collect();
    let find = |aliases: &[&str]| aliases.iter().find_map(|a| lower.iter().position(|h| h == a));
    let email_col = find(spec.email);
    let create_col = find(spec.created);
    let cols = [find(spec.title), find(spec.username), find(spec.password), find(spec.url), find(spec.notes), find(spec.totp), find(spec.modified),
        find(spec.tags), find(spec.favorite), find(spec.archived)];
    let mut out = Parsed::default();
    if cols[0].is_none() && cols[3].is_none() {
        bail!("{}: no title/name or url column found in header {:?}", spec.name, headers);
    }
    let mut used: HashSet<usize> = cols.iter().flatten().copied().collect();
    if let Some(ec) = email_col { used.insert(ec); }
    if let Some(cc) = create_col { used.insert(cc); }
    let type_col = spec.note_type.and_then(|(c, _)| lower.iter().position(|h| h == c));
    let cat_col = find(spec.category);
    if let Some(cc) = cat_col { used.insert(cc); }
    for (row, rec) in rdr.records().enumerate() {
        let rec = rec.with_context(|| format!("{}: row {}", spec.name, row + 2))?;
        let get = |i: Option<usize>| i.and_then(|i| rec.get(i)).and_then(|v| opt(Some(v)));
        let raw_url = get(cols[3]);
        // LastPass folder placeholder rows (`http://group`) carry no secret: skip them.
        if spec.name == "lastpass-csv" && raw_url.as_deref() == Some("http://group") { continue; }
        let is_lastpass_sn = spec.name == "lastpass-csv" && raw_url.as_deref() == Some("http://sn");
        // Proton Pass joins several websites into one cell with ", ".
        let mut urls: Vec<String> = match (&raw_url, spec.name) {
            (Some(u), "protonpass-csv") => u.split(", ").map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
            (Some(u), _) => vec![u.clone()],
            (None, _) => Vec::new(),
        };
        let extra_urls: Vec<String> = if urls.len() > 1 { urls.split_off(1) } else { Vec::new() };
        let raw_url = urls.into_iter().next();
        let url = if is_lastpass_sn { None } else { raw_url.clone() };
        let title = get(cols[0]).or(if is_lastpass_sn { None } else { raw_url });
        let Some(title) = title else { out.warnings.push(format!("row {}: no title or url, skipped", row + 2)); continue };
        let mut notes = get(cols[4]);
        for (i, h) in headers.iter().enumerate() {
            if used.contains(&i) || spec.ignore.contains(&lower[i].as_str()) { continue; }
            if let Some(v) = rec.get(i).and_then(|v| opt(Some(v))) { append_note(&mut notes, format!("{h}: {v}")); }
        }
        let is_note = is_lastpass_sn || matches!((spec.note_type, type_col), (Some((_, want)), Some(tc)) if rec.get(tc).map(|v| v.trim().eq_ignore_ascii_case(want)).unwrap_or(false));
        let category = if let Some(cat_str) = cat_col.and_then(|c| rec.get(c)).map(str::trim).filter(|s| !s.is_empty()) {
            let cat_lower = cat_str.to_ascii_lowercase();
            if cat_lower == "bank account" || cat_lower == "bank_account" || cat_lower == "bank" {
                Category::BankAccount
            } else if cat_lower == "server" {
                Category::Server
            } else if cat_lower == "credit card" || cat_lower == "credit_card" || cat_lower == "card" {
                Category::CreditCard
            } else if cat_lower == "secure note" || cat_lower == "secure_note" || cat_lower == "note" {
                Category::SecureNote
            } else if cat_lower == "identity" {
                Category::Identity
            } else if cat_lower == "api credential" || cat_lower == "api_credential" || cat_lower == "api" {
                Category::ApiCredential
            } else if cat_lower == "wifi" {
                Category::Wifi
            } else if cat_lower == "ssh key" || cat_lower == "ssh_key" || cat_lower == "ssh" {
                Category::SshKey
            } else if cat_lower == "document" {
                Category::Document
            } else if cat_lower == "passkey" {
                Category::Passkey
            } else if cat_lower == "login" {
                Category::Login
            } else if is_note {
                Category::SecureNote
            } else {
                Category::Login
            }
        } else if is_note {
            Category::SecureNote
        } else {
            Category::Login
        };
        let mut r = record(title, category);
        let email = get(email_col);
        let username = get(cols[1]);
        let extra_email = match (&username, &email) { (Some(u), Some(e)) if !u.eq_ignore_ascii_case(e) => Some(e.clone()), _ => None };
        r.item.username = username.or(email); r.item.password = get(cols[2]); r.item.url = url; r.item.notes = notes; r.item.totp = get(cols[5]);
        if !is_lastpass_sn {
            let extra: Vec<Option<Field>> = extra_urls.iter().enumerate().map(|(n, u)| fld(&format!("url{}", n + 2), "website", FieldKind::Url, Some(u.clone()))).collect();
            push_section(&mut r, "urls", Some("Websites"), extra);
        }
        // Keep a distinct email (e.g. Proton Pass `email` next to `username`) instead of dropping it.
        push_section(&mut r, "imported", Some("Imported"), vec![fld("email", "email", FieldKind::Text, extra_email)]);
        if category == Category::BankAccount {
            let mut bank_fields = vec![];
            for (i, _h) in headers.iter().enumerate() {
                let hl = lower[i].as_str();
                let val = rec.get(i).and_then(|v| opt(Some(v)));
                match hl {
                    "bank" | "bank name" | "bank_name" => {
                        if let Some(v) = val { bank_fields.push(fld("bank_name", "bank name", FieldKind::Text, Some(v))); }
                    }
                    "account holder" | "account_holder" | "name on account" => {
                        if let Some(v) = val { bank_fields.push(fld("account_holder", "account holder", FieldKind::Text, Some(v))); }
                    }
                    "account type" | "account_type" => {
                        if let Some(v) = val { bank_fields.push(fld("account_type", "account type", FieldKind::Text, Some(v))); }
                    }
                    "account number" | "account_number" | "account no" | "accountno" => {
                        if let Some(v) = val { bank_fields.push(fld("account_number", "account number", FieldKind::Concealed, Some(v))); }
                    }
                    "routing number" | "routing_number" | "routing" | "swift" | "iban" => {
                        if let Some(v) = val { bank_fields.push(fld("routing_number", "routing / SWIFT / IBAN", FieldKind::Text, Some(v))); }
                    }
                    "pin" => {
                        if let Some(v) = val { bank_fields.push(fld("pin", "PIN", FieldKind::Concealed, Some(v))); }
                    }
                    "branch phone" | "branch_phone" => {
                        if let Some(v) = val { bank_fields.push(fld("branch_phone", "branch phone", FieldKind::Phone, Some(v))); }
                    }
                    "branch address" | "branch_address" => {
                        if let Some(v) = val { bank_fields.push(fld("branch_address", "branch address", FieldKind::Multiline, Some(v))); }
                    }
                    _ => {}
                }
            }
            if !bank_fields.iter().any(|f| f.as_ref().is_some_and(|x| x.id == "account_holder")) && r.item.username.is_some() {
                bank_fields.push(fld("account_holder", "account holder", FieldKind::Text, r.item.username.take()));
            }
            if !bank_fields.iter().any(|f| f.as_ref().is_some_and(|x| x.id == "account_number")) && r.item.password.is_some() {
                bank_fields.push(fld("account_number", "account number", FieldKind::Concealed, r.item.password.take()));
            }
            if !bank_fields.is_empty() {
                push_section(&mut r, "main", None, bank_fields);
            }
        } else if category == Category::Server {
            let mut srv_fields = vec![];
            if let Some(u) = &r.item.url {
                srv_fields.push(fld("url", "URL or hostname", FieldKind::Url, Some(u.clone())));
            }
            if let Some(u) = r.item.username.clone() {
                srv_fields.push(fld("username", "username", FieldKind::Text, Some(u)));
            }
            if let Some(p) = r.item.password.clone() {
                srv_fields.push(fld("password", "password", FieldKind::Concealed, Some(p)));
            }
            if !srv_fields.is_empty() {
                push_section(&mut r, "main", None, srv_fields);
            }
        }
        if is_note && (r.item.username.is_some() || r.item.password.is_some() || r.item.totp.is_some()) {
            // secure notes have no login fields: keep the values visible instead of dropping them
            let fs = vec![
                fld("username", "username", FieldKind::Text, r.item.username.take()),
                fld("password", "password", FieldKind::Concealed, r.item.password.take()),
                fld("otp", "one-time password", FieldKind::Totp, r.item.totp.take())];
            push_section(&mut r, "imported", Some("Imported"), fs);
        }
        if let Some(t) = get(create_col).and_then(|v| parse_time(&v)) { r.created_at = Some(t); }
        if let Some(t) = get(cols[6]).and_then(|v| parse_time(&v)) { r.item.updated_at = t.to_string(); }
        if let Some(t) = get(cols[7]) {
            if spec.tag_sep.is_empty() { add_tag(&mut r, &group_tag(spec, &t)); } else { for x in t.split(spec.tag_sep) { add_tag(&mut r, x); } }
        }
        r.favorite = get(cols[8]).is_some_and(|v| truthy(&v));
        r.archived = get(cols[9]).is_some_and(|v| truthy(&v));
        out.items.push(r);
    }
    Ok(out)
}

/// Parse ISO 8601 strings or unix timestamps (seconds or milliseconds).
pub fn parse_time(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Ok(epoch) = s.parse::<u64>() {
        if epoch >= 100_000_000_000 {
            return Some(epoch / 1000);
        }
        if epoch > 0 {
            return Some(epoch);
        }
    }
    iso_to_epoch(s)
}

/// Parse `YYYY-MM-DD[T ]HH:MM:SS[.fff][Z]` (UTC assumed) to unix seconds. Returns None otherwise.
pub fn iso_to_epoch(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.len() < 10 { return None; }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let m: i64 = s.get(5..7)?.parse().ok()?;
    let d: i64 = s.get(8..10)?.parse().ok()?;
    if s.get(4..5)? != "-" || s.get(7..8)? != "-" || !(1..=12).contains(&m) || !(1..=31).contains(&d) { return None; }
    let (hh, mm, ss) = if s.len() >= 19 {
        (s.get(11..13)?.parse::<i64>().ok()?, s.get(14..16)?.parse::<i64>().ok()?, s.get(17..19)?.parse::<i64>().ok()?)
    } else { (0, 0, 0) };
    // days from civil (Howard Hinnant)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let t = days * 86400 + hh * 3600 + mm * 60 + ss;
    (t >= 0).then_some(t as u64)
}

fn month_year(m: Option<String>, y: Option<String>) -> Option<String> {
    let y = y?; let y = if y.len() == 2 { format!("20{y}") } else { y };
    Some(match m { Some(m) => format!("{:0>2}/{y}", m), None => y })
}

// ---------------------------------------------------------------- Bitwarden JSON

fn parse_bitwarden_json(data: &str, export_password: Option<&str>) -> Result<Parsed> {
    let v: Value = serde_json::from_str(data).context("bitwarden-json: invalid JSON")?;
    if v.get("encrypted").and_then(Value::as_bool) == Some(true) {
        let clear = decrypt_bitwarden_export(&v, export_password)?;
        return parse_bitwarden_json(&clear, None);
    }
    let folders: BTreeMap<String, String> = v.get("folders").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|f| Some((f.get("id")?.as_str()?.to_string(), f.get("name")?.as_str()?.to_string()))).collect();
    let collections: BTreeMap<String, String> = v.get("collections").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|f| Some((f.get("id")?.as_str()?.to_string(), f.get("name")?.as_str()?.to_string()))).collect();
    let items = v.get("items").and_then(Value::as_array).ok_or_else(|| anyhow!("bitwarden-json: missing \"items\" array"))?;
    let s = |o: &Value, k: &str| opt(o.get(k).and_then(Value::as_str));
    let mut out = Parsed::default();
    for (i, it) in items.iter().enumerate() {
        let Some(title) = s(it, "name") else { out.warnings.push(format!("item {i}: no name, skipped")); continue };
        if it.get("deletedDate").is_some_and(|d| !d.is_null()) { out.warnings.push(format!("item {i} ({title}): in Bitwarden trash, skipped")); continue; }
        let ty = it.get("type").and_then(Value::as_u64).unwrap_or(1);
        let cat = match ty { 1 => Category::Login, 2 => Category::SecureNote, 3 => Category::CreditCard, 4 => Category::Identity, 5 => Category::SshKey, _ => Category::SecureNote };
        let mut r = record(title.clone(), cat);
        r.item.notes = s(it, "notes");
        r.favorite = it.get("favorite").and_then(Value::as_bool).unwrap_or(false);
        if let Some(f) = s(it, "folderId").and_then(|fid| folders.get(&fid).cloned()) { add_tag(&mut r, &f); }
        for c in it.get("collectionIds").and_then(Value::as_array).into_iter().flatten() {
            if let Some(n) = c.as_str().and_then(|c| collections.get(c)) { add_tag(&mut r, n); }
        }
        match ty {
            1 => if let Some(l) = it.get("login") {
                r.item.username = s(l, "username"); r.item.password = s(l, "password"); r.item.totp = s(l, "totp");
                let uris: Vec<String> = l.get("uris").and_then(Value::as_array).into_iter().flatten().filter_map(|u| s(u, "uri")).collect();
                r.item.url = uris.first().cloned();
                let extra: Vec<Option<Field>> = uris.iter().skip(1).enumerate().map(|(n, u)| fld(&format!("url{}", n + 2), "website", FieldKind::Url, Some(u.clone()))).collect();
                push_section(&mut r, "urls", Some("Websites"), extra);
                let pw_history = l.get("passwordHistory").or_else(|| it.get("passwordHistory")).and_then(Value::as_array);
                if let Some(hist) = pw_history {
                    for entry in hist {
                        let pw = entry.get("password").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty());
                        let Some(pw) = pw else { continue };
                        let replaced_at = entry.get("lastUsedDate").and_then(Value::as_str).map(str::trim)
                            .filter(|d| !d.is_empty())
                            .map(|d| d.to_string())
                            .unwrap_or_else(|| r.item.updated_at.clone());
                        r.password_history.push(crate::item::PasswordHistoryEntry {
                            value: pw.to_string(),
                            replaced_at,
                        });
                    }
                    r.password_history.sort_by(|a, b| b.replaced_at.cmp(&a.replaced_at));
                    if r.password_history.len() > crate::item::PASSWORD_HISTORY_LIMIT {
                        r.password_history.truncate(crate::item::PASSWORD_HISTORY_LIMIT);
                    }
                }
            },
            3 => if let Some(c) = it.get("card") {
                push_section(&mut r, "main", None, vec![
                    fld("cardholder", "cardholder name", FieldKind::Text, s(c, "cardholderName")),
                    fld("type", "type", FieldKind::Text, s(c, "brand")),
                    fld("number", "number", FieldKind::Concealed, s(c, "number")),
                    fld("cvv", "verification number", FieldKind::Concealed, s(c, "code")),
                    fld("expiry", "expiry date", FieldKind::MonthYear, month_year(s(c, "expMonth"), s(c, "expYear")))]);
            },
            4 => if let Some(d) = it.get("identity") {
                let addr = ["address1", "address2", "address3"].iter().filter_map(|k| s(d, k)).collect::<Vec<_>>();
                let city = [s(d, "city"), s(d, "state"), s(d, "postalCode")].into_iter().flatten().collect::<Vec<_>>().join(" ");
                let full = addr.into_iter().chain([city].into_iter().filter(|x| !x.is_empty())).chain(s(d, "country")).collect::<Vec<_>>().join("\n");
                push_section(&mut r, "name", Some("Identification"), vec![
                    fld("title", "title", FieldKind::Text, s(d, "title")),
                    fld("first", "first name", FieldKind::Text, s(d, "firstName")),
                    fld("middle", "middle name", FieldKind::Text, s(d, "middleName")),
                    fld("last", "last name", FieldKind::Text, s(d, "lastName")),
                    fld("username", "username", FieldKind::Text, s(d, "username")),
                    fld("company", "company", FieldKind::Text, s(d, "company"))]);
                push_section(&mut r, "address", Some("Address"), vec![fld("address", "address", FieldKind::Multiline, Some(full))]);
                push_section(&mut r, "contact", Some("Contact"), vec![
                    fld("email", "email", FieldKind::Email, s(d, "email")), fld("phone", "phone", FieldKind::Phone, s(d, "phone"))]);
                push_section(&mut r, "ids", Some("Documents"), vec![
                    fld("ssn", "social security number", FieldKind::Concealed, s(d, "ssn")),
                    fld("passport", "passport number", FieldKind::Concealed, s(d, "passportNumber")),
                    fld("license", "license number", FieldKind::Concealed, s(d, "licenseNumber"))]);
            },
            5 => if let Some(k) = it.get("sshKey") {
                push_section(&mut r, "main", None, vec![
                    fld("private_key", "private key", FieldKind::Concealed, s(k, "privateKey")),
                    fld("public_key", "public key", FieldKind::Multiline, s(k, "publicKey")),
                    fld("fingerprint", "fingerprint", FieldKind::Text, s(k, "keyFingerprint"))]);
            },
            _ => {}
        }
        for f in it.get("fields").and_then(Value::as_array).into_iter().flatten() {
            let Some(n) = s(f, "name") else { continue };
            let kind = match f.get("type").and_then(Value::as_u64) { Some(1) => FieldKind::Concealed, _ => FieldKind::Text };
            let val = s(f, "value").or_else(|| f.get("value").filter(|v| v.is_boolean() || v.is_number()).map(|v| v.to_string()));
            let id = next_id(&r, "custom");
            push_section(&mut r, "custom", Some("Custom fields"), vec![fld(&id, &n, kind, val)]);
        }
        if let Some(t) = s(it, "revisionDate").and_then(|d| iso_to_epoch(&d)) { r.item.updated_at = t.to_string(); }
        r.created_at = s(it, "creationDate").and_then(|d| iso_to_epoch(&d));
        out.items.push(r);
    }
    Ok(out)
}

// ------------------------------------------------ Bitwarden password-protected JSON

/// True when `data` is a Bitwarden JSON export protected by a user-chosen password
/// (`encrypted` + `passwordProtected`). Account-key-encrypted exports return false.
pub fn bitwarden_json_needs_password(data: &str) -> bool {
    serde_json::from_str::<Value>(data).is_ok_and(|v| is_password_protected(&v))
}

fn is_password_protected(v: &Value) -> bool {
    v.get("encrypted").and_then(Value::as_bool) == Some(true)
        && v.get("passwordProtected").and_then(Value::as_bool) == Some(true)
}

// Bounds Bitwarden clients accept for import KDF parameters. An import file is attacker controlled
// and Argon2 allocates `kdfMemory` MiB up front, so reject out-of-range values before deriving.
const BW_PBKDF2_ITER_MIN: u64 = 5_000;
const BW_PBKDF2_ITER_MAX: u64 = 2_000_000;
const BW_ARGON2_ITER_MIN: u64 = 2;
const BW_ARGON2_ITER_MAX: u64 = 10;
const BW_ARGON2_MEMORY_MIN: u64 = 16; // MiB
const BW_ARGON2_MEMORY_MAX: u64 = 1024;
const BW_ARGON2_PARALLELISM_MIN: u64 = 1;
const BW_ARGON2_PARALLELISM_MAX: u64 = 16;

fn within(v: u64, lo: u64, hi: u64) -> bool { (lo..=hi).contains(&v) }

/// Decrypt a password-protected Bitwarden JSON export: derive the key from the export password with
/// the file's KDF settings, HKDF-Expand it into the AES-256-CBC and HMAC-SHA256 keys, verify
/// `encKeyValidation_DO_NOT_EDIT`, then MAC-verify and decrypt `data`.
fn decrypt_bitwarden_export(v: &Value, export_password: Option<&str>) -> Result<Zeroizing<String>> {
    if !is_password_protected(v) {
        bail!("bitwarden-json: this export is encrypted with your Bitwarden account key, not an export password; in Bitwarden choose “Password protected” or an unencrypted export");
    }
    let password = export_password
        .filter(|p| !p.is_empty())
        .ok_or_else(|| anyhow!("bitwarden-json: this export is password-protected; the export password is required"))?;
    let field = |k: &str| {
        v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing or has an empty \"{k}\""))
    };
    let master = bitwarden_export_key(password, field("salt")?, v)?;
    let mut enc_key = Zeroizing::new([0u8; 32]);
    let mut mac_key = Zeroizing::new([0u8; 32]);
    let hk = hkdf::Hkdf::<Sha256>::from_prk(master.as_slice())
        .map_err(|_| anyhow!("bitwarden-json: invalid export key material"))?;
    hk.expand(b"enc", enc_key.as_mut_slice()).map_err(|_| anyhow!("bitwarden-json: cannot stretch the export key"))?;
    hk.expand(b"mac", mac_key.as_mut_slice()).map_err(|_| anyhow!("bitwarden-json: cannot stretch the export key"))?;
    // A MAC failure on the validation blob means the password is wrong; on the payload it means tampering.
    bitwarden_decrypt(field("encKeyValidation_DO_NOT_EDIT")?, enc_key.as_slice(), mac_key.as_slice())
        .map_err(|_| anyhow!("bitwarden-json: wrong export password (or the file is corrupted)"))?;
    let plain = bitwarden_decrypt(field("data")?, enc_key.as_slice(), mac_key.as_slice())
        .map_err(|_| anyhow!("bitwarden-json: encrypted contents failed the integrity check (tampered file or wrong password)"))?;
    let text = std::str::from_utf8(plain.as_slice()).context("bitwarden-json: decrypted export is not UTF-8")?;
    Ok(Zeroizing::new(text.to_string()))
}

/// Derive the 32-byte export key from the password and the file's KDF settings. Bitwarden feeds the
/// salt text itself (a base64 string) to the KDF as UTF-8, not the decoded bytes.
fn bitwarden_export_key(password: &str, salt: &str, v: &Value) -> Result<Zeroizing<[u8; 32]>> {
    let num = |k: &str| v.get(k).and_then(Value::as_u64);
    let kdf = num("kdfType").ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing \"kdfType\""))?;
    let mut key = Zeroizing::new([0u8; 32]);
    match kdf {
        0 => {
            let iterations = num("kdfIterations").ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing \"kdfIterations\""))?;
            if !within(iterations, BW_PBKDF2_ITER_MIN, BW_PBKDF2_ITER_MAX) {
                bail!("bitwarden-json: unsupported PBKDF2 iteration count {iterations} (expected {BW_PBKDF2_ITER_MIN}–{BW_PBKDF2_ITER_MAX})");
            }
            pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), iterations as u32, key.as_mut_slice());
        }
        1 => {
            let iterations = num("kdfIterations").ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing \"kdfIterations\""))?;
            let memory = num("kdfMemory").ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing \"kdfMemory\""))?;
            let parallelism = num("kdfParallelism").ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing \"kdfParallelism\""))?;
            if !within(iterations, BW_ARGON2_ITER_MIN, BW_ARGON2_ITER_MAX)
                || !within(memory, BW_ARGON2_MEMORY_MIN, BW_ARGON2_MEMORY_MAX)
                || !within(parallelism, BW_ARGON2_PARALLELISM_MIN, BW_ARGON2_PARALLELISM_MAX)
            {
                bail!("bitwarden-json: unsupported Argon2id settings (iterations {iterations}, memory {memory} MiB, parallelism {parallelism})");
            }
            let params = Params::new((memory * 1024) as u32, iterations as u32, parallelism as u32, Some(32))
                .map_err(|e| anyhow!("bitwarden-json: unsupported Argon2id settings: {e}"))?;
            Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
                .hash_password_into(password.as_bytes(), salt.as_bytes(), key.as_mut_slice())
                .map_err(|_| anyhow!("bitwarden-json: cannot derive the export key"))?;
        }
        other => bail!("bitwarden-json: unsupported KDF type {other} (expected 0 = PBKDF2-SHA256 or 1 = Argon2id)"),
    }
    Ok(key)
}

/// Decrypt one Bitwarden `EncString` of type 2 (`2.<iv>|<ciphertext>|<mac>`, AesCbc256_HmacSha256).
/// The MAC covers IV ‖ ciphertext and is checked before AES-CBC/PKCS7 decryption.
fn bitwarden_decrypt(field: &str, enc_key: &[u8], mac_key: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let Some(rest) = field.strip_prefix("2.") else {
        bail!("bitwarden-json: encrypted field is not AesCbc256_HmacSha256");
    };
    let mut parts = rest.split('|');
    let (Some(iv), Some(ct), Some(mac), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        bail!("bitwarden-json: malformed encrypted field");
    };
    let bad = || anyhow!("bitwarden-json: malformed encrypted field");
    let iv = B64.decode(iv).map_err(|_| bad())?;
    let mut ct = B64.decode(ct).map_err(|_| bad())?;
    let mac = B64.decode(mac).map_err(|_| bad())?;
    let mut h = Hmac::<Sha256>::new_from_slice(mac_key).map_err(|_| anyhow!("bitwarden-json: invalid MAC key"))?;
    h.update(&iv);
    h.update(&ct);
    h.verify_slice(&mac).map_err(|_| anyhow!("bitwarden-json: MAC check failed"))?;
    let dec = cbc::Decryptor::<Aes256>::new_from_slices(enc_key, &iv)
        .map_err(|_| anyhow!("bitwarden-json: invalid AES key or IV"))?;
    let pt = dec.decrypt_padded_mut::<Pkcs7>(&mut ct).map_err(|_| anyhow!("bitwarden-json: invalid encrypted payload"))?;
    Ok(Zeroizing::new(pt.to_vec()))
}

// ---------------------------------------------------------------- .1pux

/// 1PUX = zip with `export.data` (JSON: accounts[].vaults[].items[]) plus attachment files.
fn parse_1pux(data: &[u8]) -> Result<Parsed> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).context("1pux: not a .1pux (zip) file")?;
    let mut f = z.by_name("export.data").map_err(|_| anyhow!("1pux: export.data not found in archive"))?;
    let mut s = String::new();
    f.read_to_string(&mut s).context("1pux: export.data is not UTF-8")?;
    let s = zeroize::Zeroizing::new(s);
    parse_1pux_json(&s)
}

fn pux_category(uuid: &str) -> Category {
    match uuid {
        "001" | "005" | "102" | "111" => Category::Login,
        "002" => Category::CreditCard,
        "004" | "103" | "104" | "106" | "108" => Category::Identity,
        "006" => Category::Document,
        "101" => Category::BankAccount,
        "109" => Category::Wifi,
        "110" => Category::Server,
        "112" => Category::ApiCredential,
        "114" => Category::SshKey,
        _ => Category::SecureNote, // 003 note, 100 software licence, 105 membership, 107 reward, 113 medical, 115 crypto wallet...
    }
}

/// 1PUX field value object -> (kind, display string).
fn pux_value(v: &Value) -> Option<(FieldKind, String)> {
    let o = v.as_object()?;
    let (k, x) = o.iter().next()?;
    let st = |x: &Value| x.as_str().map(String::from).or_else(|| (x.is_number() || x.is_boolean()).then(|| x.to_string()));
    let out = match k.as_str() {
        "concealed" | "creditCardNumber" | "gitpassConcealed" => (FieldKind::Concealed, st(x)?),
        "totp" => (FieldKind::Totp, st(x)?),
        "url" => (FieldKind::Url, st(x)?),
        "phone" => (FieldKind::Phone, st(x)?),
        "email" => (FieldKind::Email, x.get("email_address").and_then(Value::as_str).map(String::from).or_else(|| st(x))?),
        "monthYear" => { let n = x.as_u64()?; (FieldKind::MonthYear, format!("{:02}/{}", n % 100, n / 100)) }
        "date" => { let secs = x.as_i64()?; let d = secs.div_euclid(86400); (FieldKind::Date, civil_date(d)) }
        "address" => {
            let parts: Vec<String> = ["street", "city", "state", "zip", "country"].iter().filter_map(|k| x.get(*k).and_then(Value::as_str)).filter(|s| !s.is_empty()).map(String::from).collect();
            (FieldKind::Multiline, parts.join("\n"))
        }
        "sshKey" => (FieldKind::Concealed, x.get("privateKey").and_then(Value::as_str).map(String::from).or_else(|| st(x))?),
        "string" | "menu" | "creditCardType" | "gender" => (FieldKind::Text, st(x)?),
        _ => (FieldKind::Text, st(x)?),
    };
    (!out.1.trim().is_empty()).then_some(out)
}

/// Days since 1970-01-01 -> "YYYY-MM-DD".
fn civil_date(z: i64) -> String {
    let z = z + 719468; let era = z.div_euclid(146097); let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1; let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

/// Well-known 1PUX field ids -> gitpass template ids (so the item editor shows them in place).
fn pux_field_id(cat: Category, id: &str) -> Option<&'static str> {
    Some(match (cat, id) {
        (Category::CreditCard, "cardholder") => "cardholder", (Category::CreditCard, "type") => "type",
        (Category::CreditCard, "ccnum") => "number", (Category::CreditCard, "cvv") => "cvv",
        (Category::CreditCard, "expiry") => "expiry", (Category::CreditCard, "validFrom") => "valid_from", (Category::CreditCard, "pin") => "pin",
        (Category::Identity, "firstname") => "first", (Category::Identity, "lastname") => "last", (Category::Identity, "birthdate") => "birth",
        (Category::Identity, "address") => "address", (Category::Identity, "email") => "email", (Category::Identity, "defphone") => "phone",
        (Category::Wifi, "network_name") => "ssid", (Category::Wifi, "wireless_password") => "password", (Category::Wifi, "wireless_security") => "security",
        (Category::ApiCredential, "credential") => "credential", (Category::ApiCredential, "username") => "username",
        (Category::ApiCredential, "type") => "type", (Category::ApiCredential, "hostname") => "hostname", (Category::ApiCredential, "expires") => "expires",
        (Category::SshKey, "private_key") => "private_key", (Category::SshKey, "public_key") => "public_key", (Category::SshKey, "fingerprint") => "fingerprint",
        (Category::BankAccount, "bankName" | "bank_name" | "bank") => "bank_name",
        (Category::BankAccount, "owner" | "accountHolder" | "nameOnAccount" | "account_holder") => "account_holder",
        (Category::BankAccount, "accountType" | "account_type" | "type") => "account_type",
        (Category::BankAccount, "accountNo" | "accountNumber" | "account_number" | "account") => "account_number",
        (Category::BankAccount, "routingNo" | "routingNumber" | "routing_number" | "routing" | "transitNumber") => "routing_number",
        (Category::BankAccount, "swift" | "swiftCode") => "routing_number",
        (Category::BankAccount, "iban" | "ibanNumber") => "routing_number",
        (Category::BankAccount, "pin" | "telephonePin") => "pin",
        (Category::BankAccount, "telephone" | "branchPhone" | "branch_phone" | "phone") => "branch_phone",
        (Category::BankAccount, "branchAddress" | "branch_address" | "address") => "branch_address",
        (Category::Server, "admin_console_url" | "adminConsoleUrl") => "admin_console_url",
        (Category::Server, "hosting_provider_name" | "hostingProvider" | "hosting_provider") => "hosting_provider",
        (Category::Server, "username") => "username",
        (Category::Server, "password") => "password",
        (Category::Server, "url" | "hostname") => "url",
        _ => return None,
    })
}

fn parse_1pux_json(data: &str) -> Result<Parsed> {
    let v: Value = serde_json::from_str(data).context("1pux: export.data is not valid JSON")?;
    let accounts = v.get("accounts").and_then(Value::as_array).ok_or_else(|| anyhow!("1pux: missing \"accounts\" array"))?;
    let s = |o: &Value, k: &str| opt(o.get(k).and_then(Value::as_str));
    let mut out = Parsed::default();
    for acc in accounts {
        for vault in acc.get("vaults").and_then(Value::as_array).into_iter().flatten() {
            let vname = vault.get("attrs").and_then(|a| s(a, "name"));
            for (i, it) in vault.get("items").and_then(Value::as_array).into_iter().flatten().enumerate() {
                let empty = Value::Null;
                let ov = it.get("overview").unwrap_or(&empty);
                let de = it.get("details").unwrap_or(&empty);
                let state = s(it, "state").unwrap_or_else(|| "active".into());
                let Some(title) = s(ov, "title") else { out.warnings.push(format!("1pux item {i}: no title, skipped")); continue };
                if state == "trashed" || state == "deleted" { out.warnings.push(format!("{title}: in source trash, skipped")); continue; }
                let cu = s(it, "categoryUuid").unwrap_or_else(|| "001".into());
                let cat = pux_category(&cu);
                let mut r = record(title.clone(), cat);
                r.archived = state == "archived";
                r.favorite = it.get("favIndex").and_then(Value::as_i64).unwrap_or(0) > 0;
                if let Some(n) = &vname { add_tag(&mut r, n); }
                for t in ov.get("tags").and_then(Value::as_array).into_iter().flatten() { if let Some(t) = t.as_str() { add_tag(&mut r, t); } }
                r.item.notes = s(de, "notesPlain");
                if let Some(t) = it.get("updatedAt").and_then(Value::as_u64) { r.item.updated_at = t.to_string(); }
                r.created_at = it.get("createdAt").and_then(Value::as_u64);
                // URLs: primary overview.url, then overview.urls[]
                let mut urls: Vec<String> = s(ov, "url").into_iter().collect();
                for u in ov.get("urls").and_then(Value::as_array).into_iter().flatten() { if let Some(x) = s(u, "url") { if !urls.contains(&x) { urls.push(x); } } }
                let login_like = cat == Category::Login || cat == Category::Server;
                if login_like { r.item.url = urls.first().cloned(); }
                let extra: Vec<Option<Field>> = urls.iter().skip(usize::from(login_like)).enumerate().map(|(n, u)| fld(&format!("url{}", n + 1), "website", FieldKind::Url, Some(u.clone()))).collect();
                push_section(&mut r, "urls", Some("Websites"), extra);
                // login fields (designation username/password); others become custom fields
                for lf in de.get("loginFields").and_then(Value::as_array).into_iter().flatten() {
                    let val = s(lf, "value");
                    match (s(lf, "designation").as_deref(), login_like) {
                        (Some("username"), true) if r.item.username.is_none() => r.item.username = val,
                        (Some("password"), true) if r.item.password.is_none() => r.item.password = val,
                        _ => {
                            let kind = if s(lf, "fieldType").as_deref() == Some("P") { FieldKind::Concealed } else { FieldKind::Text };
                            let label = s(lf, "name").or_else(|| s(lf, "designation")).unwrap_or_else(|| "field".into());
                            let id = next_id(&r, "login_f");
                            push_section(&mut r, "login_extra", Some("Login fields"), vec![fld(&id, &label, kind, val)]);
                        }
                    }
                }
                // "Password" category (005) stores the secret in details.password
                if let Some(p) = s(de, "password") {
                    if login_like && r.item.password.is_none() { r.item.password = Some(p); }
                    else { let id = next_id(&r, "pw"); push_section(&mut r, "main", None, vec![fld(&id, "password", FieldKind::Concealed, Some(p))]); }
                }
                for (si, sec) in de.get("sections").and_then(Value::as_array).into_iter().flatten().enumerate() {
                    let stitle = s(sec, "title");
                    let sid = s(sec, "name").filter(|n| !n.is_empty()).unwrap_or_else(|| format!("section{si}"));
                    // templated sections land in "main" (cards, wifi, api, ssh) / their own id (identity)
                    let target = match cat { Category::CreditCard | Category::Wifi | Category::ApiCredential | Category::SshKey | Category::BankAccount | Category::Server if stitle.is_none() => "main".to_string(), _ => sid.clone() };
                    for f in sec.get("fields").and_then(Value::as_array).into_iter().flatten() {
                        let Some((kind, val)) = f.get("value").and_then(pux_value) else { continue };
                        let fid = s(f, "id").unwrap_or_default();
                        let label = s(f, "title").unwrap_or_else(|| fid.clone());
                        if kind == FieldKind::Totp && login_like && r.item.totp.is_none() { r.item.totp = Some(val); continue; }
                        let id = pux_field_id(cat, &fid).map(String::from).unwrap_or_else(|| next_id(&r, "f"));
                        let tgt = if cat == Category::Identity { match id.as_str() { "first" | "last" | "birth" => "name", "address" => "address", "email" | "phone" => "contact", _ => target.as_str() } } else { target.as_str() };
                        let ttl = match tgt { "name" if cat == Category::Identity => Some("Identification".to_string()), "address" => Some("Address".into()), "contact" => Some("Contact".into()), "main" => None, _ => stitle.clone() };
                        push_section(&mut r, tgt, ttl.as_deref(), vec![fld(&id, &label, kind, Some(val))]);
                        if let Some(md) = f.get("value").and_then(|v| v.get("sshKey")).and_then(|k| k.get("metadata")) {
                            push_section(&mut r, tgt, ttl.as_deref(), vec![
                                fld("public_key", "public key", FieldKind::Multiline, s(md, "publicKey")),
                                fld("fingerprint", "fingerprint", FieldKind::Text, s(md, "fingerprint"))]);
                        }
                    }
                }
                if let Some(doc) = de.get("documentAttributes") {
                    if let Some(n) = s(doc, "fileName") { append_note(&mut r.item.notes, format!("Document file (not imported, re-attach manually): {n}")); }
                }
                if cat == Category::Server {
                    let mut srv_fields = vec![];
                    if let Some(u) = &r.item.url {
                        if r.find_field("url").is_none() {
                            srv_fields.push(fld("url", "URL or hostname", FieldKind::Url, Some(u.clone())));
                        }
                    }
                    if let Some(u) = &r.item.username {
                        if r.find_field("username").is_none() {
                            srv_fields.push(fld("username", "username", FieldKind::Text, Some(u.clone())));
                        }
                    }
                    if let Some(p) = &r.item.password {
                        if r.find_field("password").is_none() {
                            srv_fields.push(fld("password", "password", FieldKind::Concealed, Some(p.clone())));
                        }
                    }
                    if !srv_fields.is_empty() {
                        push_section(&mut r, "main", None, srv_fields);
                    }
                }
                if let Some(hist) = de.get("passwordHistory").and_then(Value::as_array) {
                    for entry in hist {
                        let pw = entry.get("value").or_else(|| entry.get("password")).and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty());
                        let Some(pw) = pw else { continue };
                        let replaced_at = if let Some(t) = entry.get("time").and_then(Value::as_i64) {
                            crate::device::iso8601(t)
                        } else {
                            r.item.updated_at.clone()
                        };
                        r.password_history.push(crate::item::PasswordHistoryEntry {
                            value: pw.to_string(),
                            replaced_at,
                        });
                    }
                    r.password_history.sort_by(|a, b| b.replaced_at.cmp(&a.replaced_at));
                    if r.password_history.len() > crate::item::PASSWORD_HISTORY_LIMIT {
                        r.password_history.truncate(crate::item::PASSWORD_HISTORY_LIMIT);
                    }
                }
                out.items.push(r);
            }
        }
    }
    Ok(out)
}


// ---------------------------------------------------------------- .1pif

fn onepif_category(type_name: &str) -> Category {
    let t = type_name.to_ascii_lowercase();
    if t.contains("webform") || t == "passwords.password" || t.ends_with(".login") {
        Category::Login
    } else if t.contains("creditcard") || t.ends_with(".card") {
        Category::CreditCard
    } else if t.contains("bankaccount") {
        Category::BankAccount
    } else if t.contains("identity") || t.contains("passport") || t.contains("driver") {
        Category::Identity
    } else if t.contains("router") || t.contains("wifi") {
        Category::Wifi
    } else if t.contains("server") || t.contains("database") {
        Category::Server
    } else if t.contains("ssh") {
        Category::SshKey
    } else if t.contains("api") {
        Category::ApiCredential
    } else if t.contains("document") {
        Category::Document
    } else {
        Category::SecureNote
    }
}

/// .1pif = text file with one JSON record per line/chunk, separated by lines `***<id>***`.
fn parse_onepif(data: &str) -> Result<Parsed> {
    let mut out = Parsed::default();
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in data.lines() {
        let t = line.trim();
        if t.starts_with("***") && t.ends_with("***") && t.len() >= 6 {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                chunks.push(trimmed.to_string());
                current.clear();
            }
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        chunks.push(trimmed.to_string());
    }

    if chunks.is_empty() {
        bail!("1pif: no records found in export");
    }

    let s = |o: &Value, k: &str| opt(o.get(k).and_then(Value::as_str));

    for (i, chunk) in chunks.iter().enumerate() {
        let v: Value = match serde_json::from_str(chunk) {
            Ok(v) => v,
            Err(e) => {
                out.warnings.push(format!("1pif item {i}: malformed JSON ({e}), skipped"));
                continue;
            }
        };

        let type_name = v.get("typeName").and_then(Value::as_str).unwrap_or("");
        if type_name.starts_with("system.") {
            continue;
        }

        let empty = Value::Null;
        let sec = v.get("secureContents").unwrap_or(&empty);
        let open = v.get("openContents").unwrap_or(&empty);

        let title = s(&v, "title")
            .or_else(|| s(open, "title"))
            .or_else(|| s(sec, "title"));
        let Some(title) = title else {
            out.warnings.push(format!("1pif item {i}: no title, skipped"));
            continue;
        };

        if v.get("trashed").and_then(Value::as_bool) == Some(true)
            || open.get("trashed").and_then(Value::as_bool) == Some(true)
            || sec.get("trashed").and_then(Value::as_bool) == Some(true)
        {
            out.warnings.push(format!("{title}: in source trash, skipped"));
            continue;
        }

        let cat = onepif_category(type_name);
        let mut r = record(title.clone(), cat);

        if cat == Category::SecureNote && !type_name.to_ascii_lowercase().contains("note") && !type_name.is_empty() {
            out.warnings.push(format!("{title}: imported as Secure Note (type: {type_name})"));
        }

        let updated_at = v.get("updatedAt").or_else(|| v.get("modified")).and_then(Value::as_u64)
            .or_else(|| open.get("updatedAt").or_else(|| open.get("modified")).and_then(Value::as_u64))
            .or_else(|| sec.get("updatedAt").or_else(|| sec.get("modified")).and_then(Value::as_u64));
        if let Some(t) = updated_at {
            r.item.updated_at = t.to_string();
        }

        let created_at = v.get("createdAt").or_else(|| v.get("created")).and_then(Value::as_u64)
            .or_else(|| open.get("createdAt").or_else(|| open.get("created")).and_then(Value::as_u64))
            .or_else(|| sec.get("createdAt").or_else(|| sec.get("created")).and_then(Value::as_u64));
        r.created_at = created_at;

        // Tags from openContents.tags or tags
        if let Some(tags) = open.get("tags").or_else(|| v.get("tags")) {
            if let Some(arr) = tags.as_array() {
                for t in arr {
                    if let Some(ts) = t.as_str() { add_tag(&mut r, ts); }
                }
            } else if let Some(s) = tags.as_str() {
                for t in s.split([',', ';']) { add_tag(&mut r, t); }
            }
        }

        r.item.notes = s(sec, "notesPlain")
            .or_else(|| s(sec, "notes"))
            .or_else(|| s(&v, "notesPlain"))
            .or_else(|| s(&v, "notes"));

        // URLs
        let mut urls: Vec<String> = Vec::new();
        if let Some(u) = s(&v, "location").or_else(|| s(sec, "location")).or_else(|| s(open, "location")) {
            if !urls.contains(&u) { urls.push(u); }
        }
        if let Some(u) = s(&v, "url").or_else(|| s(sec, "url")).or_else(|| s(open, "url")) {
            if !urls.contains(&u) { urls.push(u); }
        }
        let url_arr = v.get("URLs")
            .or_else(|| open.get("URLs"))
            .or_else(|| sec.get("URLs"))
            .and_then(Value::as_array);
        if let Some(arr) = url_arr {
            for item in arr {
                let u = s(item, "url").or_else(|| opt(item.as_str()));
                if let Some(u) = u {
                    if !urls.contains(&u) { urls.push(u); }
                }
            }
        }
        let login_like = cat == Category::Login || cat == Category::Server;
        if login_like {
            r.item.url = urls.first().cloned();
        }
        let extra_urls: Vec<Option<Field>> = urls.iter().skip(usize::from(login_like)).enumerate()
            .map(|(n, u)| fld(&format!("url{}", n + 1), "website", FieldKind::Url, Some(u.clone())))
            .collect();
        push_section(&mut r, "urls", Some("Websites"), extra_urls);

        // Map web form fields (secureContents.fields)
        if let Some(fields) = sec.get("fields").and_then(Value::as_array) {
            for f in fields {
                let val = s(f, "value");
                if let Some(ref v) = val {
                    if (v.starts_with("otpauth://") || v.contains("otpauth://totp/")) && r.item.totp.is_none() {
                        r.item.totp = Some(v.clone());
                        continue;
                    }
                }
                let des = s(f, "designation").map(|d| d.to_ascii_lowercase());
                let name = s(f, "name").map(|n| n.to_ascii_lowercase());
                match (des.as_deref(), name.as_deref(), login_like) {
                    (Some("username"), _, true) if r.item.username.is_none() => r.item.username = val,
                    (_, Some("username" | "email"), true) if r.item.username.is_none() => r.item.username = val,
                    (Some("password"), _, true) if r.item.password.is_none() => r.item.password = val,
                    (_, Some("password"), true) if r.item.password.is_none() => r.item.password = val,
                    _ => {
                        if let Some(v) = val {
                            let kind = if s(f, "type").as_deref() == Some("P") { FieldKind::Concealed } else { FieldKind::Text };
                            let label = s(f, "title").or_else(|| s(f, "name")).or_else(|| s(f, "designation")).unwrap_or_else(|| "field".into());
                            let id = next_id(&r, "custom");
                            push_section(&mut r, "custom", Some("Custom fields"), vec![fld(&id, &label, kind, Some(v))]);
                        }
                    }
                }
            }
        }

        // Direct top-level fields in secureContents
        if login_like {
            if r.item.username.is_none() {
                r.item.username = s(sec, "username").or_else(|| s(&v, "username"));
            }
            if r.item.password.is_none() {
                r.item.password = s(sec, "password").or_else(|| s(&v, "password"));
            }
        }

        // Check for TOTP in secureContents top-level
        if r.item.totp.is_none() {
            if let Some(o) = sec.as_object() {
                for (_, val) in o {
                    if let Some(vs) = val.as_str() {
                        if vs.starts_with("otpauth://") || vs.contains("otpauth://totp/") {
                            r.item.totp = Some(vs.to_string());
                            break;
                        }
                    }
                }
            }
        }

        // Category-specific fields from secureContents
        let mut handled_keys = HashSet::new();
        handled_keys.insert("fields".to_string());
        handled_keys.insert("sections".to_string());
        handled_keys.insert("notesPlain".to_string());
        handled_keys.insert("notes".to_string());
        handled_keys.insert("location".to_string());
        handled_keys.insert("url".to_string());
        handled_keys.insert("URLs".to_string());
        handled_keys.insert("passwordHistory".to_string());
        if login_like {
            handled_keys.insert("username".to_string());
            handled_keys.insert("password".to_string());
        }

        if let Some(sec_obj) = sec.as_object() {
            for (k, val) in sec_obj {
                if handled_keys.contains(k) { continue; }
                let val_str = match val {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                };
                let Some(val_str) = val_str.map(|x| x.trim().to_string()).filter(|x| !x.is_empty()) else { continue };

                if let Some(mapped_id) = pux_field_id(cat, k) {
                    let kind = match mapped_id {
                        "number" | "cvv" | "pin" | "password" | "private_key" | "credential" => FieldKind::Concealed,
                        "url" | "admin_console_url" => FieldKind::Url,
                        "email" => FieldKind::Email,
                        "phone" | "branch_phone" => FieldKind::Phone,
                        "address" | "branch_address" | "public_key" => FieldKind::Multiline,
                        "expiry" | "valid_from" => FieldKind::MonthYear,
                        "birth" => FieldKind::Date,
                        _ => FieldKind::Text,
                    };
                    let formatted_val = if mapped_id == "expiry" && val_str.len() == 6 && val_str.chars().all(|c| c.is_ascii_digit()) {
                        format!("{}/{}", &val_str[4..6], &val_str[0..4])
                    } else {
                        val_str
                    };
                    let section_id = match cat {
                        Category::Identity => mapped_id,
                        _ => "main",
                    };
                    push_section(&mut r, section_id, None, vec![fld(mapped_id, mapped_id, kind, Some(formatted_val))]);
                } else {
                    let id = next_id(&r, "custom");
                    push_section(&mut r, "custom", Some("Custom fields"), vec![fld(&id, k, FieldKind::Text, Some(val_str))]);
                }
            }
        }

        // Process secureContents.sections if present
        if let Some(sections) = sec.get("sections").and_then(Value::as_array) {
            for (si, sec_el) in sections.iter().enumerate() {
                let stitle = s(sec_el, "title").or_else(|| s(sec_el, "name"));
                let sid = s(sec_el, "name").filter(|n| !n.is_empty()).unwrap_or_else(|| format!("sec{si}"));
                if let Some(fields) = sec_el.get("fields").and_then(Value::as_array) {
                    for f in fields {
                        let fname = s(f, "n").or_else(|| s(f, "name")).or_else(|| s(f, "id")).unwrap_or_default();
                        let ftitle = s(f, "t").or_else(|| s(f, "title")).unwrap_or_else(|| fname.clone());
                        let fval = s(f, "v").or_else(|| s(f, "value"));
                        let Some(fval) = fval else { continue };

                        if (fval.starts_with("otpauth://") || fval.contains("otpauth://totp/")) && r.item.totp.is_none() {
                            r.item.totp = Some(fval);
                            continue;
                        }

                        if let Some(mapped_id) = pux_field_id(cat, &fname) {
                            let kind = match mapped_id {
                                "number" | "cvv" | "pin" | "password" | "private_key" | "credential" => FieldKind::Concealed,
                                "url" | "admin_console_url" => FieldKind::Url,
                                "email" => FieldKind::Email,
                                "phone" | "branch_phone" => FieldKind::Phone,
                                "address" | "branch_address" | "public_key" => FieldKind::Multiline,
                                "expiry" | "valid_from" => FieldKind::MonthYear,
                                "birth" => FieldKind::Date,
                                _ => FieldKind::Text,
                            };
                            let target = match cat {
                                Category::Identity => mapped_id,
                                _ => "main",
                            };
                            push_section(&mut r, target, None, vec![fld(mapped_id, &ftitle, kind, Some(fval))]);
                        } else {
                            let kind = if s(f, "k").as_deref() == Some("concealed") || fname.to_ascii_lowercase().contains("password") || fname.to_ascii_lowercase().contains("pin") {
                                FieldKind::Concealed
                            } else {
                                FieldKind::Text
                            };
                            let id = next_id(&r, "custom");
                            push_section(&mut r, &sid, stitle.as_deref().or(Some("Custom fields")), vec![fld(&id, &ftitle, kind, Some(fval))]);
                        }
                    }
                }
            }
        }

        if let Some(hist) = sec.get("passwordHistory").or_else(|| v.get("passwordHistory")).and_then(Value::as_array) {
            for entry in hist {
                let pw = entry.get("value").or_else(|| entry.get("password")).and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty());
                let Some(pw) = pw else { continue };
                let replaced_at = if let Some(t) = entry.get("time").and_then(Value::as_i64) {
                    crate::device::iso8601(t)
                } else if let Some(t_str) = entry.get("time").and_then(Value::as_str).and_then(|s| s.parse::<i64>().ok()) {
                    crate::device::iso8601(t_str)
                } else if let Some(d) = entry.get("date").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
                    d.to_string()
                } else {
                    r.item.updated_at.clone()
                };
                r.password_history.push(crate::item::PasswordHistoryEntry {
                    value: pw.to_string(),
                    replaced_at,
                });
            }
            r.password_history.sort_by(|a, b| b.replaced_at.cmp(&a.replaced_at));
            if r.password_history.len() > crate::item::PASSWORD_HISTORY_LIMIT {
                r.password_history.truncate(crate::item::PASSWORD_HISTORY_LIMIT);
            }
        }

        out.items.push(r);
    }

    Ok(out)
}

// ---------------------------------------------------------------- gitpass JSON (plain + encrypted)

#[derive(Serialize, Deserialize)]
struct PlainExport { gitpass_export: u32, items: Vec<ExportRecord> }

/// One exported item: the legacy login fields (flattened, so v1 files still parse) plus the v5
/// category / sections / tags / flags. Clocks, history and attachments are not exported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExportRecord {
    #[serde(flatten)]
    pub item: Item,
    #[serde(default, skip_serializing_if = "is_login")]
    pub category: Category,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<Section>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub favorite: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub password_history: Vec<crate::item::PasswordHistoryEntry>,
}
fn is_login(c: &Category) -> bool { *c == Category::Login }
fn is_false(b: &bool) -> bool { !*b }
impl From<&ItemRecord> for ExportRecord {
    fn from(r: &ItemRecord) -> Self {
        let sections = r.sections.iter().map(|s| Section { id: s.id.clone(), title: s.title.clone(), fields: s.fields.iter().filter(|f| !f.removed).cloned().collect() })
            .filter(|s| !s.fields.is_empty() || s.title.is_some()).collect();
        ExportRecord { item: r.item.clone(), category: r.category, sections, tags: r.tags.clone(), favorite: r.favorite, archived: r.archived, created_at: r.created_at, password_history: r.password_history.clone() }
    }
}
impl From<ExportRecord> for ItemRecord {
    fn from(e: ExportRecord) -> Self {
        let mut r = ItemRecord::new(e.item); r.category = e.category; r.sections = e.sections; r.tags = e.tags;
        r.favorite = e.favorite; r.archived = e.archived; r.created_at = e.created_at;
        r.password_history = e.password_history; r
    }
}
fn plain_item(it: &Item) -> ExportRecord {
    ExportRecord { item: it.clone(), category: Category::Login, sections: vec![], tags: vec![], favorite: false, archived: false, created_at: None, password_history: vec![] }
}

#[derive(Serialize, Deserialize)]
struct EncryptedExport { gitpass_encrypted_export: u32, kdf: KdfParams, salt: String, envelope: Envelope }

const EXPORT_AAD: &[u8] = b"gitpass-export-v1";

/// Export KDF parameters (same Argon2id defaults as the vault). Tests may pass cheaper ones.
pub fn export_kdf_default() -> KdfParams { KdfParams::default() }

fn passphrase_key(pass: &str, salt: &[u8], kdf: &KdfParams) -> Result<Key32> {
    if kdf.alg != "argon2id" { bail!("unsupported export kdf {}", kdf.alg); }
    if kdf.m_kib < crate::crypto::KDF_FLOOR_M_KIB || kdf.t < 1 { bail!("export kdf parameters below safety floor"); }
    let params = Params::new(kdf.m_kib, kdf.t, kdf.p, Some(32)).map_err(|e| anyhow!("argon2 params: {e}"))?;
    let mut k = Key32([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pass.as_bytes(), salt, &mut k.0).map_err(|e| anyhow!("argon2: {e}"))?;
    Ok(k)
}

/// Plaintext gitpass-json export.
pub fn export_json(items: &[Item]) -> Result<String> {
    Ok(serde_json::to_string_pretty(&PlainExport { gitpass_export: 1, items: items.iter().map(plain_item).collect() })?)
}

/// Plaintext gitpass-json export of full records (category, sections, tags, favorite, archived).
/// Soft-deleted records are skipped.
pub fn export_records_json(recs: &[ItemRecord]) -> Result<String> {
    Ok(serde_json::to_string_pretty(&PlainExport { gitpass_export: 1, items: recs.iter().filter(|r| r.deleted_at.is_none()).map(ExportRecord::from).collect() })?)
}

/// Passphrase-protected export of full records.
pub fn export_records_encrypted(recs: &[ItemRecord], passphrase: &str, kdf: &KdfParams) -> Result<String> {
    encrypt_export(export_records_json(recs)?, passphrase, kdf)
}

/// Passphrase-protected export: Argon2id(passphrase, random salt) -> XChaCha20-Poly1305 over the plain JSON.
pub fn export_encrypted(items: &[Item], passphrase: &str, kdf: &KdfParams) -> Result<String> {
    encrypt_export(export_json(items)?, passphrase, kdf)
}

fn encrypt_export(json: String, passphrase: &str, kdf: &KdfParams) -> Result<String> {
    if passphrase.len() < 8 { bail!("export passphrase must be at least 8 characters"); }
    let salt: [u8; 16] = random_bytes();
    let key = passphrase_key(passphrase, &salt, kdf)?;
    let mut pt = json.into_bytes();
    let env = seal(&key, &pt, EXPORT_AAD);
    zeroize::Zeroize::zeroize(&mut pt);
    Ok(serde_json::to_string_pretty(&EncryptedExport { gitpass_encrypted_export: 1, kdf: kdf.clone(), salt: B64.encode(salt), envelope: env? })?)
}

/// True if `data` looks like an encrypted gitpass export (so the CLI knows to ask for a passphrase).
pub fn is_encrypted_export(data: &str) -> bool {
    serde_json::from_str::<Value>(data).ok().and_then(|v| v.get("gitpass_encrypted_export").cloned()).is_some()
}

fn parse_gitpass_json(data: &str, passphrase: Option<&str>) -> Result<Parsed> {
    let v: Value = serde_json::from_str(data).context("gitpass-json: invalid JSON")?;
    if v.get("gitpass_encrypted_export").is_some() {
        let e: EncryptedExport = serde_json::from_value(v)?;
        let pass = passphrase.ok_or_else(|| anyhow!("gitpass-json: file is encrypted; a passphrase is required"))?;
        let key = passphrase_key(pass, &B64.decode(&e.salt)?, &e.kdf)?;
        let pt = zeroize::Zeroizing::new(open(&key, &e.envelope, EXPORT_AAD).map_err(|_| anyhow!("gitpass-json: wrong passphrase or corrupted file"))?);
        return parse_gitpass_json(std::str::from_utf8(&pt)?, None);
    }
    let p: PlainExport = serde_json::from_value(v).context("gitpass-json: expected {\"gitpass_export\":1,\"items\":[...]}")?;
    if p.gitpass_export != 1 { bail!("gitpass-json: unsupported export version {}", p.gitpass_export); }
    Ok(Parsed { items: p.items.into_iter().map(ItemRecord::from).collect(), warnings: vec![] })
}

/// CSV export: title,username,password,url,notes,totp,updated_at
pub fn export_csv(items: &[Item]) -> Result<String> {
    let mut w = csv::Writer::from_writer(vec![]);
    w.write_record(["title", "username", "password", "url", "notes", "totp", "updated_at"])?;
    for it in items {
        let o = |x: &Option<String>| x.clone().unwrap_or_default();
        w.write_record([it.title.clone(), o(&it.username), o(&it.password), o(&it.url), o(&it.notes), o(&it.totp), it.updated_at.clone()])?;
    }
    Ok(String::from_utf8(w.into_inner().map_err(|e| anyhow!("csv: {e}"))?)?)
}

// ---------------------------------------------------------------- write into a vault

#[derive(Default, Debug, Clone, Copy)]
pub struct ImportOpts { pub dry_run: bool, pub allow_duplicates: bool }

#[derive(Default, Debug)]
pub struct ImportReport {
    pub parsed: usize,
    pub added: usize,
    /// Titles of skipped duplicates (same title + username + url as an existing or earlier item).
    pub duplicates: Vec<String>,
    pub warnings: Vec<String>,
}

fn dedup_key(it: &Item) -> (String, String, String) {
    let n = |x: &Option<String>| x.as_deref().unwrap_or("").trim().to_lowercase();
    (it.title.trim().to_lowercase(), n(&it.username), n(&it.url))
}

/// Add parsed records to vault `vid`. With `dry_run`, nothing is written but counts are computed.
/// Duplicates (same title + username + url, case-insensitive, against live items already in the vault
/// or earlier rows of the same file) are skipped unless `allow_duplicates`.
pub fn import_items(s: &Session, vid: &str, parsed: Parsed, opts: ImportOpts) -> Result<ImportReport> {
    let mut seen: HashSet<_> = s.list_records(vid)?.iter().filter(|(_, _, r)| r.deleted_at.is_none()).map(|(_, _, r)| dedup_key(&r.item)).collect();
    let mut r = ImportReport { parsed: parsed.items.len(), warnings: parsed.warnings, ..Default::default() };
    for rec in parsed.items {
        if !seen.insert(dedup_key(&rec.item)) && !opts.allow_duplicates { r.duplicates.push(rec.item.title.clone()); continue; }
        if !opts.dry_run { s.add_record(vid, rec)?; }
        r.added += 1;
    }
    Ok(r)
}

/// Auto-detect the import format from text content or headers.
pub fn detect_format(data: &str) -> Option<ImportFormat> {
    // Check .1pif separator and typeName
    if data.contains("\"typeName\"") && data.lines().any(|l| {
        let t = l.trim();
        t.starts_with("***") && t.ends_with("***") && t.len() >= 6
    }) {
        return Some(ImportFormat::OnePif);
    }
    let data = data.strip_prefix('\u{feff}').unwrap_or(data);
    let trimmed = data.trim_start();
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            if v.get("gitpass_export").is_some() || v.get("gitpass_encrypted_export").is_some() {
                return Some(ImportFormat::GitpassJson);
            }
            if v.get("items").is_some() || v.get("folders").is_some() || v.get("collections").is_some()
                || v.get("encKeyValidation_DO_NOT_EDIT").is_some()
                || (v.get("encrypted").and_then(Value::as_bool) == Some(true) && v.get("data").is_some()) {
                return Some(ImportFormat::BitwardenJson);
            }
        }
    }
    // Check CSV header
    let first_line = trimmed.lines().next()?;
    let mut rdr = csv::ReaderBuilder::new().flexible(true).trim(csv::Trim::Headers).from_reader(first_line.as_bytes());
    let headers: Vec<String> = rdr.headers().ok()?.iter().map(|h| h.trim().to_ascii_lowercase()).collect();
    if headers.is_empty() { return None; }

    // Bitwarden CSV distinctive headers
    if headers.iter().any(|h| ["login_username", "login_uri", "login_password", "login_totp", "reprompt"].contains(&h.as_str())) {
        return Some(ImportFormat::BitwardenCsv);
    }

    // LastPass CSV: url,username,password,totp,extra,name,grouping,fav
    if headers.contains(&"grouping".to_string()) || (headers.contains(&"extra".to_string()) && headers.contains(&"fav".to_string())) {
        return Some(ImportFormat::LastpassCsv);
    }

    // Proton Pass CSV: type,name,url,email,username,password,note,totp,createTime,modifyTime,vault
    if (headers.contains(&"createtime".to_string()) && headers.contains(&"modifytime".to_string()))
        || (headers.contains(&"vault".to_string()) && headers.contains(&"email".to_string()))
        || (headers.contains(&"type".to_string()) && headers.contains(&"email".to_string()) && headers.contains(&"username".to_string()))
    {
        return Some(ImportFormat::ProtonPassCsv);
    }

    // KeePass CSV distinctive headers
    if headers.iter().any(|h| ["group", "account", "login name", "web site", "comments", "icon", "last modified"].contains(&h.as_str())) {
        return Some(ImportFormat::KeepassCsv);
    }

    // Apple Passwords vs generic CSV (Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes):
    // Apple Passwords CSV header: Title,URL,Username,Password,Notes,OTPAuth
    if headers.contains(&"otpauth".to_string()) {
        if headers.iter().any(|h| ["favorite", "archived", "tags"].contains(&h.as_str())) {
            return Some(ImportFormat::GenericCsv);
        }
        if headers.contains(&"title".to_string()) || headers.contains(&"url".to_string()) {
            return Some(ImportFormat::ApplePasswordsCsv);
        }
    }

    // Generic CSV (without otpauth) or containing favorite/archived/tags/notesplain
    if headers.iter().any(|h| ["favorite", "archived", "tags", "notesplain"].contains(&h.as_str())) {
        return Some(ImportFormat::GenericCsv);
    }

    // Chrome CSV: name,url,username,password
    if headers.contains(&"url".to_string())
        && headers.contains(&"username".to_string())
        && headers.contains(&"password".to_string())
        && (headers.contains(&"name".to_string())
            || headers.contains(&"origin".to_string())
            || headers.contains(&"origin_url".to_string()))
    {
        return Some(ImportFormat::ChromeCsv);
    }

    None
}

/// Auto-detect the import format from raw file bytes.
pub fn detect_format_bytes(data: &[u8]) -> Option<ImportFormat> {
    if data.starts_with(&[0x50, 0x4b, 0x03, 0x04]) {
        if let Ok(mut z) = zip::ZipArchive::new(std::io::Cursor::new(data)) {
            if z.by_name("export.data").is_ok() {
                return Some(ImportFormat::Pux);
            }
        }
    }
    let s = std::str::from_utf8(data).ok()?;
    detect_format(s)
}
