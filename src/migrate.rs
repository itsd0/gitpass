//! v1 -> v2 migration. Also contains a minimal v1 *writer* used to build test fixtures.
use crate::crypto::*;
use crate::fsutil::{self, atomic_write};
use crate::store::{self, read_json, write_json, Format, Item, ItemEntry, ItemFile, Manifest, VaultEntry, VaultKeyFile, VaultMeta};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

const TMP: &str = ".gitpass-migrate-tmp";
const MARKER: &str = ".gitpass-migrating";

/// v1 on-disk layout and AADs (read-only, plus a fixture writer for tests).
pub mod v1 {
    use super::*;
    pub fn keyset_aad(account_id: &str) -> Vec<u8> { format!("{account_id}|keyset|1").into_bytes() }
    pub fn vault_key_aad(vault_id: &str) -> Vec<u8> { format!("{vault_id}|vault-key|1").into_bytes() }

    #[derive(serde::Serialize, serde::Deserialize)]
    pub struct MetaV1 { pub name: String }

    /// Decrypted v1 content held in memory (items zeroize on drop).
    pub struct Plain {
        pub fmt: Format,
        pub account_key: Key32,
        pub vaults: Vec<(String, Key32, Zeroizing<String>, Vec<(String, u64, Item)>)>,
    }

    pub fn unlock(root: &Path, password: &str, secret: &SecretKey) -> Result<(Format, Key32)> {
        let fmt: Format = read_json(&root.join("format.json"))?;
        if fmt.version != 1 { bail!("expected format v1, found v{}", fmt.version); }
        let salt = B64.decode(&fmt.salt)?;
        let auk = derive_auk(password, secret, &salt, &fmt.account_id, &fmt.kdf)?;
        let env: Envelope = read_json(&root.join("keyset.enc"))?;
        let ak = unwrap_key(&auk, &env, &keyset_aad(&fmt.account_id))
            .map_err(|_| anyhow!("unlock failed: wrong master password or Secret Key"))?;
        Ok((fmt, ak))
    }

    pub fn decrypt_all(root: &Path, password: &str, secret: &SecretKey) -> Result<Plain> {
        let (fmt, ak) = unlock(root, password, secret)?;
        let mut vaults = vec![];
        let vd = root.join("vaults");
        if vd.exists() {
            for e in fs::read_dir(&vd)? {
                let d = e?.path();
                if !d.join("meta.json").exists() { continue; }
                let vid = d.file_name().unwrap().to_string_lossy().to_string();
                let meta: MetaV1 = read_json(&d.join("meta.json"))?;
                let venv: Envelope = read_json(&d.join("vault.key.enc"))?;
                let vk = unwrap_key(&ak, &venv, &vault_key_aad(&vid))?;
                let mut items = vec![];
                for ie in fs::read_dir(d.join("items")).into_iter().flatten() {
                    let p = ie?.path();
                    if p.extension().and_then(|s| s.to_str()) != Some("enc") { continue; }
                    let iid = p.file_stem().unwrap().to_string_lossy().to_string();
                    let f: ItemFile = read_json(&p)?;
                    let pt = Zeroizing::new(open(&vk, &f.envelope, &store::item_aad(&vid, &iid, f.version))
                        .with_context(|| format!("decrypting v1 item {vid}/{iid}"))?);
                    items.push((iid, f.version, serde_json::from_slice(&pt)?));
                }
                vaults.push((vid, vk, Zeroizing::new(meta.name), items));
            }
        }
        Ok(Plain { fmt, account_key: ak, vaults })
    }

