//! SSH keys (item category `ssh_key`) and the ssh-agent wire protocol.
//!
//! Item layout (section `main`, unchanged from v5 templates): `private_key` (concealed, unencrypted
//! OpenSSH PEM: the vault itself is the encryption layer), `public_key` (OpenSSH line), `fingerprint`
//! (`SHA256:...`), `key_type` (`ssh-ed25519` / `ecdsa-sha2-nistp256` ...).
//!
//! Agent protocol (draft-miller-ssh-agent): REQUEST_IDENTITIES(11) -> IDENTITIES_ANSWER(12),
//! SIGN_REQUEST(13) -> SIGN_RESPONSE(14), anything else -> FAILURE(5). Supported key types:
//! Ed25519, ECDSA (P-256/P-384), and RSA (2048+ bits). RSA signature requests require
//! SSH_AGENT_RSA_SHA2_256 or SSH_AGENT_RSA_SHA2_512 (SHA-1 ssh-rsa is refused).
use crate::item::{Category, Field, FieldKind, Section};
use crate::merge::ItemRecord;
use crate::store::{Item, Session};
use anyhow::{anyhow, bail, Context, Result};
use signature::{SignatureEncoding, Signer};
use ssh_encoding::Encode;
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey};
use std::io::{Read, Write};
use zeroize::Zeroizing;

pub const F_PRIVATE: &str = "private_key";
pub const F_PUBLIC: &str = "public_key";
pub const F_FINGERPRINT: &str = "fingerprint";
pub const F_KEY_TYPE: &str = "key_type";
pub const F_BIT_LENGTH: &str = "bit_length";

pub const SSH_AGENT_RSA_SHA2_256: u32 = 0x02;
pub const SSH_AGENT_RSA_SHA2_512: u32 = 0x04;

/// Calculates bit length from an RSA modulus `n`.
pub fn rsa_bit_length(n: &ssh_key::Mpint) -> usize {
    let bytes = match n.as_positive_bytes() {
        Some(b) => b,
        None => return 0,
    };
    let mut bytes = bytes;
    while let Some((&0, rest)) = bytes.split_first() {
        bytes = rest;
    }
    if let Some((first, rest)) = bytes.split_first() {
        rest.len() * 8 + (8 - first.leading_zeros() as usize)
    } else {
        0
    }
}

/// Public facts about a key (safe to display).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct KeyInfo {
    pub key_type: String,
    pub public_key: String,
    pub fingerprint: String,
    pub comment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_length: Option<usize>,
}

/// A parsed, decrypted private key plus its display info.
#[derive(Debug)]
pub struct SshKey { pub key: PrivateKey, pub info: KeyInfo }

fn check_alg(key: &PrivateKey) -> Result<()> {
    match key.algorithm() {
        Algorithm::Ed25519 | Algorithm::Ecdsa { .. } => Ok(()),
        Algorithm::Rsa { .. } => {
            let rsa = key
                .public_key()
                .key_data()
                .rsa()
                .ok_or_else(|| anyhow!("missing RSA public key data"))?;
            let bits = rsa_bit_length(&rsa.n);
            if bits < 2048 {
                bail!("RSA key is too short ({bits} bits; gitpass requires at least 2048 bits)");
            }
            Ok(())
        }
        other => bail!(
            "unsupported SSH key type {} (gitpass supports ed25519, ecdsa, and rsa)",
            other.as_str()
        ),
    }
}

fn rsa_signing_key<D: sha2::Digest + sha2::digest::const_oid::AssociatedOid>(
    keypair: &ssh_key::private::RsaKeypair,
) -> Result<rsa::pkcs1v15::SigningKey<D>> {
    let n = rsa::BigUint::from_bytes_be(keypair.public.n.as_positive_bytes().ok_or_else(|| anyhow!("invalid modulus"))?);
    let e = rsa::BigUint::from_bytes_be(keypair.public.e.as_positive_bytes().ok_or_else(|| anyhow!("invalid exponent"))?);
    let d = rsa::BigUint::from_bytes_be(keypair.private.d.as_positive_bytes().ok_or_else(|| anyhow!("invalid d"))?);
    let p = rsa::BigUint::from_bytes_be(keypair.private.p.as_positive_bytes().ok_or_else(|| anyhow!("invalid p"))?);
    let q = rsa::BigUint::from_bytes_be(keypair.private.q.as_positive_bytes().ok_or_else(|| anyhow!("invalid q"))?);
    let priv_key = rsa::RsaPrivateKey::from_components(n, e, d, vec![p, q])
        .map_err(|e| anyhow!("RSA components: {e}"))?;
    Ok(rsa::pkcs1v15::SigningKey::new(priv_key))
}

