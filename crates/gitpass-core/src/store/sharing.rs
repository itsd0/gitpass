//! Sharing and identity types (v4): identity entries, member files,
//! share signatures and revoke reports. Pure move from `store.rs`.

use crate::crypto::*;
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// v4: hashes of the account identity files.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct IdentityEntry {
    pub pub_hash: String,
    pub enc_hash: String,
}

/// Plaintext `identity.pub` (committed). Integrity is covered by the owner's manifest; other people must
/// compare the fingerprint out of band (there is no PKI / public signature).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct IdentityPub {
    pub format: u32,
    pub account_id: String,
    pub public: String,
    pub fingerprint: String,
}

/// `vaults/<vid>/members/<fpr>.enc`: the Vault Key sealed to a recipient's identity key.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemberFile {
    pub format: u32,
    pub vault_id: String,
    pub key_version: u32,
    pub recipient_fpr: String,
    pub recipient_pub: String,
    pub sender_fpr: String,
    pub sender_pub: String,
    pub sealed: crate::keys::SealedBox,
    /// Ed25519 verifying key (base64) of the sender's share-signing key.
    /// `#[serde(default)]` so pre-signature member files still parse — and
    /// are then rejected as unsigned by `accept_share`.
    #[serde(default)]
    pub sender_sign_pub: String,
    /// Ed25519 signature (base64) over `share_sig_payload` for this file.
    #[serde(default)]
    pub sender_sig: String,
    /// Sender binding tag (base64): BLAKE3-keyed over `share_sig_payload`,
    /// keyed by DH(sender X25519 secret, recipient public). Proves the share
    /// came from the holder of `sender_pub`, which the Ed25519 signature alone
    /// cannot (its verifying key is carried in the file).
    #[serde(default)]
    pub sender_auth: String,
}
pub fn member_aad(mf: &MemberFile) -> Vec<u8> {
    format!("gitpass/member/v4|{}|{}|{}|{}|{}|{}", mf.vault_id, mf.key_version, mf.recipient_fpr, mf.recipient_pub, mf.sender_fpr, mf.sender_pub).into_bytes()
}

/// Canonical payload a share signature covers: every field that decides what
/// the recipient imports, including the sealed vault key itself. Changing any
/// of them after signing invalidates the signature.
pub fn share_sig_payload(mf: &MemberFile) -> Vec<u8> {
    format!(
        "gitpass/share-sig/v1|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        mf.vault_id, mf.key_version, mf.recipient_fpr, mf.recipient_pub, mf.sender_fpr, mf.sender_pub,
        mf.sender_sign_pub, mf.sealed.alg, mf.sealed.eph, mf.sealed.envelope.alg, mf.sealed.envelope.nonce,
        mf.sealed.envelope.ct
    )
    .into_bytes()
}

/// Fingerprint of a sender's Ed25519 signing key (base64), for out-of-band
/// verification on first contact. Empty when the file carries no signing key.
pub fn sender_sign_fingerprint(mf: &MemberFile) -> String {
    let Ok(pk) = B64.decode(&mf.sender_sign_pub) else { return String::new() };
    let Ok(pk): Result<[u8; 32], _> = pk.try_into() else { return String::new() };
    crate::keys::fingerprint(&pk)
}
pub(crate) fn identity_key(ak: &Key32) -> Key32 { hkdf32(&ak.0, None, b"gitpass/identity/v4") }
pub(crate) fn identity_aad(account_id: &str, public_b64: &str) -> Vec<u8> { format!("{account_id}|identity|4|{public_b64}").into_bytes() }
pub(crate) fn device_kek(auk: &Key32, account_id: &str) -> Key32 { hkdf32(&auk.0, Some(account_id.as_bytes()), b"gitpass/device-key/v4") }

/// Result of [`Session::revoke_device_and_rotate`].
#[derive(Debug, Clone)]
pub struct RevokeReport {
    pub device_id: String,
    pub vaults_rotated: Vec<String>,
    pub account_key_backup: PathBuf,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct RevokePending { pub(crate) device_id: String }

#[cfg(test)]
mod share_signature_tests {
    use super::*;
    use crate::store::*;
    use crate::crypto::KdfParams;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }

    fn unlock_fast(root: &Path, pw: &str, sk: &crate::crypto::SecretKey) -> Session {
        Session::unlock_with(root, pw, sk, &UnlockOpts { accept_rollback: false, target_kdf: fast() }).unwrap()
    }

    struct Pair {
        _a: tempfile::TempDir,
        _b: tempfile::TempDir,
        sb: Session,
        vid: String,
        member: PathBuf,
    }

