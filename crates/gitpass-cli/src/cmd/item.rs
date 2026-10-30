use anyhow::Result;
use crate::cli::*;
use crate::cmd::common::*;
use std::path::Path;
use gitpass_core::item::Section;
use gitpass_core::store::Item;
use std::io::Read;
use gitpass_core::item::Category;
use gitpass_core::item::FieldKind;
use gitpass_core::store;
use gitpass_pasteboard::PasteboardMarkers;
use anyhow::Context;
use anyhow::bail;
use gitpass_core::crypto::generate_password;
use gitpass_core::merge::ItemRecord;

pub(crate) fn run(cli: &Cli, root: &Path, cmd: &Cmd) -> Result<()> {
    match cmd {
        Cmd::Add {
            vault,
            category,
            title,
            username,
            url,
            totp,
            generate,
            password_stdin,
            credential_stdin,
            expires,
            notes,
            notes_stdin,
            holder,
            expiry,
            bank,
            account_type,
            routing,
            branch_phone,
            branch_address,
            account_number_stdin,
            pin_stdin,
            admin_url,
            provider,
            first_name,
            last_name,
            email,
            phone,
            address,
            ssid,
            security,
            private_key_file,
            credential: _,
        } => {
            if title.trim().is_empty() {
                bail!("title cannot be empty");
            }
            validate_add_flags(&AddFlags {
                category: *category,
                username,
                url,
                totp,
                generate,
                password_stdin: *password_stdin,
                credential_stdin: *credential_stdin,
                expires,
                notes,
                notes_stdin: *notes_stdin,
                holder,
                expiry,
                bank,
                account_type,
                routing,
                branch_phone,
                branch_address,
                account_number_stdin: *account_number_stdin,
                pin_stdin: *pin_stdin,
                admin_url,
                provider,
                first_name,
                last_name,
                email,
                phone,
                address,
                ssid,
                security,
                private_key_file,
            })?;
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;

            let rec = match category {
                AddCategory::Login => {
                    let password = if *password_stdin {
                        let mut p = String::new();
                        std::io::stdin().read_line(&mut p)?;
                        let p = p.trim_end_matches(['\r', '\n']).to_string();
                        if p.is_empty() { None } else { Some(p) }
                    } else if let Some(n) = generate {
                        let len = n.parse::<usize>().unwrap_or(24);
                        Some(generate_password(len))
                    } else {
                        let p = prompt_secret("GITPASS_ITEM_PASSWORD", "Item password (empty for none): ")?;
                        if p.is_empty() { None } else { Some(p.to_string()) }
                    };
                    let it = Item {
                        title: title.clone(),
                        username: username.clone(),
                        password,
                        url: url.clone(),
                        notes: notes.clone(),
                        totp: totp.clone(),
                        updated_at: store::now(),
                    };
                    ItemRecord::new(it)
                }
                AddCategory::ApiCredential => {
                    let cred_val = if *credential_stdin {
                        let mut c = String::new();
                        std::io::stdin().read_line(&mut c)?;
                        let c = c.trim_end_matches(['\r', '\n']).to_string();
                        if c.is_empty() {
                            bail!("credential from stdin cannot be empty");
                        }
                        c
                    } else {
                        let p = prompt_secret("GITPASS_ITEM_CREDENTIAL", "API credential: ")?;
                        if p.is_empty() {
                            bail!("credential cannot be empty");
                        }
                        p.to_string()
                    };
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: None,
                        url: url.clone(),
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut fields = Vec::new();
                    if let Some(u) = username.as_ref().filter(|u| !u.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "username".into(),
                            label: "username".into(),
                            kind: FieldKind::Text,
                            value: Some(u.clone()),
                            removed: false,
                        });
                    }
                    fields.push(gitpass_core::item::Field {
                        id: "credential".into(),
                        label: "credential".into(),
                        kind: FieldKind::Concealed,
                        value: Some(cred_val),
                        removed: false,
                    });
                    if let Some(u) = url.as_ref().filter(|u| !u.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "hostname".into(),
                            label: "hostname".into(),
                            kind: FieldKind::Url,
                            value: Some(u.clone()),
                            removed: false,
                        });
                    }
                    if let Some(exp) = expires.as_ref().filter(|e| !e.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "expires".into(),
                            label: "expires".into(),
                            kind: FieldKind::Date,
                            value: Some(exp.clone()),
                            removed: false,
                        });
                    }
                    let mut r = ItemRecord::new(it);
                    r.category = Category::ApiCredential;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields,
                    }];
                    r
                }
                AddCategory::SecureNote => {
                    let note_text = if *notes_stdin {
                        let mut buf = String::new();
                        std::io::stdin().read_to_string(&mut buf)?;
                        buf.trim_end_matches(['\r', '\n']).to_string()
                    } else if let Some(n) = notes {
                        n.clone()
                    } else {
                        String::new()
                    };
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: None,
                        url: None,
                        notes: None,
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut r = ItemRecord::new(it);
                    r.category = Category::SecureNote;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields: vec![gitpass_core::item::Field {
                            id: "note".into(),
                            label: "note".into(),
                            kind: FieldKind::Multiline,
                            value: Some(note_text),
                            removed: false,
                        }],
                    }];
                    r
                }
                AddCategory::CreditCard => {
                    let num = prompt_secret("GITPASS_CARD_NUMBER", "Card number: ")?;
                    if num.trim().is_empty() {
                        bail!("card number cannot be empty");
                    }
                    let cvv = prompt_secret("GITPASS_CARD_CVV", "Card CVV / verification code (empty for none): ")?;
                    let brand = gitpass_core::item::card_brand(&num);
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: None,
                        url: None,
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut fields = Vec::new();
                    if let Some(h) = holder.as_ref().filter(|h| !h.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "cardholder".into(),
                            label: "cardholder name".into(),
                            kind: FieldKind::Text,
                            value: Some(h.clone()),
                            removed: false,
                        });
                    }
                    if let Some(b) = brand {
                        fields.push(gitpass_core::item::Field {
                            id: "type".into(),
                            label: "type".into(),
                            kind: FieldKind::Text,
                            value: Some(b.to_string()),
                            removed: false,
                        });
                    }
                    fields.push(gitpass_core::item::Field {
                        id: "number".into(),
                        label: "number".into(),
                        kind: FieldKind::Concealed,
                        value: Some(num.to_string()),
                        removed: false,
                    });
                    if !cvv.trim().is_empty() {
                        fields.push(gitpass_core::item::Field {
                            id: "cvv".into(),
                            label: "verification number".into(),
                            kind: FieldKind::Concealed,
                            value: Some(cvv.to_string()),
                            removed: false,
                        });
                    }
                    if let Some(exp) = expiry.as_ref().filter(|e| !e.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "expiry".into(),
                            label: "expiry date".into(),
                            kind: FieldKind::MonthYear,
                            value: Some(exp.clone()),
                            removed: false,
                        });
                    }
                    let mut r = ItemRecord::new(it);
                    r.category = Category::CreditCard;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields,
                    }];
                    r
                }
                AddCategory::BankAccount => {
                    let acct_num = if *account_number_stdin {
                        let mut a = String::new();
                        std::io::stdin().read_line(&mut a)?;
                        let a = a.trim_end_matches(['\r', '\n']).to_string();
                        if a.trim().is_empty() { bail!("account number from stdin cannot be empty"); }
                        a
                    } else {
                        let a = prompt_secret("GITPASS_BANK_ACCOUNT_NUMBER", "Account number: ")?;
                        if a.trim().is_empty() { bail!("account number cannot be empty"); }
                        a.to_string()
                    };
                    let pin_val = if *pin_stdin {
                        let mut p = String::new();
                        std::io::stdin().read_line(&mut p)?;
                        let p = p.trim_end_matches(['\r', '\n']).to_string();
                        if p.trim().is_empty() { None } else { Some(p) }
                    } else {
                        let p = prompt_secret("GITPASS_BANK_PIN", "PIN (empty for none): ")?;
                        if p.trim().is_empty() { None } else { Some(p.to_string()) }
                    };
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: None,
                        url: None,
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut fields = Vec::new();
                    if let Some(b) = bank.as_ref().filter(|b| !b.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "bank_name".into(),
                            label: "bank name".into(),
                            kind: FieldKind::Text,
                            value: Some(b.clone()),
                            removed: false,
                        });
                    }
                    if let Some(h) = holder.as_ref().filter(|h| !h.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "account_holder".into(),
                            label: "account holder".into(),
                            kind: FieldKind::Text,
                            value: Some(h.clone()),
                            removed: false,
                        });
                    }
                    if let Some(t) = account_type.as_ref().filter(|t| !t.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "account_type".into(),
                            label: "account type".into(),
                            kind: FieldKind::Text,
                            value: Some(t.clone()),
                            removed: false,
                        });
                    }
                    fields.push(gitpass_core::item::Field {
                        id: "account_number".into(),
                        label: "account number".into(),
                        kind: FieldKind::Concealed,
                        value: Some(acct_num),
                        removed: false,
                    });
                    if let Some(r) = routing.as_ref().filter(|r| !r.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "routing_number".into(),
                            label: "routing / SWIFT / IBAN".into(),
                            kind: FieldKind::Text,
                            value: Some(r.clone()),
                            removed: false,
                        });
                    }
                    if let Some(p) = pin_val {
                        fields.push(gitpass_core::item::Field {
                            id: "pin".into(),
                            label: "PIN".into(),
                            kind: FieldKind::Concealed,
                            value: Some(p),
                            removed: false,
                        });
                    }
                    if let Some(p) = branch_phone.as_ref().filter(|p| !p.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "branch_phone".into(),
                            label: "branch phone".into(),
                            kind: FieldKind::Phone,
                            value: Some(p.clone()),
                            removed: false,
                        });
                    }
                    if let Some(a) = branch_address.as_ref().filter(|a| !a.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "branch_address".into(),
                            label: "branch address".into(),
                            kind: FieldKind::Multiline,
                            value: Some(a.clone()),
                            removed: false,
                        });
                    }
                    let mut r = ItemRecord::new(it);
                    r.category = Category::BankAccount;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields,
                    }];
                    r
                }
                AddCategory::Server => {
                    let password = if *password_stdin {
                        let mut p = String::new();
                        std::io::stdin().read_line(&mut p)?;
                        let p = p.trim_end_matches(['\r', '\n']).to_string();
                        if p.is_empty() { None } else { Some(p) }
                    } else if let Some(n) = generate {
                        let len = n.parse::<usize>().unwrap_or(24);
                        Some(generate_password(len))
                    } else {
                        let p = prompt_secret("GITPASS_ITEM_PASSWORD", "Server password (empty for none): ")?;
                        if p.is_empty() { None } else { Some(p.to_string()) }
                    };
                    let it = Item {
                        title: title.clone(),
                        username: username.clone(),
                        password: password.clone(),
                        url: url.clone(),
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut fields = Vec::new();
                    if let Some(u) = url.as_ref().filter(|u| !u.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "url".into(),
                            label: "URL or hostname".into(),
                            kind: FieldKind::Url,
                            value: Some(u.clone()),
                            removed: false,
                        });
                    }
                    if let Some(u) = username.as_ref().filter(|u| !u.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "username".into(),
                            label: "username".into(),
                            kind: FieldKind::Text,
                            value: Some(u.clone()),
                            removed: false,
                        });
                    }
                    if let Some(p) = password {
                        fields.push(gitpass_core::item::Field {
                            id: "password".into(),
                            label: "password".into(),
                            kind: FieldKind::Concealed,
                            value: Some(p),
                            removed: false,
                        });
                    }
                    if let Some(a) = admin_url.as_ref().filter(|a| !a.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "admin_console_url".into(),
                            label: "admin console URL".into(),
                            kind: FieldKind::Url,
                            value: Some(a.clone()),
                            removed: false,
                        });
                    }
                    if let Some(p) = provider.as_ref().filter(|p| !p.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "hosting_provider".into(),
                            label: "hosting provider".into(),
                            kind: FieldKind::Text,
                            value: Some(p.clone()),
                            removed: false,
                        });
                    }
                    let mut r = ItemRecord::new(it);
                    r.category = Category::Server;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields,
                    }];
                    r
                }
                AddCategory::Identity => {
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: None,
                        url: None,
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut sections = Vec::new();
                    let mut name_fields = Vec::new();
                    if let Some(f) = first_name.as_ref().filter(|v| !v.is_empty()) {
                        name_fields.push(gitpass_core::item::Field {
                            id: "first".into(),
                            label: "first name".into(),
                            kind: FieldKind::Text,
                            value: Some(f.clone()),
                            removed: false,
                        });
                    }
                    if let Some(l) = last_name.as_ref().filter(|v| !v.is_empty()) {
                        name_fields.push(gitpass_core::item::Field {
                            id: "last".into(),
                            label: "last name".into(),
                            kind: FieldKind::Text,
                            value: Some(l.clone()),
                            removed: false,
                        });
                    }
                    if !name_fields.is_empty() {
                        sections.push(Section {
                            id: "name".into(),
                            title: Some("Identification".into()),
                            fields: name_fields,
                        });
                    }

                    if let Some(a) = address.as_ref().filter(|v| !v.is_empty()) {
                        sections.push(Section {
                            id: "address".into(),
                            title: Some("Address".into()),
                            fields: vec![gitpass_core::item::Field {
                                id: "address".into(),
                                label: "address".into(),
                                kind: FieldKind::Multiline,
                                value: Some(a.clone()),
                                removed: false,
                            }],
                        });
                    }

                    let mut contact_fields = Vec::new();
                    if let Some(e) = email.as_ref().filter(|v| !v.is_empty()) {
                        contact_fields.push(gitpass_core::item::Field {
                            id: "email".into(),
                            label: "email".into(),
                            kind: FieldKind::Email,
                            value: Some(e.clone()),
                            removed: false,
                        });
                    }
                    if let Some(p) = phone.as_ref().filter(|v| !v.is_empty()) {
                        contact_fields.push(gitpass_core::item::Field {
                            id: "phone".into(),
                            label: "phone".into(),
                            kind: FieldKind::Phone,
                            value: Some(p.clone()),
                            removed: false,
                        });
                    }
                    if !contact_fields.is_empty() {
                        sections.push(Section {
                            id: "contact".into(),
                            title: Some("Contact".into()),
                            fields: contact_fields,
                        });
                    }

                    let mut r = ItemRecord::new(it);
                    r.category = Category::Identity;
                    r.sections = sections;
                    r
                }
                AddCategory::Wifi => {
                    let password = if *password_stdin {
                        let mut p = String::new();
                        std::io::stdin().read_line(&mut p)?;
                        let p = p.trim_end_matches(['\r', '\n']).to_string();
                        if p.is_empty() { None } else { Some(p) }
                    } else {
                        let p = if std::env::var("GITPASS_WIFI_PASSWORD").is_ok() {
                            prompt_secret("GITPASS_WIFI_PASSWORD", "Wi-Fi password (empty for none): ")?
                        } else {
                            prompt_secret("GITPASS_ITEM_PASSWORD", "Wi-Fi password (empty for none): ")?
                        };
                        if p.is_empty() { None } else { Some(p.to_string()) }
                    };
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: password.clone(),
                        url: None,
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut fields = Vec::new();
                    if let Some(s) = ssid.as_ref().filter(|s| !s.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "ssid".into(),
                            label: "network name".into(),
                            kind: FieldKind::Text,
                            value: Some(s.clone()),
                            removed: false,
                        });
                    }
                    if let Some(p) = password {
                        fields.push(gitpass_core::item::Field {
                            id: "password".into(),
                            label: "wireless password".into(),
                            kind: FieldKind::Concealed,
                            value: Some(p),
                            removed: false,
                        });
                    }
                    if let Some(s) = security.as_ref().filter(|s| !s.is_empty()) {
                        let norm = match s.trim().to_ascii_lowercase().as_str() {
                            "wpa2" => "WPA2".to_string(),
                            "wpa3" => "WPA3".to_string(),
                            "wep" => "WEP".to_string(),
                            "none" => "none".to_string(),
                            _ => s.trim().to_string(),
                        };
                        fields.push(gitpass_core::item::Field {
                            id: "security".into(),
                            label: "security".into(),
                            kind: FieldKind::Text,
                            value: Some(norm),
                            removed: false,
                        });
                    }
                    let mut r = ItemRecord::new(it);
                    r.category = Category::Wifi;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields,
                    }];
                    r
                }
                AddCategory::SshKey => {
                    use gitpass_core::ssh::SshKey;
                    use zeroize::Zeroizing;
                    let k = if let Some(file) = private_key_file {
                        let pem = Zeroizing::new(std::fs::read_to_string(file).with_context(|| format!("read key file {}", file.display()))?);
                        match SshKey::import_openssh(&pem, None) {
                            Ok(k) => k,
                            Err(e) if e.to_string().contains("encrypted") => {
                                let p = prompt_secret("GITPASS_SSH_PASSPHRASE", "Key passphrase: ")?;
                                SshKey::import_openssh(&pem, Some(&p))?
                            }
                            Err(e) => return Err(e),
                        }
                    } else {
                        SshKey::generate_ed25519(title)?
                    };
                    let mut r = k.to_record(title)?;
                    if let Some(n) = notes {
                        r.item.notes = Some(n.clone());
                    }
                    r
                }
            };
            println!("added {}", s.add_record(&vid, rec)?);
        }
        Cmd::Get { vault, query, password_only, pick, copy, field, clear_secs, quiet, json, reveal, include_archived, password_history } => {
            if *json && *password_only {
                bail!("cannot combine --json and --password-only");
            }
            if *json && *copy {
                bail!("cannot combine --json and --copy");
            }
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let (id, rec) = pick_record(&s, &vid, query, *pick, *include_archived)?;
            if *password_history {
                let entries: Vec<CliPasswordHistoryEntry> = rec.password_history.iter().map(|h| {
                    CliPasswordHistoryEntry {
                        replaced_at: ts(&h.replaced_at),
                        password: if *reveal { h.value.clone() } else { "••••••••••".to_string() },
                    }
                }).collect();
                if *json {
                    println!("{}", serde_json::to_string_pretty(&entries)?);
                } else if entries.is_empty() {
                    println!("No password history for this item.");
                } else {
                    for e in &entries {
                        println!("{}  {}", e.replaced_at, e.password);
                    }
                }
            } else if *json {
                if *reveal {
                    let _ = s.touch_item_used(&id);
                }
                let last_used_at = s.last_used(&id).map(|t| gitpass_core::device::iso8601(t as i64));
                let detail = item_detail(&id, &rec, *reveal, last_used_at, resolve_related_items(&s, &rec));
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else if *copy {
                let _ = s.touch_item_used(&id);
                do_copy(&field_value(&rec.item, *field)?, *clear_secs, *quiet, "field", field_markers(*field))?;
            } else if *password_only {
                let _ = s.touch_item_used(&id);
                let secret = rec.item.password.clone().or_else(|| {
                    rec.view_sections()
                        .into_iter()
                        .flat_map(|s| s.fields)
                        .find(|f| f.kind.is_secret() && f.value.is_some())
                        .and_then(|f| f.value)
                }).unwrap_or_default();
                println!("{secret}");
            } else {
                let related_titles: Vec<String> = rec.related_item_ids.iter().map(|rid| {
                    match s.get_record_by_id(rid) {
                        Ok(Some((_, _, rel_rec))) if rel_rec.deleted_at.is_none() => rel_rec.item.title.clone(),
                        _ => "Item not available".to_string(),
                    }
                }).collect();
                print_item(&id, &rec, *reveal, &related_titles);
            }
        }
        Cmd::Show { item, item2, vault, password_history, reveal, json, include_archived } => {
            let s = unlock(cli, root)?;
            let (vault_arg, item_query) = match item2 {
                Some(ref i2) => (Some(item.as_str()), i2.as_str()),
                None => (vault.as_deref(), item.as_str()),
            };
            let (vid, iid) = resolve_item_across_vaults(&s, vault_arg, item_query)?;
            let (_, rec) = s.read_record(&vid, &iid)?;
            if !include_archived && (rec.deleted_at.is_some() || rec.archived) {
                bail!("item '{iid}' is archived or deleted (use --include-archived to include it)");
            }
            if *password_history {
                let entries: Vec<CliPasswordHistoryEntry> = rec.password_history.iter().map(|h| {
                    CliPasswordHistoryEntry {
                        replaced_at: ts(&h.replaced_at),
                        password: if *reveal { h.value.clone() } else { "••••••••••".to_string() },
                    }
                }).collect();
                if *json {
                    println!("{}", serde_json::to_string_pretty(&entries)?);
                } else if entries.is_empty() {
                    println!("No password history for this item.");
                } else {
                    for e in &entries {
                        println!("{}  {}", e.replaced_at, e.password);
                    }
                }
            } else if *json {
                if *reveal {
                    let _ = s.touch_item_used(&iid);
                }
                let last_used_at = s.last_used(&iid).map(|t| gitpass_core::device::iso8601(t as i64));
                let detail = item_detail(&iid, &rec, *reveal, last_used_at, resolve_related_items(&s, &rec));
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else {
                let related_titles: Vec<String> = rec.related_item_ids.iter().map(|rid| {
                    match s.get_record_by_id(rid) {
                        Ok(Some((_, _, rel_rec))) if rel_rec.deleted_at.is_none() => rel_rec.item.title.clone(),
                        _ => "Item not available".to_string(),
                    }
                }).collect();
                print_item(&iid, &rec, *reveal, &related_titles);
            }
        }
        Cmd::Copy { vault, query, field, pick, clear_secs, quiet } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let (id, it) = pick_item(&s, &vid, query, *pick)?;
            let _ = s.touch_item_used(&id);
            do_copy(&field_value(&it, *field)?, *clear_secs, *quiet, &format!("'{}'", it.title), field_markers(*field))?;
        }
        Cmd::Totp { vault, query, pick, copy, clear_secs } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let (id, it) = pick_item(&s, &vid, query, *pick)?;
            let t = it.totp.as_deref().ok_or_else(|| anyhow::anyhow!("item '{}' has no TOTP secret", it.title))?;
            let (code, rem) = gitpass_core::totp::code_now(&gitpass_core::totp::parse(t)?);
            println!("{code}  ({rem}s remaining)");
            if *copy {
                let _ = s.touch_item_used(&id);
                do_copy(&code, *clear_secs, true, "TOTP code", Some(PasteboardMarkers::SECRET))?;
            }
        }
        Cmd::Find { vault, query, json, include_archived } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let mut recs = s.list_records(&vid)?;
            if !include_archived {
                recs.retain(|(_, _, r)| r.deleted_at.is_none() && !r.archived);
            }
            let vname = s.vault_meta(&vid).map(|m| m.name.clone()).unwrap_or_default();
            let usage = s.all_last_used();
            let ranked = gitpass_core::search::rank_by_candidate(
                query,
                recs.into_iter().map(|(id, _, rec)| (id, rec)),
                |r| gitpass_core::search::CandidateInfo::from_record(r, Some(vname.as_str())),
            );
            if *json {
                let summaries: Vec<CliItemSummary> = ranked.iter().map(|(_, id, rec)| {
                    let mut sum = item_summary(id, rec);
                    sum.last_used = usage.get(id).copied();
                    sum
                }).collect();
                println!("{}", serde_json::to_string_pretty(&summaries)?);
            } else {
                for (sc, id, rec) in ranked {
                    let col = if rec.category == Category::ApiCredential {
                        gitpass_core::mask::api_credential_secret(&rec)
                            .map(gitpass_core::mask::secret_hint)
                            .or_else(|| rec.item.username.clone())
                            .unwrap_or_default()
                    } else {
                        rec.item.username.clone().unwrap_or_default()
                    };
                    println!("{id}  {}  {}  {}  (score {sc})", rec.item.title, col, rec.item.url.clone().unwrap_or_default());
                }
            }
        }
        Cmd::List { vault, query, category, sort, json, include_archived } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let mut recs = s.list_records(&vid)?;
            if !include_archived {
                recs.retain(|(_, _, r)| r.deleted_at.is_none() && !r.archived);
            }
            if let Some(cat) = category {
                let want = Category::from(*cat);
                recs.retain(|(_, _, r)| r.category == want);
            }
            let usage = s.all_last_used();
            let items: Vec<(String, ItemRecord)> = match query {
                Some(q) => {
                    let vname = s.vault_meta(&vid).map(|m| m.name.clone()).unwrap_or_default();
                    gitpass_core::search::rank_by_candidate(
                        q,
                        recs.into_iter().map(|(id, _, rec)| (id, rec)),
                        |r| gitpass_core::search::CandidateInfo::from_record(r, Some(vname.as_str())),
                    )
                    .into_iter()
                    .map(|(_, id, rec)| (id, rec))
                    .collect()
                }
                None => {
                    if *sort == Some(ListSort::Recent) {
                        recs.sort_by(|a, b| {
                            let a_u = usage.get(&a.0).copied().unwrap_or(0);
                            let b_u = usage.get(&b.0).copied().unwrap_or(0);
                            b_u.cmp(&a_u).then_with(|| a.2.item.title.to_lowercase().cmp(&b.2.item.title.to_lowercase()))
                        });
                    } else {
                        recs.sort_by(|a, b| a.2.item.title.to_lowercase().cmp(&b.2.item.title.to_lowercase()));
                    }
                    recs.into_iter().map(|(id, _, rec)| (id, rec)).collect()
                }
            };
            if *json {
                let summaries: Vec<CliItemSummary> = items.iter().map(|(id, rec)| {
                    let mut sum = item_summary(id, rec);
                    sum.last_used = usage.get(id).copied();
                    sum
                }).collect();
                println!("{}", serde_json::to_string_pretty(&summaries)?);
            } else {
                for (id, rec) in items {
                    let col = if rec.category == Category::ApiCredential {
                        gitpass_core::mask::api_credential_secret(&rec)
                            .map(gitpass_core::mask::secret_hint)
                            .or_else(|| rec.item.username.clone())
                            .unwrap_or_default()
                    } else {
                        rec.item.username.clone().unwrap_or_default()
                    };
                    println!("{id}  {}  {col}", rec.item.title);
                }
            }
        }
        Cmd::Edit {
            item,
            item2,
            vault,
            title,
            username,
            url,
            notes,
            totp,
            password,
            generate,
            tag,
            untag,
            favorite,
            unfavorite,
            rotate_every,
            link,
            unlink,
            pick,
            json,
        } => {
            let rotation = rotate_every.as_deref().map(parse_rotation).transpose()?;
            let s = unlock(cli, root)?;
            let (vid, iid, mut rec) = resolve_target_item(&s, vault.as_deref(), item, item2.as_deref(), *pick, false)?;
            if let Some(r) = rotation { rec.rotate_every_days = r; }
            if let Some(v) = title { rec.item.title = v.clone(); }
            if let Some(v) = username { rec.item.username = Some(v.clone()); }
            if let Some(v) = url { rec.item.url = Some(v.clone()); }
            if let Some(v) = notes { rec.item.notes = Some(v.clone()); }
            if let Some(v) = totp { rec.item.totp = Some(v.clone()); }
            if let Some(n) = generate { rec.item.password = Some(generate_password(*n)); }
            else if *password { rec.item.password = Some(prompt_secret("GITPASS_ITEM_PASSWORD", "New item password: ")?.to_string()); }

            for u in untag {
                let u_trim = u.trim();
                rec.tags.retain(|t| t != u_trim);
            }
            for t in tag {
                let t_trim = t.trim();
                if !t_trim.is_empty() && !rec.tags.iter().any(|existing| existing == t_trim) {
                    rec.tags.push(t_trim.to_string());
                }
            }
            if *favorite {
                rec.favorite = true;
            }
            if *unfavorite {
                rec.favorite = false;
            }
            for l in link {
                let l_trim = l.trim();
                if l_trim.is_empty() { continue; }
                let target_id = if s.find_item_vault(l_trim)?.is_some() {
                    l_trim.to_string()
                } else {
                    pick_record(&s, &vid, l_trim, *pick, false).map(|(id, _)| id)?
                };
                if target_id == iid {
                    bail!("cannot link an item to itself");
                }
                if !rec.related_item_ids.contains(&target_id) {
                    rec.related_item_ids.push(target_id);
                }
            }
            for u in unlink {
                let u_trim = u.trim();
                if u_trim.is_empty() { continue; }
                // Resolve titles the same way --link does, so --unlink accepts
                // a title or an ID. An unresolvable value keeps the previous
                // silent no-op (retain against the raw value).
                let target_id = if s.find_item_vault(u_trim)?.is_some() {
                    u_trim.to_string()
                } else {
                    pick_record(&s, &vid, u_trim, *pick, false)
                        .map(|(id, _)| id)
                        .unwrap_or_else(|_| u_trim.to_string())
                };
                rec.related_item_ids.retain(|id| *id != target_id);
            }
            rec.item.updated_at = store::now();
            s.update_record(&vid, &iid, rec)?;

            if *json {
                let (_, updated) = s.read_record(&vid, &iid)?;
                let detail = item_detail(&iid, &updated, false, None, resolve_related_items(&s, &updated));
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else {
                println!("updated {iid}");
            }
        }
        Cmd::Archive { item, item2, vault, pick, json } => {
            let s = unlock(cli, root)?;
            let (vid, iid, _rec) = resolve_target_item(&s, vault.as_deref(), item, item2.as_deref(), *pick, false)?;
            s.set_archived(&vid, &iid, true)?;
            if *json {
                let (_, updated) = s.read_record(&vid, &iid)?;
                let detail = item_detail(&iid, &updated, false, None, resolve_related_items(&s, &updated));
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else {
                println!("archived {iid}");
            }
        }
        Cmd::Unarchive { item, item2, vault, pick, json } => {
            let s = unlock(cli, root)?;
            let (vid, iid, rec) = resolve_target_item(&s, vault.as_deref(), item, item2.as_deref(), *pick, true)?;
            if !rec.archived {
                bail!("item '{}' is not archived", rec.item.title);
            }
            s.set_archived(&vid, &iid, false)?;
            if *json {
                let (_, updated) = s.read_record(&vid, &iid)?;
                let detail = item_detail(&iid, &updated, false, None, resolve_related_items(&s, &updated));
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else {
                println!("unarchived {iid}");
            }
        }
        Cmd::Undelete { item, item2, vault, pick, json } => {
            let s = unlock(cli, root)?;
            let (vid, iid, _rec) = resolve_deleted_item(&s, vault.as_deref(), item, item2.as_deref(), *pick)?;
            s.restore(&vid, &iid)?;
            if *json {
                let (_, updated) = s.read_record(&vid, &iid)?;
                let detail = item_detail(&iid, &updated, false, None, resolve_related_items(&s, &updated));
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else {
                println!("restored {iid}");
            }
        }
        Cmd::Mv { item, item2, vault, to, pick, json } => {
            let s = unlock(cli, root)?;
            let (from_vid, iid, _rec) = resolve_target_item(&s, vault.as_deref(), item, item2.as_deref(), *pick, false)?;
            let to_vid = s.resolve_vault(to)?;
            if from_vid == to_vid {
                bail!("item is already in vault '{to}'");
            }
            let new_id = s.move_item(&from_vid, &iid, &to_vid)?;
            if *json {
                let (_, updated) = s.read_record(&to_vid, &new_id)?;
                let detail = item_detail(&new_id, &updated, false, None, resolve_related_items(&s, &updated));
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else {
                println!("moved {iid} to {to} (now {new_id})");
            }
        }
        Cmd::Rm { vault, id } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let iid = s.resolve_item(&vid, id)?;
            s.remove_item(&vid, &iid)?;
            println!("removed {iid}");
        }
        Cmd::History { item, vault, json } => {
            let s = unlock(cli, root)?;
            let (vid, iid) = resolve_item_across_vaults(&s, vault.as_deref(), item)?;
            let (cur_v, r) = s.read_record(&vid, &iid)?;
            let mut entries = Vec::new();
            entries.push(CliHistoryEntry {
                version: cur_v,
                updated_at: ts(&r.item.updated_at),
                current: true,
            });
            for h in r.history.iter().rev() {
                entries.push(CliHistoryEntry {
                    version: h.version,
                    updated_at: ts(&h.updated_at),
                    current: false,
                });
            }
            if *json {
                println!("{}", serde_json::to_string_pretty(&entries)?);
            } else {
                for e in &entries {
                    if e.current {
                        println!("version {} (current)  {}", e.version, e.updated_at);
                    } else {
                        println!("version {}            {}", e.version, e.updated_at);
                    }
                }
            }
        }
        Cmd::Restore { item, version, vault } => {
            let s = unlock(cli, root)?;
            let (vid, iid) = resolve_item_across_vaults(&s, vault.as_deref(), item)?;
            let new_v = s.restore_version(&vid, &iid, *version)?;
            println!("restored {iid} to version {version} (now version {new_v})");
        }
        Cmd::Ref { vault, query, field, pick, copy } => {
            use gitpass_cli::inject::{field_segment, Resolver, SecretRef};
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let (id, rec) = pick_record(&s, &vid, query, *pick, false)?;
            let vaults = s.list_vaults()?;
            let vname = vaults.iter().find(|(v, _)| *v == vid).map(|(_, m)| m.name.clone()).unwrap_or_default();
            // Name the vault and item by title only while that name resolves to exactly one of them.
            let vault_seg = if !vname.is_empty() && vaults.iter().filter(|(_, m)| m.name == vname).count() == 1 { vname } else { vid.clone() };
            let same_title = s.list_items(&vid)?.iter().filter(|(_, it)| it.title.eq_ignore_ascii_case(&rec.item.title)).count();
            let item_seg = if same_title == 1 && !rec.item.title.is_empty() { rec.item.title.clone() } else { id.clone() };
            let fields: Vec<(String, String)> = rec.sections.iter().flat_map(|sec| sec.fields.iter())
                .filter(|f| !f.removed).map(|f| (f.id.clone(), f.label.clone())).collect();
            let field_seg = field_segment(field.as_deref().unwrap_or("password"), &fields)?;
            let r = SecretRef { vault: vault_seg, item: item_seg, field: field_seg };
            // Round trip through the same resolver `run`/`inject` use, so a printed reference always works.
            SessionResolver(&s).resolve(&r)?;
            let text = r.to_string();
            if *copy { do_copy(&text, 0, false, "secret reference", None)?; } else { println!("{text}"); }
        }
        _ => unreachable!(),
    }
    Ok(())
}
