//! `passkey_create` / `passkey_get` (contract v0.7), shared by the desktop agent and the standalone
//! fallback. Callers MUST check user approval before calling these (popup `approved:true`, or an
//! app prompt); this module enforces origin / rpId policy and does the crypto.
//!
//! The authenticator (us) builds `clientDataJSON` itself from the browser-verified `origin` (the
//! extension background takes it from `sender.origin`, never from the page), so a page cannot ask
//! for an assertion for another origin.
use crate::handler::{code, coded};
use crate::origin::{parse_request_origin, registrable_domain};
use anyhow::Result;
use gitpass_core::item::Category;
use gitpass_core::passkey::{b64, unb64, Passkey};
use gitpass_core::store::Session;
use serde_json::{json, Value};

fn opt<'a>(q: &'a Value, k: &str) -> Option<&'a str> { q.get(k).and_then(Value::as_str).filter(|v| !v.is_empty()) }
fn req<'a>(q: &'a Value, k: &str) -> Result<&'a str> { opt(q, k).ok_or_else(|| coded(code::BAD_REQUEST, format!("missing '{k}'"))) }
fn b64_list(q: &Value, k: &str) -> Vec<Vec<u8>> {
    q.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().and_then(|s| unb64(s).ok())).collect()).unwrap_or_default()
}

/// Validate origin and resolve the rpId (WebAuthn §5.1.3 step 8 / §5.1.4 step 7).
pub fn check_rp(origin: &str, rp_id: Option<&str>) -> Result<String> {
    let u = parse_request_origin(origin).ok_or_else(|| coded(code::ORIGIN_MISMATCH, "origin must be https (or http on localhost)"))?;
    let host = u.host_str().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase();
    let rp = rp_id.map(|r| r.trim().trim_end_matches('.').to_ascii_lowercase()).unwrap_or_else(|| host.clone());
    if rp.is_empty() { return Err(coded(code::BAD_REQUEST, "empty rpId")); }
    if rp == host { return Ok(rp); }
    let suffix_ok = host.ends_with(&format!(".{rp}")) && registrable_domain(&rp).is_some();
    if !suffix_ok { return Err(coded(code::ORIGIN_MISMATCH, format!("rpId '{rp}' is not valid for origin {origin}"))); }
    Ok(rp)
}

