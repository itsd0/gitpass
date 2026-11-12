//! WebAuthn authenticator logic shared by every gitpass passkey front end (#477): the extension
//! host and desktop agent (`gitpass-host::passkey`, JSON contract v0.7) and the Android
//! Credential Manager provider (through `gitpass-mobile-ffi`). Callers MUST get user approval or
//! verification before calling [`create`] / [`get`]; this module enforces origin / rpId policy,
//! builds clientDataJSON and does the crypto.
//!
//! A relying party is only usable as a [`VerifiedRp`], which exists only after a check:
//! - [`verify_web`]: a browser-verified `https://` origin (http on loopback only) and an rpId that
//!   is the origin's host or a registrable parent of it (WebAuthn §5.1.3 step 8 / §5.1.4 step 7).
//! - [`verify_app`]: an Android app identified by package + signing-certificate SHA-256, accepted
//!   only when the rpId's Digital Asset Links file grants that app
//!   `delegate_permission/common.get_login_creds`. The clientDataJSON origin is then
//!   `android:apk-key-hash:<base64url(cert sha256)>`, as Android Credential Manager specifies.
use crate::item::Category;
use crate::merge::ItemRecord;
use crate::origin::{is_package_name, parse_request_origin, registrable_domain};
use crate::passkey::{b64, unb64, Passkey, F_RP_ID};
use crate::store::Session;
use anyhow::Result;
use serde_json::{json, Value};
use url::Host;

/// What went wrong, in terms every front end can map to its own error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind { OriginMismatch, BadRequest, NotSupported, Exists, NotFound }

#[derive(Debug)]
pub struct WebauthnError { pub kind: ErrorKind, pub message: String }
impl std::fmt::Display for WebauthnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.message) }
}
impl std::error::Error for WebauthnError {}

fn fail(kind: ErrorKind, message: impl Into<String>) -> anyhow::Error { WebauthnError { kind, message: message.into() }.into() }
/// The [`ErrorKind`] of an error from this module, if it is one.
pub fn error_kind(e: &anyhow::Error) -> Option<ErrorKind> { e.downcast_ref::<WebauthnError>().map(|w| w.kind) }

/// Largest Digital Asset Links document accepted (bytes).
pub const MAX_ASSET_LINKS: usize = 512 * 1024;
/// The DAL relation that lets an app use a site's passkeys and passwords.
pub const GET_LOGIN_CREDS: &str = "delegate_permission/common.get_login_creds";
pub const APK_KEY_HASH_PREFIX: &str = "android:apk-key-hash:";

/// A relying party whose rpId was checked against the caller. Only this module can build one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedRp { rp_id: String, origin: String }
impl VerifiedRp {
    pub fn rp_id(&self) -> &str { &self.rp_id }
    /// The origin written into clientDataJSON.
    pub fn origin(&self) -> &str { &self.origin }
}

fn norm_rp(rp: &str) -> String { rp.trim().trim_end_matches('.').to_ascii_lowercase() }

/// Web caller: validate `origin` and resolve the rpId.
pub fn verify_web(origin: &str, rp_id: Option<&str>) -> Result<VerifiedRp> {
    let u = parse_request_origin(origin).ok_or_else(|| fail(ErrorKind::OriginMismatch, "origin must be https (or http on localhost)"))?;
    let host = u.host_str().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase();
    let rp = rp_id.map(norm_rp).unwrap_or_else(|| host.clone());
    if rp.is_empty() { return Err(fail(ErrorKind::BadRequest, "empty rpId")); }
    let ok = rp == host || (host.ends_with(&format!(".{rp}")) && registrable_domain(&rp).is_some());
    if !ok { return Err(fail(ErrorKind::OriginMismatch, format!("rpId '{rp}' is not valid for origin {origin}"))); }
    Ok(VerifiedRp { rp_id: rp, origin: origin.trim_end_matches('/').to_string() })
}

