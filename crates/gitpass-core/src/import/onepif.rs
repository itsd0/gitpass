use super::common::{add_tag, fld, next_id, opt, push_section, record};
use super::pux::pux_field_id;
use super::Parsed;
use crate::item::{Category, Field, FieldKind};
use std::collections::HashSet;
use anyhow::{bail, Result};
use serde_json::Value;

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
pub(crate) fn parse_onepif(data: &str) -> Result<Parsed> {
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
