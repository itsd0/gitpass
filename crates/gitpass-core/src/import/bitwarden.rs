use super::common::{add_tag, fld, iso_to_epoch, month_year, next_id, opt, push_section, record};
use super::Parsed;
use crate::item::{Category, Field, FieldKind};
use std::collections::BTreeMap;
use aes::Aes256;
use anyhow::{anyhow, bail, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use zeroize::Zeroizing;

// ---------------------------------------------------------------- Bitwarden JSON

pub(crate) fn parse_bitwarden_json(data: &str, export_password: Option<&str>) -> Result<Parsed> {
    let v: Value = serde_json::from_str(data).context("bitwarden-json: invalid JSON")?;
    if v.get("encrypted").and_then(Value::as_bool) == Some(true) {
        let clear = decrypt_bitwarden_export(&v, export_password)?;
        return parse_bitwarden_json(&clear, None);
    }
    let folders: BTreeMap<String, String> = v.get("folders").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|f| Some((f.get("id")?.as_str()?.to_string(), f.get("name")?.as_str()?.to_string()))).collect();
    let collections: BTreeMap<String, String> = v.get("collections").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|f| Some((f.get("id")?.as_str()?.to_string(), f.get("name")?.as_str()?.to_string()))).collect();
    let items = v.get("items").and_then(Value::as_array).ok_or_else(|| anyhow!("bitwarden-json: missing \"items\" array"))?;
    let s = |o: &Value, k: &str| opt(o.get(k).and_then(Value::as_str));
    let mut out = Parsed::default();
    for (i, it) in items.iter().enumerate() {
        let Some(title) = s(it, "name") else { out.warnings.push(format!("item {i}: no name, skipped")); continue };
        if it.get("deletedDate").is_some_and(|d| !d.is_null()) { out.warnings.push(format!("item {i} ({title}): in Bitwarden trash, skipped")); continue; }
        let ty = it.get("type").and_then(Value::as_u64).unwrap_or(1);
        let cat = match ty { 1 => Category::Login, 2 => Category::SecureNote, 3 => Category::CreditCard, 4 => Category::Identity, 5 => Category::SshKey, _ => Category::SecureNote };
        let mut r = record(title.clone(), cat);
        r.item.notes = s(it, "notes");
        r.favorite = it.get("favorite").and_then(Value::as_bool).unwrap_or(false);
        if let Some(f) = s(it, "folderId").and_then(|fid| folders.get(&fid).cloned()) { add_tag(&mut r, &f); }
        for c in it.get("collectionIds").and_then(Value::as_array).into_iter().flatten() {
            if let Some(n) = c.as_str().and_then(|c| collections.get(c)) { add_tag(&mut r, n); }
        }
        match ty {
            1 => if let Some(l) = it.get("login") {
                r.item.username = s(l, "username"); r.item.password = s(l, "password"); r.item.totp = s(l, "totp");
                let uris: Vec<String> = l.get("uris").and_then(Value::as_array).into_iter().flatten().filter_map(|u| s(u, "uri")).collect();
                r.item.url = uris.first().cloned();
                let extra: Vec<Option<Field>> = uris.iter().skip(1).enumerate().map(|(n, u)| fld(&format!("url{}", n + 2), "website", FieldKind::Url, Some(u.clone()))).collect();
                push_section(&mut r, "urls", Some("Websites"), extra);
                let pw_history = l.get("passwordHistory").or_else(|| it.get("passwordHistory")).and_then(Value::as_array);
                if let Some(hist) = pw_history {
                    for entry in hist {
                        let pw = entry.get("password").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty());
                        let Some(pw) = pw else { continue };
                        let replaced_at = entry.get("lastUsedDate").and_then(Value::as_str).map(str::trim)
                            .filter(|d| !d.is_empty())
                            .map(|d| d.to_string())
                            .unwrap_or_else(|| r.item.updated_at.clone());
                        r.password_history.push(crate::item::PasswordHistoryEntry {
                            value: pw.to_string(),
                            replaced_at,
                        });
                    }
                    r.password_history.sort_by(|a, b| b.replaced_at.cmp(&a.replaced_at));
                    if r.password_history.len() > crate::item::PASSWORD_HISTORY_LIMIT {
                        r.password_history.truncate(crate::item::PASSWORD_HISTORY_LIMIT);
                    }
                }
            },
            3 => if let Some(c) = it.get("card") {
                push_section(&mut r, "main", None, vec![
                    fld("cardholder", "cardholder name", FieldKind::Text, s(c, "cardholderName")),
                    fld("type", "type", FieldKind::Text, s(c, "brand")),
                    fld("number", "number", FieldKind::Concealed, s(c, "number")),
                    fld("cvv", "verification number", FieldKind::Concealed, s(c, "code")),
                    fld("expiry", "expiry date", FieldKind::MonthYear, month_year(s(c, "expMonth"), s(c, "expYear")))]);
            },
            4 => if let Some(d) = it.get("identity") {
                let addr = ["address1", "address2", "address3"].iter().filter_map(|k| s(d, k)).collect::<Vec<_>>();
                let city = [s(d, "city"), s(d, "state"), s(d, "postalCode")].into_iter().flatten().collect::<Vec<_>>().join(" ");
                let full = addr.into_iter().chain([city].into_iter().filter(|x| !x.is_empty())).chain(s(d, "country")).collect::<Vec<_>>().join("\n");
                push_section(&mut r, "name", Some("Identification"), vec![
                    fld("title", "title", FieldKind::Text, s(d, "title")),
                    fld("first", "first name", FieldKind::Text, s(d, "firstName")),
                    fld("middle", "middle name", FieldKind::Text, s(d, "middleName")),
                    fld("last", "last name", FieldKind::Text, s(d, "lastName")),
                    fld("username", "username", FieldKind::Text, s(d, "username")),
                    fld("company", "company", FieldKind::Text, s(d, "company"))]);
                push_section(&mut r, "address", Some("Address"), vec![fld("address", "address", FieldKind::Multiline, Some(full))]);
                push_section(&mut r, "contact", Some("Contact"), vec![
                    fld("email", "email", FieldKind::Email, s(d, "email")), fld("phone", "phone", FieldKind::Phone, s(d, "phone"))]);
                push_section(&mut r, "ids", Some("Documents"), vec![
                    fld("ssn", "social security number", FieldKind::Concealed, s(d, "ssn")),
                    fld("passport", "passport number", FieldKind::Concealed, s(d, "passportNumber")),
                    fld("license", "license number", FieldKind::Concealed, s(d, "licenseNumber"))]);
            },
            5 => if let Some(k) = it.get("sshKey") {
                push_section(&mut r, "main", None, vec![
                    fld("private_key", "private key", FieldKind::Concealed, s(k, "privateKey")),
                    fld("public_key", "public key", FieldKind::Multiline, s(k, "publicKey")),
                    fld("fingerprint", "fingerprint", FieldKind::Text, s(k, "keyFingerprint"))]);
            },
            _ => {}
        }
        for f in it.get("fields").and_then(Value::as_array).into_iter().flatten() {
            let Some(n) = s(f, "name") else { continue };
            let kind = match f.get("type").and_then(Value::as_u64) { Some(1) => FieldKind::Concealed, _ => FieldKind::Text };
            let val = s(f, "value").or_else(|| f.get("value").filter(|v| v.is_boolean() || v.is_number()).map(|v| v.to_string()));
            let id = next_id(&r, "custom");
            push_section(&mut r, "custom", Some("Custom fields"), vec![fld(&id, &n, kind, val)]);
        }
        if let Some(t) = s(it, "revisionDate").and_then(|d| iso_to_epoch(&d)) { r.item.updated_at = t.to_string(); }
        r.created_at = s(it, "creationDate").and_then(|d| iso_to_epoch(&d));
        out.items.push(r);
    }
    Ok(out)
}

