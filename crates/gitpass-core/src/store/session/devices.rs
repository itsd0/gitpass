//! Session methods (pure move from `store.rs`; see `session/mod.rs`).

use super::*;
use crate::fsutil::{self, atomic_write};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

impl Session {
    // ---------- device registry (v3) ----------

    /// Refuse mutating operations if this device has been revoked.
    pub fn ensure_not_revoked(&self) -> Result<()> {
        if self.device_revoked {
            bail!("this device ({}) has been REVOKED in the device registry; mutating operations are refused. \
                   Allowed: `gitpass device list`, `gitpass rotate-account-key`.", self.device_id);
        }
        Ok(())
    }

    /// Decrypted device registry (empty if devices.enc does not exist yet).
    pub fn devices(&self) -> Result<crate::device::DeviceRegistry> {
        Ok(crate::device::read_registry(&self.root, &self.account_key, &self.fmt.account_id)?.unwrap_or_default())
    }

    fn mutate_devices<F: FnOnce(&mut crate::device::DeviceRegistry) -> Result<()>>(&self, f: F) -> Result<()> {
        self.commit(|m| {
            let mut r = crate::device::read_registry(&self.root, &self.account_key, &self.fmt.account_id)?.unwrap_or_default();
            f(&mut r)?;
            m.devices_hash = Some(crate::device::write_registry(&self.root, &self.account_key, &self.fmt.account_id, &r)?);
            Ok(())
        })?;
        Ok(())
    }

    /// Register this device or refresh its last_seen_at. A revoked device is not modified; it is flagged.
    pub fn touch_device(&mut self, name: &str) -> Result<()> {
        let reg = self.devices()?;
        if let Some(d) = reg.get(&self.device_id) {
            if d.revoked { self.device_revoked = true; return Ok(()); }
        }
        let (id, now) = (self.device_id.clone(), crate::device::now_iso8601());
        // Throttled: a no-op unlock must not rewrite devices.enc / manifest.enc.
        if reg.needs_touch(&id, name, &now) {
            self.mutate_devices(|r| { r.touch(&id, name, &now); Ok(()) })?;
        }
        if self.has_identity() { self.record_device_key()?; }
        Ok(())
    }

    pub fn rename_device(&self, query: &str, name: &str) -> Result<String> {
        self.ensure_not_revoked()?;
        if name.trim().is_empty() { bail!("device name must not be empty"); }
        let id = self.devices()?.resolve(query)?;
        self.mutate_devices(|r| {
            r.devices.iter_mut().find(|d| d.id == id).unwrap().name = name.trim().into();
            Ok(())
        })?;
        Ok(id)
    }

    /// Mark a device revoked (policy marker only; does NOT re-encrypt keys).
    pub fn revoke_device(&mut self, query: &str) -> Result<String> {
        self.ensure_not_revoked()?;
        let id = self.devices()?.resolve(query)?;
        let now = crate::device::now_iso8601();
        self.mutate_devices(|r| {
            let d = r.devices.iter_mut().find(|d| d.id == id).unwrap();
            if d.revoked { bail!("device {id} is already revoked"); }
            d.revoked = true;
            d.revoked_at = Some(now);
            Ok(())
        })?;
        if id == self.device_id { self.device_revoked = true; }
        Ok(id)
    }

    /// Clear a revocation marker. Must be run from a different, non-revoked device.
    pub fn unrevoke_device(&self, query: &str) -> Result<String> {
        self.ensure_not_revoked()?;
        let id = self.devices()?.resolve(query)?;
        self.mutate_devices(|r| {
            let d = r.devices.iter_mut().find(|d| d.id == id).unwrap();
            if !d.revoked { bail!("device {id} is not revoked"); }
            d.revoked = false;
            d.revoked_at = None;
            Ok(())
        })?;
        Ok(id)
    }