/// The rpId (a domain name) whose `/.well-known/assetlinks.json` must be fetched for an app caller,
/// or an error when it can never be valid (IP address, public suffix, port, path, ...).
pub fn app_rp_host(rp_id: &str) -> Result<String> {
    let rp = norm_rp(rp_id);
    let bad = || fail(ErrorKind::OriginMismatch, format!("rpId '{rp}' is not a domain an app can use"));
    let u = url::Url::parse(&format!("https://{rp}/")).map_err(|_| bad())?;
    match u.host() {
        Some(Host::Domain(d)) if d == rp && u.port().is_none() && registrable_domain(&rp).is_some() => Ok(rp),
        _ => Err(bad()),
    }
}

fn hex_upper(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02X}")).collect() }

/// True when `asset_links_json` (the rpId's Digital Asset Links document) grants `package`, signed
/// with the certificate whose SHA-256 is `cert_sha256`, the `get_login_creds` relation.
/// `include` statements are not followed.
pub fn asset_links_allow(asset_links_json: &str, package: &str, cert_sha256: &[u8]) -> bool {
    if asset_links_json.len() > MAX_ASSET_LINKS || cert_sha256.len() != 32 { return false; }
    let Ok(Value::Array(statements)) = serde_json::from_str::<Value>(asset_links_json) else { return false };
    let want = hex_upper(cert_sha256);
    statements.iter().any(|st| {
        let rel = st.get("relation").and_then(Value::as_array).is_some_and(|r| r.iter().any(|x| x.as_str() == Some(GET_LOGIN_CREDS)));
        let t = st.get("target");
        let ns = t.and_then(|t| t.get("namespace")).and_then(Value::as_str) == Some("android_app");
        let pkg = t.and_then(|t| t.get("package_name")).and_then(Value::as_str) == Some(package);
        let fp = t.and_then(|t| t.get("sha256_cert_fingerprints")).and_then(Value::as_array).is_some_and(|a| {
            a.iter().filter_map(Value::as_str).any(|f| f.replace(':', "").to_ascii_uppercase() == want)
        });
        rel && ns && pkg && fp
    })
}

/// The clientDataJSON origin for an Android app signed with `cert_sha256`.
pub fn apk_key_hash_origin(cert_sha256: &[u8]) -> String { format!("{APK_KEY_HASH_PREFIX}{}", b64(cert_sha256)) }

/// App caller: `rp_id` is required and must be vouched for by its Digital Asset Links document.
pub fn verify_app(package: &str, cert_sha256: &[u8], rp_id: Option<&str>, asset_links_json: &str) -> Result<VerifiedRp> {
    let rp = app_rp_host(rp_id.ok_or_else(|| fail(ErrorKind::BadRequest, "an app request must name its rpId"))?)?;
    if !is_package_name(package) { return Err(fail(ErrorKind::OriginMismatch, "invalid calling package")); }
    if cert_sha256.len() != 32 { return Err(fail(ErrorKind::OriginMismatch, "invalid signing certificate digest")); }
    if !asset_links_allow(asset_links_json, package, cert_sha256) {
        return Err(fail(ErrorKind::OriginMismatch, format!("{rp} does not list {package} for sign-in (Digital Asset Links)")));
    }
    Ok(VerifiedRp { rp_id: rp, origin: apk_key_hash_origin(cert_sha256) })
}

/// True when a passkey stored for `rp_id` is usable on web `origin` (same rules as sign-in).
pub fn rp_matches_origin(rp_id: &str, origin: &str) -> bool { !rp_id.trim().is_empty() && verify_web(origin, Some(rp_id)).is_ok() }

/// A live passkey item whose stored rpId is valid for `origin` (popup "this site" list).
pub fn record_matches_origin(r: &ItemRecord, origin: &str) -> bool {
    r.category == Category::Passkey && r.deleted_at.is_none() && !r.archived
        && r.find_field(F_RP_ID).and_then(|f| f.value).is_some_and(|rp| rp_matches_origin(&rp, origin))
}

/// Every live passkey for `rp_id`: (vault id, item id, passkey).
pub fn find(s: &Session, rp_id: &str) -> Result<Vec<(String, String, Passkey)>> {
    let mut out = vec![];
    for (vid, _) in s.list_vaults()? {
        for (iid, _, r) in s.list_records(&vid)? {
            if r.category != Category::Passkey || r.deleted_at.is_some() || r.archived { continue; }
            if let Ok(p) = Passkey::from_record(&r) { if p.rp_id == rp_id { out.push((vid.clone(), iid, p)); } }
        }
    }
    Ok(out)
}