// ------------------------------------------------ Bitwarden password-protected JSON

/// True when `data` is a Bitwarden JSON export protected by a user-chosen password
/// (`encrypted` + `passwordProtected`). Account-key-encrypted exports return false.
pub fn bitwarden_json_needs_password(data: &str) -> bool {
    serde_json::from_str::<Value>(data).is_ok_and(|v| is_password_protected(&v))
}

fn is_password_protected(v: &Value) -> bool {
    v.get("encrypted").and_then(Value::as_bool) == Some(true)
        && v.get("passwordProtected").and_then(Value::as_bool) == Some(true)
}

// Bounds Bitwarden clients accept for import KDF parameters. An import file is attacker controlled
// and Argon2 allocates `kdfMemory` MiB up front, so reject out-of-range values before deriving.
const BW_PBKDF2_ITER_MIN: u64 = 5_000;
const BW_PBKDF2_ITER_MAX: u64 = 2_000_000;
const BW_ARGON2_ITER_MIN: u64 = 2;
const BW_ARGON2_ITER_MAX: u64 = 10;
const BW_ARGON2_MEMORY_MIN: u64 = 16; // MiB
const BW_ARGON2_MEMORY_MAX: u64 = 1024;
const BW_ARGON2_PARALLELISM_MIN: u64 = 1;
const BW_ARGON2_PARALLELISM_MAX: u64 = 16;