    /// REAL revocation (v4): mark `query` revoked, then rotate every Vault Key (re-encrypting all items and
    /// re-sealing to vault members) and the Account Key, bumping the manifest epoch each step. A local
    /// `.gitpass-revoke-pending.json` is written first and removed last; an interrupted run is rolled
    /// forward on the next unlock (every step is idempotent).
    ///
    /// LIMITS: this protects FUTURE changes only. A device that already copied the repo / keys keeps what it
    /// had, and if it knew the master password + Secret Key it can derive the new AUK, so rotate those too
    /// ([`Session::rotate_credentials`]).
    pub fn revoke_device_and_rotate(&mut self, query: &str, password: &str, secret: &SecretKey) -> Result<RevokeReport> {
        self.ensure_not_revoked()?;
        let id = self.devices()?.resolve(query)?;
        if id == self.device_id { bail!("refusing to revoke THIS device with key rotation (it would receive the new keys); run from another device or use --marker-only"); }
        let pending = self.root.join(REVOKE_PENDING_FILE);
        write_json(&pending, &RevokePending { device_id: id.clone() })?;
        crate::device::ensure_ignored(&self.root, REVOKE_PENDING_FILE)?;
        if !self.devices()?.get(&id).map(|d| d.revoked).unwrap_or(false) {
            let now = crate::device::now_iso8601();
            self.mutate_devices(|r| {
                let d = r.devices.iter_mut().find(|d| d.id == id).unwrap();
                d.revoked = true;
                d.revoked_at = Some(now);
                Ok(())
            })?;
        }
        let mut rotated = vec![];
        for vid in self.load_manifest()?.vaults.keys() { self.rotate_vault(vid)?; rotated.push(vid.clone()); }
        let backup = self.rotate_account_key(password, secret)?;
        fs::remove_file(&pending)?;
        fsutil::fsync_dir(&self.root);
        Ok(RevokeReport { device_id: id, vaults_rotated: rotated, account_key_backup: backup })
    }

    /// New master password AND new Secret Key in one keyset re-wrap. Returns the new Secret Key.
    pub fn rotate_credentials(&mut self, new_password: &str, kdf: KdfParams) -> Result<SecretKey> {
        self.ensure_not_revoked()?;
        let sk = SecretKey::generate();
        self.rewrap_keyset(new_password, &sk, kdf)?;
        Ok(sk)
    }

    /// This device's X25519 public key (v4 accounts only; created on demand).
    pub fn device_public_key(&self) -> Result<[u8; 32]> {
        Ok(crate::device::load_or_create_device_key(&self.root, &self.device_id, &self.device_kek)?.0.public)
    }

    fn record_device_key(&self) -> Result<()> {
        let (kp, _) = crate::device::load_or_create_device_key(&self.root, &self.device_id, &self.device_kek)?;
        let pk = B64.encode(kp.public);
        if self.devices()?.get(&self.device_id).and_then(|d| d.public_key.clone()) == Some(pk.clone()) { return Ok(()); }
        let id = self.device_id.clone();
        self.mutate_devices(|r| {
            if let Some(d) = r.devices.iter_mut().find(|d| d.id == id) { d.public_key = Some(pk); }
            Ok(())
        })
    }

    /// Record that an item was used right now on this device.
    pub fn touch_item_used(&self, item_id: &str) -> Result<u64> {
        let ts = now_secs();
        touch_item_used(&self.root, item_id, ts)?;
        Ok(ts)
    }

    /// Last used timestamp for an item on this device (None if never used).
    pub fn last_used(&self, item_id: &str) -> Option<u64> {
        get_item_last_used(&self.root, item_id)
    }

    /// All item last used timestamps recorded on this device.
    pub fn all_last_used(&self) -> BTreeMap<String, u64> {
        get_all_last_used(&self.root)
    }

    // ---------- identity + sharing (v4) ----------

    pub fn has_identity(&self) -> bool { self.root.join(IDENTITY_ENC_FILE).exists() }

    pub(crate) fn load_identity(&self) -> Result<crate::keys::KeyPair> {
        let ip: IdentityPub = read_json(&self.root.join(IDENTITY_PUB_FILE))?;
        if ip.account_id != self.fmt.account_id { bail!("identity.pub belongs to another account"); }
        let env: Envelope = read_json(&self.root.join(IDENTITY_ENC_FILE))?;
        let pt = zeroize::Zeroizing::new(open(&identity_key(&self.account_key), &env, &identity_aad(&ip.account_id, &ip.public))?);
        let b: [u8; 32] = pt[..].try_into().map_err(|_| anyhow!("identity key has wrong length"))?;
        let kp = crate::keys::KeyPair::from_secret_bytes(b);
        if B64.encode(kp.public) != ip.public { bail!("identity.pub does not match identity.enc"); }
        Ok(kp)
    }

