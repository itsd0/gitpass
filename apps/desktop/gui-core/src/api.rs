//! v0.5.0 contract API: one method per command in `src/api/client.ts` (same snake_case names),
//! returning exactly the contract shapes (camelCase JSON). Tauri wrappers live in src-tauri.
//!
//! Security: concealed / TOTP field values are `null` in every list/detail payload; only
//! [`App::reveal_field`] returns a secret. [`App::copy_field`] copies inside Rust and clears the
//! clipboard after `clipboardClearSecs` if it still holds the copied value.
use crate::{GuiState, Unlocked};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use gitpass_core::audit::{self, AuditOpts, Finding, PwnedRangeClient};
use gitpass_core::crypto::SecretKey;
use gitpass_core::device::iso8601;
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{Item, Session, UnlockOpts};
use gitpass_core::{import, item, totp};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

// ------------------------------------------------------------------ DTOs (contract types.ts)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Field { pub id: String, pub label: String, pub kind: FieldKind, pub value: Option<String>,
    /// Display-only mask for a concealed card number ("•••• •••• •••• 7575"); computed after decryption, never persisted.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub masked: Option<String> }
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Section { pub id: String, pub title: Option<String>, pub fields: Vec<Field> }
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Attachment { pub id: String, pub name: String, pub size: u64, pub mime: String }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemSummary {
    pub id: String, pub vault_id: String, pub category: Category, pub title: String, pub subtitle: String,
    pub favorite: bool, pub archived: bool, pub deleted_at: Option<String>, pub tags: Vec<String>,
    pub updated_at: String, pub created_at: String, pub url: Option<String>,
}
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRef { pub version: u64, pub updated_at: String }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    #[serde(flatten)]
    pub summary: ItemSummary,
    pub sections: Vec<Section>, pub notes: Option<String>, pub attachments: Vec<Attachment>,
    pub history: Vec<HistoryRef>, pub password_strength: Option<u8>, pub has_totp: bool,
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
pub enum Sort { Updated, Title, Created }
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub kind: FilterKind, #[serde(default)] pub vault_id: Option<String>, #[serde(default)] pub tag: Option<String>,
    #[serde(default)] pub category: Option<Category>, #[serde(default)] pub query: Option<String>, #[serde(default)] pub sort: Option<Sort>,
}
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WatchtowerReport {
    pub score: u8, pub weak: Vec<String>, pub reused: Vec<Vec<String>>, pub old: Vec<String>, pub breached: Vec<String>,
    pub missing_totp: Vec<String>, pub totp_available: Vec<String>, pub passkey_available: Vec<String>,
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub auto_lock_minutes: u64, pub clipboard_clear_secs: u64, pub theme: Theme, pub language: Language,
    pub biometric_unlock: bool, pub quick_access_hotkey: String,
    /// Opt-in Have I Been Pwned k-anonymity range check in Watchtower. Default off; older settings
    /// files without the field deserialize to off.
    #[serde(default)] pub hibp_check: bool,
    /// Ask before every SSH agent signature (default off: unlocked vault = keys usable).
    #[serde(default)] pub ssh_approve_each_use: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Settings { auto_lock_minutes: 5, clipboard_clear_secs: crate::CLIPBOARD_CLEAR_SECS, theme: Theme::Dark, language: Language::En,
            biometric_unlock: false, quick_access_hotkey: "CommandOrControl+Shift+Space".into(), hibp_check: false, ssh_approve_each_use: false }
    }
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
    #[serde(default)] pub biometric_available: bool }
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

// ------------------------------------------------------------------ local (non-secret) state file
pub const SETTINGS_FILE: &str = ".gitpass-settings.json";

/// Contents of `.gitpass-settings.json` (gitignored; non-secret only).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalFile {
    #[serde(default)] pub settings: Option<Settings>,
    #[serde(default)] pub onboarding: Onboarding,
    /// Rust-only setting: allow fetching favicons. Default off. (MVP: favicon_url still returns null.)
    #[serde(default)] pub allow_favicons: bool,
    /// Local labels for vault member fingerprints.
    #[serde(default)] pub member_labels: BTreeMap<String, String>,
    #[serde(default)] pub last_sync: Option<String>,
    #[serde(default)] pub last_vault_path: Option<String>,
}

// ------------------------------------------------------------------ clipboard
pub trait Clipboard: Send + Sync {
    fn set_text(&self, s: &str) -> Result<()>;
    fn get_text(&self) -> Option<String>;
}

/// In-memory clipboard (tests / headless).
#[derive(Default)]
pub struct MemClipboard(pub Mutex<Option<String>>);
impl Clipboard for MemClipboard {
    fn set_text(&self, s: &str) -> Result<()> { *self.0.lock().unwrap() = Some(s.to_string()); Ok(()) }
    fn get_text(&self) -> Option<String> { self.0.lock().unwrap().clone() }
}

/// Real HIBP range client (k-anonymity; only 5 hex chars of SHA-1 leave the process).
pub struct UreqHibp;
impl PwnedRangeClient for UreqHibp {
    fn range(&self, prefix: &str) -> Result<String> {
        let r = ureq::get(&format!("{}{prefix}", audit::HIBP_RANGE_URL)).set("Add-Padding", "true")
            .set("User-Agent", concat!("gitpass-desktop/", env!("CARGO_PKG_VERSION"))).call()
            .map_err(|e| anyhow!("HIBP request failed: {e}"))?;
        Ok(r.into_string()?)
    }
}

// ------------------------------------------------------------------ App
pub struct App {
    pub state: GuiState,
    settings_dir: PathBuf,
    clipboard: Arc<dyn Clipboard>,
    clip_gen: Arc<AtomicU64>,
    hibp: Box<dyn PwnedRangeClient + Send + Sync>,
    unlock_opts: Mutex<UnlockOpts>,
    /// Emergency kit text with the NEW Secret Key, kept only right after rotate_secret_key (until lock).
    fresh_kit: Mutex<Option<Zeroizing<String>>>,
    /// (account id, encoded Secret Key) right after create_account / rotate_secret_key (until lock).
    fresh_secret: Mutex<Option<(String, Zeroizing<String>)>>,
    biometric: Box<dyn BiometricStore>,
    file_lock: Mutex<()>,
}

fn ts(secs_str: &str) -> String { secs_str.trim().parse::<i64>().map(iso8601).unwrap_or_else(|_| secs_str.to_string()) }
fn nonempty(o: Option<String>) -> Option<String> { o.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) }

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
fn card_digits(r: &ItemRecord) -> Option<String> {
    if r.category != Category::CreditCard { return None; }
    // First card-number-like field that actually holds >= 8 digits ("cardholder" etc. are excluded above).
    r.view_sections().into_iter().flat_map(|s| s.fields).filter(is_card_number).find_map(|f| f.value.as_deref().and_then(card_digits_of))
}
/// Card brand from the BIN (leading digits). None when unknown.
pub fn card_brand(d: &str) -> Option<&'static str> {
    let p = |n: usize| d.get(..n).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    Some(match () {
        _ if d.starts_with('4') => "Visa",
        _ if (51..=55).contains(&p(2)) || (2221..=2720).contains(&p(4)) => "Mastercard",
        _ if p(2) == 34 || p(2) == 37 => "Amex",
        _ if p(4) == 6011 || p(2) == 65 || (644..=649).contains(&p(3)) => "Discover",
        _ if (3528..=3589).contains(&p(4)) => "JCB",
        _ if p(2) == 36 || p(2) == 38 || (300..=305).contains(&p(3)) => "Diners Club",
        _ if p(2) == 62 => "UnionPay",
        _ => return None,
    })
}
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
    match r.category {
        Category::Login => r.item.username.clone().unwrap_or_default(),
        Category::CreditCard => card_digits(r).map(|d| card_label(&d)).unwrap_or_default(),
        Category::Identity => find(&|f| { let l = f.label.to_lowercase(); f.id == "name" || l.contains("name") })
            .or_else(|| find(&|f| !f.kind.is_secret())).unwrap_or_default(),
        _ => find(&|f| !f.kind.is_secret() && f.value.as_deref().is_some_and(|v| !v.is_empty())).unwrap_or_default(),
    }
}

