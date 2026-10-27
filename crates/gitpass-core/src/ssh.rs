//! SSH keys (item category `ssh_key`) and the ssh-agent wire protocol.
//!
//! Item layout (section `main`, unchanged from v5 templates): `private_key` (concealed, unencrypted
//! OpenSSH PEM: the vault itself is the encryption layer), `public_key` (OpenSSH line), `fingerprint`
//! (`SHA256:...`), `key_type` (`ssh-ed25519` / `ecdsa-sha2-nistp256` ...).
//!
//! Agent protocol (draft-miller-ssh-agent): REQUEST_IDENTITIES(11) -> IDENTITIES_ANSWER(12),
//! SIGN_REQUEST(13) -> SIGN_RESPONSE(14), anything else -> FAILURE(5). Supported key types:
//! Ed25519 and ECDSA (P-256/P-384). RSA keys are rejected on import.
use crate::item::{Category, Field, FieldKind, Section};
use crate::merge::ItemRecord;
use crate::store::{Item, Session};
use anyhow::{anyhow, bail, Context, Result};
use ssh_encoding::Encode;
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey};
use std::io::{Read, Write};
use zeroize::Zeroizing;

pub const F_PRIVATE: &str = "private_key";
pub const F_PUBLIC: &str = "public_key";
pub const F_FINGERPRINT: &str = "fingerprint";
pub const F_KEY_TYPE: &str = "key_type";

/// Public facts about a key (safe to display).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct KeyInfo { pub key_type: String, pub public_key: String, pub fingerprint: String, pub comment: String }

/// A parsed, decrypted private key plus its display info.
pub struct SshKey { pub key: PrivateKey, pub info: KeyInfo }

fn check_alg(a: &Algorithm) -> Result<()> {
    match a {
        Algorithm::Ed25519 | Algorithm::Ecdsa { .. } => Ok(()),
        other => bail!("unsupported SSH key type {} (gitpass supports ed25519 and ecdsa)", other.as_str()),
    }
}

impl SshKey {
    fn from_key(key: PrivateKey) -> Result<Self> {
        check_alg(&key.algorithm())?;
        let p = key.public_key();
        let info = KeyInfo { key_type: key.algorithm().as_str().to_string(), public_key: p.to_openssh()?.trim().to_string(),
            fingerprint: p.fingerprint(HashAlg::Sha256).to_string(), comment: key.comment().to_string() };
        Ok(SshKey { key, info })
    }
    /// Fresh Ed25519 key from the OS RNG.
    pub fn generate_ed25519(comment: &str) -> Result<Self> {
        let mut k = PrivateKey::random(&mut rand::rngs::OsRng, Algorithm::Ed25519).map_err(|e| anyhow!("keygen: {e}"))?;
        k.set_comment(comment);
        Self::from_key(k)
    }
    /// Parse an OpenSSH private key (`-----BEGIN OPENSSH PRIVATE KEY-----`), decrypting it with
    /// `passphrase` when it is encrypted.
    pub fn import_openssh(pem: &str, passphrase: Option<&str>) -> Result<Self> {
        let k = PrivateKey::from_openssh(pem.trim()).map_err(|e| anyhow!("not an OpenSSH private key: {e}"))?;
        let k = if k.is_encrypted() {
            let p = passphrase.filter(|p| !p.is_empty()).ok_or_else(|| anyhow!("this key is encrypted; a passphrase is required"))?;
            k.decrypt(p).map_err(|_| anyhow!("wrong passphrase for the SSH key"))?
        } else { k };
        Self::from_key(k)
    }
    /// Unencrypted OpenSSH PEM (stored inside the encrypted item).
    pub fn private_pem(&self) -> Result<Zeroizing<String>> { self.key.to_openssh(LineEnding::LF).map_err(|e| anyhow!("encode: {e}")) }
    /// SSH wire-format public key blob (what the agent advertises).
    pub fn public_blob(&self) -> Result<Vec<u8>> { self.key.public_key().to_bytes().map_err(|e| anyhow!("encode: {e}")) }
    /// Sign `data`; returns the SSH signature blob (`string alg || string sig`).
    pub fn sign(&self, data: &[u8]) -> Result<Vec<u8>> {
        let sig: ssh_key::Signature = signature::Signer::try_sign(&self.key, data).map_err(|e| anyhow!("sign: {e}"))?;
        let mut v = Vec::new(); sig.encode(&mut v).map_err(|e| anyhow!("encode: {e}"))?; Ok(v)
    }
    /// Build a v5 item record for this key.
    pub fn to_record(&self, title: &str) -> Result<ItemRecord> {
        let f = |id: &str, label: &str, kind: FieldKind, v: String| Field { id: id.into(), label: label.into(), kind, value: Some(v), removed: false };
        let mut it = Item::default(); it.title = title.into();
        let mut r = ItemRecord::new(it);
        r.category = Category::SshKey;
        r.sections = vec![Section { id: "main".into(), title: None, fields: vec![
            f(F_PRIVATE, "private key", FieldKind::Concealed, self.private_pem()?.to_string()),
            f(F_PUBLIC, "public key", FieldKind::Multiline, self.info.public_key.clone()),
            f(F_FINGERPRINT, "fingerprint", FieldKind::Text, self.info.fingerprint.clone()),
            f(F_KEY_TYPE, "key type", FieldKind::Text, self.info.key_type.clone()),
        ] }];
        Ok(r)
    }
    /// Parse the private key stored in an `ssh_key` record.
    pub fn from_record(r: &ItemRecord) -> Result<Self> {
        if r.category != Category::SshKey { bail!("item is not an SSH key"); }
        let pem = r.find_field(F_PRIVATE).and_then(|f| f.value).filter(|v| !v.trim().is_empty()).ok_or_else(|| anyhow!("SSH key item has no private key"))?;
        Self::import_openssh(&pem, None)
    }
}

