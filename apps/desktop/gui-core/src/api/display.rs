use super::app::App;
use super::dto::*;
use anyhow::Result;
use gitpass_core::audit::{self, AuditOpts};
use gitpass_core::device::iso8601;
use gitpass_core::item::{self, Category, FieldKind};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::Session;
use std::collections::BTreeSet;

pub(crate) fn ts(secs_str: &str) -> String { secs_str.trim().parse::<i64>().map(iso8601).unwrap_or_else(|_| secs_str.to_string()) }
pub(crate) fn nonempty(o: Option<String>) -> Option<String> { o.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) }

/// 0-100 from the core entropy estimate (common passwords / few distinct chars capped at 10).
pub fn strength(pw: &str) -> u8 {
    if pw.is_empty() { return 0; }
    let bits = audit::entropy_bits(pw);
    let mut s = (bits * 100.0 / 128.0).round().clamp(0.0, 100.0) as u8;
    if let Some(r) = audit::weakness(pw, &AuditOpts { min_length: 1, min_entropy_bits: 0.0, ..Default::default() }) { let _ = r; s = s.min(10); }
    s
}

/// List subtitle: login -> username; credit_card -> "Visa •••• 1111" (brand + last 4 only, computed in memory after decryption); identity -> name;
/// others -> first non-concealed field value or "".
pub(crate) fn is_card_number(f: &item::Field) -> bool {
    let l = f.label.to_lowercase();
    if f.id == "cvv" || f.id == "pin" || ["cvv", "cvc", "verification", "security", "pin", "holder", "name", "type", "expir", "valid"].iter().any(|k| l.contains(k)) { return false; }
    f.id == "number" || l.contains("number") || l.contains("card")
}
pub(crate) fn is_card_cvv(f: &item::Field) -> bool {
    let l = f.label.to_lowercase();
    f.id == "cvv" || f.id == "cvc" || ["cvv", "cvc", "verification", "security code"].iter().any(|k| l.contains(k))
}

/// Fields whose copied value clipboard managers must not record. Secret field kinds cover
/// passwords, TOTP codes, card numbers/CVVs and API credentials; card data stored as plain text
/// by an old import is still recognised by label/id.
pub(crate) fn is_secret_copy(category: Category, f: &item::Field) -> bool {
    let l = f.label.to_lowercase();
    f.kind.is_secret()
        || (category == Category::CreditCard && (is_card_number(f) || is_card_cvv(f)))
        || (category == Category::BankAccount && (f.id == "account_number" || f.id == "pin" || l.contains("account number") || l.contains("pin")))
}
pub(crate) fn card_digits(r: &ItemRecord) -> Option<String> {
    if r.category != Category::CreditCard { return None; }
    // First card-number-like field that actually holds >= 8 digits ("cardholder" etc. are excluded above).
    r.view_sections().into_iter().flat_map(|s| s.fields).filter(is_card_number).find_map(|f| f.value.as_deref().and_then(card_digits_of))
}
pub use gitpass_core::item::card_brand;
/// "Visa •••• 7575" / "•••• 7575": only the brand and last 4 digits ever leave this function.
pub fn card_label(d: &str) -> String {
    let l4 = &d[d.len() - 4..];
    match card_brand(d) { Some(b) => format!("{b} •••• {l4}"), None => format!("•••• {l4}") }
}
pub(crate) fn card_digits_of(v: &str) -> Option<String> { Some(v.chars().filter(|c| c.is_ascii_digit()).collect::<String>()).filter(|d| d.len() >= 8) }
pub fn card_mask(d: &str) -> String { format!("•••• •••• •••• {}", &d[d.len() - 4..]) }

