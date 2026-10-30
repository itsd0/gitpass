use anyhow::Result;
use crate::cli::Field;
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{Item, Session};
use serde::Serialize;
use zeroize::Zeroizing;

pub(crate) fn print_item(id: &str, r: &ItemRecord, reveal: bool, related: &[String]) {
    let it = &r.item;
    println!("id:       {id}");
    println!("title:    {}", it.title);
    if r.category != Category::Login {
        println!("category: {}", r.category.as_str());
    }
    if r.category == Category::Login {
        if let Some(v) = &it.username { println!("username: {v}"); }
        if let Some(v) = &it.password {
            let p = if reveal { v.as_str() } else { "••••••••" };
            println!("password: {p}");
            if let Some(t) = r.password_changed_at() {
                println!("password changed: {}", gitpass_core::device::iso8601(t as i64));
            }
            if let Some(d) = r.rotate_every_days { println!("rotate every: {d} days"); }
        }
        if let Some(v) = &it.url { println!("url:      {v}"); }
        if let Some(v) = &it.totp {
            let t = if reveal { v.as_str() } else { "••••••••" };
            println!("totp:     {t}");
        }
        if let Some(v) = &it.notes { println!("notes:    {v}"); }
        for s in &r.sections {
            for f in &s.fields {
                if f.removed { continue; }
                if let Some(v) = &f.value {
                    let val = if f.kind.is_secret() {
                        if reveal { v.as_str() } else { "••••••••" }
                    } else {
                        v.as_str()
                    };
                    println!("{}: {val}", f.label);
                }
            }
        }
    } else {
        for s in r.view_sections() {
            if let Some(t) = &s.title {
                println!("[{t}]");
            }
            for f in s.fields {
                if let Some(v) = &f.value {
                    let val = if f.kind.is_secret() {
                        if reveal { v.as_str() } else { "••••••••" }
                    } else {
                        v.as_str()
                    };
                    println!("{}: {val}", f.label);
                }
            }
        }
        if let Some(v) = &it.url {
            let has_url = r.view_sections().iter().any(|s| s.fields.iter().any(|f| f.kind == FieldKind::Url));
            if !has_url {
                println!("url:      {v}");
            }
        }
        if let Some(v) = &it.notes {
            let has_note = r.view_sections().iter().any(|s| s.fields.iter().any(|f| f.id == "note" || f.id == "notes"));
            if !has_note {
                println!("notes:    {v}");
            }
        }
    }
    if !related.is_empty() {
        println!("related:  {}", related.join(", "));
    }
}

pub(crate) fn field_value(it: &Item, f: Field) -> Result<Zeroizing<String>> {
    let v = match f {
        Field::Password => it.password.clone(), Field::Username => it.username.clone(), Field::Url => it.url.clone(),
        Field::Notes => it.notes.clone(), Field::Title => Some(it.title.clone()),
        Field::Totp => match &it.totp { Some(t) => Some(gitpass_core::totp::code_now(&gitpass_core::totp::parse(t)?).0), None => None },
    };
    v.map(Zeroizing::new).ok_or_else(|| anyhow::anyhow!("item '{}' has no such field", it.title))
}

#[derive(Serialize, Clone, Debug, PartialEq)]

pub(crate) struct CliItemSummary {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) category: Category,
    pub(crate) username: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) favorite: bool,
    pub(crate) updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_used: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subtitle: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]

pub(crate) struct CliField {
    pub(crate) label: String,
    pub(crate) kind: FieldKind,
    pub(crate) value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) masked: Option<String>,
}

/// One entry of `related_items` in `gitpass get --json` output: the linked item's
/// id plus its resolved title. `title` is null and `available` is false when the
/// target is missing or deleted; titles only, never secrets.
#[derive(Serialize, Clone, Debug, PartialEq)]

pub(crate) struct CliRelatedItem {
    pub(crate) id: String,
    pub(crate) title: Option<String>,
    pub(crate) available: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]

pub(crate) struct CliItemDetail {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) category: Category,
    pub(crate) username: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) favorite: bool,
    pub(crate) archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) deleted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) password_changed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rotate_every_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) notes: Option<String>,
    pub(crate) fields: Vec<CliField>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) related_item_ids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) related_items: Vec<CliRelatedItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) password_history: Vec<CliPasswordHistoryEntry>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]

