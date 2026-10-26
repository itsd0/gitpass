//! v4 data import / export.
//!
//! Importers parse vendor export files into [`Item`]s; [`import_items`] writes them through the
//! public [`Session`] API (no on-disk format changes). Column lookup in CSV formats is
//! case-insensitive and accepts several aliases per field, because vendors have changed their
//! header names across versions. Any non-empty column that is not mapped is appended to `notes`
//! as `Column: value` so nothing is silently dropped.
use crate::crypto::{open, random_bytes, seal, Envelope, KdfParams, Key32};
use crate::store::{now, Item, Session};
use anyhow::{anyhow, bail, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat { BitwardenJson, BitwardenCsv, OnePasswordCsv, KeepassCsv, ChromeCsv, GitpassJson }

impl std::str::FromStr for ImportFormat {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "bitwarden-json" => Self::BitwardenJson,
            "bitwarden-csv" => Self::BitwardenCsv,
            "1password-csv" => Self::OnePasswordCsv,
            "keepass-csv" | "keepassxc-csv" => Self::KeepassCsv,
            "chrome-csv" => Self::ChromeCsv,
            "gitpass-json" => Self::GitpassJson,
            o => bail!("unknown import format '{o}' (bitwarden-json, bitwarden-csv, 1password-csv, keepass-csv, chrome-csv, gitpass-json)"),
        })
    }
}

/// Result of parsing a file: items plus human-readable warnings (skipped rows, unknown columns...).
#[derive(Default)]
pub struct Parsed { pub items: Vec<Item>, pub warnings: Vec<String> }

/// Parse `data` in `format`. `passphrase` is only needed for encrypted gitpass-json exports.
pub fn parse(format: ImportFormat, data: &str, passphrase: Option<&str>) -> Result<Parsed> {
    let data = data.strip_prefix('\u{feff}').unwrap_or(data); // tolerate UTF-8 BOM (Excel / 1Password)
    match format {
        ImportFormat::BitwardenJson => parse_bitwarden_json(data),
        ImportFormat::GitpassJson => parse_gitpass_json(data, passphrase),
        ImportFormat::BitwardenCsv => parse_csv(data, &BITWARDEN_CSV),
        ImportFormat::OnePasswordCsv => parse_csv(data, &ONEPASSWORD_CSV),
        ImportFormat::KeepassCsv => parse_csv(data, &KEEPASS_CSV),
        ImportFormat::ChromeCsv => parse_csv(data, &CHROME_CSV),
    }
}

fn opt(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|v| !v.is_empty()).map(String::from)
}

fn append_note(notes: &mut Option<String>, line: String) {
    match notes { Some(n) => { n.push('\n'); n.push_str(&line); } None => *notes = Some(line) }
}

// ---------------------------------------------------------------- CSV

/// Column aliases (lower-case) per target field. Columns listed in `ignore` are dropped silently.
struct CsvSpec {
    name: &'static str,
    title: &'static [&'static str],
    username: &'static [&'static str],
    password: &'static [&'static str],
    url: &'static [&'static str],
    notes: &'static [&'static str],
    totp: &'static [&'static str],
    modified: &'static [&'static str],
    ignore: &'static [&'static str],
    /// (column, value) — rows whose column equals value are not logins and are imported as notes-only.
    note_type: Option<(&'static str, &'static str)>,
}

