use anyhow::{bail, Result};
use gitpass_core::item::{Category, FieldKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use zeroize::Zeroizing;

// ------------------------------------------------------------------ DTOs (contract types.ts)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Field { pub id: String, pub label: String, pub kind: FieldKind, pub value: Option<String>,
    /// Display-only mask for a concealed card number ("•••• •••• •••• 7575"); computed after decryption, never persisted.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub masked: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub diff: Option<String> }
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Section { pub id: String, pub title: Option<String>, pub fields: Vec<Field> }
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Attachment { pub id: String, pub name: String, pub size: u64, pub mime: String }
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemSummary {
    pub id: String, pub vault_id: String, pub category: Category, pub title: String, pub subtitle: String,
    pub favorite: bool, pub archived: bool, pub deleted_at: Option<String>, pub tags: Vec<String>,
    pub updated_at: String, pub created_at: String, pub url: Option<String>,
    pub expiry: Option<String>,
    /// `https://<host>/.well-known/change-password` derived from `url`; absent when there is no usable URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_password_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_findings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<u64>,
    /// The login's "change password every N days" reminder (30, 90, 180 or 365); absent when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate_every_days: Option<u32>,
    /// Set when that reminder is due, or falls due within 14 days (drives the list/detail pill).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<RotationInfo>,
}
/// Rotation reminder state: `state` is `"due"` (`days` = age of the password) or `"soon"` (`days` = days left).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RotationInfo { pub state: String, pub every_days: u32, pub days: u64 }
/// Active Watchtower finding for the item-detail banner (description is English, like the audit text).
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FindingInfo { pub kind: String, pub description: String }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRef { pub version: u64, pub updated_at: String }
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RelatedItem {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<Category>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub available: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PasswordHistoryRef {
    pub replaced_at: String,
}
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    #[serde(flatten)]
    pub summary: ItemSummary,
    #[serde(default)]
    pub version: u64,
    pub sections: Vec<Section>, pub notes: Option<String>, pub attachments: Vec<Attachment>,
    pub history: Vec<HistoryRef>, pub password_strength: Option<u8>, pub has_totp: bool,
    /// Active Watchtower findings (HIBP excluded: the detail view never makes network calls).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<FindingInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_changed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_items: Vec<RelatedItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_item_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub password_history: Vec<PasswordHistoryRef>,
}
/// One item reference for bulk commands.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemRef { pub vault_id: String, pub item_id: String }
/// Multi-select bulk operation (`bulk_items`). `action`: trash | restore | purge | archive | unarchive |
/// favorite | unfavorite | add_tag | remove_tag | move | copy.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BulkRequest { pub action: String, pub items: Vec<ItemRef>, #[serde(default)] pub tag: Option<String>, #[serde(default)] pub to_vault_id: Option<String> }
/// `count` = items changed; `items` = resulting refs (new ids for move/copy, else the inputs that still exist).
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BulkResult { pub count: usize, pub items: Vec<ItemRef> }
/// Upper bound for one bulk request (keeps a single commit reasonable).
pub const BULK_MAX: usize = 5000;

/// `export_items`: export a whole vault or an explicit `{vaultId,itemId}` selection to `path`.
/// `format`: `gitpass-json` (full records) or `csv` (login fields only); encrypted is gitpass-json only.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    #[serde(default)] pub vault_id: Option<String>,
    #[serde(default)] pub items: Option<Vec<ItemRef>>,
    pub format: String,
    pub encrypted: bool,
    #[serde(default)] pub passphrase: Option<String>,
    pub path: String,
}
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult { pub path: String, pub items: usize, pub format: String, pub encrypted: bool }

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ItemInput {
    pub vault_id: String, pub category: Category, pub title: String, pub sections: Vec<Section>,
    #[serde(default)] pub notes: Option<String>, #[serde(default)] pub tags: Option<Vec<String>>,
    #[serde(default)] pub favorite: Option<bool>, #[serde(default)] pub url: Option<String>,
    #[serde(default)] pub related_item_ids: Option<Vec<String>>,
}
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Member { pub fingerprint: String, pub label: Option<String>, pub is_self: bool }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub id: String, pub name: String, pub description: Option<String>, pub item_count: usize, pub shared: bool,
    pub members: Vec<Member>, pub icon: Option<String>, pub color: Option<String>,
}
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FilterKind { All, Favorites, Archive, Deleted, Vault, Tag, Category }
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Sort { Updated, Title, Created, Recent }
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub kind: FilterKind, #[serde(default)] pub vault_id: Option<String>, #[serde(default)] pub tag: Option<String>,
    #[serde(default)] pub category: Option<Category>, #[serde(default)] pub query: Option<String>, #[serde(default)] pub sort: Option<Sort>,
}
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IgnoredFindingItem {
    pub item_id: String,
    pub vault_id: String,
    pub title: String,
    pub kind: String,
    pub description: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RotationItem {
    pub item_id: String,
    pub every_days: u32,
    /// Due: whole days since the password changed. Soon: whole days until the reminder is due.
    pub days: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnusedItem {
    pub item_id: String,
    /// Whole days since the last use, or since the last edit/creation when the item was never used here.
    pub days: u64,
    /// True when local usage state recorded a use; false means `days` is measured from the edit/creation time.
    pub last_used: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WatchtowerReport {
    pub score: u8, pub weak: Vec<String>, pub reused: Vec<Vec<String>>, pub old: Vec<String>, pub breached: Vec<String>,
    pub missing_totp: Vec<String>, pub totp_available: Vec<String>, pub passkey_available: Vec<String>,
    pub expired: Vec<String>, pub expiring_soon: Vec<String>,
    pub insecure_urls: Vec<String>,
    pub duplicates: Vec<Vec<String>>,
    pub weak_ssh_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unused: Vec<UnusedItem>,
    /// Logins past their own "change password every N days" reminder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rotation_due: Vec<RotationItem>,
    /// Logins whose reminder falls due within 14 days.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rotation_soon: Vec<RotationItem>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub weak_reasons: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored: Vec<IgnoredFindingItem>,
    pub checked_at: String, pub hibp_checked: bool,
}
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo { pub id: String, pub name: String, pub last_seen: String, pub revoked: bool, pub current: bool }
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme { Dark, Light, System }
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Language { En, My, Th }
/// Item-list row density. Serializes as "comfortable" / "compact" (JSON key
/// `listDensity` via the struct's camelCase rename).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ListDensity { #[default] Comfortable, Compact }
fn default_true() -> bool { true }
fn default_quick_access_hotkey() -> String { "CommandOrControl+Shift+Space".into() }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockTrigger {
    Sleep,
    ScreenLock,
    UserSwitch,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub auto_lock_minutes: u64, pub clipboard_clear_secs: u64, pub theme: Theme, pub language: Language,
    pub biometric_unlock: bool, #[serde(default = "default_quick_access_hotkey")] pub quick_access_hotkey: String,
    /// Opt-in Have I Been Pwned k-anonymity range check in Watchtower. Default off; older settings
    /// files without the field deserialize to off.
    #[serde(default)] pub hibp_check: bool,
    /// Ask before every SSH agent signature (default off: unlocked vault = keys usable).
    #[serde(default)] pub ssh_approve_each_use: bool,
    /// Lock the vault immediately when the Mac goes to sleep (default on).
    #[serde(default = "default_true")] pub lock_on_sleep: bool,
    /// Lock the vault when the screen locks (⌃⌘Q or hot corner, default on).
    #[serde(default = "default_true")] pub lock_on_screen_lock: bool,
    /// Lock the vault when switching away with fast user switching (default on).
    #[serde(default = "default_true")] pub lock_on_user_switch: bool,
    /// Hide gitpass windows from screen recording and screen sharing (default on: screenshots
    /// and screen shares show a blank window; turn off for demos).
    #[serde(default = "default_true")] pub hide_from_screen_recording: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub generator_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub generator_length: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub generator_digits: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub generator_symbols: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub generator_separator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub generator_capitalize: Option<bool>,
    /// Privacy: Settings → Appearance gets a toggle, "Show website icons" (off by default, because fetching tells each site you have a login there).
    #[serde(default)] pub show_website_icons: bool,
    /// Display: render notes and secure notes as safe Markdown (default on).
    #[serde(default = "default_true")] pub render_notes_markdown: bool,
    /// Item-list density (default comfortable; older settings files without
    /// the field deserialize to comfortable).
    #[serde(default)] pub list_density: ListDensity,
    /// Menu bar: Show gitpass in the macOS menu bar (default on).
    #[serde(default = "default_true")] pub show_menu_bar_icon: bool,
    /// Remember generated passwords in an encrypted vault history (default on).
    #[serde(default = "default_true")] pub remember_generated_passwords: bool,
    /// Watchtower "Unused items" threshold in months: 6, 12 or 24; 0 turns the check off (default 12).
    #[serde(default = "default_unused_months")] pub watchtower_unused_months: u64,
    /// Last seen app version for showing the "What's new" release sheet once per upgrade.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub last_seen_version: Option<String>,
}
fn default_unused_months() -> u64 { 12 }
impl Default for Settings {
    fn default() -> Self {
        Settings { auto_lock_minutes: 5, clipboard_clear_secs: crate::CLIPBOARD_CLEAR_SECS, theme: Theme::Dark, language: Language::En,
            biometric_unlock: false, quick_access_hotkey: "CommandOrControl+Shift+Space".into(), hibp_check: false, ssh_approve_each_use: false,
            lock_on_sleep: true, lock_on_screen_lock: true, lock_on_user_switch: true,
            hide_from_screen_recording: true,
            generator_mode: None, generator_length: None, generator_digits: None, generator_symbols: None,
            generator_separator: None, generator_capitalize: None,
            show_website_icons: false, render_notes_markdown: true, show_menu_bar_icon: true, list_density: ListDensity::Comfortable, remember_generated_passwords: true,
            watchtower_unused_months: 12, last_seen_version: None }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeneratorHistoryEntryDto {
    pub id: String,
    pub value: String,
    pub created_at: u64,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus { pub remote: Option<String>, pub last_sync: Option<String>, pub ahead: u64, pub behind: u64, pub dirty: bool }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Identity { pub public_key: String, pub fingerprint: String }
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Onboarding { pub extension_installed: bool, pub emergency_kit_saved: bool, pub repo_connected: bool, pub imported: bool, pub dismissed: bool }
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingPatch {
    pub extension_installed: Option<bool>, pub emergency_kit_saved: Option<bool>, pub repo_connected: Option<bool>,
    pub imported: Option<bool>, pub dismissed: Option<bool>,
}
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnlockState { pub unlocked: bool, pub vault_path: String, pub account: Option<String>,
    /// true when `vault_path` already holds a gitpass vault (format.json); false => offer "create".
    #[serde(default)] pub vault_exists: bool,
    /// true when biometric unlock is enabled and supported on this machine.
    #[serde(default)] pub biometric_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub throttled_secs: Option<u64> }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateAccountResult { pub vault_path: String, pub secret_key: String }

/// Platform secure store for biometric unlock (macOS: Keychain + LocalAuthentication).
/// `store`/`load` hold the master password + Secret Key for `vault_path`; `load` MUST require a
/// successful biometric check before returning anything.
pub trait BiometricStore: Send + Sync {
    fn supported(&self) -> bool;
    fn store(&self, vault_path: &str, password: &str, secret_key: &str) -> Result<()>;
    fn load(&self, vault_path: &str, reason: &str) -> Result<(Zeroizing<String>, Zeroizing<String>)>;
    fn delete(&self, vault_path: &str) -> Result<()>;
}
pub struct NoBiometrics;
impl BiometricStore for NoBiometrics {
    fn supported(&self) -> bool { false }
    fn store(&self, _: &str, _: &str, _: &str) -> Result<()> { bail!("biometric unlock is not supported yet on this platform") }
    fn load(&self, _: &str, _: &str) -> Result<(Zeroizing<String>, Zeroizing<String>)> { bail!("biometric unlock is not supported yet on this platform") }
    fn delete(&self, _: &str) -> Result<()> { Ok(()) }
}
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TotpNow { pub code: String, pub remaining: u64, pub period: u64 }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult { pub parsed: usize, pub added: usize, pub duplicates: usize,
    /// Titles of skipped duplicates and parser warnings (skipped rows, trashed items...).
    #[serde(default)] pub duplicate_titles: Vec<String>, #[serde(default)] pub warnings: Vec<String>,
    /// Item count per category (e.g. {"login": 3, "credit_card": 1}).
    #[serde(default)] pub categories: BTreeMap<String, usize>,
    /// First items of the file as they will be stored (no secrets: only whether a password / TOTP is present).
    #[serde(default)] pub preview: Vec<ImportPreviewItem> }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewItem { pub title: String, pub category: Category, pub username: Option<String>, pub url: Option<String>,
    pub has_password: bool, pub has_totp: bool, pub tags: Vec<String>, pub fields: usize, pub archived: bool }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RotateResult { pub emergency_kit: String }
/// Structured Emergency Kit for the printable kit page. `secret_key` is only filled right after
/// create_account / rotate_secret_key, or after the master password was re-entered (`reveal`).
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EmergencyKitData { pub account_id: String, pub vault_path: String, pub remote: Option<String>, pub device_name: Option<String>,
    pub created_at: String, pub secret_key_available: bool, pub secret_key: Option<String> }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult { pub message: String }

