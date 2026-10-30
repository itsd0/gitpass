//! Format v5 item schema: categories, sections + typed fields, tags, favorite/archive, soft delete,
//! version history, login password history and attachment metadata. Everything here lives INSIDE the encrypted, padded item
//! payload ([`crate::merge::ItemRecord`]); nothing is visible on disk.
//!
//! Back-compat: the v2-v4 login fields (`username`, `password`, `url`, `totp`, `notes`, `title`) stay
//! the canonical storage for login data (so v3 field-level merge and the CLI keep working); views
//! synthesize a `login` section from them. Extra fields of any category live in `sections`.
//! Login items may optionally retain a `password_history` list (newest first, capped at 20 entries)
//! recording earlier passwords and their replacement timestamps.
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

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
    BankAccount,
    Server,
    #[serde(other)]
    Other,
}
impl Category {
    pub const ALL: [Category; 11] = [Category::Login, Category::CreditCard, Category::SecureNote, Category::Identity,
        Category::ApiCredential, Category::Document, Category::SshKey, Category::Wifi, Category::Passkey,
        Category::BankAccount, Category::Server];
    pub fn as_str(&self) -> &'static str {
        match self {
            Category::Login => "login", Category::CreditCard => "credit_card", Category::SecureNote => "secure_note",
            Category::Identity => "identity", Category::ApiCredential => "api_credential", Category::Document => "document",
            Category::SshKey => "ssh_key", Category::Wifi => "wifi", Category::Passkey => "passkey",
            Category::BankAccount => "bank_account", Category::Server => "server",
            Category::Other => "other",
        }
    }

    /// Default section and field templates for this category.
    pub fn field_template(&self) -> Vec<Section> {
        match self {
            Category::Login => vec![Section {
                id: "login".into(),
                title: None,
                fields: vec![
                    Field { id: "username".into(), label: "username".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "password".into(), label: "password".into(), kind: FieldKind::Concealed, value: None, removed: false },
                    Field { id: "website".into(), label: "website".into(), kind: FieldKind::Url, value: None, removed: false },
                    Field { id: "otp".into(), label: "one-time password".into(), kind: FieldKind::Totp, value: None, removed: false },
                ],
            }],
            Category::CreditCard => vec![Section {
                id: "main".into(),
                title: None,
                fields: vec![
                    Field { id: "cardholder".into(), label: "cardholder name".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "type".into(), label: "type".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "number".into(), label: "number".into(), kind: FieldKind::Concealed, value: None, removed: false },
                    Field { id: "cvv".into(), label: "verification number".into(), kind: FieldKind::Concealed, value: None, removed: false },
                    Field { id: "expiry".into(), label: "expiry date".into(), kind: FieldKind::MonthYear, value: None, removed: false },
                    Field { id: "valid_from".into(), label: "valid from".into(), kind: FieldKind::MonthYear, value: None, removed: false },
                    Field { id: "pin".into(), label: "PIN".into(), kind: FieldKind::Concealed, value: None, removed: false },
                ],
            }],
            Category::SecureNote => vec![Section {
                id: "main".into(),
                title: None,
                fields: vec![
                    Field { id: "note".into(), label: "note".into(), kind: FieldKind::Multiline, value: None, removed: false },
                ],
            }],
            Category::Identity => vec![
                Section {
                    id: "name".into(),
                    title: Some("Identification".into()),
                    fields: vec![
                        Field { id: "first".into(), label: "first name".into(), kind: FieldKind::Text, value: None, removed: false },
                        Field { id: "last".into(), label: "last name".into(), kind: FieldKind::Text, value: None, removed: false },
                        Field { id: "birth".into(), label: "birth date".into(), kind: FieldKind::Date, value: None, removed: false },
                    ],
                },
                Section {
                    id: "address".into(),
                    title: Some("Address".into()),
                    fields: vec![
                        Field { id: "address".into(), label: "address".into(), kind: FieldKind::Multiline, value: None, removed: false },
                    ],
                },
                Section {
                    id: "contact".into(),
                    title: Some("Contact".into()),
                    fields: vec![
                        Field { id: "email".into(), label: "email".into(), kind: FieldKind::Email, value: None, removed: false },
                        Field { id: "phone".into(), label: "phone".into(), kind: FieldKind::Phone, value: None, removed: false },
                    ],
                },
            ],
            Category::ApiCredential => vec![Section {
                id: "main".into(),
                title: None,
                fields: vec![
                    Field { id: "username".into(), label: "username".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "credential".into(), label: "credential".into(), kind: FieldKind::Concealed, value: None, removed: false },
                    Field { id: "type".into(), label: "type".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "hostname".into(), label: "hostname".into(), kind: FieldKind::Url, value: None, removed: false },
                    Field { id: "expires".into(), label: "expires".into(), kind: FieldKind::Date, value: None, removed: false },
                ],
            }],
            Category::Document => vec![],
            Category::SshKey => vec![Section {
                id: "main".into(),
                title: None,
                fields: vec![
                    Field { id: "private_key".into(), label: "private key".into(), kind: FieldKind::Concealed, value: None, removed: false },
                    Field { id: "public_key".into(), label: "public key".into(), kind: FieldKind::Multiline, value: None, removed: false },
                    Field { id: "fingerprint".into(), label: "fingerprint".into(), kind: FieldKind::Text, value: None, removed: false },
                ],
            }],
            Category::Wifi => vec![Section {
                id: "main".into(),
                title: None,
                fields: vec![
                    Field { id: "ssid".into(), label: "network name".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "password".into(), label: "wireless password".into(), kind: FieldKind::Concealed, value: None, removed: false },
                    Field { id: "security".into(), label: "security".into(), kind: FieldKind::Text, value: None, removed: false },
                ],
            }],
            Category::Passkey => vec![Section {
                id: "main".into(),
                title: None,
                fields: vec![
                    Field { id: "rp_id".into(), label: "rp_id".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "credential_id".into(), label: "credential_id".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "private_key".into(), label: "private_key".into(), kind: FieldKind::Concealed, value: None, removed: false },
                ],
            }],
            Category::BankAccount => vec![Section {
                id: "main".into(),
                title: None,
                fields: vec![
                    Field { id: "bank_name".into(), label: "bank name".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "account_holder".into(), label: "account holder".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "account_type".into(), label: "account type".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "account_number".into(), label: "account number".into(), kind: FieldKind::Concealed, value: None, removed: false },
                    Field { id: "routing_number".into(), label: "routing / SWIFT / IBAN".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "pin".into(), label: "PIN".into(), kind: FieldKind::Concealed, value: None, removed: false },
                    Field { id: "branch_phone".into(), label: "branch phone".into(), kind: FieldKind::Phone, value: None, removed: false },
                    Field { id: "branch_address".into(), label: "branch address".into(), kind: FieldKind::Multiline, value: None, removed: false },
                ],
            }],
            Category::Server => vec![Section {
                id: "main".into(),
                title: None,
                fields: vec![
                    Field { id: "url".into(), label: "URL or hostname".into(), kind: FieldKind::Url, value: None, removed: false },
                    Field { id: "username".into(), label: "username".into(), kind: FieldKind::Text, value: None, removed: false },
                    Field { id: "password".into(), label: "password".into(), kind: FieldKind::Concealed, value: None, removed: false },
                    Field { id: "admin_console_url".into(), label: "admin console URL".into(), kind: FieldKind::Url, value: None, removed: false },
                    Field { id: "hosting_provider".into(), label: "hosting provider".into(), kind: FieldKind::Text, value: None, removed: false },
                ],
            }],
            Category::Other => vec![],
        }
    }
}
impl std::str::FromStr for Category {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        let norm = s.trim().to_ascii_lowercase();
        if norm == "bank-account" || norm == "bank_account" || norm == "bank" {
            return Ok(Category::BankAccount);
        }
        if norm == "server" {
            return Ok(Category::Server);
        }
        if norm == "credit-card" || norm == "credit_card" || norm == "card" {
            return Ok(Category::CreditCard);
        }
        if norm == "secure-note" || norm == "secure_note" || norm == "note" {
            return Ok(Category::SecureNote);
        }
        if norm == "api-credential" || norm == "api_credential" || norm == "api" {
            return Ok(Category::ApiCredential);
        }
        if norm == "ssh-key" || norm == "ssh_key" || norm == "ssh" {
            return Ok(Category::SshKey);
        }
        Category::ALL.into_iter().find(|c| c.as_str() == norm).ok_or_else(|| anyhow::anyhow!("unknown category '{s}'"))
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_item_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, Zeroize)]
pub struct HistoryEntry { pub version: u64, pub updated_at: String, pub snapshot: Snapshot }

/// Keep at most this many prior versions inside the item record.
pub const HISTORY_LIMIT: usize = 20;

/// A previous password value on a login item, kept encrypted inside the item record.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default, Zeroize, ZeroizeOnDrop)]
pub struct PasswordHistoryEntry {
    pub value: String,
    pub replaced_at: String,
}

/// Keep at most this many previous passwords on a login item.
pub const PASSWORD_HISTORY_LIMIT: usize = 20;
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