fn to_dto_sections(r: &ItemRecord) -> Vec<Section> {
    r.view_sections().into_iter().map(|s| Section { id: s.id, title: s.title, fields: s.fields.into_iter().map(|f| Field {
        masked: (r.category == Category::CreditCard && f.kind.is_secret() && is_card_number(&f))
            .then(|| f.value.as_deref().map(|n| n.chars().filter(|c| c.is_ascii_digit()).collect::<String>()).filter(|d| d.len() >= 8).map(|d| card_mask(&d))).flatten(),
        value: if f.kind.is_secret() { None } else { f.value.clone() }, id: f.id, label: f.label, kind: f.kind }).collect() }).collect()
}

fn summary(vid: &str, iid: &str, r: &ItemRecord) -> ItemSummary {
    ItemSummary {
        id: iid.into(), vault_id: vid.into(), category: r.category, title: r.item.title.clone(), subtitle: subtitle(r),
        favorite: r.favorite, archived: r.archived, deleted_at: r.deleted_at.map(|d| iso8601(d as i64)), tags: r.tags.clone(),
        updated_at: ts(&r.item.updated_at), created_at: r.created_at.map(|c| iso8601(c as i64)).unwrap_or_else(|| ts(&r.item.updated_at)),
        url: r.item.url.clone(),
    }
}

fn has_totp(r: &ItemRecord) -> bool { r.view_sections().iter().flat_map(|s| s.fields.iter()).any(|f| f.kind == FieldKind::Totp && f.value.is_some()) }

fn detail(vid: &str, iid: &str, r: &ItemRecord) -> ItemDetail {
    let pw = r.view_sections().into_iter().flat_map(|s| s.fields).find(|f| f.kind == FieldKind::Concealed && f.value.is_some());
    ItemDetail {
        summary: summary(vid, iid, r), sections: to_dto_sections(r), notes: r.item.notes.clone(),
        attachments: r.attachments.iter().map(|a| Attachment { id: a.id.clone(), name: a.name.clone(), size: a.size, mime: a.mime.clone() }).collect(),
        history: r.history.iter().rev().map(|h| HistoryRef { version: h.version, updated_at: ts(&h.updated_at) }).collect(),
        password_strength: if r.category == Category::Login { r.item.password.as_deref().map(strength) } else { pw.and_then(|f| f.value).map(|v| strength(&v)) },
        has_totp: has_totp(r),
    }
}

fn mime_for(name: &str) -> &'static str {
    match Path::new(name).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("pdf") => "application/pdf", Some("png") => "image/png", Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif", Some("txt") => "text/plain", Some("json") => "application/json", Some("zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

const WORDS: &[&str] = &["acorn","amber","anchor","apple","arrow","aspen","atlas","badge","bamboo","banjo","beacon","birch","bison","blossom","breeze","bridge",
    "cactus","canyon","carbon","cedar","cherry","cinder","citrus","clover","cobalt","comet","coral","cosmos","crane","crystal","dahlia","delta",
    "desert","dolphin","dragon","drift","eagle","ember","falcon","fern","fjord","forest","fossil","galaxy","garnet","geyser","glacier","granite",
    "harbor","hazel","heron","horizon","indigo","island","ivory","jasmine","jungle","kayak","kernel","lagoon","lantern","lava","lemon","lilac",
    "lotus","lunar","magnet","mango","maple","marble","meadow","meteor","mint","mosaic","nectar","nebula","oasis","ocean","olive","onyx",
    "orbit","orchid","otter","panda","pebble","pepper","phoenix","pine","planet","plum","polar","prairie","quartz","quill","radar","raven",
    "reef","ripple","river","rocket","saffron","salmon","sapphire","savanna","sequoia","shadow","sierra","silver","sparrow","spruce","summit","sunset",
    "thistle","thunder","tiger","topaz","tulip","tundra","turtle","velvet","violet","walnut","willow","winter","zebra","zenith","yarrow","yonder"];

