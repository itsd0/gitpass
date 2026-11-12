//! `passkey_create` / `passkey_get` (contract v0.7), shared by the desktop agent and the standalone
//! fallback. Callers MUST check user approval before calling these (popup `approved:true`, or an
//! app prompt). The authenticator logic (origin / rpId policy, clientDataJSON, crypto) lives in
//! `gitpass_core::webauthn` so the Android passkey provider (#477) uses the same implementation;
//! this module only maps the JSON contract and error codes.
//!
//! The authenticator (us) builds `clientDataJSON` itself from the browser-verified `origin` (the
//! extension background takes it from `sender.origin`, never from the page), so a page cannot ask
//! for an assertion for another origin.
use crate::handler::{code, coded};
use anyhow::Result;
use gitpass_core::passkey::{b64, unb64};
use gitpass_core::store::Session;
use gitpass_core::webauthn::{self as wa, ErrorKind};
use serde_json::{json, Value};

fn opt<'a>(q: &'a Value, k: &str) -> Option<&'a str> { q.get(k).and_then(Value::as_str).filter(|v| !v.is_empty()) }
fn req<'a>(q: &'a Value, k: &str) -> Result<&'a str> { opt(q, k).ok_or_else(|| coded(code::BAD_REQUEST, format!("missing '{k}'"))) }
fn b64_list(q: &Value, k: &str) -> Vec<Vec<u8>> {
    q.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().and_then(|s| unb64(s).ok())).collect()).unwrap_or_default()
}

/// Core errors -> host error codes.
fn mapped(e: anyhow::Error) -> anyhow::Error {
    let msg = e.to_string();
    match wa::error_kind(&e) {
        Some(ErrorKind::OriginMismatch) => coded(code::ORIGIN_MISMATCH, msg),
        Some(ErrorKind::BadRequest) => coded(code::BAD_REQUEST, msg),
        Some(ErrorKind::NotSupported) => coded("not_supported", msg),
        Some(ErrorKind::Exists) => coded("exists", msg),
        Some(ErrorKind::NotFound) => coded(code::NOT_FOUND, msg),
        None => e,
    }
}

/// Validate origin and resolve the rpId (WebAuthn §5.1.3 step 8 / §5.1.4 step 7).
pub fn check_rp(origin: &str, rp_id: Option<&str>) -> Result<String> {
    wa::verify_web(origin, rp_id).map(|rp| rp.rp_id().to_string()).map_err(mapped)
}

pub use gitpass_core::webauthn::{find, record_matches_origin, rp_matches_origin};

/// `passkey_query {origin, rp_id?, allow_credentials?}`
pub fn query(s: &Session, q: &Value) -> Result<Value> {
    let rp = wa::verify_web(req(q, "origin")?, opt(q, "rp_id")).map_err(mapped)?;
    let matching: Vec<Value> = wa::candidates(s, &rp, &b64_list(q, "allow_credentials"))?.into_iter().map(|c| json!({
        "credential_id": b64(&c.credential_id),
        "user_name": c.user_name,
        "user_handle": b64(&c.user_handle),
        "vault_name": c.vault_name,
        "last_used": c.last_used,
    })).collect();
    Ok(json!({ "rp_id": rp.rp_id(), "matching": matching }))
}

/// `passkey_create {origin, rp_id?, rp_name?, challenge, user_id, user_name, user_display_name?, exclude_credentials?, algorithms?, vault_id?}`
pub fn create(s: &Session, q: &Value) -> Result<Value> {
    let rp = wa::verify_web(req(q, "origin")?, opt(q, "rp_id")).map_err(mapped)?;
    let challenge = req(q, "challenge")?;
    unb64(challenge).map_err(|_| coded(code::BAD_REQUEST, "challenge must be base64url"))?;
    let user_id = unb64(req(q, "user_id")?).map_err(|_| coded(code::BAD_REQUEST, "user_id must be base64url"))?;
    let o = wa::CreateOptions {
        rp_id: Some(rp.rp_id().to_string()), rp_name: opt(q, "rp_name").map(str::to_string),
        challenge: challenge.to_string(), user_id,
        user_name: opt(q, "user_name").or_else(|| opt(q, "user_display_name")).unwrap_or("").to_string(),
        algorithms: q.get("algorithms").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_i64).collect()),
        exclude: b64_list(q, "exclude_credentials"),
    };
    let r = wa::create(s, &rp, &o, opt(q, "vault_id")).map_err(mapped)?;
    Ok(json!({
        "item_id": format!("{}:{}", r.vault_id, r.item_id), "rp_id": r.rp_id, "credential_id": b64(&r.credential_id),
        "client_data_json": b64(&r.client_data_json), "attestation_object": b64(&r.attestation_object),
        "authenticator_data": b64(&r.authenticator_data), "public_key": b64(&r.public_key), "public_key_algorithm": -7,
        "transports": ["internal", "hybrid"],
    }))
}

/// `passkey_get {origin, rp_id?, challenge, allow_credentials?, credential_id?}`
pub fn get(s: &Session, q: &Value) -> Result<Value> {
    let rp = wa::verify_web(req(q, "origin")?, opt(q, "rp_id")).map_err(mapped)?;
    let challenge = req(q, "challenge")?;
    unb64(challenge).map_err(|_| coded(code::BAD_REQUEST, "challenge must be base64url"))?;
    let chosen = match opt(q, "credential_id") {
        Some(c) => Some(unb64(c).map_err(|_| coded(code::BAD_REQUEST, "credential_id must be base64url"))?),
        None => None,
    };
    let o = wa::GetOptions { rp_id: Some(rp.rp_id().to_string()), challenge: challenge.to_string(), allow: b64_list(q, "allow_credentials") };
    let a = wa::get(s, &rp, &o, chosen.as_deref(), None).map_err(mapped)?;
    Ok(json!({
        "item_id": format!("{}:{}", a.vault_id, a.item_id), "credential_id": b64(&a.credential_id), "client_data_json": b64(&a.client_data_json),
        "authenticator_data": b64(&a.authenticator_data), "signature": b64(&a.signature), "user_handle": b64(&a.user_handle), "sign_count": a.sign_count,
    }))
}

/// Approval rule shared by both implementations: the popup (a user gesture in extension UI) must say
/// `approved: true`.
pub fn popup_approved(q: &Value) -> bool { q.get("source").and_then(Value::as_str) == Some("popup") && q.get("approved").and_then(Value::as_bool) == Some(true) }
