//! Crypto primitives: two-secret key derivation + XChaCha20-Poly1305 envelopes.
use anyhow::{anyhow, bail, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// 32-byte key wiped on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Key32(pub [u8; 32]);

impl Key32 {
    pub fn random() -> Self {
        let mut k = [0u8; 32];
        OsRng.fill_bytes(&mut k);
        Key32(k)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct KdfParams {
    pub alg: String,
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        KdfParams { alg: "argon2id".into(), m_kib: 65536, t: 3, p: 4 }
    }
}

/// Secret Key: 128 random bits, displayed as A3-XXXXXX-XXXXXX-XXXXX-XXXXX-XXXXX
/// (26 base32 data chars + 1 checksum char).
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey(pub [u8; 16]);

const ALPHA: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTVWXYZ0"; // no I,O,1,U

impl SecretKey {
    pub fn generate() -> Self {
        let mut b = [0u8; 16];
        OsRng.fill_bytes(&mut b);
        SecretKey(b)
    }

    pub fn encode(&self) -> String {
        let mut v = u128::from_be_bytes(self.0);
        let mut idx = [0u8; 27];
        for i in 0..26 {
            idx[25 - i] = (v & 31) as u8;
            v >>= 5;
        }
        idx[26] = (idx[..26].iter().map(|&x| x as u32).sum::<u32>() % 32) as u8;
        let c: String = idx.iter().map(|&i| ALPHA[i as usize] as char).collect();
        let out = format!("A3-{}-{}-{}-{}-{}", &c[0..6], &c[6..12], &c[12..17], &c[17..22], &c[22..27]);
        idx.zeroize();
        out
    }

    pub fn parse(s: &str) -> Result<Self> {
        let s: String = s.trim().to_uppercase().chars().filter(|c| *c != '-' && !c.is_whitespace()).collect();
        let body = s.strip_prefix("A3").ok_or_else(|| anyhow!("secret key must start with A3"))?;
        if body.len() != 27 { bail!("secret key has wrong length"); }
        let mut idx = Vec::with_capacity(27);
        for ch in body.bytes() {
            let p = ALPHA.iter().position(|&a| a == ch).ok_or_else(|| anyhow!("invalid character in secret key"))?;
            idx.push(p as u8);
        }
        let sum = (idx[..26].iter().map(|&x| x as u32).sum::<u32>() % 32) as u8;
        if sum != idx[26] { bail!("secret key checksum mismatch (typo?)"); }
        if idx[0] >= 8 { bail!("secret key out of range"); } // first char carries 3 bits (128 = 3 + 25*5)
        let mut v: u128 = 0;
        for &i in &idx[..26] { v = (v << 5) | i as u128; }
        idx.zeroize();
        Ok(SecretKey(v.to_be_bytes()))
    }
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    OsRng.fill_bytes(&mut b);
    b
}

/// AUK = Argon2id(password, salt) XOR HKDF-SHA256(secret_key, account_id, "gitpass-sk-v1")
pub fn derive_auk(password: &str, secret: &SecretKey, salt: &[u8], account_id: &str, kdf: &KdfParams) -> Result<Key32> {
    if kdf.alg != "argon2id" { bail!("unsupported kdf {}", kdf.alg); }
    let params = Params::new(kdf.m_kib, kdf.t, kdf.p, Some(32)).map_err(|e| anyhow!("argon2 params: {e}"))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut pw_key = Key32([0u8; 32]);
    argon.hash_password_into(password.as_bytes(), salt, &mut pw_key.0).map_err(|e| anyhow!("argon2: {e}"))?;

    let hk = Hkdf::<Sha256>::new(Some(account_id.as_bytes()), &secret.0);
    let mut sk_key = Key32([0u8; 32]);
    hk.expand(b"gitpass-sk-v1", &mut sk_key.0).map_err(|_| anyhow!("hkdf expand"))?;

    let mut auk = Key32([0u8; 32]);
    for i in 0..32 { auk.0[i] = pw_key.0[i] ^ sk_key.0[i]; }
    Ok(auk) // pw_key / sk_key zeroized on drop
}

/// Serialized AEAD envelope.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Envelope {
    pub alg: String,
    pub nonce: String,
    pub ct: String,
}

pub fn seal(key: &Key32, plaintext: &[u8], aad: &[u8]) -> Result<Envelope> {
    let cipher = XChaCha20Poly1305::new((&key.0).into());
    let nonce: [u8; 24] = random_bytes();
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad })
        .map_err(|_| anyhow!("encryption failed"))?;
    Ok(Envelope { alg: "xchacha20poly1305".into(), nonce: B64.encode(nonce), ct: B64.encode(ct) })
}

pub fn open(key: &Key32, env: &Envelope, aad: &[u8]) -> Result<Vec<u8>> {
    if env.alg != "xchacha20poly1305" { bail!("unsupported cipher {}", env.alg); }
    let nonce = B64.decode(&env.nonce)?;
    if nonce.len() != 24 { bail!("bad nonce length"); }
    let ct = B64.decode(&env.ct)?;
    let cipher = XChaCha20Poly1305::new((&key.0).into());
    cipher
        .decrypt(XNonce::from_slice(&nonce), Payload { msg: &ct, aad })
        .map_err(|_| anyhow!("decryption failed: authentication tag mismatch (wrong password/secret key, or tampered data)"))
}

pub fn wrap_key(kek: &Key32, key: &Key32, aad: &[u8]) -> Result<Envelope> { seal(kek, &key.0, aad) }

pub fn unwrap_key(kek: &Key32, env: &Envelope, aad: &[u8]) -> Result<Key32> {
    let mut pt = open(kek, env, aad)?;
    if pt.len() != 32 { pt.zeroize(); bail!("wrapped key has wrong length"); }
    let mut k = Key32([0u8; 32]);
    k.0.copy_from_slice(&pt);
    pt.zeroize();
    Ok(k)
}

/// Password generator using rejection-free uniform sampling via rand.
pub fn generate_password(len: usize) -> String {
    use rand::seq::SliceRandom;
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*-_=+?";
    let mut rng = OsRng;
    (0..len.max(4)).map(|_| *CHARS.choose(&mut rng).unwrap() as char).collect()
}