    pub(crate) fn write_identity(&self, ak: &Key32, kp: &crate::keys::KeyPair) -> Result<IdentityEntry> {
        let public = B64.encode(kp.public);
        let ip = IdentityPub { format: 4, account_id: self.fmt.account_id.clone(), public: public.clone(), fingerprint: kp.fingerprint() };
        let enc_hash = write_json(&self.root.join(IDENTITY_ENC_FILE), &seal(&identity_key(ak), &kp.secret_bytes()[..], &identity_aad(&ip.account_id, &public))?)?;
        let pub_hash = write_json(&self.root.join(IDENTITY_PUB_FILE), &ip)?;
        Ok(IdentityEntry { pub_hash, enc_hash })
    }

    /// Create the account identity keypair if missing (upgrades the manifest to v4) and register this
    /// device's X25519 public key. Returns the identity keypair.
    pub fn ensure_identity(&self) -> Result<crate::keys::KeyPair> {
        if self.has_identity() { return self.load_identity(); }
        self.ensure_not_revoked()?;
        let kp = crate::keys::KeyPair::generate();
        self.commit(|m| { m.identity = Some(self.write_identity(&self.account_key, &kp)?); Ok(()) })?;
        self.record_device_key()?;
        Ok(kp)
    }

    /// (public key, fingerprint) of the identity if it exists (never creates one).
    pub fn identity_if_exists(&self) -> Result<Option<([u8; 32], String)>> {
        if !self.has_identity() { return Ok(None); }
        let kp = self.load_identity()?;
        Ok(Some((kp.public, kp.fingerprint())))
    }

    /// (public key, fingerprint) of this account's identity (created on first use).
    pub fn identity(&self) -> Result<([u8; 32], String)> {
        let kp = self.ensure_identity()?;
        Ok((kp.public, kp.fingerprint()))
    }

    pub(crate) fn write_member(dir: &Path, vid: &str, kv: u32, vk: &Key32, recipient: &[u8; 32], me: &crate::keys::KeyPair) -> Result<String> {
        let mut mf = MemberFile { format: 4, vault_id: vid.into(), key_version: kv, recipient_fpr: crate::keys::fingerprint(recipient),
            recipient_pub: B64.encode(recipient), sender_fpr: me.fingerprint(), sender_pub: B64.encode(me.public),
            sealed: crate::keys::SealedBox { alg: String::new(), eph: String::new(), envelope: Envelope { alg: String::new(), nonce: String::new(), ct: String::new() } },
            sender_sign_pub: String::new(), sender_sig: String::new(), sender_auth: String::new() };
        mf.sealed = crate::keys::seal_to(recipient, &vk.0, &member_aad(&mf))?;
        // Sign the finished share (sealed box included) with the sender's
        // share-signing key, derived from the identity secret.
        let secret = me.secret_bytes();
        let signing = crate::keys::signing_key_from_identity_secret(&secret);
        mf.sender_sign_pub = B64.encode(signing.verifying_key().to_bytes());
        mf.sender_sig = B64.encode(crate::keys::sign_share_payload(&signing, &share_sig_payload(&mf)));
        mf.sender_auth = B64.encode(crate::keys::share_auth_tag(me, recipient, &share_sig_payload(&mf))?);
        write_json(&dir.join("members").join(format!("{}.enc", mf.recipient_fpr)), &mf)
    }

