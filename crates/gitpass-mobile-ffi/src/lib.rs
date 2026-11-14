//! UniFFI surface shared by the gitpass iOS and Android companion apps.
//!
//! Every secret operation delegates to `gitpass-core`: Argon2id key
//! derivation (from the vault's stored KDF parameters), the
//! XChaCha20-Poly1305 envelopes, the v5 item records, field-level merge
//! and RFC 6238 TOTP. No crypto is re-implemented in Swift or Kotlin,
//! and no crypto is re-implemented here either — this crate is a thin
//! wrapper around the same `Session` API the desktop app (via gui-core)
//! and the CLI use.
//!
//! Unlock semantics: a plain mobile unlock (`open_vault`, or
//! `open_vault_with(default_unlock_options())`) is side-effect free. It
//! does not register/refresh the device in `devices.enc`, does not purge
//! expired trash and does not upgrade the KDF, so unlocking never changes
//! a synced vault file. The unlock throttle and the integrity/rollback
//! checks still run. Each maintenance write can be opted into with
//! `UnlockOptions`. The unlocked session is held in memory for the
//! lifetime of the `Vault` object; locking is dropping it.
//!
//! Version handshake: `ffi_version()` and `format_version()` let the apps
//! check at startup that the native library they load is the one their
//! generated bindings and app code were written against.

use gitpass_core::item::{Category, Field, FieldKind, Section};
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{Item, Session, UnlockEffects, UnlockOpts};
use gitpass_core::{crypto, totp, FORMAT_VERSION, MIN_READ_FORMAT};
use std::path::Path;
use std::sync::Mutex;

mod first_pull;
mod pairing;
mod sync;
pub use first_pull::validate_first_pull_snapshot;
pub use pairing::{
    pairing_host_begin, pairing_host_cancel, pairing_host_confirm, pairing_host_push, pairing_joiner_cancel,
    pairing_joiner_code, pairing_joiner_endpoint, pairing_joiner_finish, pairing_joiner_phase, pairing_joiner_push,
    pairing_joiner_start, pairing_test_vector, pairing_test_vector_open, PairedSync, PairingDrive, PairingEndpoint,
    PairingHostInput, PairingHostView,
};
pub use sync::{
    GitHubSyncTarget, HttpHeader, HttpTransport, HttpTransportError, HttpTransportRequest, HttpTransportResponse,
    SyncCancellation, GitHubSyncReport, pull_github_snapshot,
};

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

/// Card brand by number prefix — same rules as the desktop app's
/// `detectBrand` (apps/desktop/src/features/new-item/card.ts).
fn card_brand(digits: &str) -> &'static str {
    let p = |n: usize| digits.get(..n).and_then(|s| s.parse::<u32>().ok());
    let in_range = |n: usize, lo: u32, hi: u32| p(n).is_some_and(|v| (lo..=hi).contains(&v));
    if digits.starts_with('4') {
        "Visa"
    } else if in_range(2, 34, 34) || in_range(2, 37, 37) {
        "American Express"
    } else if in_range(2, 51, 55) || in_range(4, 2221, 2720) {
        "Mastercard"
    } else if in_range(4, 6011, 6011) || in_range(2, 65, 65) || in_range(3, 644, 649) {
        "Discover"
    } else if in_range(3, 300, 305) || in_range(2, 36, 36) || in_range(2, 38, 39) {
        "Diners Club"
    } else if in_range(4, 3528, 3589) {
        "JCB"
    } else if in_range(2, 62, 62) {
        "UnionPay"
    } else {
        ""
    }
}

/// "Visa •••• 7575" — same as the desktop `cardLabel`: empty for fewer
/// than 8 digits, brand omitted when unknown. Never the full number.
fn card_label(number: &str) -> String {
    let d: String = number.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() < 8 {
        return String::new();
    }
    let brand = card_brand(&d);
    let last4 = &d[d.len() - 4..];
    if brand.is_empty() { format!("•••• {last4}") } else { format!("{brand} •••• {last4}") }
}

