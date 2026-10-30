use super::Parsed;
use crate::crypto::{open, random_bytes, seal, Envelope, KdfParams, Key32};
use crate::item::{Category, Section};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::Value;
use crate::merge::ItemRecord;
use crate::store::Item;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate_every_days: Option<u32>,
}
fn is_login(c: &Category) -> bool { *c == Category::Login }
fn is_false(b: &bool) -> bool { !*b }
impl From<&ItemRecord> for ExportRecord {
    fn from(r: &ItemRecord) -> Self {
        let sections = r.sections.iter().map(|s| Section { id: s.id.clone(), title: s.title.clone(), fields: s.fields.iter().filter(|f| !f.removed).cloned().collect() })
            .filter(|s| !s.fields.is_empty() || s.title.is_some()).collect();
        ExportRecord { item: r.item.clone(), category: r.category, sections, tags: r.tags.clone(), favorite: r.favorite, archived: r.archived, created_at: r.created_at, password_history: r.password_history.clone(), rotate_every_days: r.rotate_every_days }
    }
}
impl From<ExportRecord> for ItemRecord {
    fn from(e: ExportRecord) -> Self {
        let mut r = ItemRecord::new(e.item); r.category = e.category; r.sections = e.sections; r.tags = e.tags;
        r.favorite = e.favorite; r.archived = e.archived; r.created_at = e.created_at; r.rotate_every_days = e.rotate_every_days;
        r.password_history = e.password_history; r
    }
}
fn plain_item(it: &Item) -> ExportRecord {
    ExportRecord { item: it.clone(), category: Category::Login, sections: vec![], tags: vec![], favorite: false, archived: false, created_at: None, password_history: vec![], rotate_every_days: None }
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

pub(crate) fn parse_gitpass_json(data: &str, passphrase: Option<&str>) -> Result<Parsed> {
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