pub(crate) struct CliPasswordHistoryEntry {
    pub(crate) replaced_at: String,
    pub(crate) password: String,
}

pub(crate) fn ts(s: &str) -> String {
    s.trim().parse::<i64>().map(gitpass_core::device::iso8601).unwrap_or_else(|_| s.to_string())
}

#[derive(Serialize, Clone, Debug, PartialEq)]

pub(crate) struct CliHistoryEntry {
    pub(crate) version: u64,
    pub(crate) updated_at: String,
    pub(crate) current: bool,
}

pub(crate) fn item_summary(id: &str, r: &ItemRecord) -> CliItemSummary {
    let hint = if r.category == Category::ApiCredential {
        gitpass_core::mask::api_credential_secret(r).map(gitpass_core::mask::secret_hint)
    } else {
        None
    };
    CliItemSummary {
        id: id.to_string(),
        title: r.item.title.clone(),
        category: r.category,
        username: r.item.username.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| f.id == "username" && f.kind == FieldKind::Text)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        url: r.item.url.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| (f.id == "hostname" || f.id == "url") && f.kind == FieldKind::Url)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        tags: r.tags.clone(),
        favorite: r.favorite,
        updated_at: ts(&r.item.updated_at),
        last_used: None,
        subtitle: hint.clone(),
        hint,
    }
}

/// Resolve an item's `related_item_ids` to `{id, title, available}` entries, using
/// the same lookup as the human-readable `get` output: a target counts as
/// available only when it exists and is not deleted. Missing or deleted targets
/// yield `title: None, available: false` and never fail the command.
pub(crate) fn resolve_related_items(s: &Session, r: &ItemRecord) -> Vec<CliRelatedItem> {
    r.related_item_ids
        .iter()
        .map(|rid| match s.get_record_by_id(rid) {
            Ok(Some((_, _, rel_rec))) if rel_rec.deleted_at.is_none() => CliRelatedItem {
                id: rid.clone(),
                title: Some(rel_rec.item.title.clone()),
                available: true,
            },
            _ => CliRelatedItem {
                id: rid.clone(),
                title: None,
                available: false,
            },
        })
        .collect()
}

pub(crate) fn item_detail(
    id: &str,
    r: &ItemRecord,
    reveal: bool,
    last_used_at: Option<String>,
    related_items: Vec<CliRelatedItem>,
) -> CliItemDetail {
    let mut fields = Vec::new();
    for section in r.view_sections() {
        for f in section.fields {
            let (val, masked) = if f.kind.is_secret() {
                if reveal {
                    (f.value, None)
                } else if r.category == Category::ApiCredential {
                    (None, f.value.as_deref().map(gitpass_core::mask::secret_hint))
                } else {
                    (None, None)
                }
            } else {
                (f.value, None)
            };
            fields.push(CliField {
                label: f.label,
                kind: f.kind,
                value: val,
                masked,
            });
        }
    }
    CliItemDetail {
        id: id.to_string(),
        title: r.item.title.clone(),
        category: r.category,
        username: r.item.username.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| f.id == "username" && f.kind == FieldKind::Text)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        url: r.item.url.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| (f.id == "hostname" || f.id == "url") && f.kind == FieldKind::Url)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        tags: r.tags.clone(),
        favorite: r.favorite,
        archived: r.archived,
        deleted_at: r.deleted_at.map(|d| gitpass_core::device::iso8601(d as i64)),
        created_at: r.created_at.map(|c| gitpass_core::device::iso8601(c as i64)),
        updated_at: ts(&r.item.updated_at),
        password_changed_at: r.password_changed_at().map(|t| gitpass_core::device::iso8601(t as i64)),
        rotate_every_days: r.rotate_every_days,
        last_used_at,
        notes: r.item.notes.clone().filter(|n| !n.trim().is_empty()),
        fields,
        related_item_ids: r.related_item_ids.clone(),
        related_items,
        password_history: if r.category == Category::Login {
            r.password_history.iter().map(|h| CliPasswordHistoryEntry {
                replaced_at: ts(&h.replaced_at),
                password: if reveal { h.value.clone() } else { "••••••••••".to_string() },
            }).collect()
        } else {
            vec![]
        },
    }
}