fn subtitle(rec: &ItemRecord) -> String {
    if rec.category == Category::CreditCard {
        // Like desktop: brand + last 4 of the number, not the cardholder.
        return rec
            .view_sections()
            .into_iter()
            .flat_map(|s| s.fields)
            .find(|f| f.id == "number" || f.label == "number")
            .and_then(|f| f.value)
            .map(|v| card_label(&v))
            .unwrap_or_default();
    }
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

/// What an Android autofill request is for (#475). Exactly one identity is
/// used: `web_origin` (`https://host`, from a trusted browser's web domain)
/// when set, otherwise `android_package` (the requesting app, reported by
/// the OS). With neither, nothing matches.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct AutofillQuery {
    pub web_origin: Option<String>,
    pub android_package: Option<String>,
}

/// One login that matches an [`AutofillQuery`]. Only returned from an
/// unlocked vault, and only for items whose URL matches the request.
#[derive(Clone, uniffi::Record)]
pub struct AutofillLogin {
    pub vault_id: String,
    pub item_id: String,
    pub title: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl std::fmt::Debug for AutofillLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutofillLogin").field("vault_id", &self.vault_id).field("item_id", &self.item_id).finish_non_exhaustive()
    }
}

/// The shared eTLD+1 web rule (`gitpass_core::origin::origin_matches`).
#[uniffi::export]
pub fn autofill_origin_matches(item_url: String, web_origin: String) -> bool {
    gitpass_core::origin::origin_matches(&item_url, &web_origin)
}

/// Exact Android package rule (`gitpass_core::origin::app_matches`).
#[uniffi::export]
pub fn autofill_app_matches(item_url: String, package: String) -> bool {
    gitpass_core::origin::app_matches(&item_url, &package)
}

fn record_urls(rec: &ItemRecord) -> impl Iterator<Item = &str> {
    rec.item.url.as_deref().into_iter().chain(
        rec.sections.iter().flat_map(|s| &s.fields).filter(|f| f.kind == FieldKind::Url && !f.removed).filter_map(|f| f.value.as_deref()),
    )
}

/// True when `rec` is a live login whose URL (main or a URL field) matches `q`.
pub fn autofill_record_matches(rec: &ItemRecord, q: &AutofillQuery) -> bool {
    if rec.category != Category::Login || rec.archived || rec.deleted_at.is_some() {
        return false;
    }
    if rec.item.password.as_deref().unwrap_or("").is_empty() && rec.item.username.as_deref().unwrap_or("").is_empty() {
        return false;
    }
    match (&q.web_origin, &q.android_package) {
        (Some(origin), _) => record_urls(rec).any(|u| gitpass_core::origin::origin_matches(u, origin)),
        (None, Some(pkg)) => record_urls(rec).any(|u| gitpass_core::origin::app_matches(u, pkg)),
        (None, None) => false,
    }
}

/// Who is asking for a passkey (#477). Built by the app from Android's `CallingAppInfo`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum PasskeyCaller {
    /// A browser on the privileged allowlist; `origin` is the web origin the browser vouches for
    /// (`CallingAppInfo.getOrigin(allowlist)`), never one taken from the request.
    Browser { origin: String },
    /// Any other app: its package, the SHA-256 of its signing certificate (from the OS), and the
    /// rpId's `/.well-known/assetlinks.json` as fetched by the app (may be empty when unavailable).
    App { package: String, cert_sha256: Vec<u8>, asset_links_json: String },
}

fn verified_rp(caller: &PasskeyCaller, rp_id: Option<&str>) -> anyhow::Result<gitpass_core::webauthn::VerifiedRp> {
    match caller {
        PasskeyCaller::Browser { origin } => gitpass_core::webauthn::verify_web(origin, rp_id),
        PasskeyCaller::App { package, cert_sha256, asset_links_json } => gitpass_core::webauthn::verify_app(package, cert_sha256, rp_id, asset_links_json),
    }
}

/// Non-secret summary of a WebAuthn request JSON, for the provider's first ("begin") step.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PasskeyRequestInfo {
    pub rp_id: Option<String>,
    pub rp_name: Option<String>,
    pub user_name: Option<String>,
}

/// Parse a WebAuthn creation (`create = true`) or request options JSON.
#[uniffi::export]
pub fn passkey_request_info(request_json: String, create: bool) -> Result<PasskeyRequestInfo, GitpassError> {
    use gitpass_core::webauthn as wa;
    if create {
        let o = wa::parse_create_options(&request_json)?;
        Ok(PasskeyRequestInfo { rp_id: o.rp_id, rp_name: o.rp_name, user_name: Some(o.user_name).filter(|u| !u.is_empty()) })
    } else {
        let o = wa::parse_get_options(&request_json)?;
        Ok(PasskeyRequestInfo { rp_id: o.rp_id, rp_name: None, user_name: None })
    }
}

