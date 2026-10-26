//! Format v4 public-key primitives: X25519 keypairs (via `x25519-dalek`, no homemade curve math),
//! fingerprints, and an anonymous "sealed box" (ephemeral X25519 + HKDF-SHA256 + XChaCha20-Poly1305).
//!
//! A sealed box authenticates the *ciphertext* under the recipient's key, but NOT the sender: anyone who
//! knows a recipient public key can produce a valid box. Sender identity must be checked out of band.
use crate::crypto::{hkdf32, open, random_bytes, seal, Envelope};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

pub const SEALBOX_ALG: &str = "x25519-hkdf-sha256-xchacha20poly1305";
/// Text prefix used when printing/parsing a public key.
pub const PUB_PREFIX: &str = "gitpass-x25519:";

/// X25519 keypair; the secret is zeroized on drop by x25519-dalek (`zeroize` feature).
pub struct KeyPair {
    secret: StaticSecret,
    pub public: [u8; 32],
}

impl KeyPair {
    pub fn generate() -> Self { Self::from_secret_bytes(random_bytes::<32>()) }
    pub fn from_secret_bytes(mut b: [u8; 32]) -> Self {
        let secret = StaticSecret::from(b);
        zeroize::Zeroize::zeroize(&mut b);
        let public = PublicKey::from(&secret).to_bytes();
        KeyPair { secret, public }
    }
    pub fn secret_bytes(&self) -> Zeroizing<[u8; 32]> { Zeroizing::new(self.secret.to_bytes()) }
    pub fn fingerprint(&self) -> String { fingerprint(&self.public) }
}

/// 128-bit fingerprint: hex(blake3("gitpass/fpr/v4" || pk))[..32].
pub fn fingerprint(pk: &[u8; 32]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"gitpass/fpr/v4");
    h.update(pk);
    h.finalize().to_hex()[..32].to_string()
}

pub fn encode_pub(pk: &[u8; 32]) -> String { format!("{PUB_PREFIX}{}", B64.encode(pk)) }

/// Parse a public key from `gitpass-x25519:<b64>`, bare base64, or an `identity.pub` JSON document.
pub fn parse_pub(s: &str) -> Result<[u8; 32]> {
    let s = s.trim();
    if s.starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(s).context("parsing public key JSON")?;
        let p = v.get("public").and_then(|p| p.as_str()).ok_or_else(|| anyhow!("JSON has no \"public\" field"))?;
        return parse_pub(p);
    }
    let b = B64.decode(s.strip_prefix(PUB_PREFIX).unwrap_or(s)).context("public key is not valid base64")?;
    let pk: [u8; 32] = b.try_into().map_err(|_| anyhow!("public key must be 32 bytes"))?;
    if pk == [0u8; 32] { bail!("all-zero public key rejected"); }
    Ok(pk)
}

/// Read a public key from a file path (if it exists) or treat the argument as the key text.
pub fn load_pub(arg: &str) -> Result<[u8; 32]> {
    let p = std::path::Path::new(arg);
    if p.is_file() { return parse_pub(&std::fs::read_to_string(p)?); }
    parse_pub(arg)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SealedBox {
    pub alg: String,
    /// Ephemeral X25519 public key (base64).
    pub eph: String,
    pub envelope: Envelope,
}

fn box_key(shared: &[u8; 32], eph: &[u8; 32], recipient: &[u8; 32]) -> crate::crypto::Key32 {
    let mut salt = [0u8; 64];
    salt[..32].copy_from_slice(eph);
    salt[32..].copy_from_slice(recipient);
    hkdf32(shared, Some(&salt), b"gitpass/sealbox/v4")
}

/// Encrypt `pt` to `recipient` with a fresh ephemeral key. `aad` is authenticated, not encrypted.
pub fn seal_to(recipient: &[u8; 32], pt: &[u8], aad: &[u8]) -> Result<SealedBox> {
    let eph = KeyPair::generate();
    let shared = eph.secret.diffie_hellman(&PublicKey::from(*recipient));
    if !shared.was_contributory() { bail!("recipient public key is a low-order point; refusing"); }
    let k = box_key(shared.as_bytes(), &eph.public, recipient);
    Ok(SealedBox { alg: SEALBOX_ALG.into(), eph: B64.encode(eph.public), envelope: seal(&k, pt, aad)? })
}

pub fn open_sealed(kp: &KeyPair, b: &SealedBox, aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    if b.alg != SEALBOX_ALG { bail!("unsupported sealed-box algorithm {}", b.alg); }
    let eph: [u8; 32] = B64.decode(&b.eph)?.try_into().map_err(|_| anyhow!("bad ephemeral key length"))?;
    let shared = kp.secret.diffie_hellman(&PublicKey::from(eph));
    if !shared.was_contributory() { bail!("low-order ephemeral key; refusing"); }
    let k = box_key(shared.as_bytes(), &eph, &kp.public);
    Ok(Zeroizing::new(open(&k, &b.envelope, aad).map_err(|_| anyhow!("sealed box failed authentication (wrong recipient or tampered)"))?))
}