// Bitwarden: folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp
const BITWARDEN_CSV: CsvSpec = CsvSpec {
    name: "bitwarden-csv",
    title: &["name"], username: &["login_username"], password: &["login_password"],
    url: &["login_uri"], notes: &["notes"], totp: &["login_totp"], modified: &[],
    ignore: &["favorite", "reprompt", "type"], note_type: Some(("type", "note")),
};
// 1Password 8 CSV: Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes
// (older 1Password 7 exports used "URL"/"Website", "One-time password", "notesPlain").
const ONEPASSWORD_CSV: CsvSpec = CsvSpec {
    name: "1password-csv",
    title: &["title", "name"], username: &["username", "user name", "login"], password: &["password"],
    url: &["url", "website", "urls", "location"], notes: &["notes", "notesplain", "note"],
    totp: &["otpauth", "one-time password", "totp", "otp"], modified: &[],
    ignore: &["favorite", "archived"], note_type: None,
};
// KeePassXC: "Group","Title","Username","Password","URL","Notes","TOTP","Icon","Last Modified","Created"
const KEEPASS_CSV: CsvSpec = CsvSpec {
    name: "keepass-csv",
    title: &["title", "account"], username: &["username", "login name", "user name"], password: &["password"],
    url: &["url", "web site"], notes: &["notes", "comments"], totp: &["totp", "otp"],
    modified: &["last modified"], ignore: &["icon", "created"], note_type: None,
};
// Chrome / Edge / Brave: name,url,username,password[,note]
const CHROME_CSV: CsvSpec = CsvSpec {
    name: "chrome-csv",
    title: &["name", "origin"], username: &["username"], password: &["password"],
    url: &["url", "origin_url"], notes: &["note", "notes"], totp: &[], modified: &[],
    ignore: &[], note_type: None,
};

fn parse_csv(data: &str, spec: &CsvSpec) -> Result<Parsed> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).trim(csv::Trim::Headers).from_reader(data.as_bytes());
    let headers: Vec<String> = rdr.headers().with_context(|| format!("{}: reading header row", spec.name))?
        .iter().map(|h| h.trim().to_string()).collect();
    let lower: Vec<String> = headers.iter().map(|h| h.to_ascii_lowercase()).collect();
    let find = |aliases: &[&str]| aliases.iter().find_map(|a| lower.iter().position(|h| h == a));
    let cols = [find(spec.title), find(spec.username), find(spec.password), find(spec.url), find(spec.notes), find(spec.totp), find(spec.modified)];
    let mut out = Parsed::default();
    if cols[0].is_none() && cols[3].is_none() {
        bail!("{}: no title/name or url column found in header {:?}", spec.name, headers);
    }
    let used: HashSet<usize> = cols.iter().flatten().copied().collect();
    let type_col = spec.note_type.and_then(|(c, _)| lower.iter().position(|h| h == c));
    for (row, rec) in rdr.records().enumerate() {
        let rec = rec.with_context(|| format!("{}: row {}", spec.name, row + 2))?;
        let get = |i: Option<usize>| i.and_then(|i| rec.get(i)).and_then(|v| opt(Some(v)));
        let url = get(cols[3]);
        let title = get(cols[0]).or_else(|| url.clone());
        let Some(title) = title else { out.warnings.push(format!("row {}: no title or url, skipped", row + 2)); continue };
        let mut notes = get(cols[4]);
        for (i, h) in headers.iter().enumerate() {
            if used.contains(&i) || spec.ignore.contains(&lower[i].as_str()) { continue; }
            if let Some(v) = rec.get(i).and_then(|v| opt(Some(v))) { append_note(&mut notes, format!("{h}: {v}")); }
        }
        let is_note = matches!((spec.note_type, type_col), (Some((_, want)), Some(tc)) if rec.get(tc).map(|v| v.trim().eq_ignore_ascii_case(want)).unwrap_or(false));
        if is_note { append_note(&mut notes, "(imported as secure note)".into()); }
        out.items.push(Item {
            title, username: get(cols[1]), password: get(cols[2]), url, notes, totp: get(cols[5]),
            updated_at: get(cols[6]).and_then(|v| iso_to_epoch(&v)).map(|s| s.to_string()).unwrap_or_else(now),
        });
    }
    Ok(out)
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

// ---------------------------------------------------------------- Bitwarden JSON