/// The host whose Digital Asset Links file an app caller must be listed in, or an error when the
/// rpId can never be used by an app. Call before fetching anything.
#[uniffi::export]
pub fn passkey_app_rp_host(rp_id: String) -> Result<String, GitpassError> {
    Ok(gitpass_core::webauthn::app_rp_host(&rp_id)?)
}

/// The rpId if `caller` may use it, else an error. Never touches a vault.
#[uniffi::export]
pub fn passkey_verify_caller(caller: PasskeyCaller, rp_id: Option<String>) -> Result<String, GitpassError> {
    Ok(verified_rp(&caller, rp_id.as_deref())?.rp_id().to_string())
}

/// A stored passkey offered for sign-in; no secret material.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PasskeyEntry {
    /// base64url credential id.
    pub credential_id: String,
    pub user_name: String,
    pub vault_name: String,
    pub last_used: u64,
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

/// API level of this bridge. Bump it whenever an exported function,
/// record or behaviour changes in a way the apps must know about; the
/// apps compare it with the level they were written against at startup.
/// Level 2: `Vault::sync_github` + `HttpTransport` (#455).
/// Level 3: quick-unlock key (`derive_quick_unlock_key`,
/// `open_vault_with_quick_unlock_key`, `QUICK_UNLOCK_STALE`) (#473).
/// Level 4: reserved for QR pairing, then left unused when autofill landed first (#475).
/// Level 5: autofill (`Vault::autofill_logins`, `AutofillQuery`, `AutofillLogin`,
/// `autofill_origin_matches`, `autofill_app_matches`) (#475).
/// Level 6: reserved for QR pairing, then left unused when the passkey provider landed first (#477).
/// Level 7: passkey provider (`PasskeyCaller`, `passkey_request_info`,
/// `passkey_app_rp_host`, `passkey_verify_caller`, `Vault::passkey_entries`,
/// `Vault::passkey_register`, `Vault::passkey_assert`) on `gitpass_core::webauthn` (#477).
/// Level 8: QR pairing v1 (#457). Replaced before any release by level 9.
/// Level 9: pairing v2 (#491). The phone scans a secret-free QR, the code is
/// commitment-bound, and the desktop does not receive its own code.
pub const FFI_API_LEVEL: u32 = 9;

/// Bridge version: the crate (= workspace) version plus the API level.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FfiVersion {
    /// Semver of this bridge crate; always the workspace version.
    pub crate_version: String,
    /// See [`FFI_API_LEVEL`].
    pub api_level: u32,
}

/// Vault format range of the linked `gitpass-core`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FormatVersion {
    /// Oldest on-disk format this build can still open (`MIN_READ_FORMAT`).
    pub min_read: u32,
    /// Format this build writes (`FORMAT_VERSION`).
    pub current_write: u32,
}

#[uniffi::export]
pub fn ffi_version() -> FfiVersion {
    FfiVersion { crate_version: env!("CARGO_PKG_VERSION").to_string(), api_level: FFI_API_LEVEL }
}

#[uniffi::export]
pub fn format_version() -> FormatVersion {
    FormatVersion { min_read: MIN_READ_FORMAT, current_write: FORMAT_VERSION }
}

/// Optional write side effects of an unlock. All `false` (see
/// `default_unlock_options`) is the mobile default: unlocking leaves every
/// tracked vault file byte-identical. Throttling and integrity/rollback
/// checks always run, whatever is set here.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct UnlockOptions {
    /// Register/refresh this device in `devices.enc` (and roll forward an
    /// interrupted device revoke started elsewhere).
    pub register_device: bool,
    /// Device name used when `register_device` is set; `None` = core default.
    pub device_name: Option<String>,
    /// Permanently purge items that have been in the trash for over 30 days.
    pub purge_expired_trash: bool,
    /// Re-wrap the keyset with the current default KDF parameters when the
    /// stored ones are weaker. Never lowers or clamps KDF parameters.
    pub upgrade_kdf: bool,
}

/// The mobile default: no device registration, no trash purge, no KDF upgrade.
#[uniffi::export]
pub fn default_unlock_options() -> UnlockOptions {
    UnlockOptions::default()
}