impl SshKey {
    fn from_key(key: PrivateKey) -> Result<Self> {
        check_alg(&key)?;
        let p = key.public_key();
        let bit_length = match p.key_data() {
            ssh_key::public::KeyData::Rsa(rsa) => Some(rsa_bit_length(&rsa.n)),
            _ => None,
        };
        let info = KeyInfo {
            key_type: key.algorithm().as_str().to_string(),
            public_key: p.to_openssh()?.trim().to_string(),
            fingerprint: p.fingerprint(HashAlg::Sha256).to_string(),
            comment: key.comment().to_string(),
            bit_length,
        };
        Ok(SshKey { key, info })
    }
    /// Bit length for RSA keys (and other variable-length keys), or None.
    pub fn bit_length(&self) -> Option<usize> {
        self.info.bit_length
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
    /// Sign `data` using agent flags (e.g. `SSH_AGENT_RSA_SHA2_256`, `SSH_AGENT_RSA_SHA2_512`).
    /// Refuses plain SHA-1 ssh-rsa requests.
    pub fn sign_with_flags(&self, data: &[u8], flags: u32) -> Result<Vec<u8>> {
        if self.key.algorithm().is_rsa() {
            let rsa = self.key.key_data().rsa().ok_or_else(|| anyhow!("missing RSA keypair data"))?;
            let sig = if flags & SSH_AGENT_RSA_SHA2_512 != 0 {
                let signing_key = rsa_signing_key::<sha2::Sha512>(rsa)?;
                let data = signing_key.try_sign(data).map_err(|e| anyhow!("sign: {e}"))?;
                ssh_key::Signature::new(Algorithm::Rsa { hash: Some(HashAlg::Sha512) }, data.to_vec())
                    .map_err(|e| anyhow!("signature encoding: {e}"))?
            } else if flags & SSH_AGENT_RSA_SHA2_256 != 0 {
                let signing_key = rsa_signing_key::<sha2::Sha256>(rsa)?;
                let data = signing_key.try_sign(data).map_err(|e| anyhow!("sign: {e}"))?;
                ssh_key::Signature::new(Algorithm::Rsa { hash: Some(HashAlg::Sha256) }, data.to_vec())
                    .map_err(|e| anyhow!("signature encoding: {e}"))?
            } else {
                bail!("refusing plain ssh-rsa (SHA-1) signature request: agent client must request rsa-sha2-256 or rsa-sha2-512");
            };
            let mut v = Vec::new();
            sig.encode(&mut v).map_err(|e| anyhow!("encode: {e}"))?;
            Ok(v)
        } else {
            self.sign(data)
        }
    }
    /// Verify an SSH signature blob (`sig_blob`) for `data` against this key's public key.
    pub fn verify(&self, data: &[u8], sig_blob: &[u8]) -> Result<()> {
        let sig = <ssh_key::Signature as ssh_encoding::Decode>::decode(&mut &sig_blob[..]).map_err(|e| anyhow!("decode signature: {e}"))?;
        signature::Verifier::verify(self.key.public_key().key_data(), data, &sig).map_err(|e| anyhow!("verify signature: {e}"))
    }
    /// Build a v5 item record for this key.
    pub fn to_record(&self, title: &str) -> Result<ItemRecord> {
        let f = |id: &str, label: &str, kind: FieldKind, v: String| Field { id: id.into(), label: label.into(), kind, value: Some(v), removed: false };
        let mut it = Item::default(); it.title = title.into();
        let mut r = ItemRecord::new(it);
        r.category = Category::SshKey;
        let mut fields = vec![
            f(F_PRIVATE, "private key", FieldKind::Concealed, self.private_pem()?.to_string()),
            f(F_PUBLIC, "public key", FieldKind::Multiline, self.info.public_key.clone()),
            f(F_FINGERPRINT, "fingerprint", FieldKind::Text, self.info.fingerprint.clone()),
            f(F_KEY_TYPE, "key type", FieldKind::Text, self.info.key_type.clone()),
        ];
        if let Some(bits) = self.bit_length() {
            fields.push(f(F_BIT_LENGTH, "bit length", FieldKind::Text, bits.to_string()));
        }
        r.sections = vec![Section { id: "main".into(), title: None, fields }];
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
    let bit_length = match p.key_data() {
        ssh_key::public::KeyData::Rsa(rsa) => Some(rsa_bit_length(&rsa.n)),
        _ => None,
    };
    Ok(KeyInfo {
        key_type: p.algorithm().as_str().into(),
        public_key: p.to_openssh()?.trim().into(),
        fingerprint: p.fingerprint(HashAlg::Sha256).to_string(),
        comment: p.comment().into(),
        bit_length,
    })
}

/// Parse an OpenSSH public key line (`ssh-ed25519 AAAA...`) and return its wire-format public blob.
pub fn public_blob_from_openssh(line: &str) -> Result<Vec<u8>> {
    let p = ssh_key::PublicKey::from_openssh(line.trim()).map_err(|e| anyhow!("bad public key: {e}"))?;
    p.to_bytes().map_err(|e| anyhow!("encode public key blob: {e}"))
}

/// Verify an SSH signature blob (`sig_blob`) for `data` against an OpenSSH wire-format public key blob.
pub fn verify_blob(blob: &[u8], data: &[u8], sig_blob: &[u8]) -> Result<()> {
    let p = ssh_key::PublicKey::from_bytes(blob).map_err(|e| anyhow!("parse public key blob: {e}"))?;
    let sig = <ssh_key::Signature as ssh_encoding::Decode>::decode(&mut &sig_blob[..]).map_err(|e| anyhow!("decode signature: {e}"))?;
    signature::Verifier::verify(p.key_data(), data, &sig).map_err(|e| anyhow!("verify signature: {e}"))
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
    fn sign(&self, blob: &[u8], data: &[u8], flags: u32) -> Result<Vec<u8>> {
        let id = self.ids.iter().find(|i| i.blob == blob).ok_or_else(|| anyhow!("unknown key"))?;
        if !(self.approve)(&id.title, &id.key.info.fingerprint) { bail!("denied"); }
        id.key.sign_with_flags(data, flags)
    }
}

/// The `~/.ssh/config` snippet pointing ssh at `socket`.
pub fn identity_agent_line(socket: &std::path::Path) -> String {
    let home = dirs::home_dir().map(|h| h.display().to_string()).unwrap_or_default();
    let s = socket.display().to_string();
    let s = if !home.is_empty() && s.starts_with(&home) { format!("~{}", &s[home.len()..]) } else { s };
    format!("Host *\n  IdentityAgent \"{s}\"")
}

// ------------------------------------------------------------------ Git commit signing (v0.7)

/// Check whether the key type is supported for Git commit and tag signing.
/// Ed25519 and RSA (2048+ bits) are supported. Other key types (e.g. ECDSA) are refused.
pub fn check_git_signing_support(key: &SshKey) -> Result<()> {
    match key.key.algorithm() {
        Algorithm::Ed25519 => Ok(()),
        Algorithm::Rsa { .. } => Ok(()),
        other => bail!(
            "Git commit signing supports Ed25519 and RSA keys; found {}",
            other.as_str()
        ),
    }
}

/// Default allowed_signers path (`~/.ssh/allowed_signers`).
pub fn default_allowed_signers_path() -> std::path::PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".ssh/allowed_signers"))
        .unwrap_or_else(|| std::path::PathBuf::from(".ssh/allowed_signers"))
}