pub fn subtitle(r: &ItemRecord) -> String {
    let fields: Vec<item::Field> = r.view_sections().into_iter().flat_map(|s| s.fields).collect();
    let find = |pred: &dyn Fn(&item::Field) -> bool| fields.iter().find(|f| pred(f)).and_then(|f| f.value.clone());
    let field_val = |ids: &[&str]| ids.iter().find_map(|id| fields.iter().find(|f| f.id == *id).and_then(|f| f.value.clone()).filter(|v| !v.is_empty()));
    match r.category {
        Category::Login => r.item.username.clone().unwrap_or_default(),
        Category::CreditCard => card_digits(r).map(|d| card_label(&d)).unwrap_or_default(),
        Category::Identity => find(&|f| { let l = f.label.to_lowercase(); f.id == "name" || l.contains("name") })
            .or_else(|| find(&|f| !f.kind.is_secret())).unwrap_or_default(),
        Category::BankAccount => {
            let acct = field_val(&["account_number", "account_no", "account"]);
            let holder = field_val(&["account_holder", "holder", "owner"]);
            let bank = field_val(&["bank_name", "bank"]);
            let acct_type = field_val(&["account_type", "type"]);
            let l4 = acct.as_deref().map(|s| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>()).filter(|d| d.len() >= 4).map(|d| format!("•••• {}", &d[d.len() - 4..]));
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
            let u = r.item.username.clone().or_else(|| field_val(&["username"]));
            let h = r.item.url.clone().or_else(|| field_val(&["url", "hostname"]));
            match (u, h) {
                (Some(u), Some(h)) if !u.is_empty() && !h.is_empty() => format!("{u}@{h}"),
                (Some(u), _) if !u.is_empty() => u,
                (_, Some(h)) if !h.is_empty() => h,
                _ => String::new(),
            }
        }
        Category::ApiCredential => gitpass_core::mask::api_credential_secret(r)
            .map(gitpass_core::mask::secret_hint)
            .or_else(|| find(&|f| !f.kind.is_secret() && f.value.as_deref().is_some_and(|v| !v.is_empty())))
            .unwrap_or_default(),
        _ => find(&|f| !f.kind.is_secret() && f.value.as_deref().is_some_and(|v| !v.is_empty())).unwrap_or_default(),
    }
}

pub(crate) fn to_dto_sections(r: &ItemRecord) -> Vec<Section> {
    r.view_sections().into_iter().map(|s| Section { id: s.id, title: s.title, fields: s.fields.into_iter().map(|f| {
        let masked = if f.kind.is_secret() {
            if r.category == Category::CreditCard && is_card_number(&f) {
                f.value.as_deref().map(|n| n.chars().filter(|c| c.is_ascii_digit()).collect::<String>()).filter(|d| d.len() >= 8).map(|d| card_mask(&d))
            } else if r.category == Category::BankAccount && (f.id == "account_number" || f.id == "account_no" || f.label.to_lowercase().contains("account number")) {
                f.value.as_deref().map(|n| n.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>()).filter(|d| d.len() >= 4).map(|d| format!("•••• •••• •••• {}", &d[d.len() - 4..]))
            } else if r.category == Category::ApiCredential {
                f.value.as_deref().map(gitpass_core::mask::secret_hint)
            } else {
                None
            }
        } else {
            None
        };
        Field {
            masked,
            value: if f.kind.is_secret() { None } else { f.value.clone() },
            id: f.id, label: f.label, kind: f.kind, diff: None
        }
    }).collect() }).collect()
}

pub fn item_expiry(r: &ItemRecord) -> Option<String> {
    let target = match r.category {
        Category::CreditCard => "expiry",
        Category::ApiCredential => "expires",
        _ => return None,
    };
    r.view_sections()
        .into_iter()
        .flat_map(|s| s.fields)
        .find(|f| {
            !f.removed
                && (f.id.eq_ignore_ascii_case(target)
                    || f.label.eq_ignore_ascii_case(target)
                    || (r.category == Category::CreditCard && f.label.eq_ignore_ascii_case("expiry date")))
                && f.value.as_deref().map(str::trim).filter(|v| !v.is_empty()).is_some()
        })
        .and_then(|f| f.value)
        .map(|v| v.trim().to_string())
}

pub(crate) fn summary(vid: &str, iid: &str, r: &ItemRecord, last_used: Option<u64>) -> ItemSummary {
    ItemSummary {
        id: iid.into(), vault_id: vid.into(), category: r.category, title: r.item.title.clone(), subtitle: subtitle(r),
        favorite: r.favorite, archived: r.archived, deleted_at: r.deleted_at.map(|d| iso8601(d as i64)), tags: r.tags.clone(),
        updated_at: ts(&r.item.updated_at), created_at: r.created_at.map(|c| iso8601(c as i64)).unwrap_or_else(|| ts(&r.item.updated_at)),
        url: r.item.url.clone(),
        expiry: item_expiry(r),
        change_password_url: r.item.url.as_deref().and_then(audit::change_password_url),
        ignored_findings: r.ignored_findings.clone(),
        last_used,
    }
}

pub(crate) fn has_totp(r: &ItemRecord) -> bool { r.view_sections().iter().flat_map(|s| s.fields.iter()).any(|f| f.kind == FieldKind::Totp && f.value.is_some()) }