/// A passkey offered to the user; no secret material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate { pub vault_id: String, pub item_id: String, pub vault_name: String, pub credential_id: Vec<u8>, pub user_name: String, pub user_handle: Vec<u8>, pub last_used: u64 }

/// Passkeys for `rp` (restricted to `allow` when non-empty), most recently used first.
pub fn candidates(s: &Session, rp: &VerifiedRp, allow: &[Vec<u8>]) -> Result<Vec<Candidate>> {
    let names: std::collections::HashMap<_, _> = s.list_vaults()?.into_iter().map(|(id, m)| (id, m.name.clone())).collect();
    let mut c: Vec<Candidate> = find(s, &rp.rp_id)?.into_iter()
        .filter(|(_, _, p)| allow.is_empty() || allow.contains(&p.credential_id))
        .map(|(vid, iid, p)| Candidate {
            last_used: s.last_used(&iid).unwrap_or(0), vault_name: names.get(&vid).cloned().unwrap_or_default(),
            vault_id: vid, item_id: iid, credential_id: p.credential_id.clone(), user_name: p.user_name.clone(), user_handle: p.user_handle.clone(),
        }).collect();
    c.sort_by(|a, b| b.last_used.cmp(&a.last_used).then_with(|| a.user_name.cmp(&b.user_name)));
    Ok(c)
}

/// clientDataJSON; key order matches browsers: type, challenge, origin, crossOrigin.
pub fn client_data(ty: &str, challenge: &str, origin: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"type": ty, "challenge": challenge, "origin": origin.trim_end_matches('/'), "crossOrigin": false})).unwrap()
}

/// Registration parameters (from `PublicKeyCredentialCreationOptionsJSON` or the host contract).
#[derive(Debug, Clone, Default)]
pub struct CreateOptions {
    pub rp_id: Option<String>, pub rp_name: Option<String>,
    /// base64url, passed through to clientDataJSON.
    pub challenge: String,
    pub user_id: Vec<u8>, pub user_name: String,
    /// `None` = the caller did not say (ES256 assumed).
    pub algorithms: Option<Vec<i64>>,
    pub exclude: Vec<Vec<u8>>,
}
/// Assertion parameters (from `PublicKeyCredentialRequestOptionsJSON` or the host contract).
#[derive(Debug, Clone, Default)]
pub struct GetOptions { pub rp_id: Option<String>, pub challenge: String, pub allow: Vec<Vec<u8>> }

fn s_opt<'a>(v: &'a Value, k: &str) -> Option<&'a str> { v.get(k).and_then(Value::as_str).filter(|x| !x.is_empty()) }
fn cred_list(v: &Value, k: &str) -> Result<Vec<Vec<u8>>> {
    let Some(a) = v.get(k) else { return Ok(vec![]) };
    let a = a.as_array().ok_or_else(|| fail(ErrorKind::BadRequest, format!("'{k}' must be a list")))?;
    a.iter().map(|d| {
        let id = d.get("id").and_then(Value::as_str).ok_or_else(|| fail(ErrorKind::BadRequest, format!("'{k}' entry without id")))?;
        unb64(id).map_err(|_| fail(ErrorKind::BadRequest, format!("'{k}' id must be base64url")))
    }).collect()
}
fn check_challenge(c: Option<&str>) -> Result<String> {
    let c = c.ok_or_else(|| fail(ErrorKind::BadRequest, "missing 'challenge'"))?;
    let raw = unb64(c).map_err(|_| fail(ErrorKind::BadRequest, "challenge must be base64url"))?;
    if raw.is_empty() { return Err(fail(ErrorKind::BadRequest, "empty challenge")); }
    Ok(c.trim().trim_end_matches('=').to_string())
}
fn parse_json(json: &str) -> Result<Value> {
    if json.len() > 64 * 1024 { return Err(fail(ErrorKind::BadRequest, "request too large")); }
    let v: Value = serde_json::from_str(json).map_err(|_| fail(ErrorKind::BadRequest, "request is not JSON"))?;
    if !v.is_object() { return Err(fail(ErrorKind::BadRequest, "request is not a JSON object")); }
    Ok(v)
}