    fn setup() -> Pair {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ska = init(a.path(), "pw-a", fast()).unwrap();
        let skb = init(b.path(), "pw-b", fast()).unwrap();
        let sa = unlock_fast(a.path(), "pw-a", &ska);
        let sb = unlock_fast(b.path(), "pw-b", &skb);
        let vid = sa.create_vault_full("Shared", None, None, None).unwrap();
        let (rec_pub, _) = sb.identity().unwrap();
        let member = sa.share_vault(&vid, &rec_pub).unwrap();
        Pair { _a: a, _b: b, sb, vid, member }
    }

    fn read_member(p: &Path) -> MemberFile {
        serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
    }

    fn write_member(p: &Path, mf: &MemberFile) {
        fs::write(p, serde_json::to_vec_pretty(mf).unwrap()).unwrap();
    }

    #[test]
    fn signed_share_accepts_and_tamper_rejects() {
        let p = setup();
        // A genuine share carries a signature and accepts.
        let mf = read_member(&p.member);
        assert!(!mf.sender_sig.is_empty() && !mf.sender_sign_pub.is_empty());
        assert_eq!(p.sb.accept_share(&p.member).unwrap(), p.vid);
        assert!(p.sb.list_vaults().unwrap().iter().any(|(id, _)| id == &p.vid));
    }

    #[test]
    fn tampered_sealed_box_rejects() {
        let p = setup();
        let mut mf = read_member(&p.member);
        // Flip one character of the sealed vault-key ciphertext. The payload
        // covers it, so the signature must fail before the box is opened.
        let mut ct = mf.sealed.envelope.ct.clone();
        let first = ct.chars().next().unwrap();
        ct.replace_range(0..1, if first == 'A' { "B" } else { "A" });
        mf.sealed.envelope.ct = ct;
        write_member(&p.member, &mf);
        assert!(p.sb.accept_share(&p.member).is_err());
    }

    #[test]
    fn stripped_signature_rejects() {
        let p = setup();
        let mut mf = read_member(&p.member);
        mf.sender_sig.clear();
        mf.sender_sign_pub.clear();
        write_member(&p.member, &mf);
        let err = p.sb.accept_share(&p.member).unwrap_err().to_string();
        assert!(err.contains("unsigned"), "unexpected error: {err}");
    }

    #[test]
    fn forged_sender_rejects() {
        let p = setup();
        // Attacker re-labels the share with their own X25519 identity but
        // cannot re-sign it: the original signature covers the sender fields.
        let mut mf = read_member(&p.member);
        let attacker = crate::keys::KeyPair::generate();
        mf.sender_pub = B64.encode(attacker.public);
        mf.sender_fpr = attacker.fingerprint();
        write_member(&p.member, &mf);
        assert!(p.sb.accept_share(&p.member).is_err());
    }

    #[test]
    fn substituted_signing_key_rejects() {
        // Attacker keeps the victim's sender_pub/sender_fpr but signs with
        // their own Ed25519 key: the signature verifies, the binding must not.
        let p = setup();
        let mut mf = read_member(&p.member);
        let atk = crate::keys::signing_key_from_identity_secret(&crate::crypto::random_bytes::<32>());
        mf.sender_sign_pub = B64.encode(atk.verifying_key().to_bytes());
        mf.sender_sig = B64.encode(crate::keys::sign_share_payload(&atk, &share_sig_payload(&mf)));
        write_member(&p.member, &mf);
        let err = format!("{:#}", p.sb.accept_share(&p.member).unwrap_err());
        assert!(err.contains("not bound"), "unexpected error: {err}");
    }

    #[test]
    fn forged_sender_with_own_keys_rejects() {
        // Attacker seals a fresh vault key to the recipient, claims the
        // victim's identity, and signs + tags with their own keys.
        let p = setup();
        let mut mf = read_member(&p.member);
        let atk = crate::keys::KeyPair::generate();
        let rec = crate::keys::parse_pub(&mf.recipient_pub).unwrap();
        mf.sealed = crate::keys::seal_to(&rec, &crate::crypto::random_bytes::<32>(), &member_aad(&mf)).unwrap();
        let sk = crate::keys::signing_key_from_identity_secret(&atk.secret_bytes());
        mf.sender_sign_pub = B64.encode(sk.verifying_key().to_bytes());
        mf.sender_sig = B64.encode(crate::keys::sign_share_payload(&sk, &share_sig_payload(&mf)));
        mf.sender_auth = B64.encode(crate::keys::share_auth_tag(&atk, &rec, &share_sig_payload(&mf)).unwrap());
        write_member(&p.member, &mf);
        assert!(p.sb.accept_share(&p.member).is_err());
    }

    #[test]
    fn wrong_recipient_share_rejects() {
        // A share sealed to someone else is not addressed to us, even with a
        // valid signature — the recipient check still runs first.
        let p = setup();
        let c = tempfile::tempdir().unwrap();
        let skc = init(c.path(), "pw-c", fast()).unwrap();
        let sc = unlock_fast(c.path(), "pw-c", &skc);
        assert!(sc.accept_share(&p.member).is_err());
    }
}
