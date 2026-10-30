use super::common::{add_tag, append_note, fld, opt, parse_time, push_section, record, truthy};
use super::Parsed;
use crate::item::{Category, Field, FieldKind};
use std::collections::HashSet;
use anyhow::{bail, Context, Result};

// ---------------------------------------------------------------- CSV

/// Column aliases (lower-case) per target field. Columns listed in `ignore` are dropped silently.
pub(crate) struct CsvSpec {
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
pub(crate) const BITWARDEN_CSV: CsvSpec = CsvSpec {
    name: "bitwarden-csv",
    title: &["name"], username: &["login_username"], email: &[], password: &["login_password"],
    url: &["login_uri"], notes: &["notes"], totp: &["login_totp"], created: &[], modified: &[],
    tags: &["folder"], tag_sep: &[], favorite: &["favorite"], archived: &[],
    ignore: &["reprompt", "type"], note_type: Some(("type", "note")), category: &["type"],
};
// Generic CSV: Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes
pub(crate) const GENERIC_CSV: CsvSpec = CsvSpec {
    name: "generic-csv",
    title: &["title", "name"], username: &["username", "user name", "login"], email: &[], password: &["password"],
    url: &["url", "website", "urls", "location"], notes: &["notes", "notesplain", "note"],
    totp: &["otpauth", "one-time password", "totp", "otp"], created: &[], modified: &[],
    tags: &["tags"], tag_sep: &[',', ';'], favorite: &["favorite"], archived: &["archived"],
    ignore: &[], note_type: None, category: &["type", "category"],
};
// KeePassXC: "Group","Title","Username","Password","URL","Notes","TOTP","Icon","Last Modified","Created"
// KeePass 2 (generic CSV): "Account","Login Name","Password","Web Site","Comments"
pub(crate) const KEEPASS_CSV: CsvSpec = CsvSpec {
    name: "keepass-csv",
    title: &["title", "account"], username: &["username", "login name", "user name"], email: &[], password: &["password"],
    url: &["url", "web site"], notes: &["notes", "comments"], totp: &["totp", "otp"],
    created: &[], modified: &["last modified"], tags: &["group"], tag_sep: &[], favorite: &[], archived: &[],
    ignore: &["icon", "created"], note_type: None, category: &[],
};
// Chrome / Edge / Brave: name,url,username,password[,note]
pub(crate) const CHROME_CSV: CsvSpec = CsvSpec {
    name: "chrome-csv",
    title: &["name", "origin"], username: &["username"], email: &[], password: &["password"],
    url: &["url", "origin_url"], notes: &["note", "notes"], totp: &[], created: &[], modified: &[],
    tags: &[], tag_sep: &[], favorite: &[], archived: &[], ignore: &[], note_type: None, category: &[],
};
// LastPass: url,username,password,totp,extra,name,grouping,fav
pub(crate) const LASTPASS_CSV: CsvSpec = CsvSpec {
    name: "lastpass-csv",
    title: &["name"], username: &["username"], email: &[], password: &["password"],
    url: &["url"], notes: &["extra", "notes", "note"], totp: &["totp"], created: &[], modified: &[],
    tags: &["grouping"], tag_sep: &[], favorite: &["fav", "favorite"], archived: &[],
    ignore: &[], note_type: None, category: &[],
};
// Proton Pass: type,name,url,email,username,password,note,totp,createTime,modifyTime,vault
pub(crate) const PROTONPASS_CSV: CsvSpec = CsvSpec {
    name: "protonpass-csv",
    title: &["name", "title"], username: &["username"], email: &["email"], password: &["password"],
    url: &["url"], notes: &["note", "notes"], totp: &["totp"],
    created: &["createtime", "create_time"], modified: &["modifytime", "modify_time"],
    tags: &["vault"], tag_sep: &[], favorite: &[], archived: &[],
    ignore: &["type"], note_type: Some(("type", "note")), category: &["type"],
};
// Apple Passwords / Safari CSV (macOS 15 Passwords app / Safari export): Title,URL,Username,Password,Notes,OTPAuth
pub(crate) const APPLE_PASSWORDS_CSV: CsvSpec = CsvSpec {
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

pub(crate) fn parse_csv(data: &str, spec: &CsvSpec) -> Result<Parsed> {
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