    /// `gitpass vault share`: seal the current Vault Key to `recipient`. Returns the member file path.
    pub fn share_vault(&self, vid: &str, recipient: &[u8; 32]) -> Result<PathBuf> {
        self.ensure_not_revoked()?;
        let me = self.ensure_identity()?;
        if *recipient == me.public { bail!("refusing to share a vault with your own identity"); }
        let fpr = crate::keys::fingerprint(recipient);
        let vk = self.vault_key(vid)?;
        let d = self.vdir(vid);
        self.commit(|m| {
            let ve = m.vaults.get_mut(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
            let h = Self::write_member(&d, vid, ve.key_version, &vk, recipient, &me)?;
            ve.members.insert(fpr.clone(), h);
            Ok(())
        })?;
        Ok(d.join("members").join(format!("{fpr}.enc")))
    }

    /// Members of a vault: (fingerprint, base64 public key).
    pub fn members(&self, vid: &str) -> Result<Vec<(String, String)>> {
        let m = self.load_manifest()?;
        let ve = m.vaults.get(vid).ok_or_else(|| anyhow!("vault {vid} not in manifest"))?;
        ve.members.keys().map(|f| {
            let mf: MemberFile = read_json(&self.vdir(vid).join("members").join(format!("{f}.enc")))?;
            Ok((f.clone(), mf.recipient_pub))
        }).collect()
    }

    /// `gitpass vault unshare`: drop a member (fingerprint or unique prefix) and rotate the Vault Key so the
    /// member cannot read FUTURE changes. Returns the removed fingerprint.
    pub fn unshare_vault(&self, vid: &str, member: &str) -> Result<String> {
        self.ensure_not_revoked()?;
        let all: Vec<String> = self.members(vid)?.into_iter().map(|(f, _)| f).collect();
        let hits: Vec<&String> = all.iter().filter(|f| f.starts_with(member) && !member.is_empty()).collect();
        let fpr = match hits.len() { 0 => bail!("no member matches '{member}'"), 1 => hits[0].clone(), n => bail!("{n} members match '{member}'") };
        self.rotate_vault_excluding(vid, Some(&fpr))?;
        Ok(fpr)
    }

    /// `gitpass vault accept`: open a member file sealed to our identity and import a SNAPSHOT of that vault
    /// (meta + items, each authenticated under the shared Vault Key) into this account. The vault directory
    /// is taken as `<share>/../..` (i.e. `vaults/<id>/members/<fpr>.enc` in the sharer's checkout).
    /// Returns the imported vault id.
    pub fn accept_share(&self, share: &Path) -> Result<String> {
        self.ensure_not_revoked()?;
        let me = self.ensure_identity()?;
        let mf: MemberFile = read_json(share)?;
        if mf.format != 4 { bail!("unsupported member file format {}", mf.format); }
        if mf.recipient_fpr != me.fingerprint() || mf.recipient_pub != B64.encode(me.public) { bail!("this share is not addressed to your identity ({})", me.fingerprint()); }
        // Sender authentication: the sender's X25519 public key is self-asserted
        // in the file, so the sealed box alone proves nothing about who sent
        // this. Require a valid Ed25519 signature over the whole share.
        // Pre-signature shares (no signature fields) are rejected, not
        // grandfathered in — ask the sender to re-share with a current build.
        if mf.sender_sig.is_empty() || mf.sender_sign_pub.is_empty() || mf.sender_auth.is_empty() {
            bail!("share is unsigned (created before share signatures, or stripped) — refusing; ask the sender to re-share");
        }
        let sender_pub = crate::keys::parse_pub(&mf.sender_pub)?;
        if crate::keys::fingerprint(&sender_pub) != mf.sender_fpr {
            bail!("share sender fingerprint does not match sender public key — refusing");
        }
        let sign_pub: [u8; 32] = B64.decode(&mf.sender_sign_pub).context("bad sender signing key")?.try_into().map_err(|_| anyhow!("bad sender signing key length"))?;
        let sig: [u8; 64] = B64.decode(&mf.sender_sig).context("bad share signature")?.try_into().map_err(|_| anyhow!("bad share signature length"))?;
        crate::keys::verify_share_payload(&sign_pub, &share_sig_payload(&mf), &sig)
            .context("share signature invalid — wrong sender or tampered share")?;
        // Bind the share to sender_pub (the signing key above is self-asserted).
        let tag: [u8; 32] = B64.decode(&mf.sender_auth).context("bad sender binding")?.try_into().map_err(|_| anyhow!("bad sender binding length"))?;
        crate::keys::verify_share_auth_tag(&me, &sender_pub, &share_sig_payload(&mf), &tag)
            .context("share is not bound to the claimed sender identity — refusing")?;
        let pt = crate::keys::open_sealed(&me, &mf.sealed, &member_aad(&mf))?;
        let b: [u8; 32] = pt[..].try_into().map_err(|_| anyhow!("shared vault key has wrong length"))?;
        let vk = Key32(b);
        let vid = mf.vault_id.clone();
        uuid::Uuid::parse_str(&vid).context("shared vault id is not a uuid")?;
        let src = share.parent().and_then(|p| p.parent()).ok_or_else(|| anyhow!("share path must be vaults/<id>/members/<fpr>.enc"))?;
        if src.file_name().map(|n| n.to_string_lossy() != vid.as_str()).unwrap_or(true) { bail!("share file is not inside vaults/{vid}/members/"); }
        let menv: Envelope = read_json(&src.join("meta.enc"))?;
        let meta: VaultMeta = serde_json::from_slice(&zeroize::Zeroizing::new(open(&vk, &menv, &meta_aad(&vid)).context("shared vault meta failed authentication")?))?;
        if self.load_manifest()?.vaults.contains_key(&vid) { bail!("vault {vid} is already in this account"); }
        if self.list_vaults()?.iter().any(|(_, m)| m.name == meta.name) { bail!("a vault named '{}' already exists here; rename it first", meta.name); }
        let mut items = vec![];
        for iid in list_enc(&src.join("items")) {
            let p = src.join("items").join(format!("{iid}.enc"));
            let f: ItemFile = read_json(&p)?;
            open(&vk, &f.envelope, &item_aad(&vid, &iid, f.version)).with_context(|| format!("shared item {iid} failed authentication"))?;
            items.push((iid, f.version, fs::read(&p)?));
        }
        let mut files = vec![];
        for fid in list_enc(&src.join(FILES_DIR)) {
            let p = src.join(FILES_DIR).join(format!("{fid}.enc"));
            let b: FileBlob = read_json(&p)?;
            open(&vk, &b.envelope, &file_aad(&vid, &b.item_id, &fid)).with_context(|| format!("shared attachment {fid} failed authentication"))?;
            files.push((fid, fs::read(&p)?));
        }
        let _g = fsutil::lock(&self.root)?;
        let tmp = self.root.join("vaults").join(format!(".accept-{vid}"));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("items"))?;
        let mut ve = VaultEntry { key_version: mf.key_version, ..Default::default() };
        ve.key_hash = write_json(&tmp.join("vault.key.enc"), &VaultKeyFile { key_version: mf.key_version, envelope: wrap_key(&self.account_key, &vk, &vault_key_aad(&vid, mf.key_version))? })?;
        let mb = fs::read(src.join("meta.enc"))?;
        atomic_write(&tmp.join("meta.enc"), &mb)?;
        ve.meta_hash = hash_bytes(&mb);
        atomic_write(&tmp.join("items/.keep"), b"")?;
        for (iid, v, bytes) in &items {
            atomic_write(&tmp.join("items").join(format!("{iid}.enc")), bytes)?;
            ve.items.insert(iid.clone(), ItemEntry { version: *v, hash: hash_bytes(bytes) });
        }
        if !files.is_empty() { fs::create_dir_all(tmp.join(FILES_DIR))?; }
        for (fid, bytes) in &files {
            atomic_write(&tmp.join(FILES_DIR).join(format!("{fid}.enc")), bytes)?;
            ve.files.insert(fid.clone(), hash_bytes(bytes));
        }
        fsutil::rename(&tmp, &self.vdir(&vid))?;
        self.commit_locked(|m| { m.vaults.insert(vid.clone(), ve); Ok(()) })?;
        Ok(vid)
    }

    #[doc(hidden)]
    pub fn vault_key_bytes_for_tests(&self, vid: &str) -> Result<[u8; 32]> { Ok(self.vault_key(vid)?.0) }

    #[doc(hidden)]
    pub fn account_key_bytes_for_tests(&self) -> [u8; 32] { self.account_key.0 }
}