fn within(v: u64, lo: u64, hi: u64) -> bool { (lo..=hi).contains(&v) }

/// Decrypt a password-protected Bitwarden JSON export: derive the key from the export password with
/// the file's KDF settings, HKDF-Expand it into the AES-256-CBC and HMAC-SHA256 keys, verify
/// `encKeyValidation_DO_NOT_EDIT`, then MAC-verify and decrypt `data`.
fn decrypt_bitwarden_export(v: &Value, export_password: Option<&str>) -> Result<Zeroizing<String>> {
    if !is_password_protected(v) {
        bail!("bitwarden-json: this export is encrypted with your Bitwarden account key, not an export password; in Bitwarden choose “Password protected” or an unencrypted export");
    }
    let password = export_password
        .filter(|p| !p.is_empty())
        .ok_or_else(|| anyhow!("bitwarden-json: this export is password-protected; the export password is required"))?;
    let field = |k: &str| {
        v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing or has an empty \"{k}\""))
    };
    let master = bitwarden_export_key(password, field("salt")?, v)?;
    let mut enc_key = Zeroizing::new([0u8; 32]);
    let mut mac_key = Zeroizing::new([0u8; 32]);
    let hk = hkdf::Hkdf::<Sha256>::from_prk(master.as_slice())
        .map_err(|_| anyhow!("bitwarden-json: invalid export key material"))?;
    hk.expand(b"enc", enc_key.as_mut_slice()).map_err(|_| anyhow!("bitwarden-json: cannot stretch the export key"))?;
    hk.expand(b"mac", mac_key.as_mut_slice()).map_err(|_| anyhow!("bitwarden-json: cannot stretch the export key"))?;
    // A MAC failure on the validation blob means the password is wrong; on the payload it means tampering.
    bitwarden_decrypt(field("encKeyValidation_DO_NOT_EDIT")?, enc_key.as_slice(), mac_key.as_slice())
        .map_err(|_| anyhow!("bitwarden-json: wrong export password (or the file is corrupted)"))?;
    let plain = bitwarden_decrypt(field("data")?, enc_key.as_slice(), mac_key.as_slice())
        .map_err(|_| anyhow!("bitwarden-json: encrypted contents failed the integrity check (tampered file or wrong password)"))?;
    let text = std::str::from_utf8(plain.as_slice()).context("bitwarden-json: decrypted export is not UTF-8")?;
    Ok(Zeroizing::new(text.to_string()))
}