/// Generate a password (`words`: `length` words from a 128-word list joined by '-', ~7 bits/word).
pub fn generate(length: usize, symbols: bool, digits: bool, words: bool) -> String {
    use rand::rngs::OsRng;
    use rand::seq::SliceRandom;
    use rand::Rng;
    let mut rng = OsRng;
    if words {
        let n = length.clamp(3, 20);
        let mut w: Vec<String> = (0..n).map(|_| WORDS.choose(&mut rng).unwrap().to_string()).collect();
        if digits { let i = rng.gen_range(0..w.len()); w[i].push_str(&rng.gen_range(0..10).to_string()); }
        return w.join(if symbols { "-" } else { "." });
    }
    let len = length.clamp(8, 128);
    let mut pool: Vec<u8> = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".to_vec();
    let dg = b"23456789"; let sy = b"!@#$%^&*-_=+?";
    if digits { pool.extend_from_slice(dg); }
    if symbols { pool.extend_from_slice(sy); }
    loop {
        let p: String = (0..len).map(|_| *pool.choose(&mut rng).unwrap() as char).collect();
        let ok = (!digits || p.bytes().any(|b| dg.contains(&b))) && (!symbols || p.bytes().any(|b| sy.contains(&b)));
        if ok { return p; }
    }
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let o = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

impl App {
    /// `settings_dir`: where `.gitpass-settings.json` lives (the desktop app uses the default vault dir).
    pub fn new(settings_dir: PathBuf, clipboard: Arc<dyn Clipboard>) -> Self {
        let app = App { state: GuiState::new(5), settings_dir, clipboard, clip_gen: Arc::new(AtomicU64::new(0)), hibp: Box::new(UreqHibp),
            unlock_opts: Mutex::new(UnlockOpts::default()), fresh_kit: Mutex::new(None), fresh_secret: Mutex::new(None), biometric: Box::new(NoBiometrics), file_lock: Mutex::new(()) };
        let s = app.get_settings();
        app.state.set_auto_lock_minutes(s.auto_lock_minutes);
        app
    }
    /// Inject an HIBP client (tests).
    /// Inject the platform biometric store.
    pub fn with_biometric(mut self, b: Box<dyn BiometricStore>) -> Self { self.biometric = b; self }
    pub fn with_hibp(mut self, c: Box<dyn PwnedRangeClient + Send + Sync>) -> Self { self.hibp = c; self }
    #[doc(hidden)]
    pub fn set_unlock_opts(&self, o: UnlockOpts) { *self.unlock_opts.lock().unwrap() = o; }

    fn settings_path(&self) -> PathBuf { self.settings_dir.join(SETTINGS_FILE) }
    pub fn read_local(&self) -> LocalFile {
        std::fs::read(self.settings_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    fn update_local<T>(&self, f: impl FnOnce(&mut LocalFile) -> T) -> Result<T> {
        let _g = self.file_lock.lock().map_err(|_| anyhow!("poisoned"))?;
        let mut l = self.read_local();
        let r = f(&mut l);
        std::fs::create_dir_all(&self.settings_dir)?;
        let tmp = self.settings_path().with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&l)?)?;
        std::fs::rename(&tmp, self.settings_path())?;
        if self.settings_dir.join("format.json").exists() { gitpass_core::device::ensure_ignored(&self.settings_dir, SETTINGS_FILE)?; }
        Ok(r)
    }

    fn with<T>(&self, f: impl FnOnce(&mut Unlocked) -> Result<T>) -> Result<T> { self.state.with(f) }
    fn write<T>(&self, f: impl FnOnce(&mut Unlocked) -> Result<T>) -> Result<T> { self.state.with(|u| { GuiState::writable(u)?; f(u) }) }

    // ---------------- session
    pub fn status(&self) -> Result<UnlockState> {
        self.state.check_idle();
        let g = self.state.inner.lock().map_err(|_| anyhow!("state poisoned"))?;
        Ok(match &g.unlocked {
            Some(u) => UnlockState { unlocked: true, vault_path: u.session.root.display().to_string(), account: Some(u.session.fmt.account_id.clone()), vault_exists: true,
                biometric_available: false },
            None => {
                let vp = self.read_local().last_vault_path.unwrap_or_else(|| crate::default_vault_dir().display().to_string());
                let exists = PathBuf::from(&vp).join("format.json").exists();
                let bio = exists && self.biometric.supported() && self.get_settings().biometric_unlock;
                UnlockState { unlocked: false, vault_path: vp, account: None, vault_exists: exists, biometric_available: bio }
            }
        })
    }
    pub fn unlock(&self, vault_path: &str, password: &str, secret_key: Option<&str>) -> Result<UnlockState> {
        let dir = PathBuf::from(if vault_path.trim().is_empty() { crate::default_vault_dir().display().to_string() } else { vault_path.trim().to_string() });
        if password.is_empty() { bail!("master password is empty"); }
        if !dir.join("format.json").exists() { bail!("{} is not a gitpass vault (no format.json)", dir.display()); }
        let sk = match secret_key.filter(|k| !k.trim().is_empty()) {
            Some(k) => crate::parse_secret_key(k)?,
            None => crate::read_secret_key_file(&dir.join("secret.key"))?,
        };
        let opts = { let o = self.unlock_opts.lock().map_err(|_| anyhow!("poisoned"))?; UnlockOpts { accept_rollback: o.accept_rollback, target_kdf: o.target_kdf.clone() } };
        self.state.unlock_with_opts(&dir, password, sk, None, &opts)?;
        let _ = self.update_local(|l| l.last_vault_path = Some(dir.display().to_string()));
        self.ensure_default_vault();
        self.status()
    }
    /// `gitpass init` creates no vault; give a fresh account a "Personal" vault so the sidebar is not empty.
    fn ensure_default_vault(&self) {
        let _ = self.write(|u| { if u.session.list_vaults()?.is_empty() { u.session.create_vault_full("Personal", None, None, None)?; } Ok(()) });
    }
    /// Create a new account/vault at `vault_path` (like `gitpass init`) and unlock it.
    /// Returns the Secret Key ONCE for the user to save; it is written to disk only if `save_secret_key`.
    pub fn create_account(&self, vault_path: &str, password: &str, save_secret_key: bool) -> Result<CreateAccountResult> {
        let dir = PathBuf::from(if vault_path.trim().is_empty() { crate::default_vault_dir().display().to_string() } else { vault_path.trim().to_string() });
        if password.chars().count() < 8 { bail!("master password must be at least 8 characters"); }
        if dir.join("format.json").exists() { bail!("{} already contains a gitpass vault", dir.display()); }
        let sk = gitpass_core::store::init(&dir, password, gitpass_core::crypto::KdfParams::default())?;
        let enc = Zeroizing::new(sk.encode());
        if save_secret_key {
            gitpass_core::fsutil::atomic_write(&dir.join("secret.key"), format!("{}\n", enc.as_str()).as_bytes())?;
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(dir.join("secret.key"), std::fs::Permissions::from_mode(0o600))?; }
        }
        self.unlock(&dir.display().to_string(), password, Some(&enc))?;
        if let Ok(acc) = self.with(|u| Ok(u.session.fmt.account_id.clone())) {
            *self.fresh_secret.lock().map_err(|_| anyhow!("poisoned"))? = Some((acc, enc.clone()));
        }
        Ok(CreateAccountResult { vault_path: dir.display().to_string(), secret_key: enc.to_string() })
    }
    /// Unlock with credentials released by the platform biometric store (Touch ID on macOS).
    pub fn unlock_biometric(&self, vault_path: &str) -> Result<UnlockState> {
        if !self.get_settings().biometric_unlock { bail!("biometric unlock is not enabled"); }
        let vp = if vault_path.trim().is_empty() { self.status()?.vault_path } else { vault_path.trim().to_string() };
        let (pw, sk) = self.biometric.load(&vp, "unlock gitpass")?;
        self.unlock(&vp, &pw, Some(&sk))
    }
    pub fn lock(&self) {
        self.state.lock();
        if let Ok(mut k) = self.fresh_kit.lock() { *k = None; }
        if let Ok(mut k) = self.fresh_secret.lock() { *k = None; }
    }

    // ---------------- vaults
    fn members_of(s: &Session, vid: &str, labels: &BTreeMap<String, String>) -> Result<Vec<Member>> {
        let mut out = vec![];
        let ms = s.members(vid)?;
        if !ms.is_empty() {
            if let Some((_, fpr)) = s.identity_if_exists()? { out.push(Member { label: labels.get(&fpr).cloned(), fingerprint: fpr, is_self: true }); }
        }
        for (fpr, _) in ms { out.push(Member { label: labels.get(&fpr).cloned(), fingerprint: fpr, is_self: false }); }
        Ok(out)
    }
    fn vault_info(s: &Session, vid: &str, labels: &BTreeMap<String, String>) -> Result<VaultInfo> {
        let m = s.vault_meta(vid)?;
        let count = s.list_records(vid)?.iter().filter(|(_, _, r)| r.deleted_at.is_none()).count();
        let members = Self::members_of(s, vid, labels)?;
        Ok(VaultInfo { id: vid.into(), name: m.name.clone(), description: m.description.clone(), item_count: count, shared: !members.is_empty(),
            members, icon: m.icon.clone(), color: m.color.clone() })
    }
    pub fn list_vaults(&self) -> Result<Vec<VaultInfo>> {
        let labels = self.read_local().member_labels;
        self.with(|u| u.session.list_vaults()?.iter().map(|(id, _)| Self::vault_info(&u.session, id, &labels)).collect())
    }
    pub fn create_vault(&self, name: &str, description: Option<String>, icon: Option<String>, color: Option<String>) -> Result<VaultInfo> {
        if name.trim().is_empty() { bail!("vault name is empty"); }
        let labels = self.read_local().member_labels;
        self.write(|u| {
            let id = u.session.create_vault_full(name.trim(), nonempty(description), nonempty(icon), nonempty(color))?;
            Self::vault_info(&u.session, &id, &labels)
        })
    }

    /// Rename / re-describe / re-style a vault. `None` description/icon/color clears them.
    pub fn update_vault(&self, vault_id: &str, name: &str, description: Option<String>, icon: Option<String>, color: Option<String>) -> Result<VaultInfo> {
        let labels = self.read_local().member_labels;
        self.write(|u| {
            u.session.update_vault_meta(vault_id, gitpass_core::store::VaultMeta { name: name.trim().into(), description: nonempty(description), icon: nonempty(icon), color: nonempty(color) })?;
            Self::vault_info(&u.session, vault_id, &labels)
        })
    }
    /// Permanently delete a vault. `confirm_name` must equal the vault's current name (typed by the user).
    pub fn delete_vault(&self, vault_id: &str, confirm_name: &str) -> Result<()> {
        self.write(|u| {
            let name = u.session.vault_meta(vault_id)?.name.clone();
            if confirm_name != name { bail!("confirmation name does not match"); }
            u.session.delete_vault(vault_id)
        })
    }

    // ---------------- items
    fn all_records(s: &Session) -> Result<Vec<(String, String, ItemRecord)>> {
        let mut out = vec![];
        for (vid, _) in s.list_vaults()? { for (iid, _, r) in s.list_records(&vid)? { out.push((vid.clone(), iid, r)); } }
        Ok(out)
    }
    pub fn list_items(&self, filter: &Filter) -> Result<Vec<ItemSummary>> {
        self.with(|u| {
            let all = Self::all_records(&u.session)?;
            let q = filter.query.as_deref().unwrap_or("").trim().to_lowercase();
            let mut v: Vec<(ItemSummary, u64)> = all.iter().filter(|(vid, _, r)| {
                let live = r.deleted_at.is_none();
                let keep = match filter.kind {
                    FilterKind::All => live && !r.archived,
                    FilterKind::Favorites => live && !r.archived && r.favorite,
                    FilterKind::Archive => live && r.archived,
                    FilterKind::Deleted => !live,
                    FilterKind::Vault => live && !r.archived && Some(vid) == filter.vault_id.as_ref(),
                    FilterKind::Tag => live && !r.archived && filter.tag.as_ref().is_some_and(|t| r.tags.contains(t)),
                    FilterKind::Category => live && !r.archived && Some(r.category) == filter.category,
                };
                keep
            }).map(|(vid, iid, r)| (summary(vid, iid, r), r.created_at.unwrap_or(0)))
              .filter(|(s, _)| q.is_empty() || [s.title.as_str(), s.subtitle.as_str(), s.url.as_deref().unwrap_or("")].iter().any(|x| x.to_lowercase().contains(&q))
                    || s.tags.iter().any(|t| t.to_lowercase().contains(&q)))
              .collect();
            match filter.sort.unwrap_or(Sort::Updated) {
                Sort::Title => v.sort_by(|a, b| a.0.title.to_lowercase().cmp(&b.0.title.to_lowercase())),
                Sort::Updated => v.sort_by(|a, b| b.0.updated_at.cmp(&a.0.updated_at).then_with(|| a.0.title.cmp(&b.0.title))),
                Sort::Created => v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.created_at.cmp(&a.0.created_at))),
            }
            Ok(v.into_iter().map(|x| x.0).collect())
        })
    }
    pub fn get_item(&self, vault_id: &str, item_id: &str) -> Result<ItemDetail> {
        self.with(|u| { let (_, r) = u.session.read_record(vault_id, item_id)?; Ok(detail(vault_id, item_id, &r)) })
    }
    /// The ONLY call that returns a stored secret value.
    pub fn reveal_field(&self, vault_id: &str, item_id: &str, field_id: &str) -> Result<Zeroizing<String>> {
        self.with(|u| {
            let (_, r) = u.session.read_record(vault_id, item_id)?;
            let f = r.find_field(field_id).ok_or_else(|| anyhow!("item has no field '{field_id}'"))?;
            Ok(Zeroizing::new(f.value.clone().unwrap_or_default()))
        })
    }
    /// Copy a field value (TOTP fields: the current code) to the clipboard; cleared after
    /// `clipboardClearSecs` only if the clipboard still holds this value and nothing newer was copied.
    pub fn copy_field(&self, vault_id: &str, item_id: &str, field_id: &str) -> Result<()> {
        let v = self.with(|u| {
            let (_, r) = u.session.read_record(vault_id, item_id)?;
            let f = r.find_field(field_id).ok_or_else(|| anyhow!("item has no field '{field_id}'"))?;
            let raw = f.value.clone().filter(|v| !v.is_empty()).ok_or_else(|| anyhow!("field '{field_id}' is empty"))?;
            Ok(Zeroizing::new(if f.kind == FieldKind::Totp { totp::code_now(&totp::parse(&raw)?).0 } else { raw }))
        })?;
        self.clipboard.set_text(&v)?;
        let gen = self.clip_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let secs = self.get_settings().clipboard_clear_secs;
        if secs > 0 {
            let (cb, g) = (self.clipboard.clone(), self.clip_gen.clone());
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(secs));
                if g.load(Ordering::SeqCst) == gen && cb.get_text().as_deref() == Some(v.as_str()) { let _ = cb.set_text(""); }
            });
        }
        Ok(())
    }
    pub fn totp_now(&self, vault_id: &str, item_id: &str) -> Result<TotpNow> {
        self.with(|u| {
            let (_, r) = u.session.read_record(vault_id, item_id)?;
            let f = r.view_sections().into_iter().flat_map(|s| s.fields).find(|f| f.kind == FieldKind::Totp && f.value.is_some()).ok_or_else(|| anyhow!("item has no TOTP"))?;
            let spec = totp::parse(f.value.as_deref().unwrap())?;
            let (code, remaining) = totp::code_now(&spec);
            Ok(TotpNow { code, remaining, period: spec.period })
        })
    }

    /// The UI sends `id: ""` for newly added sections/fields; assign fresh random ids here (before
    /// validation) so they are unique and never collide with legacy login ids.
    fn assign_ids(input: &ItemInput) -> ItemInput {
        use rand::Rng;
        let fresh = |p: &str| { let n: u64 = rand::rngs::OsRng.gen(); format!("{p}_{n:016x}") };
        let mut out = input.clone();
        for s in &mut out.sections {
            if s.id.trim().is_empty() { s.id = fresh("s"); }
            for f in &mut s.fields { if f.id.trim().is_empty() { f.id = fresh("f"); } }
        }
        out
    }
    fn validate(input: &ItemInput) -> Result<()> {
        if input.title.trim().is_empty() { bail!("title is required"); }
        let mut ids = BTreeSet::new();
        for s in &input.sections { for f in &s.fields {
            if f.id.trim().is_empty() { bail!("field id is empty"); }
            if !ids.insert(f.id.clone()) { bail!("duplicate field id '{}'", f.id); }
            if f.kind == FieldKind::Totp { if let Some(v) = f.value.as_deref().filter(|v| !v.is_empty()) { totp::parse(v)?; } }
        } }
        Ok(())
    }
    /// Build a record from input. Secret fields sent back as `null` keep their stored value.
    fn apply_input(r: &mut ItemRecord, input: &ItemInput) {
        let old = r.clone();
        r.category = input.category;
        r.item.title = input.title.trim().into();
        r.item.notes = nonempty(input.notes.clone());
        r.item.url = nonempty(input.url.clone());
        if let Some(t) = &input.tags { let mut t: Vec<String> = t.iter().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(); t.dedup(); r.tags = t; }
        if let Some(f) = input.favorite { r.favorite = f; }
        let secs: Vec<item::Section> = input.sections.iter().map(|s| item::Section { id: s.id.clone(), title: s.title.clone(), fields: s.fields.iter().map(|f| {
            let value = match &f.value { None if f.kind.is_secret() => old.find_field(&f.id).and_then(|o| o.value), v => v.clone() };
            item::Field { id: f.id.clone(), label: f.label.clone(), kind: f.kind, value, removed: false }
        }).collect() }).collect();
        r.apply_sections(&secs);
    }
    pub fn create_item(&self, input: &ItemInput) -> Result<ItemSummary> {
        let input = &Self::assign_ids(input);
        Self::validate(input)?;
        self.write(|u| {
            let mut r = ItemRecord::new(Item::default());
            Self::apply_input(&mut r, input);
            let iid = u.session.add_record(&input.vault_id, r)?;
            let (_, r) = u.session.read_record(&input.vault_id, &iid)?;
            Ok(summary(&input.vault_id, &iid, &r))
        })
    }
    pub fn update_item(&self, item_id: &str, input: &ItemInput) -> Result<ItemSummary> {
        let input = &Self::assign_ids(input);
        Self::validate(input)?;
        self.write(|u| {
            let (_, mut r) = u.session.read_record(&input.vault_id, item_id)?;
            Self::apply_input(&mut r, input);
            u.session.update_record(&input.vault_id, item_id, r)?;
            let (_, r) = u.session.read_record(&input.vault_id, item_id)?;
            Ok(summary(&input.vault_id, item_id, &r))
        })
    }
    pub fn set_favorite(&self, vault_id: &str, item_id: &str, favorite: bool) -> Result<()> { self.write(|u| u.session.set_favorite(vault_id, item_id, favorite)) }
    pub fn archive_item(&self, vault_id: &str, item_id: &str, archived: bool) -> Result<()> { self.write(|u| u.session.set_archived(vault_id, item_id, archived)) }
    pub fn delete_item(&self, vault_id: &str, item_id: &str) -> Result<()> { self.write(|u| u.session.soft_delete(vault_id, item_id)) }
    pub fn restore_item(&self, vault_id: &str, item_id: &str) -> Result<()> { self.write(|u| u.session.restore(vault_id, item_id)) }
    pub fn purge_item(&self, vault_id: &str, item_id: &str) -> Result<()> { self.write(|u| u.session.purge_item(vault_id, item_id)) }
    /// Bulk action on many items: one vault write (single manifest commit) for every action except
    /// move (copy commit + remove commit) and items with attachments on move/copy (one extra commit per file).
    pub fn bulk_items(&self, req: &BulkRequest) -> Result<BulkResult> {
        if req.items.is_empty() { return Ok(BulkResult { count: 0, items: vec![] }); }
        if req.items.len() > BULK_MAX { bail!("too many items selected (max {BULK_MAX})"); }
        let t: Vec<(String, String)> = req.items.iter().map(|r| (r.vault_id.clone(), r.item_id.clone())).collect();
        let tag = || -> Result<String> {
            let x = req.tag.as_deref().map(str::trim).unwrap_or_default();
            if x.is_empty() { bail!("tag is required"); }
            if x.chars().count() > 64 { bail!("tag is too long"); }
            Ok(x.to_string())
        };
        let same = |count: usize| BulkResult { count, items: req.items.clone() };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        self.write(|u| {
            let s = &u.session;
            Ok(match req.action.as_str() {
                "trash" => same(s.mutate_records(&t, |r| { if r.deleted_at.is_none() { r.deleted_at = Some(now); } })?),
                "restore" => same(s.mutate_records(&t, |r| { r.deleted_at = None; })?),
                "archive" => same(s.mutate_records(&t, |r| r.archived = true)?),
                "unarchive" => same(s.mutate_records(&t, |r| r.archived = false)?),
                "favorite" => same(s.mutate_records(&t, |r| r.favorite = true)?),
                "unfavorite" => same(s.mutate_records(&t, |r| r.favorite = false)?),
                "add_tag" => { let g = tag()?; same(s.mutate_records(&t, |r| if !r.tags.contains(&g) { r.tags.push(g.clone()); })?) }
                "remove_tag" => { let g = tag()?; same(s.mutate_records(&t, |r| r.tags.retain(|x| x != &g))?) }
                "purge" => BulkResult { count: s.remove_items(&t)?, items: vec![] },
                "move" | "copy" => {
                    let to = req.to_vault_id.as_deref().filter(|x| !x.is_empty()).ok_or_else(|| anyhow!("toVaultId is required"))?;
                    if !s.list_vaults()?.iter().any(|(v, _)| v == to) { bail!("no vault {to}"); }
                    if req.action == "copy" {
                        let n = s.copy_items_to(&t, to)?;
                        BulkResult { count: n.len(), items: n.into_iter().map(|i| ItemRef { vault_id: to.to_string(), item_id: i }).collect() }
                    } else {
                        let n = s.move_items(&t, to)?;
                        BulkResult { count: n.len(), items: n.into_iter().map(|(_, _, i)| ItemRef { vault_id: to.to_string(), item_id: i }).collect() }
                    }
                }
                other => bail!("unknown bulk action '{other}'"),
            })
        })
    }
    pub fn move_item(&self, vault_id: &str, item_id: &str, to_vault_id: &str) -> Result<ItemSummary> {
        self.write(|u| {
            let nid = u.session.move_item(vault_id, item_id, to_vault_id)?;
            let (_, r) = u.session.read_record(to_vault_id, &nid)?;
            Ok(summary(to_vault_id, &nid, &r))
        })
    }
    pub fn duplicate_item(&self, vault_id: &str, item_id: &str) -> Result<ItemSummary> {
        self.write(|u| {
            let nid = u.session.copy_item_to(vault_id, item_id, vault_id)?;
            let (_, mut r) = u.session.read_record(vault_id, &nid)?;
            r.item.title = format!("{} copy", r.item.title);
            r.favorite = false;
            u.session.update_record(vault_id, &nid, r)?;
            let (_, r) = u.session.read_record(vault_id, &nid)?;
            Ok(summary(vault_id, &nid, &r))
        })
    }
    pub fn item_version(&self, vault_id: &str, item_id: &str, version: u64) -> Result<ItemDetail> {
        self.with(|u| {
            let r = u.session.item_version(vault_id, item_id, version)?;
            let mut d = detail(vault_id, item_id, &r);
            d.history.clear();
            d.attachments.clear();
            if let Some(h) = u.session.read_record(vault_id, item_id)?.1.history.iter().find(|h| h.version == version) { d.summary.updated_at = ts(&h.updated_at); }
            Ok(d)
        })
    }
    pub fn add_attachment(&self, vault_id: &str, item_id: &str, path: &str) -> Result<Attachment> {
        let p = PathBuf::from(path);
        let meta = std::fs::metadata(&p).with_context(|| format!("cannot read {path}"))?;
        if meta.len() as usize > item::MAX_ATTACHMENT_BYTES { bail!("attachment too large (limit {} MiB)", item::MAX_ATTACHMENT_BYTES / 1024 / 1024); }
        let data = Zeroizing::new(std::fs::read(&p)?);
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
        self.write(|u| {
            let a = u.session.add_attachment(vault_id, item_id, &name, mime_for(&name), &data)?;
            Ok(Attachment { id: a.id.clone(), name: a.name.clone(), size: a.size, mime: a.mime.clone() })
        })
    }
    pub fn save_attachment(&self, vault_id: &str, item_id: &str, attachment_id: &str, dest_path: &str) -> Result<()> {
        let data = self.with(|u| Ok(Zeroizing::new(u.session.read_attachment(vault_id, item_id, attachment_id)?)))?;
        std::fs::write(dest_path, &*data).with_context(|| format!("cannot write {dest_path}"))
    }
    pub fn list_tags(&self) -> Result<Vec<String>> {
        self.with(|u| Ok(Self::all_records(&u.session)?.iter().filter(|(_, _, r)| r.deleted_at.is_none())
            .flat_map(|(_, _, r)| r.tags.clone()).collect::<BTreeSet<_>>().into_iter().collect()))
    }

    // ---------------- tools
    pub fn generate_password(&self, length: usize, symbols: bool, digits: bool, words: bool) -> String { generate(length, symbols, digits, words) }
    pub fn password_strength(&self, password: &str) -> u8 { strength(password) }

    /// Watchtower over live login items, reusing core `audit` (HIBP only when `hibp`).
    pub fn watchtower(&self, hibp: bool) -> Result<WatchtowerReport> {
        // Network breach check only when the caller asks AND the user opted in via Settings.hibpCheck.
        let hibp = hibp && self.get_settings().hibp_check;
        let client: Option<&dyn PwnedRangeClient> = if hibp { Some(self.hibp.as_ref() as &dyn PwnedRangeClient) } else { None };
        self.with(|u| {
            let records = Self::all_records(&u.session)?;
            let passkey_sites: BTreeSet<String> = records.iter()
                .filter(|(_, _, r)| r.deleted_at.is_none() && !r.archived && r.category == Category::Passkey)
                .filter_map(|(_, _, r)| r.find_field(gitpass_core::passkey::F_RP_ID).and_then(|f| f.value))
                .collect();
            let items: Vec<(String, String, Item)> = records.into_iter()
                .filter(|(_, _, r)| r.deleted_at.is_none() && !r.archived && r.category == Category::Login)
                .map(|(v, i, r)| (v, i, r.item.clone())).collect();
            let rep = audit::audit_with_passkeys(&items, &passkey_sites, &AuditOpts::default(), client)?;
            let mut out = WatchtowerReport { score: 100, weak: vec![], reused: vec![], old: vec![], breached: vec![], missing_totp: vec![],
                totp_available: vec![], passkey_available: vec![],
                checked_at: gitpass_core::device::now_iso8601(), hibp_checked: hibp };
            let mut groups: BTreeMap<usize, Vec<String>> = BTreeMap::new();
            for f in &rep.per_item {
                for x in &f.findings {
                    match x {
                        Finding::Weak { .. } => out.weak.push(f.id.clone()),
                        Finding::Reused { group, .. } => groups.entry(*group).or_default().push(f.id.clone()),
                        Finding::Old { .. } => out.old.push(f.id.clone()),
                        Finding::Pwned { .. } => out.breached.push(f.id.clone()),
                        Finding::MissingTotp => out.missing_totp.push(f.id.clone()),
                        Finding::TotpAvailable { .. } => out.totp_available.push(f.id.clone()),
                        Finding::PasskeyAvailable { .. } => out.passkey_available.push(f.id.clone()),
                    }
                }
            }
            out.reused = groups.into_values().collect();
            let with_pw = items.iter().filter(|(_, _, i)| i.password.as_deref().is_some_and(|p| !p.is_empty())).count();
            if with_pw > 0 {
                let bad: BTreeSet<&String> = out.weak.iter().chain(out.breached.iter()).chain(out.reused.iter().flatten()).collect();
                // Sites with documented TOTP surface as TotpAvailable instead of MissingTotp, but both mean
                // "2FA not set up", so both keep the same weight in the score.
                let missing_totp = out.missing_totp.len() + out.totp_available.len();
                let penalty = bad.len() as f64 / with_pw as f64 * 80.0 + out.old.len() as f64 / with_pw as f64 * 10.0
                    + missing_totp as f64 / items.len().max(1) as f64 * 10.0;
                out.score = (100.0 - penalty).round().clamp(0.0, 100.0) as u8;
            }
            Ok(out)
        })
    }
    pub fn import_file(&self, vault_id: &str, format: &str, path: &str, dry_run: bool) -> Result<ImportResult> {
        let fmt: import::ImportFormat = format.parse()?;
        let path = path.trim();
        let path = if let Some(rest) = path.strip_prefix("~/") { dirs::home_dir().map(|h| h.join(rest).display().to_string()).unwrap_or_else(|| path.to_string()) } else { path.to_string() };
        let path = path.as_str();
        let data = Zeroizing::new(std::fs::read(path).with_context(|| format!("cannot read {path}"))?);
        if fmt == import::ImportFormat::GitpassJson && import::is_encrypted_export(&String::from_utf8_lossy(&data)) { bail!("encrypted gitpass exports need a passphrase; import them with the CLI"); }
        let parsed = import::parse_bytes(fmt, &data, None)?;
        let mut categories = BTreeMap::new();
        for r in &parsed.items { *categories.entry(r.category.as_str().to_string()).or_insert(0) += 1; }
        let preview = parsed.items.iter().take(50).map(|r| ImportPreviewItem { title: r.item.title.clone(), category: r.category,
            username: r.item.username.clone(), url: r.item.url.clone(), has_password: r.item.password.is_some() || r.view_sections().iter().flat_map(|s| s.fields.iter()).any(|f| f.kind == FieldKind::Concealed),
            has_totp: has_totp(r), tags: r.tags.clone(), fields: r.view_sections().iter().map(|s| s.fields.len()).sum(), archived: r.archived }).collect();
        let f = move |u: &mut Unlocked| {
            let r = import::import_items(&u.session, vault_id, parsed, import::ImportOpts { dry_run, allow_duplicates: false })?;
            Ok(ImportResult { parsed: r.parsed, added: r.added, duplicates: r.duplicates.len(), duplicate_titles: r.duplicates, warnings: r.warnings, categories, preview })
        };
        let r = if dry_run { self.with(f) } else { self.write(f) }?;
        if !dry_run && r.added > 0 { let _ = self.update_local(|l| l.onboarding.imported = true); }
        Ok(r)
    }
    /// Export a whole vault or an explicit selection to a user-chosen path. `csv` is login fields only;
    /// encrypted exports are gitpass-json. The file is written atomically and, on Unix, mode 0600.
    /// The passphrase and item contents never go into errors or logs.
    pub fn export_items(&self, req: &ExportRequest) -> Result<ExportResult> {
        let format = req.format.as_str();
        if !matches!(format, "gitpass-json" | "csv") { bail!("unknown export format '{}'", req.format); }
        if format == "csv" && req.encrypted { bail!("encrypted export only supports gitpass-json"); }
        let raw = req.path.trim();
        if raw.is_empty() { bail!("export path is required"); }
        let path = if let Some(rest) = raw.strip_prefix("~/") { dirs::home_dir().map(|h| h.join(rest)).unwrap_or_else(|| PathBuf::from(raw)) } else { PathBuf::from(raw) };
        let path = std::path::absolute(&path)?;
        self.with(|u| {
            let recs: Vec<ItemRecord> = match (&req.items, &req.vault_id) {
                (Some(refs), _) if !refs.is_empty() => {
                    let mut seen = BTreeSet::new(); let mut out = vec![];
                    for r in refs {
                        if !seen.insert((r.vault_id.clone(), r.item_id.clone())) { continue; }
                        let (_, rec) = u.session.read_record(&r.vault_id, &r.item_id)?;
                        if rec.deleted_at.is_none() { out.push(rec); }
                    }
                    out
                }
                (Some(_), _) => bail!("no items selected"),
                (None, Some(vid)) => u.session.list_records(vid)?.into_iter().filter(|(_, _, r)| r.deleted_at.is_none()).map(|(_, _, r)| r).collect(),
                (None, None) => bail!("export needs a vault or a list of items"),
            };
            if recs.is_empty() { bail!("nothing to export"); }
            let body = if req.encrypted {
                let pass = Zeroizing::new(req.passphrase.clone().unwrap_or_default());
                Zeroizing::new(import::export_records_encrypted(&recs, &pass, &import::export_kdf_default())?)
            } else if format == "gitpass-json" {
                Zeroizing::new(import::export_records_json(&recs)?)
            } else {
                let items: Vec<Item> = recs.iter().map(|r| r.item.clone()).collect();
                Zeroizing::new(import::export_csv(&items)?)
            };
            gitpass_core::fsutil::atomic_write(&path, body.as_bytes())?;
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?; }
            Ok(ExportResult { path: path.display().to_string(), items: recs.len(), format: format.to_string(), encrypted: req.encrypted })
        })
    }

    // ---------------- sharing
    pub fn identity(&self) -> Result<Identity> {
        self.with(|u| { let (pk, fpr) = u.session.identity()?; Ok(Identity { public_key: B64.encode(pk), fingerprint: fpr }) })
    }
    /// Fingerprint of an arbitrary recipient public key (same encoding `share_vault` accepts), so the
    /// UI can show the exact value both parties compare out of band. Needs no unlocked session.
    pub fn fingerprint_of(&self, public_key: &str) -> Result<String> {
        Ok(gitpass_core::keys::fingerprint(&gitpass_core::keys::parse_pub(public_key.trim())?))
    }
    pub fn list_members(&self, vault_id: &str) -> Result<Vec<Member>> {
        let labels = self.read_local().member_labels;
        self.with(|u| Self::members_of(&u.session, vault_id, &labels))
    }
    pub fn share_vault(&self, vault_id: &str, public_key: &str, label: Option<String>) -> Result<Member> {
        let pk = gitpass_core::keys::parse_pub(public_key.trim())?;
        let fpr = self.write(|u| { u.session.share_vault(vault_id, &pk)?; Ok(gitpass_core::keys::fingerprint(&pk)) })?;
        let label = nonempty(label);
        if let Some(l) = &label { self.update_local(|f| { f.member_labels.insert(fpr.clone(), l.clone()); })?; }
        Ok(Member { fingerprint: fpr, label, is_self: false })
    }
    pub fn unshare_vault(&self, vault_id: &str, fingerprint: &str) -> Result<()> { self.write(|u| u.session.unshare_vault(vault_id, fingerprint).map(|_| ())) }
    pub fn accept_share(&self, path: &str) -> Result<VaultInfo> {
        let labels = self.read_local().member_labels;
        self.write(|u| { let vid = u.session.accept_share(Path::new(path))?; Self::vault_info(&u.session, &vid, &labels) })
    }

    // ---------------- devices
    pub fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        self.with(|u| {
            let cur = u.session.device_id.clone();
            Ok(u.session.devices()?.devices.into_iter().map(|d| DeviceInfo { current: d.id == cur, id: d.id, name: d.name, last_seen: d.last_seen_at, revoked: d.revoked }).collect())
        })
    }
    /// `rotateCredentials=false`: revocation marker only. `true`: real revocation (rotate every Vault Key
    /// and the Account Key, like `gitpass device revoke`). Master password / Secret Key are NOT changed
    /// here; use change_password / rotate_secret_key for that.
    pub fn revoke_device(&self, id: &str, rotate_credentials: bool) -> Result<()> {
        self.write(|u| {
            if rotate_credentials {
                let (pw, sk) = (u.password.clone(), SecretKey(u.secret.0));
                u.session.revoke_device_and_rotate(id, &pw, &sk).map(|_| ())
            } else { u.session.revoke_device(id).map(|_| ()) }
        })
    }

    // ---------------- settings / onboarding
    pub fn get_settings(&self) -> Settings { self.read_local().settings.unwrap_or_default() }
    pub fn set_settings(&self, s: Settings) -> Result<Settings> {
        let was = self.get_settings().biometric_unlock;
        if s.biometric_unlock && !was {
            if !self.biometric.supported() { bail!("biometric unlock is not supported yet on this device"); }
            let (root, pw, sk) = self.with(|u| Ok((u.session.root.display().to_string(), u.password.clone(), Zeroizing::new(u.secret.encode()))))
                .map_err(|_| anyhow!("unlock the vault before enabling biometric unlock"))?;
            self.biometric.store(&root, &pw, &sk)?;
        } else if !s.biometric_unlock && was {
            let root = self.status()?.vault_path;
            self.biometric.delete(&root)?;
        }
        if s.auto_lock_minutes > 24 * 60 { bail!("autoLockMinutes must be <= 1440"); }
        if s.clipboard_clear_secs > 3600 { bail!("clipboardClearSecs must be <= 3600"); }
        self.update_local(|l| l.settings = Some(s.clone()))?;
        self.state.set_auto_lock_minutes(s.auto_lock_minutes);
        Ok(s)
    }
    pub fn onboarding(&self) -> Onboarding { self.read_local().onboarding }
    pub fn set_onboarding(&self, p: OnboardingPatch) -> Result<Onboarding> {
        self.update_local(|l| {
            let o = &mut l.onboarding;
            if let Some(v) = p.extension_installed { o.extension_installed = v; }
            if let Some(v) = p.emergency_kit_saved { o.emergency_kit_saved = v; }
            if let Some(v) = p.repo_connected { o.repo_connected = v; }
            if let Some(v) = p.imported { o.imported = v; }
            if let Some(v) = p.dismissed { o.dismissed = v; }
            o.clone()
        })
    }

    // ---------------- credentials
    pub fn change_password(&self, current: &str, next: &str) -> Result<()> {
        if next.chars().count() < 8 { bail!("new master password must be at least 8 characters"); }
        let kdf = self.unlock_opts.lock().map_err(|_| anyhow!("poisoned"))?.target_kdf.clone();
        self.write(|u| {
            if !ct_eq(current.as_bytes(), u.password.as_bytes()) { bail!("current master password is incorrect"); }
            let sk = SecretKey(u.secret.0);
            let k = u.session.fmt.kdf.max_with(&kdf);
            u.session.change_password(next, &sk, k)?;
            u.password = Zeroizing::new(next.to_string());
            Ok(())
        })
    }
    fn kit_text(account: &str, vault: &str, secret: Option<&str>) -> String {
        let mut s = format!("gitpass Emergency Kit\n=====================\nAccount ID: {account}\nVault folder: {vault}\nCreated: {}\n\n",
            gitpass_core::device::now_iso8601());
        match secret {
            Some(k) => s.push_str(&format!("Secret Key: {k}\n\nStore this kit offline. Anyone with it AND your master password can open your vault.\n")),
            None => s.push_str("Secret Key: (not shown — find it on your saved Emergency Kit or rotate it in Settings to print a new one)\n"),
        }
        s.push_str("Master password: write it down by hand: ______________________\n");
        s
    }
    pub fn rotate_secret_key(&self) -> Result<RotateResult> {
        let kdf = self.unlock_opts.lock().map_err(|_| anyhow!("poisoned"))?.target_kdf.clone();
        let kit = self.write(|u| {
            let k = u.session.fmt.kdf.max_with(&kdf);
            let pw = u.password.clone();
            let sk = u.session.rotate_secret_key(&pw, k)?;
            let enc = Zeroizing::new(sk.encode());
            u.secret = sk;
            Ok((u.session.fmt.account_id.clone(), enc.clone(), Zeroizing::new(Self::kit_text(&u.session.fmt.account_id, &u.session.root.display().to_string(), Some(&enc)))))
        })?;
        let (acc, enc, kit) = kit;
        *self.fresh_secret.lock().map_err(|_| anyhow!("poisoned"))? = Some((acc, enc));
        *self.fresh_kit.lock().map_err(|_| anyhow!("poisoned"))? = Some(kit.clone());
        Ok(RotateResult { emergency_kit: kit.to_string() })
    }
    /// Structured kit. The Secret Key is included only (a) right after create_account / rotate_secret_key
    /// in this session, or (b) when `password` is given and matches the master password of the unlocked
    /// session (the key is then taken from the unlocked session, where it already lives in memory).
    pub fn emergency_kit_data(&self, password: Option<&str>) -> Result<EmergencyKitData> {
        let fresh = self.fresh_secret.lock().map_err(|_| anyhow!("poisoned"))?.clone();
        let (mut d, root) = self.with(|u| {
            let acc = u.session.fmt.account_id.clone();
            let mut secret = fresh.filter(|(a, _)| *a == acc).map(|(_, k)| k.to_string());
            if let Some(pw) = password {
                if !ct_eq(pw.as_bytes(), u.password.as_bytes()) { bail!("wrong master password"); }
                secret = Some(u.secret.encode().to_string());
            }
            let device_name = u.session.devices().ok().and_then(|r| r.devices.into_iter().find(|d| d.id == u.session.device_id).map(|d| d.name))
                .or_else(|| u.device_name.clone());
            Ok((EmergencyKitData { account_id: acc, vault_path: u.session.root.display().to_string(), remote: None, device_name,
                created_at: gitpass_core::device::now_iso8601(), secret_key_available: true, secret_key: secret }, u.session.root.clone()))
        })?;
        d.remote = git(&root, &["remote", "get-url", "origin"]).filter(|s| !s.is_empty());
        Ok(d)
    }
    /// Kit text; contains the Secret Key only right after rotate_secret_key (until lock).
    pub fn emergency_kit(&self) -> Result<String> {
        if let Some(k) = self.fresh_kit.lock().map_err(|_| anyhow!("poisoned"))?.as_ref() { return Ok(k.to_string()); }
        self.with(|u| Ok(Self::kit_text(&u.session.fmt.account_id, &u.session.root.display().to_string(), None)))
    }

    // ---------------- sync
    pub fn sync_status(&self) -> Result<SyncStatus> {
        let root = self.with(|u| Ok(u.session.root.clone()))?;
        let last_sync = self.read_local().last_sync;
        if git(&root, &["rev-parse", "--is-inside-work-tree"]).is_none() {
            return Ok(SyncStatus { remote: None, last_sync, ahead: 0, behind: 0, dirty: false });
        }
        let remote = git(&root, &["remote", "get-url", "origin"]).filter(|s| !s.is_empty());
        let (mut ahead, mut behind) = (0, 0);
        if let Some(c) = git(&root, &["rev-list", "--left-right", "--count", "HEAD...@{u}"]) {
            let mut it = c.split_whitespace().filter_map(|x| x.parse::<u64>().ok());
            ahead = it.next().unwrap_or(0); behind = it.next().unwrap_or(0);
        }
        let dirty = git(&root, &["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
        Ok(SyncStatus { remote, last_sync, ahead, behind, dirty })
    }
    /// Settings → Sync → Connect: `gitpass remote add <url>` for the unlocked vault. Uses the user's existing git
    /// credentials only (no tokens are asked for or stored). Errors start with a `remote::E_*` category marker.
    pub fn remote_connect(&self, url: &str) -> Result<SyncStatus> { self.remote_connect_opts(url, false) }
    #[doc(hidden)]
    pub fn remote_connect_opts(&self, url: &str, allow_local: bool) -> Result<SyncStatus> {
        let root = self.with(|u| Ok(u.session.root.clone()))?;
        gitpass_core::remote::add(&root, url, allow_local)?;
        let _ = self.update_local(|l| l.last_sync = Some(gitpass_core::device::now_iso8601()));
        let _ = self.set_onboarding(OnboardingPatch { repo_connected: Some(true), ..Default::default() });
        self.sync_status()
    }
    /// Remove `origin` (local files and history are kept; nothing is deleted on the remote).
    pub fn remote_disconnect(&self) -> Result<SyncStatus> {
        let root = self.with(|u| Ok(u.session.root.clone()))?;
        gitpass_core::remote::remove(&root)?;
        let _ = self.set_onboarding(OnboardingPatch { repo_connected: Some(false), ..Default::default() });
        self.sync_status()
    }
    pub fn sync(&self, merge: bool) -> Result<SyncResult> {
        let d = self.state.sync(merge)?;
        let _ = self.update_local(|l| l.last_sync = Some(gitpass_core::device::now_iso8601()));
        let mut m = d.log;
        for x in d.merged { m.push_str(&format!("merged {x}\n")); }
        Ok(SyncResult { message: m })
    }

    /// Favicons are opt-in (Rust-only `allowFavicons` key, default off). MVP: always `None` (letter
    /// avatar); no network request is ever made.
    pub fn favicon_url(&self, _url: &str) -> Option<String> { let _ = self.read_local().allow_favicons; None }
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ---------------- SSH keys (v0.7)
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyCreated { pub item: ItemSummary, pub public_key: String, pub fingerprint: String, pub key_type: String }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SshAgentInfo { pub socket_path: String, pub config_line: String, pub running: bool, pub approve_each_use: bool }

impl App {
    fn save_ssh_key(&self, vault_id: &str, title: &str, k: &gitpass_core::ssh::SshKey) -> Result<SshKeyCreated> {
        let title = if title.trim().is_empty() { if k.info.comment.is_empty() { "SSH Key".to_string() } else { k.info.comment.clone() } } else { title.trim().to_string() };
        let rec = k.to_record(&title)?;
        let item = self.write(|u| {
            let iid = u.session.add_record(vault_id, rec)?;
            let (_, r) = u.session.read_record(vault_id, &iid)?;
            Ok(summary(vault_id, &iid, &r))
        })?;
        Ok(SshKeyCreated { item, public_key: k.info.public_key.clone(), fingerprint: k.info.fingerprint.clone(), key_type: k.info.key_type.clone() })
    }
    /// Generate an Ed25519 key into a new `ssh_key` item.
    pub fn ssh_generate(&self, vault_id: &str, title: &str, comment: &str) -> Result<SshKeyCreated> {
        let k = gitpass_core::ssh::SshKey::generate_ed25519(comment)?;
        self.save_ssh_key(vault_id, title, &k)
    }
    /// Import an OpenSSH private key (decrypted with `passphrase` if needed) into a new `ssh_key` item.
    pub fn ssh_import(&self, vault_id: &str, title: &str, pem: &str, passphrase: Option<&str>) -> Result<SshKeyCreated> {
        let k = gitpass_core::ssh::SshKey::import_openssh(pem, passphrase)?;
        self.save_ssh_key(vault_id, title, &k)
    }
    pub fn ssh_agent_info(&self, socket: &std::path::Path, running: bool) -> SshAgentInfo {
        SshAgentInfo { socket_path: socket.display().to_string(), config_line: gitpass_core::ssh::identity_agent_line(socket), running, approve_each_use: self.get_settings().ssh_approve_each_use }
    }
}