/// Display a path using `~` prefix if within the user's home directory.
pub fn display_path(path: &std::path::Path) -> String {
    let home = dirs::home_dir().map(|h| h.display().to_string()).unwrap_or_default();
    let s = path.display().to_string();
    if !home.is_empty() && s.starts_with(&home) {
        format!("~{}", &s[home.len()..])
    } else {
        s
    }
}

/// Detect the user's email from `git config --get user.email`.
pub fn detect_git_email() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["config", "--get", "user.email"])
        .output()
        .ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

/// Git commit signing configuration and allowed_signers entry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSigningConfig {
    pub key_type: String,
    pub public_key: String,
    pub signing_key: String,
    pub allowed_signers_file: String,
    pub allowed_signers_line: String,
    pub config_lines: String,
    pub email: String,
}

/// Build the Git signing configuration lines without modifying any files.
pub fn build_git_signing_config(
    key: &SshKey,
    email: &str,
    allowed_signers_path: &std::path::Path,
) -> Result<GitSigningConfig> {
    check_git_signing_support(key)?;

    let pubkey = key.info.public_key.trim();
    let signing_key = format!("key::{pubkey}");
    let allowed_signers_file = display_path(allowed_signers_path);
    let allowed_signers_line = format!("{email} {pubkey}");

    let config_lines = format!(
        "gpg.format=ssh\nuser.signingkey=\"{signing_key}\"\ncommit.gpgsign=true\ntag.gpgsign=true\ngpg.ssh.allowedSignersFile={allowed_signers_file}"
    );

    Ok(GitSigningConfig {
        key_type: key.info.key_type.clone(),
        public_key: pubkey.to_string(),
        signing_key,
        allowed_signers_file,
        allowed_signers_line,
        config_lines,
        email: email.to_string(),
    })
}

