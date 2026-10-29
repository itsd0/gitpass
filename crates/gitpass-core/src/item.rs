//! Format v5 item schema: categories, sections + typed fields, tags, favorite/archive, soft delete,
//! version history and attachment metadata. Everything here lives INSIDE the encrypted, padded item
//! payload ([`crate::merge::ItemRecord`]); nothing is visible on disk.
//!
//! Back-compat: the v2-v4 login fields (`username`, `password`, `url`, `totp`, `notes`, `title`) stay
//! the canonical storage for login data (so v3 field-level merge and the CLI keep working); views
//! synthesize a `login` section from them. Extra fields of any category live in `sections`.
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    #[default]
    Login,
    CreditCard,
    SecureNote,
    Identity,
    ApiCredential,
    Document,
    SshKey,
    Wifi,
    Passkey,
}
impl Category {
    pub const ALL: [Category; 9] = [Category::Login, Category::CreditCard, Category::SecureNote, Category::Identity,
        Category::ApiCredential, Category::Document, Category::SshKey, Category::Wifi, Category::Passkey];
    pub fn as_str(&self) -> &'static str {
        match self {
            Category::Login => "login", Category::CreditCard => "credit_card", Category::SecureNote => "secure_note",
            Category::Identity => "identity", Category::ApiCredential => "api_credential", Category::Document => "document",
            Category::SshKey => "ssh_key", Category::Wifi => "wifi", Category::Passkey => "passkey",
        }
    }
}
impl std::str::FromStr for Category {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        Category::ALL.into_iter().find(|c| c.as_str() == s).ok_or_else(|| anyhow::anyhow!("unknown category '{s}'"))
    }
}

/// Card brand from the BIN (leading digits). None when unknown.
pub fn card_brand(d: &str) -> Option<&'static str> {
    let digits: String = d.chars().filter(|c| c.is_ascii_digit()).collect();
    let p = |n: usize| digits.get(..n).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    Some(match () {
        _ if digits.starts_with('4') => "Visa",
        _ if (51..=55).contains(&p(2)) || (2221..=2720).contains(&p(4)) => "Mastercard",
        _ if p(2) == 34 || p(2) == 37 => "Amex",
        _ if p(4) == 6011 || p(2) == 65 || (644..=649).contains(&p(3)) => "Discover",
        _ if (3528..=3589).contains(&p(4)) => "JCB",
        _ if p(2) == 36 || p(2) == 38 || (300..=305).contains(&p(3)) => "Diners Club",
        _ if p(2) == 62 => "UnionPay",
        _ => return None,
    })
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind { #[default] Text, Concealed, Url, Email, Phone, Date, MonthYear, Totp, Multiline }
impl FieldKind {
    /// Secret kinds are never returned in list/detail payloads.
    pub fn is_secret(&self) -> bool { matches!(self, FieldKind::Concealed | FieldKind::Totp) }
}

fn is_false(b: &bool) -> bool { !*b }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, Zeroize)]
pub struct Field {
    pub id: String,
    pub label: String,
    #[zeroize(skip)]
    pub kind: FieldKind,
    pub value: Option<String>,
    /// Tombstone: the field was deleted (kept so the deletion merges by clock).
    #[serde(default, skip_serializing_if = "is_false")]
    pub removed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, Zeroize)]
pub struct Section { pub id: String, pub title: Option<String>, pub fields: Vec<Field> }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, Zeroize)]
pub struct AttachmentMeta { pub id: String, pub name: String, pub size: u64, pub mime: String }

/// A prior version of an item's content (meta flags excluded).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, Zeroize)]
pub struct Snapshot {
    pub item: crate::store::Item,
    #[zeroize(skip)]
    #[serde(default)]
    pub category: Category,
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, Zeroize)]
pub struct HistoryEntry { pub version: u64, pub updated_at: String, pub snapshot: Snapshot }

/// Keep at most this many prior versions inside the item record.
pub const HISTORY_LIMIT: usize = 20;
/// Soft-deleted items are purged this long after deletion (on unlock or explicit purge).
pub const PURGE_AFTER_SECS: u64 = 30 * 86400;
/// Attachment size cap.
pub const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;

/// Resolve a MIME type for a file name based on its extension.
pub fn mime_for(name: &str) -> &'static str {
    match std::path::Path::new(name).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("txt") => "text/plain",
        Some("json") => "application/json",
        Some("zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

/// Field ids that map onto the legacy login fields of [`crate::store::Item`].
pub const LOGIN_USERNAME: &str = "username";
pub const LOGIN_PASSWORD: &str = "password";
pub const LOGIN_TOTP: &str = "totp";