/// Parse WebAuthn `PublicKeyCredentialCreationOptionsJSON` (what Android Credential Manager passes).
pub fn parse_create_options(json: &str) -> Result<CreateOptions> {
    let v = parse_json(json)?;
    let rp = v.get("rp").ok_or_else(|| fail(ErrorKind::BadRequest, "missing 'rp'"))?;
    let user = v.get("user").ok_or_else(|| fail(ErrorKind::BadRequest, "missing 'user'"))?;
    let user_id = unb64(s_opt(user, "id").ok_or_else(|| fail(ErrorKind::BadRequest, "missing 'user.id'"))?)
        .map_err(|_| fail(ErrorKind::BadRequest, "user.id must be base64url"))?;
    let algorithms = match v.get("pubKeyCredParams") {
        None => None,
        Some(Value::Array(a)) => Some(a.iter().filter(|p| p.get("type").and_then(Value::as_str).is_none_or(|t| t == "public-key"))
            .filter_map(|p| p.get("alg").and_then(Value::as_i64)).collect()),
        Some(_) => return Err(fail(ErrorKind::BadRequest, "'pubKeyCredParams' must be a list")),
    };
    Ok(CreateOptions {
        rp_id: s_opt(rp, "id").map(norm_rp), rp_name: s_opt(rp, "name").map(str::to_string),
        challenge: check_challenge(s_opt(&v, "challenge"))?, user_id,
        user_name: s_opt(user, "name").or_else(|| s_opt(user, "displayName")).unwrap_or("").to_string(),
        algorithms, exclude: cred_list(&v, "excludeCredentials")?,
    })
}

/// Parse WebAuthn `PublicKeyCredentialRequestOptionsJSON`.
pub fn parse_get_options(json: &str) -> Result<GetOptions> {
    let v = parse_json(json)?;
    Ok(GetOptions { rp_id: s_opt(&v, "rpId").map(norm_rp), challenge: check_challenge(s_opt(&v, "challenge"))?, allow: cred_list(&v, "allowCredentials")? })
}

/// A new credential, as the authenticator returns it.
#[derive(Debug, Clone)]
pub struct Registration {
    pub vault_id: String, pub item_id: String, pub rp_id: String, pub credential_id: Vec<u8>,
    pub client_data_json: Vec<u8>, pub attestation_object: Vec<u8>, pub authenticator_data: Vec<u8>, pub public_key: Vec<u8>,
}
impl Registration {
    /// `RegistrationResponseJSON` (WebAuthn L3), as Credential Manager expects.
    pub fn to_webauthn_json(&self) -> Value {
        let id = b64(&self.credential_id);
        json!({
            "id": id, "rawId": id, "type": "public-key", "authenticatorAttachment": "platform",
            "response": {
                "clientDataJSON": b64(&self.client_data_json), "attestationObject": b64(&self.attestation_object),
                "authenticatorData": b64(&self.authenticator_data), "publicKey": b64(&self.public_key), "publicKeyAlgorithm": -7,
                "transports": ["internal", "hybrid"],
            },
            "clientExtensionResults": {},
        })
    }
}

/// Create and store a passkey for `rp`. The caller has verified the user.
pub fn create(s: &Session, rp: &VerifiedRp, o: &CreateOptions, vault_id: Option<&str>) -> Result<Registration> {
    unb64(&o.challenge).map_err(|_| fail(ErrorKind::BadRequest, "challenge must be base64url"))?;
    if o.user_id.is_empty() || o.user_id.len() > 64 { return Err(fail(ErrorKind::BadRequest, "user_id must be 1..64 bytes")); }
    if let Some(a) = &o.algorithms {
        if !a.is_empty() && !a.contains(&-7) { return Err(fail(ErrorKind::NotSupported, "only ES256 (-7) is supported")); }
    }
    let existing = find(s, &rp.rp_id)?;
    if existing.iter().any(|(_, _, p)| o.exclude.contains(&p.credential_id)) { return Err(fail(ErrorKind::Exists, "a passkey for this account is already saved")); }
    let vid = match vault_id {
        Some(v) => { s.vault_meta(v).map_err(|_| fail(ErrorKind::NotFound, "vault not found"))?; v.to_string() }
        None => s.list_vaults()?.into_iter().next().map(|(v, _)| v).ok_or_else(|| fail(ErrorKind::NotFound, "no vault"))?,
    };
    let pk = Passkey::generate(&rp.rp_id, &o.user_id, &o.user_name);
    let title = o.rp_name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| rp.rp_id.clone());
    let mut rec = pk.to_record(&title)?;
    // Same user handle on the same rp = replace (WebAuthn: one credential per account per authenticator).
    for (v, i, p) in &existing { if p.user_handle == o.user_id { let _ = s.soft_delete(v, i); } }
    rec.item.username = Some(o.user_name.clone()).filter(|u| !u.is_empty());
    let iid = s.add_record(&vid, rec)?;
    Ok(Registration {
        vault_id: vid, item_id: iid, rp_id: rp.rp_id.clone(), credential_id: pk.credential_id.clone(),
        client_data_json: client_data("webauthn.create", &o.challenge, &rp.origin),
        attestation_object: pk.attestation_object(), authenticator_data: pk.auth_data(true), public_key: pk.spki_der(),
    })
}