/// Side-effect-free unlock; same as `open_vault_with(.., default_unlock_options())`.
#[uniffi::export]
pub fn open_vault(root: String, password: String, secret_key: String) -> Result<std::sync::Arc<Vault>, GitpassError> {
    open_vault_with(root, password, secret_key, UnlockOptions::default())
}

/// Unlock with explicit side-effect options (see [`UnlockOptions`]).
#[uniffi::export]
pub fn open_vault_with(
    root: String,
    password: String,
    secret_key: String,
    options: UnlockOptions,
) -> Result<std::sync::Arc<Vault>, GitpassError> {
    let secret = err(crypto::SecretKey::parse(&secret_key))?;
    let effects = UnlockEffects {
        touch_device: options.register_device,
        purge_expired: options.purge_expired_trash,
        upgrade_kdf: options.upgrade_kdf,
    };
    // Default target KDF = gitpass-core's default; only consulted when
    // `upgrade_kdf` is set. Rollback is never auto-accepted here.
    let opts = UnlockOpts::default();
    let session = err(Session::unlock_with_effects(
        Path::new(&root),
        &password,
        &secret,
        &opts,
        options.device_name.as_deref(),
        effects,
    ))?;
    Ok(std::sync::Arc::new(Vault { session: Mutex::new(session) }))
}

/// Error text of `open_vault_with_quick_unlock_key` when the stored key no
/// longer opens the vault (password, Secret Key or KDF changed, or the key is
/// wrong). Apps compare against this to drop the saved key and fall back to
/// the Master Password.
#[uniffi::export]
pub fn quick_unlock_stale_message() -> String {
    gitpass_core::store::STALE_UNLOCK_KEY.to_string()
}

/// Derive the 32-byte Account Unlock Key for quick (biometric) unlock.
///
/// Runs the vault's full Argon2id KDF (stored parameters; never lowered) and
/// verifies the result by opening the keyset, so a wrong password or Secret
/// Key fails here and counts towards the unlock throttle. The key opens only
/// this account's keyset and stops working when the password, Secret Key or
/// KDF parameters change (the keyset is re-wrapped). Callers must keep it
/// only inside hardware-backed, user-authentication-bound storage and wipe
/// their copy after wrapping it.
#[uniffi::export]
pub fn derive_quick_unlock_key(root: String, password: String, secret_key: String) -> Result<Vec<u8>, GitpassError> {
    let secret = err(crypto::SecretKey::parse(&secret_key))?;
    let key = err(Session::derive_unlock_key(Path::new(&root), &password, &secret))?;
    Ok(key.0.to_vec())
}