    // ----- fixture writer (exactly the v0.1.0 layout) -----
    pub fn init_fixture(root: &Path, password: &str, kdf: KdfParams) -> Result<SecretKey> {
        fs::create_dir_all(root.join("vaults"))?;
        let salt: [u8; 16] = random_bytes();
        let account_id = uuid::Uuid::new_v4().to_string();
        let secret = SecretKey::generate();
        let auk = derive_auk(password, &secret, &salt, &account_id, &kdf)?;
        let env = wrap_key(&auk, &Key32::random(), &keyset_aad(&account_id))?;
        write_json(&root.join("format.json"), &Format { version: 1, kdf, salt: B64.encode(salt), account_id })?;
        write_json(&root.join("keyset.enc"), &env)?;
        fs::write(root.join(".gitignore"), "secret.key\n*.tmp\n")?;
        Ok(secret)
    }
    pub fn create_vault_fixture(root: &Path, password: &str, secret: &SecretKey, name: &str) -> Result<String> {
        let (_, ak) = unlock(root, password, secret)?;
        let id = uuid::Uuid::new_v4().to_string();
        let d = root.join("vaults").join(&id);
        fs::create_dir_all(d.join("items"))?;
        write_json(&d.join("vault.key.enc"), &wrap_key(&ak, &Key32::random(), &vault_key_aad(&id))?)?;
        write_json(&d.join("meta.json"), &MetaV1 { name: name.into() })?;
        fs::write(d.join("items/.keep"), "")?;
        Ok(id)
    }
    pub fn add_item_fixture(root: &Path, password: &str, secret: &SecretKey, vid: &str, item: &Item) -> Result<String> {
        let (_, ak) = unlock(root, password, secret)?;
        let d = root.join("vaults").join(vid);
        let venv: Envelope = read_json(&d.join("vault.key.enc"))?;
        let vk = unwrap_key(&ak, &venv, &vault_key_aad(vid))?;
        let iid = uuid::Uuid::new_v4().to_string();
        let env = seal(&vk, &serde_json::to_vec(item)?, &store::item_aad(vid, &iid, 1))?;
        write_json(&d.join("items").join(format!("{iid}.enc")), &ItemFile { version: 1, envelope: env })?;
        Ok(iid)
    }
}

/// Test-only fault injection: abort migration after step N (1 = temp dir built, 2 = vaults swapped,
/// 3 = keyset+manifest swapped, format.json not yet written).
#[derive(Default, Clone, Copy)]
pub struct MigrateOpts { pub fail_after: Option<u8> }

fn restore_from_backup(root: &Path, backup: &Path) -> Result<()> {
    for n in ["vaults", "keyset.enc", "manifest.enc", "format.json"] {
        let p = root.join(n);
        if p.is_dir() { fs::remove_dir_all(&p)?; } else if p.exists() { fs::remove_file(&p)?; }
    }
    fsutil::copy_dir(backup, root, &|_| false)?;
    fsutil::fsync_dir(root);
    Ok(())
}

/// Undo a half-finished migration if one is detected. Returns true if anything was repaired.
pub fn recover(root: &Path) -> Result<bool> {
    let mut did = false;
    let marker = root.join(MARKER);
    if marker.exists() {
        let backup = PathBuf::from(fs::read_to_string(&marker)?.trim());
        if !backup.is_dir() { bail!("migration marker points to missing backup {}", backup.display()); }
        restore_from_backup(root, &backup)?;
        fs::remove_file(&marker)?;
        did = true;
    }
    if root.join(TMP).exists() { fs::remove_dir_all(root.join(TMP))?; did = true; }
    Ok(did)
}