/// An assertion, as the authenticator returns it.
#[derive(Debug, Clone)]
pub struct Assertion {
    pub vault_id: String, pub item_id: String, pub credential_id: Vec<u8>, pub client_data_json: Vec<u8>,
    pub authenticator_data: Vec<u8>, pub signature: Vec<u8>, pub user_handle: Vec<u8>, pub sign_count: u32,
}
impl Assertion {
    /// `AuthenticationResponseJSON` (WebAuthn L3), as Credential Manager expects.
    pub fn to_webauthn_json(&self) -> Value {
        let id = b64(&self.credential_id);
        json!({
            "id": id, "rawId": id, "type": "public-key", "authenticatorAttachment": "platform",
            "response": {
                "clientDataJSON": b64(&self.client_data_json), "authenticatorData": b64(&self.authenticator_data),
                "signature": b64(&self.signature), "userHandle": b64(&self.user_handle),
            },
            "clientExtensionResults": {},
        })
    }
}

/// Sign in with a stored passkey for `rp`. `credential_id` picks one (it must also pass `allow`);
/// otherwise the first match is used. `client_data_hash` is the caller-supplied SHA-256 of
/// clientDataJSON (privileged browsers on Android); without it gitpass builds clientDataJSON from
/// the verified origin. The caller has verified the user.
pub fn get(s: &Session, rp: &VerifiedRp, o: &GetOptions, credential_id: Option<&[u8]>, client_data_hash: Option<&[u8]>) -> Result<Assertion> {
    unb64(&o.challenge).map_err(|_| fail(ErrorKind::BadRequest, "challenge must be base64url"))?;
    let hash: Option<[u8; 32]> = match client_data_hash {
        None => None,
        Some(h) => Some(h.try_into().map_err(|_| fail(ErrorKind::BadRequest, "clientDataHash must be 32 bytes"))?),
    };
    let mut c: Vec<_> = find(s, &rp.rp_id)?.into_iter()
        .filter(|(_, _, p)| o.allow.is_empty() || o.allow.contains(&p.credential_id))
        .filter(|(_, _, p)| credential_id.is_none_or(|c| c == p.credential_id.as_slice())).collect();
    if c.is_empty() { return Err(fail(ErrorKind::NotFound, format!("no passkey for {}", rp.rp_id))); }
    let (vid, iid, mut pk) = c.remove(0);
    pk.sign_count = pk.sign_count.saturating_add(1);
    let (_, mut r) = s.read_record(&vid, &iid)?;
    Passkey::set_sign_count(&mut r, pk.sign_count);
    s.update_record(&vid, &iid, r)?;
    let _ = s.touch_item_used(&iid);
    let ad = pk.auth_data(false);
    let cdj = client_data("webauthn.get", &o.challenge, &rp.origin);
    let signature = match &hash { Some(h) => pk.sign_assertion_hash(&ad, h), None => pk.sign_assertion(&ad, &cdj) };
    Ok(Assertion {
        vault_id: vid, item_id: iid, credential_id: pk.credential_id.clone(), client_data_json: cdj,
        authenticator_data: ad, signature, user_handle: pk.user_handle.clone(), sign_count: pk.sign_count,
    })
}
