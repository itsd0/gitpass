//! WebAuthn passkeys (item category `passkey`): ES256 (P-256) credentials stored in the vault.
//!
//! Item layout (section `main`): `rp_id`, `credential_id` (base64url), `user_handle` (base64url),
//! `user_name`, `private_key` (concealed, PKCS#8 DER, base64url), `sign_count`. `item.url` is
//! `https://<rp_id>` for display / search.
//!
//! This module is pure crypto + encoding (authenticator data, "none" attestation, COSE key, ES256
//! assertion signatures). Origin / rpId policy and approval live in the callers (gitpass-host).
use crate::item::{Category, Field, FieldKind, Section};
use crate::merge::ItemRecord;
use crate::store::Item;
use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use p256::ecdsa::{signature::Signer, Signature, SigningKey, VerifyingKey};
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use sha2::{Digest, Sha256};

pub const F_RP_ID: &str = "rp_id";
pub const F_CRED_ID: &str = "credential_id";
pub const F_USER_HANDLE: &str = "user_handle";
pub const F_USER_NAME: &str = "user_name";
pub const F_PRIVATE: &str = "private_key";
pub const F_SIGN_COUNT: &str = "sign_count";

/// gitpass authenticator AAGUID (random, fixed; identifies "gitpass" to relying parties).
pub const AAGUID: [u8; 16] = [0x7a, 0x1c, 0x3e, 0x52, 0x9b, 0x04, 0x4d, 0x61, 0xa8, 0x2f, 0x6e, 0x19, 0xc4, 0x80, 0x5d, 0x33];

pub const FLAG_UP: u8 = 0x01;
pub const FLAG_UV: u8 = 0x04;
pub const FLAG_BE: u8 = 0x08;
pub const FLAG_BS: u8 = 0x10;
pub const FLAG_AT: u8 = 0x40;

pub fn b64(b: &[u8]) -> String { B64.encode(b) }
pub fn unb64(s: &str) -> Result<Vec<u8>> { B64.decode(s.trim().trim_end_matches('=')).map_err(|e| anyhow!("bad base64url: {e}")) }

/// A stored passkey.
pub struct Passkey { pub rp_id: String, pub credential_id: Vec<u8>, pub user_handle: Vec<u8>, pub user_name: String, pub key: SigningKey, pub sign_count: u32 }

impl Passkey {
    /// New credential with a random 32-byte id.
    pub fn generate(rp_id: &str, user_handle: &[u8], user_name: &str) -> Self {
        let mut id = vec![0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut id);
        // p256 0.14's `SigningKey::random` needs a rand_core 0.10 CryptoRng,
        // which core's rand 0.8 OsRng does not satisfy, so generate from
        // raw random bytes instead (retry on the negligible chance the
        // scalar is out of range). PKCS#8 storage format is unchanged.
        let key = loop {
            let b = crate::crypto::random_bytes::<32>();
            if let Ok(k) = SigningKey::from_slice(&b) { break k; }
        };
        Passkey { rp_id: rp_id.into(), credential_id: id, user_handle: user_handle.to_vec(), user_name: user_name.into(), key, sign_count: 0 }
    }
    pub fn verifying_key(&self) -> VerifyingKey { *self.key.verifying_key() }

    /// COSE_Key (EC2, P-256, ES256) of the public key.
    pub fn cose_public_key(&self) -> Vec<u8> {
        let p = self.verifying_key().to_sec1_point(false);
        let (x, y) = (p.x().unwrap(), p.y().unwrap());
        let mut c = Cbor::default();
        c.map(5);
        c.int(1); c.int(2);     // kty: EC2
        c.int(3); c.int(-7);    // alg: ES256
        c.int(-1); c.int(1);    // crv: P-256
        c.int(-2); c.bytes(x);
        c.int(-3); c.bytes(y);
        c.0
    }
    /// SubjectPublicKeyInfo DER (what `getPublicKey()` returns).
    pub fn spki_der(&self) -> Vec<u8> {
        use p256::pkcs8::EncodePublicKey;
        self.verifying_key().to_public_key_der().map(|d| d.as_bytes().to_vec()).unwrap_or_default()
    }
    /// authenticatorData; `attested` adds the attested credential data (registration).
    pub fn auth_data(&self, attested: bool) -> Vec<u8> {
        let mut out = Sha256::digest(self.rp_id.as_bytes()).to_vec();
        out.push(FLAG_UP | FLAG_UV | FLAG_BE | FLAG_BS | if attested { FLAG_AT } else { 0 });
        out.extend_from_slice(&self.sign_count.to_be_bytes());
        if attested {
            out.extend_from_slice(&AAGUID);
            out.extend_from_slice(&(self.credential_id.len() as u16).to_be_bytes());
            out.extend_from_slice(&self.credential_id);
            out.extend_from_slice(&self.cose_public_key());
        }
        out
    }
    /// attestationObject with `fmt: "none"`.
    pub fn attestation_object(&self) -> Vec<u8> {
        let ad = self.auth_data(true);
        let mut c = Cbor::default();
        c.map(3);
        c.text("fmt"); c.text("none");
        c.text("attStmt"); c.map(0);
        c.text("authData"); c.bytes(&ad);
        c.0
    }
    /// ES256 signature (ASN.1 DER) over `auth_data || SHA-256(client_data_json)`.
    pub fn sign_assertion(&self, auth_data: &[u8], client_data_json: &[u8]) -> Vec<u8> {
        let mut msg = auth_data.to_vec();
        msg.extend_from_slice(&Sha256::digest(client_data_json));
        let sig: Signature = self.key.sign(&msg);
        sig.to_der().as_bytes().to_vec()
    }