/// Migrate a v1 vault dir to v2. Returns the backup directory.
pub fn migrate(root: &Path, password: &str, secret: &SecretKey, opts: MigrateOpts) -> Result<PathBuf> {
    let _g = fsutil::lock(root)?;
    recover(root)?;
    let fmt_on_disk: Format = read_json(&root.join("format.json")).context("not initialized")?;
    if fmt_on_disk.version != 1 { bail!("nothing to migrate: vault is already format v{}", fmt_on_disk.version); }
    // 3. decrypt everything into zeroizing memory (also proves the credentials before touching disk)
    let plain = v1::decrypt_all(root, password, secret)?;
    // 2. backup
    let backup = fsutil::backup(root, "v1-")?;
    // 4/5. build the v2 tree in a temp dir
    let tmp = root.join(TMP);
    fs::create_dir_all(tmp.join("vaults"))?;
    let fmt = Format { version: store::FORMAT_VERSION, ..plain.fmt.clone() };
    let salt = B64.decode(&fmt.salt)?;
    let auk = derive_auk(password, secret, &salt, &fmt.account_id, &fmt.kdf)?;
    let keyset_hash = write_json(&tmp.join("keyset.enc"), &wrap_key(&auk, &plain.account_key, &store::keyset_aad(&fmt))?)?;
    let mut m = Manifest { format: store::FORMAT_VERSION, account_id: fmt.account_id.clone(), epoch: 1, keyset_hash, vaults: BTreeMap::new() };
    for (vid, vk, name, items) in &plain.vaults {
        let d = tmp.join("vaults").join(vid);
        fs::create_dir_all(d.join("items"))?;
        atomic_write(&d.join("items/.keep"), b"")?;
        let mut ve = VaultEntry { key_version: 1, ..Default::default() };
        ve.key_hash = write_json(&d.join("vault.key.enc"),
            &VaultKeyFile { key_version: 1, envelope: wrap_key(&plain.account_key, vk, &store::vault_key_aad(vid, 1))? })?;
        let mpt = Zeroizing::new(serde_json::to_vec(&VaultMeta { name: name.to_string(), description: None })?);
        ve.meta_hash = write_json(&d.join("meta.enc"), &seal(vk, &mpt, &store::meta_aad(vid))?)?;
        for (iid, ver, item) in items {
            let pt = Zeroizing::new(serde_json::to_vec(item)?);
            let padded = Zeroizing::new(store::pad(&pt));
            let env = seal(vk, &padded, &store::item_aad(vid, iid, *ver))?;
            let h = write_json(&d.join("items").join(format!("{iid}.enc")), &ItemFile { version: *ver, envelope: env })?;
            ve.items.insert(iid.clone(), ItemEntry { version: *ver, hash: h });
        }
        m.vaults.insert(vid.clone(), ve);
    }
    let menv = seal(&store::manifest_key(&plain.account_key), &serde_json::to_vec(&m)?,
        &format!("{}|manifest|2", fmt.account_id).into_bytes())?;
    write_json(&tmp.join("manifest.enc"), &menv)?;
    write_json(&tmp.join("format.json"), &fmt)?;
    fsutil::fsync_dir(&tmp);
    if opts.fail_after == Some(1) { bail!("injected failure after step 1"); }
    // swap into place; marker lets `recover` restore the backup if we die mid-swap
    atomic_write(&root.join(MARKER), backup.to_string_lossy().as_bytes())?;
    let old = tmp.join("old-vaults");
    if root.join("vaults").exists() { fsutil::rename(&root.join("vaults"), &old)?; }
    fsutil::rename(&tmp.join("vaults"), &root.join("vaults"))?;
    if opts.fail_after == Some(2) { bail!("injected failure after step 2"); }
    fsutil::rename(&tmp.join("keyset.enc"), &root.join("keyset.enc"))?;
    fsutil::rename(&tmp.join("manifest.enc"), &root.join("manifest.enc"))?;
    if opts.fail_after == Some(3) { bail!("injected failure after step 3"); }
    // 6. format.json LAST
    fsutil::rename(&tmp.join("format.json"), &root.join("format.json"))?;
    fs::remove_file(root.join(MARKER))?;
    fs::remove_dir_all(&tmp)?;
    atomic_write(&root.join(".gitignore"), fsutil::GITIGNORE.as_bytes())?;
    write_json(&root.join(".gitpass-state.json"), &serde_json::json!({"account_id": fmt.account_id, "epoch": 1}))?;
    drop(plain);
    // 7. one git commit
    if root.join(".git").exists() { crate::sync::commit_all(root, "migrate to format v2")?; }
    Ok(backup)
}