/// Public info of an OpenSSH public key line (`ssh-ed25519 AAAA... comment`).
pub fn public_info(line: &str) -> Result<KeyInfo> {
    let p = ssh_key::PublicKey::from_openssh(line.trim()).map_err(|e| anyhow!("bad public key: {e}"))?;
    Ok(KeyInfo { key_type: p.algorithm().as_str().into(), public_key: p.to_openssh()?.trim().into(), fingerprint: p.fingerprint(HashAlg::Sha256).to_string(), comment: p.comment().into() })
}

/// One agent identity backed by a vault item.
pub struct Identity { pub vault_id: String, pub item_id: String, pub title: String, pub key: SshKey, pub blob: Vec<u8> }

/// All usable SSH keys in the session (live items only; unparseable keys are skipped).
pub fn session_identities(s: &Session) -> Result<Vec<Identity>> {
    let mut out = vec![];
    for (vid, _) in s.list_vaults()? {
        for (iid, _, r) in s.list_records(&vid)? {
            if r.category != Category::SshKey || r.deleted_at.is_some() || r.archived { continue; }
            if let Ok(k) = SshKey::from_record(&r) {
                if let Ok(blob) = k.public_blob() { out.push(Identity { vault_id: vid.clone(), item_id: iid, title: r.item.title.clone(), key: k, blob }); }
            }
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ agent protocol
pub const SSH_AGENT_FAILURE: u8 = 5;
pub const SSH_AGENTC_REQUEST_IDENTITIES: u8 = 11;
pub const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
pub const SSH_AGENTC_SIGN_REQUEST: u8 = 13;
pub const SSH_AGENT_SIGN_RESPONSE: u8 = 14;
/// Max accepted agent message (OpenSSH uses 256 KiB).
pub const MAX_AGENT_MSG: usize = 256 * 1024;

/// What the agent serves. Implementations decide lock state and per-use approval.
pub trait KeyProvider: Send + Sync {
    /// (public blob, comment). Empty when locked.
    fn identities(&self) -> Vec<(Vec<u8>, String)>;
    /// Sign with the key whose public blob is `blob`. `Err` -> SSH_AGENT_FAILURE (locked, denied, unknown key).
    fn sign(&self, blob: &[u8], data: &[u8], flags: u32) -> Result<Vec<u8>>;
}

fn put_str(out: &mut Vec<u8>, b: &[u8]) { out.extend_from_slice(&(b.len() as u32).to_be_bytes()); out.extend_from_slice(b); }
fn get_u32(b: &[u8], at: &mut usize) -> Result<u32> {
    let s = b.get(*at..*at + 4).context("truncated")?; *at += 4; Ok(u32::from_be_bytes(s.try_into().unwrap()))
}
fn get_str<'a>(b: &'a [u8], at: &mut usize) -> Result<&'a [u8]> {
    let n = get_u32(b, at)? as usize; let s = b.get(*at..*at + n).context("truncated")?; *at += n; Ok(s)
}

/// Handle one agent message body (without the length prefix); returns the reply body.
pub fn handle_message(msg: &[u8], p: &dyn KeyProvider) -> Vec<u8> {
    let fail = vec![SSH_AGENT_FAILURE];
    match msg.first().copied() {
        Some(SSH_AGENTC_REQUEST_IDENTITIES) => {
            let ids = p.identities();
            let mut out = vec![SSH_AGENT_IDENTITIES_ANSWER];
            out.extend_from_slice(&(ids.len() as u32).to_be_bytes());
            for (blob, comment) in ids { put_str(&mut out, &blob); put_str(&mut out, comment.as_bytes()); }
            out
        }
        Some(SSH_AGENTC_SIGN_REQUEST) => {
            let mut at = 1;
            let parsed = (|| -> Result<(Vec<u8>, Vec<u8>, u32)> { let k = get_str(msg, &mut at)?.to_vec(); let d = get_str(msg, &mut at)?.to_vec(); let f = get_u32(msg, &mut at).unwrap_or(0); Ok((k, d, f)) })();
            match parsed.and_then(|(k, d, f)| p.sign(&k, &d, f)) {
                Ok(sig) => { let mut out = vec![SSH_AGENT_SIGN_RESPONSE]; put_str(&mut out, &sig); out }
                Err(_) => fail,
            }
        }
        _ => fail,
    }
}

/// Serve one agent connection until EOF.
pub fn serve_stream<S: Read + Write>(s: &mut S, p: &dyn KeyProvider) -> Result<()> {
    loop {
        let mut len = [0u8; 4];
        match s.read_exact(&mut len) { Ok(()) => {}, Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()), Err(e) => return Err(e.into()) }
        let n = u32::from_be_bytes(len) as usize;
        if n == 0 || n > MAX_AGENT_MSG { bail!("bad agent message length {n}"); }
        let mut msg = vec![0u8; n];
        s.read_exact(&mut msg)?;
        let reply = handle_message(&msg, p);
        let mut out = (reply.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(&reply);
        s.write_all(&out)?;
        s.flush()?;
    }
}

/// `KeyProvider` over a fixed key list (CLI `gitpass ssh agent`, tests). `approve(title, fingerprint)`
/// is consulted before every signature.
pub type ApprovalFn = Box<dyn Fn(&str, &str) -> bool + Send + Sync>;
pub struct StaticKeys { pub ids: Vec<Identity>, pub approve: ApprovalFn }
impl KeyProvider for StaticKeys {
    fn identities(&self) -> Vec<(Vec<u8>, String)> { self.ids.iter().map(|i| (i.blob.clone(), i.title.clone())).collect() }
    fn sign(&self, blob: &[u8], data: &[u8], _flags: u32) -> Result<Vec<u8>> {
        let id = self.ids.iter().find(|i| i.blob == blob).ok_or_else(|| anyhow!("unknown key"))?;
        if !(self.approve)(&id.title, &id.key.info.fingerprint) { bail!("denied"); }
        id.key.sign(data)
    }
}

/// The `~/.ssh/config` snippet pointing ssh at `socket`.
pub fn identity_agent_line(socket: &std::path::Path) -> String {
    let home = dirs::home_dir().map(|h| h.display().to_string()).unwrap_or_default();
    let s = socket.display().to_string();
    let s = if !home.is_empty() && s.starts_with(&home) { format!("~{}", &s[home.len()..]) } else { s };
    format!("Host *\n  IdentityAgent \"{s}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generate_roundtrip_and_sign_verifies() {
        let k = SshKey::generate_ed25519("me@test").unwrap();
        assert!(k.info.public_key.starts_with("ssh-ed25519 AAAA"));
        assert!(k.info.fingerprint.starts_with("SHA256:"));
        let again = SshKey::import_openssh(&k.private_pem().unwrap(), None).unwrap();
        assert_eq!(again.info, k.info);
        let sig = k.sign(b"hello").unwrap();
        let sig = <ssh_key::Signature as ssh_encoding::Decode>::decode(&mut &sig[..]).unwrap();
        signature::Verifier::verify(k.key.public_key().key_data(), b"hello", &sig).unwrap();
        assert!(signature::Verifier::verify(k.key.public_key().key_data(), b"hellO", &sig).is_err());
    }
    #[test]
    fn protocol_identities_and_sign() {
        let k = SshKey::generate_ed25519("c").unwrap();
        let blob = k.public_blob().unwrap();
        let p = StaticKeys { ids: vec![Identity { vault_id: "v".into(), item_id: "i".into(), title: "t".into(), key: k, blob: blob.clone() }], approve: Box::new(|_, _| true) };
        let r = handle_message(&[SSH_AGENTC_REQUEST_IDENTITIES], &p);
        assert_eq!(r[0], SSH_AGENT_IDENTITIES_ANSWER);
        assert_eq!(&r[1..5], &1u32.to_be_bytes());
        let mut req = vec![SSH_AGENTC_SIGN_REQUEST]; put_str(&mut req, &blob); put_str(&mut req, b"data"); req.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(handle_message(&req, &p)[0], SSH_AGENT_SIGN_RESPONSE);
        let mut bad = vec![SSH_AGENTC_SIGN_REQUEST]; put_str(&mut bad, b"nope"); put_str(&mut bad, b"data");
        assert_eq!(handle_message(&bad, &p), vec![SSH_AGENT_FAILURE]);
        assert_eq!(handle_message(&[99], &p), vec![SSH_AGENT_FAILURE]);
        let deny = StaticKeys { ids: p.ids, approve: Box::new(|_, _| false) };
        assert_eq!(handle_message(&req, &deny), vec![SSH_AGENT_FAILURE]);
    }
    #[test]
    fn rejects_garbage_and_wrong_passphrase() {
        assert!(SshKey::import_openssh("nope", None).is_err());
    }
}