/// Write `<email> <public key>` to `allowed_signers_path` and apply `git config` settings.
pub fn apply_git_signing(
    key: &SshKey,
    email: &str,
    allowed_signers_path: &std::path::Path,
    global: bool,
) -> Result<GitSigningConfig> {
    let cfg = build_git_signing_config(key, email, allowed_signers_path)?;

    // 1. Ensure allowed_signers parent directory exists with 0700 permissions
    if let Some(parent) = allowed_signers_path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }

    // 2. Read existing allowed_signers lines to avoid duplicate entries
    let line_to_add = cfg.allowed_signers_line.trim();
    let existing = if allowed_signers_path.exists() {
        std::fs::read_to_string(allowed_signers_path).unwrap_or_default()
    } else {
        String::new()
    };

    let already_present = existing.lines().any(|l| l.trim() == line_to_add);
    if !already_present {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(allowed_signers_path)?;
        if !existing.is_empty() && !existing.ends_with('\n') {
            writeln!(f)?;
        }
        writeln!(f, "{line_to_add}")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(allowed_signers_path, std::fs::Permissions::from_mode(0o600));
        }
    }

    // 3. Apply git config settings
    let scope_arg = if global { "--global" } else { "--local" };
    let git_configs = [
        ("gpg.format", "ssh"),
        ("user.signingkey", &cfg.signing_key),
        ("commit.gpgsign", "true"),
        ("tag.gpgsign", "true"),
        ("gpg.ssh.allowedSignersFile", &cfg.allowed_signers_file),
    ];

    for (k, v) in git_configs {
        let status = std::process::Command::new("git")
            .args(["config", scope_arg, k, v])
            .status()
            .with_context(|| format!("run git config {scope_arg} {k}"))?;
        if !status.success() {
            bail!("git config {scope_arg} {k} failed with status {status}");
        }
    }

    Ok(cfg)
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
        let decoded = <ssh_key::Signature as ssh_encoding::Decode>::decode(&mut &sig[..]).unwrap();
        signature::Verifier::verify(k.key.public_key().key_data(), b"hello", &decoded).unwrap();
        assert!(signature::Verifier::verify(k.key.public_key().key_data(), b"hellO", &decoded).is_err());
        assert!(k.verify(b"hello", &sig).is_ok());
        assert!(k.verify(b"hellO", &sig).is_err());
        let blob = k.public_blob().unwrap();
        assert!(verify_blob(&blob, b"hello", &sig).is_ok());
        assert!(verify_blob(&blob, b"hellO", &sig).is_err());
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
    #[test]
    fn rsa_2048_import_and_signature_flags() {
        use ssh_key::private::{KeypairData, RsaKeypair};
        let rsa_pair = RsaKeypair::random(&mut rand::rngs::OsRng, 2048).unwrap();
        let priv_key = PrivateKey::new(KeypairData::Rsa(rsa_pair), "rsa@gitpass").unwrap();
        let pem = priv_key.to_openssh(LineEnding::LF).unwrap();

        let k = SshKey::import_openssh(&pem, None).unwrap();
        assert_eq!(k.info.key_type, "ssh-rsa");
        assert_eq!(k.info.bit_length, Some(2048));
        assert!(k.info.public_key.starts_with("ssh-rsa AAAA"));
        assert_eq!(k.info.comment, "rsa@gitpass");

        // to_record includes bit length
        let rec = k.to_record("My RSA Key").unwrap();
        assert_eq!(rec.category, Category::SshKey);
        let bit_field = rec.find_field(F_BIT_LENGTH).expect("bit_length field exists");
        assert_eq!(bit_field.value.as_deref(), Some("2048"));

        let payload = b"gitpass rsa test payload";

        // Plain flags=0 (SHA-1) is REFUSED
        assert!(k.sign_with_flags(payload, 0).is_err());

        // SSH_AGENT_RSA_SHA2_256 succeeds and verifies
        let sig256 = k.sign_with_flags(payload, SSH_AGENT_RSA_SHA2_256).unwrap();
        assert!(k.verify(payload, &sig256).is_ok());
        assert!(k.verify(b"wrong", &sig256).is_err());
        let blob = k.public_blob().unwrap();
        assert!(verify_blob(&blob, payload, &sig256).is_ok());

        // SSH_AGENT_RSA_SHA2_512 succeeds and verifies
        let sig512 = k.sign_with_flags(payload, SSH_AGENT_RSA_SHA2_512).unwrap();
        assert!(k.verify(payload, &sig512).is_ok());
        assert!(verify_blob(&blob, payload, &sig512).is_ok());

        // Agent wire protocol testing
        let p = StaticKeys {
            ids: vec![Identity {
                vault_id: "v".into(),
                item_id: "i".into(),
                title: "rsa-title".into(),
                key: k,
                blob: blob.clone(),
            }],
            approve: Box::new(|_, _| true),
        };

        // REQUEST_IDENTITIES advertises the RSA key
        let r = handle_message(&[SSH_AGENTC_REQUEST_IDENTITIES], &p);
        assert_eq!(r[0], SSH_AGENT_IDENTITIES_ANSWER);

        // SIGN_REQUEST with flags=0 -> SSH_AGENT_FAILURE (5)
        let mut req_sha1 = vec![SSH_AGENTC_SIGN_REQUEST];
        put_str(&mut req_sha1, &blob);
        put_str(&mut req_sha1, payload);
        req_sha1.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(handle_message(&req_sha1, &p), vec![SSH_AGENT_FAILURE]);

        // SIGN_REQUEST with flags=SSH_AGENT_RSA_SHA2_256 -> SSH_AGENT_SIGN_RESPONSE (14)
        let mut req_sha256 = vec![SSH_AGENTC_SIGN_REQUEST];
        put_str(&mut req_sha256, &blob);
        put_str(&mut req_sha256, payload);
        req_sha256.extend_from_slice(&SSH_AGENT_RSA_SHA2_256.to_be_bytes());
        let resp256 = handle_message(&req_sha256, &p);
        assert_eq!(resp256[0], SSH_AGENT_SIGN_RESPONSE);

        // SIGN_REQUEST with flags=SSH_AGENT_RSA_SHA2_512 -> SSH_AGENT_SIGN_RESPONSE (14)
        let mut req_sha512 = vec![SSH_AGENTC_SIGN_REQUEST];
        put_str(&mut req_sha512, &blob);
        put_str(&mut req_sha512, payload);
        req_sha512.extend_from_slice(&SSH_AGENT_RSA_SHA2_512.to_be_bytes());
        let resp512 = handle_message(&req_sha512, &p);
        assert_eq!(resp512[0], SSH_AGENT_SIGN_RESPONSE);
    }

    #[test]
    fn rsa_encrypted_import_and_short_rsa_rejected() {
        use ssh_key::private::{KeypairData, RsaKeypair};
        let rsa_pair = RsaKeypair::random(&mut rand::rngs::OsRng, 2048).unwrap();
        let priv_key = PrivateKey::new(KeypairData::Rsa(rsa_pair), "enc@test").unwrap();
        let enc = priv_key.encrypt(&mut rand::rngs::OsRng, "correct passphrase").unwrap();
        let pem = enc.to_openssh(LineEnding::LF).unwrap();

        // Needs passphrase
        assert!(SshKey::import_openssh(&pem, None).is_err());
        assert!(SshKey::import_openssh(&pem, Some("wrong passphrase")).is_err());
        let imported = SshKey::import_openssh(&pem, Some("correct passphrase")).unwrap();
        assert_eq!(imported.info.bit_length, Some(2048));
        assert_eq!(imported.info.key_type, "ssh-rsa");

        // 1024-bit RSA key is rejected (< 2048 bits)
        // Construct raw 1024-bit RSA key using rsa crate directly
        use rsa::traits::{PrivateKeyParts, PublicKeyParts};
        let raw_rsa = rsa::RsaPrivateKey::new(&mut rand::rngs::OsRng, 1024).unwrap();
        let p_n = raw_rsa.n();
        let p_e = raw_rsa.e();
        let p_d = raw_rsa.d();
        let p_primes = raw_rsa.primes();
        let p_p = &p_primes[0];
        let p_q = &p_primes[1];
        let p_iqmp = raw_rsa.crt_coefficient().unwrap();

        let rsa_kp = ssh_key::private::RsaKeypair {
            public: ssh_key::public::RsaPublicKey {
                n: ssh_key::Mpint::from_positive_bytes(&p_n.to_bytes_be()).unwrap(),
                e: ssh_key::Mpint::from_positive_bytes(&p_e.to_bytes_be()).unwrap(),
            },
            private: ssh_key::private::RsaPrivateKey {
                d: ssh_key::Mpint::from_positive_bytes(&p_d.to_bytes_be()).unwrap(),
                iqmp: ssh_key::Mpint::from_positive_bytes(&p_iqmp.to_bytes_be()).unwrap(),
                p: ssh_key::Mpint::from_positive_bytes(&p_p.to_bytes_be()).unwrap(),
                q: ssh_key::Mpint::from_positive_bytes(&p_q.to_bytes_be()).unwrap(),
            },
        };
        let k1024 = PrivateKey::new(KeypairData::Rsa(rsa_kp), "short1024").unwrap();
        let pem1024 = k1024.to_openssh(LineEnding::LF).unwrap();
        let err = SshKey::import_openssh(&pem1024, None).unwrap_err();
        assert!(err.to_string().contains("RSA key is too short"), "expected too short error, got: {err}");
        assert!(err.to_string().contains("1024"));
    }

    #[test]
    fn git_signing_key_types_and_config() {
        // 1. Ed25519 works
        let ed = SshKey::generate_ed25519("ed@example.invalid").unwrap();
        assert!(check_git_signing_support(&ed).is_ok());

        let signers_path = std::path::Path::new("/tmp/test_signers");
        let cfg = build_git_signing_config(&ed, "ed@example.invalid", signers_path).unwrap();
        assert_eq!(cfg.key_type, "ssh-ed25519");
        assert_eq!(cfg.email, "ed@example.invalid");
        assert!(cfg.signing_key.starts_with("key::ssh-ed25519 AAAA"));
        assert!(cfg.config_lines.contains("gpg.format=ssh"));
        assert!(cfg.config_lines.contains(&format!("user.signingkey=\"{}\"", cfg.signing_key)));
        assert!(cfg.config_lines.contains("commit.gpgsign=true"));
        assert!(cfg.config_lines.contains("tag.gpgsign=true"));
        assert!(cfg.config_lines.contains("gpg.ssh.allowedSignersFile="));
        assert!(cfg.allowed_signers_line.starts_with("ed@example.invalid ssh-ed25519 AAAA"));

        // 2. RSA 2048 works
        use ssh_key::private::{KeypairData, RsaKeypair};
        let rsa_pair = RsaKeypair::random(&mut rand::rngs::OsRng, 2048).unwrap();
        let priv_rsa = PrivateKey::new(KeypairData::Rsa(rsa_pair), "rsa@example.invalid").unwrap();
        let rsa_key = SshKey::from_key(priv_rsa).unwrap();
        assert!(check_git_signing_support(&rsa_key).is_ok());
        let rsa_cfg = build_git_signing_config(&rsa_key, "rsa@example.invalid", signers_path).unwrap();
        assert_eq!(rsa_cfg.key_type, "ssh-rsa");
        assert!(rsa_cfg.signing_key.starts_with("key::ssh-rsa AAAA"));

        // 3. ECDSA is refused
        let ecdsa = PrivateKey::random(&mut rand::rngs::OsRng, Algorithm::Ecdsa { curve: ssh_key::EcdsaCurve::NistP256 }).unwrap();
        let ecdsa_key = SshKey::from_key(ecdsa).unwrap();
        let err = check_git_signing_support(&ecdsa_key).unwrap_err();
        assert!(err.to_string().contains("Git commit signing supports Ed25519 and RSA keys"), "got: {err}");
        assert!(err.to_string().contains("ecdsa"));
    }
}