fn client_data(ty: &str, challenge: &str, origin: &str) -> Vec<u8> {
    // Key order matches browsers: type, challenge, origin, crossOrigin.
    serde_json::to_vec(&json!({"type": ty, "challenge": challenge, "origin": origin.trim_end_matches('/'), "crossOrigin": false})).unwrap()
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

/// `passkey_query {origin, rp_id?, allow_credentials?}`
pub fn query(s: &Session, q: &Value) -> Result<Value> {
    let origin = req(q, "origin")?;
    let rp = check_rp(origin, opt(q, "rp_id"))?;
    let allow = b64_list(q, "allow_credentials");
    let c: Vec<_> = find(s, &rp)?.into_iter()
        .filter(|(_, _, p)| allow.is_empty() || allow.contains(&p.credential_id))
        .map(|(_, _, p)| {
            json!({
                "credential_id": b64(&p.credential_id),
                "user_name": p.user_name,
                "user_handle": b64(&p.user_handle),
            })
        }).collect();
    Ok(json!({ "rp_id": rp, "matching": c }))
}

/// `passkey_create {origin, rp_id?, rp_name?, challenge, user_id, user_name, user_display_name?, exclude_credentials?, algorithms?, vault_id?}`
pub fn create(s: &Session, q: &Value) -> Result<Value> {
    let origin = req(q, "origin")?;
    let rp = check_rp(origin, opt(q, "rp_id"))?;
    let challenge = req(q, "challenge")?;
    unb64(challenge).map_err(|_| coded(code::BAD_REQUEST, "challenge must be base64url"))?;
    let user_id = unb64(req(q, "user_id")?).map_err(|_| coded(code::BAD_REQUEST, "user_id must be base64url"))?;
    if user_id.is_empty() || user_id.len() > 64 { return Err(coded(code::BAD_REQUEST, "user_id must be 1..64 bytes")); }
    let user_name = opt(q, "user_name").or_else(|| opt(q, "user_display_name")).unwrap_or("");
    if let Some(a) = q.get("algorithms").and_then(Value::as_array) {
        if !a.is_empty() && !a.iter().any(|x| x.as_i64() == Some(-7)) { return Err(coded("not_supported", "only ES256 (-7) is supported")); }
    }
    let existing = find(s, &rp)?;
    let exclude = b64_list(q, "exclude_credentials");
    if existing.iter().any(|(_, _, p)| exclude.contains(&p.credential_id)) { return Err(coded("exists", "a passkey for this account is already saved")); }
    let vid = match opt(q, "vault_id") { Some(v) => { s.vault_meta(v).map_err(|_| coded(code::NOT_FOUND, "vault not found"))?; v.to_string() }
        None => s.list_vaults()?.into_iter().next().map(|(v, _)| v).ok_or_else(|| coded(code::NOT_FOUND, "no vault"))? };
    let pk = Passkey::generate(&rp, &user_id, user_name);
    let title = match opt(q, "rp_name") { Some(n) => n.to_string(), None => rp.clone() };
    let mut rec = pk.to_record(&title)?;
    // Same user handle on the same rp = replace (WebAuthn: one credential per account per authenticator).
    for (v, i, p) in &existing { if p.user_handle == user_id { let _ = s.soft_delete(v, i); } }
    rec.item.username = Some(user_name.to_string()).filter(|u| !u.is_empty());
    let iid = s.add_record(&vid, rec)?;
    let cdj = client_data("webauthn.create", challenge, origin);
    Ok(json!({
        "item_id": format!("{vid}:{iid}"), "rp_id": rp, "credential_id": b64(&pk.credential_id),
        "client_data_json": b64(&cdj), "attestation_object": b64(&pk.attestation_object()),
        "authenticator_data": b64(&pk.auth_data(true)), "public_key": b64(&pk.spki_der()), "public_key_algorithm": -7,
        "transports": ["internal", "hybrid"],
    }))
}

/// `passkey_get {origin, rp_id?, challenge, allow_credentials?, credential_id?}`
pub fn get(s: &Session, q: &Value) -> Result<Value> {
    let origin = req(q, "origin")?;
    let rp = check_rp(origin, opt(q, "rp_id"))?;
    let challenge = req(q, "challenge")?;
    unb64(challenge).map_err(|_| coded(code::BAD_REQUEST, "challenge must be base64url"))?;
    let allow = b64_list(q, "allow_credentials");
    let chosen = opt(q, "credential_id").and_then(|c| unb64(c).ok());
    let mut c: Vec<_> = find(s, &rp)?.into_iter()
        .filter(|(_, _, p)| allow.is_empty() || allow.contains(&p.credential_id))
        .filter(|(_, _, p)| chosen.as_ref().is_none_or(|c| *c == p.credential_id)).collect();
    if c.is_empty() { return Err(coded(code::NOT_FOUND, format!("no passkey for {rp}"))); }
    let (vid, iid, mut pk) = c.remove(0);
    pk.sign_count = pk.sign_count.saturating_add(1);
    let (_, mut r) = s.read_record(&vid, &iid)?;
    Passkey::set_sign_count(&mut r, pk.sign_count);
    s.update_record(&vid, &iid, r)?;
    let ad = pk.auth_data(false);
    let cdj = client_data("webauthn.get", challenge, origin);
    let sig = pk.sign_assertion(&ad, &cdj);
    Ok(json!({
        "item_id": format!("{vid}:{iid}"), "credential_id": b64(&pk.credential_id), "client_data_json": b64(&cdj),
        "authenticator_data": b64(&ad), "signature": b64(&sig), "user_handle": b64(&pk.user_handle), "sign_count": pk.sign_count,
    }))
}

/// Approval rule shared by both implementations: the popup (a user gesture in extension UI) must say
/// `approved: true`.
pub fn popup_approved(q: &Value) -> bool { q.get("source").and_then(Value::as_str) == Some("popup") && q.get("approved").and_then(Value::as_bool) == Some(true) }
