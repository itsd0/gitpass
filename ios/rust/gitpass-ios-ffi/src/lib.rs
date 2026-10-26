//! UniFFI surface for the gitpass iOS app.
//!
//! Every secret operation delegates to `gitpass-core`: Argon2id key
//! derivation (256 MiB, from the vault's stored KDF parameters), the
//! XChaCha20-Poly1305 envelopes, the v5 item records, field-level merge
//! and RFC 6238 TOTP. No crypto is re-implemented in Swift, and no
//! crypto is re-implemented here either — this crate is a thin,
//! Swift-shaped wrapper around the same `Session` API the desktop
//! app (via gui-core) and the CLI use.
//!
//! Unlock semantics match the desktop: `open_vault` performs a full
//! unlock (device registration, KDF upgrade, soft-delete purge), the
//! same as `Session::unlock`. The unlocked session is held in memory
//! for the lifetime of the `Vault` object; locking is dropping it.

use gitpass_core::item::{Category, Field, FieldKind, Section};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{Item, Session};
use gitpass_core::{crypto, totp};
use std::path::Path;
use std::sync::Mutex;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum GitpassError {
    #[error("{0}")]
    Generic(String),
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct VaultInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub item_count: u64,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub shared: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ItemSummary {
    pub id: String,
    pub vault_id: String,
    pub title: String,
    pub subtitle: String,
    pub url: Option<String>,
    pub category: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub archived: bool,
    pub deleted: bool,
    pub updated_at: String,
    pub has_totp: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiField {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiSection {
    pub id: String,
    pub title: Option<String>,
    pub fields: Vec<FfiField>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ItemDetail {
    pub id: String,
    pub vault_id: String,
    pub title: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub totp: Option<String>,
    pub category: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub archived: bool,
    pub deleted: bool,
    pub updated_at: String,
    pub sections: Vec<FfiSection>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ItemInput {
    pub title: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub totp: Option<String>,
    pub category: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub sections: Vec<FfiSection>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TotpCode {
    pub code: String,
    pub seconds_remaining: u64,
}

impl From<anyhow::Error> for GitpassError {
    fn from(e: anyhow::Error) -> Self {
        GitpassError::Generic(format!("{e:#}"))
    }
}

fn err<T, E: std::fmt::Display>(r: Result<T, E>) -> Result<T, GitpassError> {
    r.map_err(|e| GitpassError::Generic(e.to_string()))
}

fn parse_category(s: &str) -> anyhow::Result<Category> {
    use std::str::FromStr;
    Category::from_str(s)
}

fn field_kind_from_str(s: &str) -> FieldKind {
    match s {
        "concealed" => FieldKind::Concealed,
        "url" => FieldKind::Url,
        "email" => FieldKind::Email,
        "phone" => FieldKind::Phone,
        "date" => FieldKind::Date,
        "month_year" => FieldKind::MonthYear,
        "totp" => FieldKind::Totp,
        "multiline" => FieldKind::Multiline,
        _ => FieldKind::Text,
    }
}

fn field_kind_to_str(k: FieldKind) -> &'static str {
    match k {
        FieldKind::Text => "text",
        FieldKind::Concealed => "concealed",
        FieldKind::Url => "url",
        FieldKind::Email => "email",
        FieldKind::Phone => "phone",
        FieldKind::Date => "date",
        FieldKind::MonthYear => "month_year",
        FieldKind::Totp => "totp",
        FieldKind::Multiline => "multiline",
    }
}

fn ffi_field(f: &Field) -> FfiField {
    FfiField {
        id: f.id.clone(),
        label: f.label.clone(),
        kind: field_kind_to_str(f.kind).to_string(),
        value: f.value.clone(),
    }
}

fn ffi_section(s: &Section) -> FfiSection {
    FfiSection {
        id: s.id.clone(),
        title: s.title.clone(),
        fields: s.fields.iter().map(ffi_field).collect(),
    }
}

fn input_sections(input: &[FfiSection]) -> Vec<Section> {
    input
        .iter()
        .map(|s| Section {
            id: s.id.clone(),
            title: s.title.clone(),
            fields: s
                .fields
                .iter()
                .map(|f| Field {
                    id: f.id.clone(),
                    label: f.label.clone(),
                    kind: field_kind_from_str(&f.kind),
                    value: f.value.clone(),
                    removed: false,
                })
                .collect(),
        })
        .collect()
}

/// Sections to hand to `ItemRecord::apply_sections`.
///
/// `apply_sections` clears the legacy login fields and restores them
/// only from fields with the login ids, so for a login item the
/// legacy fields must be re-expressed as a `login` section first —
/// otherwise an edit that carries any extra section would silently
/// wipe the username, password and TOTP. This is the same folding
/// gui-core relies on, applied here in one place for create and
/// update alike. For other categories the sections pass through.
fn sections_for_apply(category: Category, item: &Item, extra: Vec<Section>) -> Vec<Section> {
    if category != Category::Login {
        return extra;
    }
    let mut login_fields = Vec::new();
    let mut push = |id: &str, label: &str, kind: FieldKind, v: &Option<String>| {
        if let Some(v) = v.as_ref().filter(|v| !v.is_empty()) {
            login_fields.push(Field { id: id.into(), label: label.into(), kind, value: Some(v.clone()), removed: false });
        }
    };
    push("username", "username", FieldKind::Text, &item.username);
    push("password", "password", FieldKind::Concealed, &item.password);
    push("totp", "one-time password", FieldKind::Totp, &item.totp);
    let mut out = vec![Section { id: "login".into(), title: None, fields: login_fields }];
    out.extend(extra);
    out
}

fn input_to_record(input: ItemInput) -> anyhow::Result<ItemRecord> {
    let category = parse_category(&input.category)?;
    let item = Item {
        title: input.title,
        username: input.username.filter(|v| !v.is_empty()),
        password: input.password.filter(|v| !v.is_empty()),
        url: input.url.filter(|v| !v.is_empty()),
        notes: input.notes.filter(|v| !v.is_empty()),
        totp: input.totp.filter(|v| !v.is_empty()),
        updated_at: String::new(),
    };
    let mut rec = ItemRecord::new(item);
    rec.category = category;
    rec.tags = input.tags;
    rec.favorite = input.favorite;
    let extra = input_sections(&input.sections);
    if category == Category::Login || !extra.is_empty() {
        let sections = sections_for_apply(category, &rec.item, extra);
        rec.apply_sections(&sections);
    }
    Ok(rec)
}

fn subtitle(rec: &ItemRecord) -> String {
    if rec.category == Category::Login {
        return rec.item.username.clone().unwrap_or_default();
    }
    if let Some(u) = &rec.item.url {
        if !u.is_empty() {
            return u.clone();
        }
    }
    rec.view_sections()
        .into_iter()
        .flat_map(|s| s.fields)
        .find(|f| !f.kind.is_secret() && f.value.as_deref().is_some_and(|v| !v.is_empty()))
        .and_then(|f| f.value)
        .unwrap_or_default()
}

fn has_totp(rec: &ItemRecord) -> bool {
    rec.view_sections()
        .into_iter()
        .flat_map(|s| s.fields)
        .any(|f| f.kind == FieldKind::Totp && f.value.is_some())
}

fn summary(vid: &str, iid: &str, rec: &ItemRecord) -> ItemSummary {
    ItemSummary {
        id: iid.to_string(),
        vault_id: vid.to_string(),
        title: rec.item.title.clone(),
        subtitle: subtitle(rec),
        url: rec.item.url.clone(),
        category: rec.category.as_str().to_string(),
        tags: rec.tags.clone(),
        favorite: rec.favorite,
        archived: rec.archived,
        deleted: rec.deleted_at.is_some(),
        updated_at: rec.item.updated_at.clone(),
        has_totp: has_totp(rec),
    }
}

fn detail(vid: &str, iid: &str, rec: &ItemRecord) -> ItemDetail {
    ItemDetail {
        id: iid.to_string(),
        vault_id: vid.to_string(),
        title: rec.item.title.clone(),
        username: rec.item.username.clone(),
        password: rec.item.password.clone(),
        url: rec.item.url.clone(),
        notes: rec.item.notes.clone(),
        totp: rec.item.totp.clone(),
        category: rec.category.as_str().to_string(),
        tags: rec.tags.clone(),
        favorite: rec.favorite,
        archived: rec.archived,
        deleted: rec.deleted_at.is_some(),
        updated_at: rec.item.updated_at.clone(),
        sections: rec.view_sections().iter().map(ffi_section).collect(),
    }
}

#[derive(uniffi::Object)]
pub struct Vault {
    session: Mutex<Session>,
}

impl Vault {
    fn with_session<T>(&self, f: impl FnOnce(&Session) -> anyhow::Result<T>) -> Result<T, GitpassError> {
        let session = self.session.lock().map_err(|_| GitpassError::Generic("vault lock poisoned".into()))?;
        err(f(&session))
    }
}

#[uniffi::export]
pub fn open_vault(root: String, password: String, secret_key: String) -> Result<std::sync::Arc<Vault>, GitpassError> {
    let secret = err(crypto::SecretKey::parse(&secret_key))?;
    let session = err(Session::unlock(Path::new(&root), &password, &secret))?;
    Ok(std::sync::Arc::new(Vault { session: Mutex::new(session) }))
}

#[uniffi::export]
pub fn create_account(root: String, password: String) -> Result<String, GitpassError> {
    let secret = err(gitpass_core::store::init(Path::new(&root), &password, crypto::KdfParams::default()))?;
    Ok(secret.encode())
}

#[uniffi::export]
pub fn totp_code(secret: String) -> Result<TotpCode, GitpassError> {
    let spec = err(totp::parse(&secret))?;
    let (code, seconds_remaining) = totp::code_now(&spec);
    Ok(TotpCode { code, seconds_remaining })
}

#[uniffi::export]
pub fn generate_password(length: u64) -> String {
    // gitpass-core's generator: uniform sampling over letters, digits
    // and symbols (the same charset the CLI uses). Length is clamped
    // to at least 4 by the core function.
    crypto::generate_password(length as usize)
}

#[uniffi::export]
pub fn merge_item_records(local_json: String, remote_json: String) -> Result<String, GitpassError> {
    let local: ItemRecord = err(serde_json::from_str(&local_json))?;
    let remote: ItemRecord = err(serde_json::from_str(&remote_json))?;
    let (merged, _report) = gitpass_core::merge::merge_records(&local, &remote);
    err(serde_json::to_string(&merged))
}

#[uniffi::export]
impl Vault {
    pub fn list_vaults(&self) -> Result<Vec<VaultInfo>, GitpassError> {
        self.with_session(|s| {
            let manifest = s.load_manifest()?;
            let mut out = Vec::new();
            for (id, meta) in s.list_vaults()? {
                let entry = manifest.vaults.get(&id);
                out.push(VaultInfo {
                    id: id.clone(),
                    name: meta.name.clone(),
                    description: meta.description.clone(),
                    item_count: entry.map(|e| e.items.len() as u64).unwrap_or(0),
                    icon: meta.icon.clone(),
                    color: meta.color.clone(),
                    shared: entry.map(|e| !e.members.is_empty()).unwrap_or(false),
                });
            }
            Ok(out)
        })
    }

    pub fn create_vault(&self, name: String, description: Option<String>) -> Result<String, GitpassError> {
        self.with_session(|s| s.create_vault_full(&name, description, None, None))
    }

    pub fn list_items(&self, vault_id: String) -> Result<Vec<ItemSummary>, GitpassError> {
        self.with_session(|s| {
            let mut items: Vec<ItemSummary> = s
                .list_records(&vault_id)?
                .iter()
                .map(|(iid, _v, rec)| summary(&vault_id, iid, rec))
                .collect();
            items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
            Ok(items)
        })
    }

    pub fn get_item(&self, vault_id: String, item_id: String) -> Result<ItemDetail, GitpassError> {
        self.with_session(|s| {
            let (_v, rec) = s.read_record(&vault_id, &item_id)?;
            Ok(detail(&vault_id, &item_id, &rec))
        })
    }

    pub fn create_item(&self, vault_id: String, input: ItemInput) -> Result<String, GitpassError> {
        self.with_session(|s| {
            let rec = input_to_record(input)?;
            s.add_record(&vault_id, rec)
        })
    }

    pub fn update_item(&self, vault_id: String, item_id: String, input: ItemInput) -> Result<(), GitpassError> {
        self.with_session(|s| {
            let (_v, mut old) = s.read_record(&vault_id, &item_id)?;
            let category = parse_category(&input.category)?;
            old.category = category;
            old.item.title = input.title;
            // Legacy fields are set here and then folded back through
            // apply_sections (see sections_for_apply): that keeps
            // tombstones for fields the editor removed, instead of
            // silently resurrecting them on the next merge.
            old.item.username = input.username.filter(|v| !v.is_empty());
            old.item.password = input.password.filter(|v| !v.is_empty());
            old.item.url = input.url.filter(|v| !v.is_empty());
            old.item.notes = input.notes.filter(|v| !v.is_empty());
            old.item.totp = input.totp.filter(|v| !v.is_empty());
            old.tags = input.tags;
            old.favorite = input.favorite;
            let extra = input_sections(&input.sections);
            let sections = sections_for_apply(category, &old.item, extra);
            old.apply_sections(&sections);
            s.update_record(&vault_id, &item_id, old)
        })
    }

    pub fn delete_item(&self, vault_id: String, item_id: String) -> Result<(), GitpassError> {
        self.with_session(|s| s.soft_delete(&vault_id, &item_id))
    }

    pub fn restore_item(&self, vault_id: String, item_id: String) -> Result<(), GitpassError> {
        self.with_session(|s| s.restore(&vault_id, &item_id))
    }

    pub fn set_favorite(&self, vault_id: String, item_id: String, favorite: bool) -> Result<(), GitpassError> {
        self.with_session(|s| s.set_favorite(&vault_id, &item_id, favorite))
    }

    pub fn set_archived(&self, vault_id: String, item_id: String, archived: bool) -> Result<(), GitpassError> {
        self.with_session(|s| s.set_archived(&vault_id, &item_id, archived))
    }

    /// Serialized `ItemRecord` for one item. This is what the Swift
    /// sync engine feeds into `merge_item_records`: the merge itself
    /// still runs here, in gitpass-core, with the desktop's
    /// field-clock rules (higher clock wins; equal clock + different
    /// value = conflict, local kept).
    pub fn record_json(&self, vault_id: String, item_id: String) -> Result<String, GitpassError> {
        self.with_session(|s| {
            let (_v, rec) = s.read_record(&vault_id, &item_id)?;
            Ok(serde_json::to_string(&rec)?)
        })
    }

    /// Write a record previously produced by `record_json` /
    /// `merge_item_records` back into the vault (used by sync after a
    /// merge, and for remote-only items). Version is bumped by the core.
    pub fn put_record_json(&self, vault_id: String, item_id: String, record_json: String) -> Result<(), GitpassError> {
        self.with_session(|s| {
            let rec: ItemRecord = serde_json::from_str(&record_json)?;
            s.put_record(&vault_id, &item_id, &rec)
        })
    }
}