    pub fn to_record(&self, title: &str) -> Result<ItemRecord> {
        let pk = self.key.to_pkcs8_der().map_err(|e| anyhow!("pkcs8: {e}"))?;
        let f = |id: &str, label: &str, kind: FieldKind, v: String| Field { id: id.into(), label: label.into(), kind, value: Some(v), removed: false };
        let mut it = Item::default();
        it.title = title.into();
        it.username = None;
        it.url = Some(format!("https://{}", self.rp_id));
        let mut r = ItemRecord::new(it);
        r.category = Category::Passkey;
        r.sections = vec![Section { id: "main".into(), title: None, fields: vec![
            f(F_RP_ID, "website", FieldKind::Text, self.rp_id.clone()),
            f(F_USER_NAME, "username", FieldKind::Text, self.user_name.clone()),
            f(F_CRED_ID, "credential id", FieldKind::Text, b64(&self.credential_id)),
            f(F_USER_HANDLE, "user handle", FieldKind::Text, b64(&self.user_handle)),
            f(F_PRIVATE, "private key", FieldKind::Concealed, b64(pk.as_bytes())),
            f(F_SIGN_COUNT, "sign count", FieldKind::Text, self.sign_count.to_string()),
        ] }];
        Ok(r)
    }
    pub fn from_record(r: &ItemRecord) -> Result<Self> {
        if r.category != Category::Passkey { bail!("item is not a passkey"); }
        let g = |id: &str| r.find_field(id).and_then(|f| f.value).unwrap_or_default();
        let key = SigningKey::from_pkcs8_der(&unb64(&g(F_PRIVATE))?).map_err(|e| anyhow!("bad passkey private key: {e}"))?;
        let rp_id = g(F_RP_ID);
        if rp_id.is_empty() { bail!("passkey has no rp_id"); }
        Ok(Passkey { rp_id, credential_id: unb64(&g(F_CRED_ID))?, user_handle: unb64(&g(F_USER_HANDLE))?, user_name: g(F_USER_NAME), key, sign_count: g(F_SIGN_COUNT).trim().parse().unwrap_or(0) })
    }
    /// Set `sign_count` in an existing record (keeps clocks/history semantics to the store).
    pub fn set_sign_count(r: &mut ItemRecord, n: u32) {
        for s in r.sections.iter_mut() { for f in s.fields.iter_mut() { if f.id == F_SIGN_COUNT { f.value = Some(n.to_string()); return; } } }
        if let Some(s) = r.sections.first_mut() { s.fields.push(Field { id: F_SIGN_COUNT.into(), label: "sign count".into(), kind: FieldKind::Text, value: Some(n.to_string()), removed: false }); }
    }
}

/// Minimal deterministic CBOR writer (only what WebAuthn needs).
#[derive(Default)]
pub struct Cbor(pub Vec<u8>);
impl Cbor {
    fn head(&mut self, major: u8, n: u64) {
        let m = major << 5;
        match n {
            0..=23 => self.0.push(m | n as u8),
            24..=0xff => { self.0.push(m | 24); self.0.push(n as u8) }
            0x100..=0xffff => { self.0.push(m | 25); self.0.extend_from_slice(&(n as u16).to_be_bytes()) }
            0x10000..=0xffff_ffff => { self.0.push(m | 26); self.0.extend_from_slice(&(n as u32).to_be_bytes()) }
            _ => { self.0.push(m | 27); self.0.extend_from_slice(&n.to_be_bytes()) }
        }
    }
    pub fn int(&mut self, v: i64) { if v >= 0 { self.head(0, v as u64) } else { self.head(1, (-1 - v) as u64) } }
    pub fn bytes(&mut self, b: &[u8]) { self.head(2, b.len() as u64); self.0.extend_from_slice(b) }
    pub fn text(&mut self, s: &str) { self.head(3, s.len() as u64); self.0.extend_from_slice(s.as_bytes()) }
    pub fn map(&mut self, n: u64) { self.head(5, n) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Verifier;
    #[test]
    fn assertion_signature_verifies_with_cose_key() {
        let mut pk = Passkey::generate("example.com", b"user-1", "alice");
        pk.sign_count = 7;
        let ad = pk.auth_data(false);
        assert_eq!(&ad[..32], Sha256::digest(b"example.com").as_slice());
        assert_eq!(&ad[33..37], &7u32.to_be_bytes());
        let cdj = br#"{"type":"webauthn.get","challenge":"abc","origin":"https://example.com"}"#;
        let der = pk.sign_assertion(&ad, cdj);
        // verify using only the COSE key bytes (as a relying party would)
        let cose = pk.cose_public_key();
        let x = &cose[cose.len() - 32 - 3 - 32..cose.len() - 32 - 3]; let y = &cose[cose.len() - 32..];
        let mut sec1 = vec![4u8]; sec1.extend_from_slice(x); sec1.extend_from_slice(y);
        let vk = VerifyingKey::from_sec1_bytes(&sec1).unwrap();
        let mut msg = ad.clone(); msg.extend_from_slice(&Sha256::digest(cdj));
        vk.verify(&msg, &Signature::from_der(&der).unwrap()).unwrap();
        msg[0] ^= 1;
        assert!(vk.verify(&msg, &Signature::from_der(&der).unwrap()).is_err());
    }
    #[test]
    fn record_roundtrip() {
        let pk = Passkey::generate("example.com", b"uh", "bob");
        let r = pk.to_record("example.com").unwrap();
        let back = Passkey::from_record(&r).unwrap();
        assert_eq!(back.credential_id, pk.credential_id);
        assert_eq!(back.verifying_key(), pk.verifying_key());
        let ao = pk.attestation_object();
        assert_eq!(ao[0], 0xa3);
        assert!(ao.windows(4).any(|w| w == b"none"));
    }
}