/// Derive the 32-byte export key from the password and the file's KDF settings. Bitwarden feeds the
/// salt text itself (a base64 string) to the KDF as UTF-8, not the decoded bytes.
fn bitwarden_export_key(password: &str, salt: &str, v: &Value) -> Result<Zeroizing<[u8; 32]>> {
    let num = |k: &str| v.get(k).and_then(Value::as_u64);
    let kdf = num("kdfType").ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing \"kdfType\""))?;
    let mut key = Zeroizing::new([0u8; 32]);
    match kdf {
        0 => {
            let iterations = num("kdfIterations").ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing \"kdfIterations\""))?;
            if !within(iterations, BW_PBKDF2_ITER_MIN, BW_PBKDF2_ITER_MAX) {
                bail!("bitwarden-json: unsupported PBKDF2 iteration count {iterations} (expected {BW_PBKDF2_ITER_MIN}–{BW_PBKDF2_ITER_MAX})");
            }
            pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), iterations as u32, key.as_mut_slice());
        }
        1 => {
            let iterations = num("kdfIterations").ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing \"kdfIterations\""))?;
            let memory = num("kdfMemory").ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing \"kdfMemory\""))?;
            let parallelism = num("kdfParallelism").ok_or_else(|| anyhow!("bitwarden-json: encrypted export is missing \"kdfParallelism\""))?;
            if !within(iterations, BW_ARGON2_ITER_MIN, BW_ARGON2_ITER_MAX)
                || !within(memory, BW_ARGON2_MEMORY_MIN, BW_ARGON2_MEMORY_MAX)
                || !within(parallelism, BW_ARGON2_PARALLELISM_MIN, BW_ARGON2_PARALLELISM_MAX)
            {
                bail!("bitwarden-json: unsupported Argon2id settings (iterations {iterations}, memory {memory} MiB, parallelism {parallelism})");
            }
            let params = Params::new((memory * 1024) as u32, iterations as u32, parallelism as u32, Some(32))
                .map_err(|e| anyhow!("bitwarden-json: unsupported Argon2id settings: {e}"))?;
            Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
                .hash_password_into(password.as_bytes(), salt.as_bytes(), key.as_mut_slice())
                .map_err(|_| anyhow!("bitwarden-json: cannot derive the export key"))?;
        }
        other => bail!("bitwarden-json: unsupported KDF type {other} (expected 0 = PBKDF2-SHA256 or 1 = Argon2id)"),
    }
    Ok(key)
}

/// Decrypt one Bitwarden `EncString` of type 2 (`2.<iv>|<ciphertext>|<mac>`, AesCbc256_HmacSha256).
/// The MAC covers IV ‖ ciphertext and is checked before AES-CBC/PKCS7 decryption.
fn bitwarden_decrypt(field: &str, enc_key: &[u8], mac_key: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let Some(rest) = field.strip_prefix("2.") else {
        bail!("bitwarden-json: encrypted field is not AesCbc256_HmacSha256");
    };
    let mut parts = rest.split('|');
    let (Some(iv), Some(ct), Some(mac), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        bail!("bitwarden-json: malformed encrypted field");
    };
    let bad = || anyhow!("bitwarden-json: malformed encrypted field");
    let iv = B64.decode(iv).map_err(|_| bad())?;
    let mut ct = B64.decode(ct).map_err(|_| bad())?;
    let mac = B64.decode(mac).map_err(|_| bad())?;
    let mut h = Hmac::<Sha256>::new_from_slice(mac_key).map_err(|_| anyhow!("bitwarden-json: invalid MAC key"))?;
    h.update(&iv);
    h.update(&ct);
    h.verify_slice(&mac).map_err(|_| anyhow!("bitwarden-json: MAC check failed"))?;
    let dec = cbc::Decryptor::<Aes256>::new_from_slices(enc_key, &iv)
        .map_err(|_| anyhow!("bitwarden-json: invalid AES key or IV"))?;
    let pt = dec.decrypt_padded_mut::<Pkcs7>(&mut ct).map_err(|_| anyhow!("bitwarden-json: invalid encrypted payload"))?;
    Ok(Zeroizing::new(pt.to_vec()))
}