pub(crate) fn detail(vid: &str, iid: &str, version: u64, r: &ItemRecord, findings: Vec<FindingInfo>, last_used: Option<u64>) -> ItemDetail {
    let pw = r.view_sections().into_iter().flat_map(|s| s.fields).find(|f| f.kind == FieldKind::Concealed && f.value.is_some());
    ItemDetail {
        summary: summary(vid, iid, r, last_used), version, sections: to_dto_sections(r), notes: r.item.notes.clone(),
        attachments: r.attachments.iter().map(|a| Attachment { id: a.id.clone(), name: a.name.clone(), size: a.size, mime: a.mime.clone() }).collect(),
        history: r.history.iter().rev().map(|h| HistoryRef { version: h.version, updated_at: ts(&h.updated_at) }).collect(),
        password_strength: if r.category == Category::Login { r.item.password.as_deref().map(strength) } else { pw.and_then(|f| f.value).map(|v| strength(&v)) },
        has_totp: has_totp(r),
        findings,
        password_changed_at: r.password_changed_at().map(|t| iso8601(t as i64)),
    }
}

/// Active Watchtower findings for one item. Runs the same audit as [`App::watchtower`] (without
/// the opt-in HIBP network call) so reuse groups match the Watchtower page.
pub(crate) fn item_findings(s: &Session, settings: &Settings, vault_id: &str, item_id: &str) -> Result<Vec<FindingInfo>> {
    let records = App::all_records(s)?;
    let passkey_sites: BTreeSet<String> = records.iter()
        .filter(|(_, _, r)| r.deleted_at.is_none() && !r.archived && r.category == Category::Passkey)
        .filter_map(|(_, _, r)| r.find_field(gitpass_core::passkey::F_RP_ID).and_then(|f| f.value))
        .collect();
    let items: Vec<(String, String, ItemRecord)> = records.into_iter()
        .filter(|(_, _, r)| r.deleted_at.is_none() && !r.archived && r.category != Category::Passkey)
        .collect();
    let opts = AuditOpts { unused_months: (settings.watchtower_unused_months > 0).then_some(settings.watchtower_unused_months),
        last_used: s.all_last_used(), ..AuditOpts::default() };
    let rep = audit::audit_with_passkeys(&items, &passkey_sites, &opts, None)?;
    Ok(rep.per_item.into_iter()
        .filter(|f| f.vault == vault_id && f.id == item_id)
        .flat_map(|f| f.findings.into_iter().map(|x| FindingInfo { kind: x.kind().to_string(), description: x.to_string() }))
        .collect())
}

pub(crate) fn annotate_diff(d: &mut ItemDetail, snap: &ItemRecord, cur: &ItemRecord) {
    let cur_sections = cur.view_sections();
    let cur_fields: Vec<gitpass_core::item::Field> = cur_sections.into_iter().flat_map(|s| s.fields).collect();
    let snap_sections = snap.view_sections();
    let snap_fields: Vec<gitpass_core::item::Field> = snap_sections.into_iter().flat_map(|s| s.fields).collect();

    for s in &mut d.sections {
        for f in &mut s.fields {
            if let Some(cf) = cur_fields.iter().find(|x| x.id == f.id) {
                let snap_val = snap_fields.iter().find(|x| x.id == f.id).and_then(|x| x.value.as_deref());
                let cur_val = cf.value.as_deref();
                if snap_val != cur_val {
                    f.diff = Some("changed".into());
                }
            } else {
                f.diff = Some("removed".into());
            }
        }
    }

    let added_fields: Vec<Field> = cur_fields
        .into_iter()
        .filter(|cf| !snap_fields.iter().any(|sf| sf.id == cf.id))
        .map(|cf| Field {
            id: cf.id.clone(),
            label: cf.label.clone(),
            kind: cf.kind,
            value: if cf.kind.is_secret() { None } else { cf.value.clone() },
            masked: if cf.kind.is_secret() { Some("••••••••••".into()) } else { None },
            diff: Some("added".into()),
        })
        .collect();

    if !added_fields.is_empty() {
        if let Some(first) = d.sections.first_mut() {
            first.fields.extend(added_fields);
        } else {
            d.sections.push(Section { id: "added".into(), title: None, fields: added_fields });
        }
    }
}

pub(crate) fn mime_for(name: &str) -> &'static str {
    item::mime_for(name)
}

