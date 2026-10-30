use super::common::{add_tag, append_note, fld, next_id, opt, push_section, record};
use super::Parsed;
use crate::item::{Category, Field, FieldKind};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;

// ---------------------------------------------------------------- .1pux

/// 1PUX = zip with `export.data` (JSON: accounts[].vaults[].items[]) plus attachment files.
pub(crate) fn parse_1pux(data: &[u8]) -> Result<Parsed> {
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
pub(crate) fn pux_field_id(cat: Category, id: &str) -> Option<&'static str> {
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

pub(crate) fn parse_1pux_json(data: &str) -> Result<Parsed> {
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