fn parse_bitwarden_json(data: &str) -> Result<Parsed> {
    let v: Value = serde_json::from_str(data).context("bitwarden-json: invalid JSON")?;
    if v.get("encrypted").and_then(Value::as_bool) == Some(true) {
        bail!("bitwarden-json: this is an ENCRYPTED Bitwarden export; re-export as unencrypted JSON (or CSV)");
    }
    let folders: BTreeMap<String, String> = v.get("folders").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|f| Some((f.get("id")?.as_str()?.to_string(), f.get("name")?.as_str()?.to_string()))).collect();
    let items = v.get("items").and_then(Value::as_array).ok_or_else(|| anyhow!("bitwarden-json: missing \"items\" array"))?;
    let s = |o: &Value, k: &str| opt(o.get(k).and_then(Value::as_str));
    let mut out = Parsed::default();
    for (i, it) in items.iter().enumerate() {
        let Some(title) = s(it, "name") else { out.warnings.push(format!("item {i}: no name, skipped")); continue };
        let mut notes = s(it, "notes");
        let (mut username, mut password, mut url, mut totp) = (None, None, None, None);
        if let Some(fid) = s(it, "folderId") { if let Some(f) = folders.get(&fid) { append_note(&mut notes, format!("Folder: {f}")); } }
        match it.get("type").and_then(Value::as_u64) {
            Some(1) | None => {
                if let Some(l) = it.get("login") {
                    username = s(l, "username"); password = s(l, "password"); totp = s(l, "totp");
                    let uris: Vec<String> = l.get("uris").and_then(Value::as_array).into_iter().flatten()
                        .filter_map(|u| s(u, "uri")).collect();
                    url = uris.first().cloned();
                    for extra in uris.iter().skip(1) { append_note(&mut notes, format!("Additional URL: {extra}")); }
                }
            }
            Some(t) => {
                let (kind, key) = match t { 2 => ("Secure note", ""), 3 => ("Card", "card"), 4 => ("Identity", "identity"), _ => ("Item", "") };
                append_note(&mut notes, format!("Bitwarden type: {kind}"));
                if let Some(Value::Object(o)) = it.get(key) {
                    for (k, val) in o { if let Some(x) = val.as_str().filter(|x| !x.is_empty()) { append_note(&mut notes, format!("{k}: {x}")); } }
                }
            }
        }
        for f in it.get("fields").and_then(Value::as_array).into_iter().flatten() {
            if let (Some(n), Some(val)) = (s(f, "name"), s(f, "value")) { append_note(&mut notes, format!("{n}: {val}")); }
        }
        let updated_at = s(it, "revisionDate").and_then(|d| iso_to_epoch(&d)).map(|x| x.to_string()).unwrap_or_else(now);
        out.items.push(Item { title, username, password, url, notes, totp, updated_at });
    }
    Ok(out)
}

// ---------------------------------------------------------------- gitpass JSON (plain + encrypted)

#[derive(Serialize, Deserialize)]
struct PlainExport { gitpass_export: u32, items: Vec<Item> }

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
    Ok(serde_json::to_string_pretty(&PlainExport { gitpass_export: 1, items: items.to_vec() })?)
}

/// Passphrase-protected export: Argon2id(passphrase, random salt) -> XChaCha20-Poly1305 over the plain JSON.
pub fn export_encrypted(items: &[Item], passphrase: &str, kdf: &KdfParams) -> Result<String> {
    if passphrase.len() < 8 { bail!("export passphrase must be at least 8 characters"); }
    let salt: [u8; 16] = random_bytes();
    let key = passphrase_key(passphrase, &salt, kdf)?;
    let mut pt = export_json(items)?.into_bytes();
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
    Ok(Parsed { items: p.items, warnings: vec![] })
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

/// Add parsed items to vault `vid`. With `dry_run`, nothing is written but counts are computed.
pub fn import_items(s: &Session, vid: &str, parsed: Parsed, opts: ImportOpts) -> Result<ImportReport> {
    let mut seen: HashSet<_> = s.list_items(vid)?.iter().map(|(_, it)| dedup_key(it)).collect();
    let mut r = ImportReport { parsed: parsed.items.len(), warnings: parsed.warnings, ..Default::default() };
    for it in &parsed.items {
        if !seen.insert(dedup_key(it)) && !opts.allow_duplicates { r.duplicates.push(it.title.clone()); continue; }
        if !opts.dry_run { s.add_item(vid, it)?; }
        r.added += 1;
    }
    Ok(r)
}