/// Side-effect-free unlock with a key from [`derive_quick_unlock_key`]. Skips
/// the KDF; integrity and rollback checks still run. Never registers the
/// device or upgrades the KDF (those need the Master Password); only
/// `purge_expired_trash` from `options` is honoured. The input buffer is
/// wiped before returning.
#[uniffi::export]
pub fn open_vault_with_quick_unlock_key(
    root: String,
    key: Vec<u8>,
    options: UnlockOptions,
) -> Result<std::sync::Arc<Vault>, GitpassError> {
    use zeroize::Zeroize;
    let mut key = key;
    let parsed: Option<[u8; 32]> = key.as_slice().try_into().ok();
    key.zeroize();
    let Some(bytes) = parsed else {
        return Err(GitpassError::Generic(gitpass_core::store::STALE_UNLOCK_KEY.to_string()));
    };
    let key = crypto::Key32(bytes);
    if options.register_device || options.upgrade_kdf {
        return Err(GitpassError::Generic("quick unlock cannot register the device or upgrade the KDF".into()));
    }
    let effects = UnlockEffects { touch_device: false, purge_expired: options.purge_expired_trash, upgrade_kdf: false };
    let session = err(Session::unlock_with_unlock_key(Path::new(&root), &key, &UnlockOpts::default(), effects))?;
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

/// Is `path` (repo-relative, `/`-separated) allowed to be committed or
/// accepted from a remote tree? This is the single allowed-path rule set,
/// owned by core (`gitpass_core::sync_engine::is_allowed_path`); mobile
/// must call this instead of keeping its own copy, so a remote tree can
/// never smuggle a path past the rule core enforces. (Issue #452; shared
/// test table: `crates/gitpass-core/tests/fixtures/shared/allowed_paths.json`.
/// The Swift `isSyncable` / Kotlin `allowedFile` call-site migration belongs
/// with the transport work, #453.)
#[uniffi::export]
pub fn is_allowed_path(path: String) -> bool {
    gitpass_core::sync_engine::is_allowed_path(&path)
}

#[uniffi::export]
impl Vault {
    /// Logins matching an autofill request (#475), across all vaults, sorted
    /// by title. Never returns an item whose URL does not match.
    pub fn autofill_logins(&self, query: AutofillQuery) -> Result<Vec<AutofillLogin>, GitpassError> {
        self.with_session(|s| {
            let mut out = Vec::new();
            for (vid, _meta) in s.list_vaults()? {
                for (iid, _v, rec) in s.list_records(&vid)? {
                    if autofill_record_matches(&rec, &query) {
                        out.push(AutofillLogin {
                            vault_id: vid.clone(),
                            item_id: iid,
                            title: rec.item.title.clone(),
                            username: rec.item.username.clone(),
                            password: rec.item.password.clone(),
                        });
                    }
                }
            }
            out.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
            Ok(out)
        })
    }

    /// Passkeys `caller` may use for the request (`PublicKeyCredentialRequestOptionsJSON`), most
    /// recently used first. Fails when the caller is not valid for the request's rpId (#477).
    pub fn passkey_entries(&self, caller: PasskeyCaller, request_json: String) -> Result<Vec<PasskeyEntry>, GitpassError> {
        use gitpass_core::webauthn as wa;
        let o = wa::parse_get_options(&request_json)?;
        let rp = verified_rp(&caller, o.rp_id.as_deref())?;
        self.with_session(|s| Ok(wa::candidates(s, &rp, &o.allow)?.into_iter().map(|c| PasskeyEntry {
            credential_id: gitpass_core::passkey::b64(&c.credential_id), user_name: c.user_name, vault_name: c.vault_name, last_used: c.last_used,
        }).collect()))
    }

    /// Create a passkey (`PublicKeyCredentialCreationOptionsJSON`) in `vault_id` (default: the
    /// first vault). Returns `RegistrationResponseJSON`. The app must have verified the user.
    pub fn passkey_register(&self, caller: PasskeyCaller, request_json: String, vault_id: Option<String>) -> Result<String, GitpassError> {
        use gitpass_core::webauthn as wa;
        let o = wa::parse_create_options(&request_json)?;
        let rp = verified_rp(&caller, o.rp_id.as_deref())?;
        self.with_session(|s| Ok(wa::create(s, &rp, &o, vault_id.as_deref())?.to_webauthn_json().to_string()))
    }

    /// Sign in with passkey `credential_id` (base64url). `client_data_hash` is the SHA-256 of
    /// clientDataJSON when a privileged browser supplies it. Returns `AuthenticationResponseJSON`.
    /// The app must have verified the user.
    pub fn passkey_assert(&self, caller: PasskeyCaller, request_json: String, credential_id: String, client_data_hash: Option<Vec<u8>>) -> Result<String, GitpassError> {
        use gitpass_core::webauthn as wa;
        let o = wa::parse_get_options(&request_json)?;
        let rp = verified_rp(&caller, o.rp_id.as_deref())?;
        let cred = gitpass_core::passkey::unb64(&credential_id)?;
        self.with_session(|s| Ok(wa::get(s, &rp, &o, Some(&cred), client_data_hash.as_deref())?.to_webauthn_json().to_string()))
    }

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

#[cfg(test)]
mod card_tests {
    use super::{card_brand, card_label};

    #[test]
    fn brands_match_desktop_rules() {
        assert_eq!(card_brand("4111111111111111"), "Visa");
        assert_eq!(card_brand("371449635398431"), "American Express");
        assert_eq!(card_brand("5555555555554444"), "Mastercard");
        assert_eq!(card_brand("2221000000000009"), "Mastercard");
        assert_eq!(card_brand("6011111111111117"), "Discover");
        assert_eq!(card_brand("30569309025904"), "Diners Club");
        assert_eq!(card_brand("3530111333300000"), "JCB");
        assert_eq!(card_brand("6200000000000005"), "UnionPay");
        assert_eq!(card_brand("9999999999999999"), "");
    }

    #[test]
    fn label_is_brand_and_last_four() {
        assert_eq!(card_label("4242 4242 4242 7575"), "Visa •••• 7575");
        assert_eq!(card_label("9999-9999-9999-1234"), "•••• 1234");
        assert_eq!(card_label("1234"), "");
    }
}
