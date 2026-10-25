//! On-disk vault layout and envelope hierarchy: AUK -> Account Key -> Vault Key -> Item.
use crate::crypto::*;
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Format {
    pub version: u32,
    pub kdf: KdfParams,
    pub salt: String,
    pub account_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, Zeroize, ZeroizeOnDrop)]
pub struct Item {
    pub title: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub totp: Option<String>,
    pub updated_at: String,
}

/// Item file: plaintext version (needed to rebuild AAD) + encrypted envelope.
#[derive(Serialize, Deserialize, Debug)]
pub struct ItemFile {
    pub version: u64,
    pub envelope: Envelope,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VaultMeta {
    pub name: String,
}

pub fn item_aad(vault_id: &str, item_id: &str, version: u64) -> Vec<u8> {
    format!("{vault_id}|{item_id}|{version}").into_bytes()
}
fn keyset_aad(account_id: &str) -> Vec<u8> { format!("{account_id}|keyset|1").into_bytes() }
fn vault_key_aad(vault_id: &str) -> Vec<u8> { format!("{vault_id}|vault-key|1").into_bytes() }

fn write_json<T: Serialize>(p: &Path, v: &T) -> Result<()> {
    if let Some(d) = p.parent() { fs::create_dir_all(d)?; }
    let tmp = p.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    fs::rename(&tmp, p)?;
    Ok(())
}
fn read_json<T: for<'de> Deserialize<'de>>(p: &Path) -> Result<T> {
    let b = fs::read(p).with_context(|| format!("reading {}", p.display()))?;
    Ok(serde_json::from_slice(&b)?)
}

/// Create a new account. Returns the generated Secret Key.
pub fn init(root: &Path, password: &str, kdf: KdfParams) -> Result<SecretKey> {
    if root.join("format.json").exists() { bail!("already initialized at {}", root.display()); }
    fs::create_dir_all(root.join("vaults"))?;
    let salt: [u8; 16] = random_bytes();
    let account_id = uuid::Uuid::new_v4().to_string();
    let secret = SecretKey::generate();
    let auk = derive_auk(password, &secret, &salt, &account_id, &kdf)?;
    let account_key = Key32::random();
    let env = wrap_key(&auk, &account_key, &keyset_aad(&account_id))?;
    let fmt = Format { version: 1, kdf, salt: B64.encode(salt), account_id };
    write_json(&root.join("format.json"), &fmt)?;
    write_json(&root.join("keyset.enc"), &env)?;
    fs::write(root.join(".gitignore"), "secret.key\n*.tmp\n")?;
    Ok(secret)
}

/// Unlocked session holding the Account Key.
pub struct Session {
    pub root: PathBuf,
    pub fmt: Format,
    account_key: Key32,
}

impl Session {
    pub fn unlock(root: &Path, password: &str, secret: &SecretKey) -> Result<Self> {
        let fmt: Format = read_json(&root.join("format.json")).context("not initialized? run `gitpass init`")?;
        if fmt.version != 1 { bail!("unsupported format version {}", fmt.version); }
        let salt = B64.decode(&fmt.salt)?;
        let auk = derive_auk(password, secret, &salt, &fmt.account_id, &fmt.kdf)?;
        let env: Envelope = read_json(&root.join("keyset.enc"))?;
        let account_key = unwrap_key(&auk, &env, &keyset_aad(&fmt.account_id))
            .map_err(|_| anyhow!("unlock failed: wrong master password or Secret Key"))?;
        Ok(Session { root: root.to_path_buf(), fmt, account_key })
    }

    fn vdir(&self, id: &str) -> PathBuf { self.root.join("vaults").join(id) }

    pub fn create_vault(&self, name: &str) -> Result<String> {
        if self.list_vaults()?.iter().any(|(_, m)| m.name == name) { bail!("vault '{name}' already exists"); }
        let id = uuid::Uuid::new_v4().to_string();
        let vk = Key32::random();
        let env = wrap_key(&self.account_key, &vk, &vault_key_aad(&id))?;
        let d = self.vdir(&id);
        fs::create_dir_all(d.join("items"))?;
        write_json(&d.join("vault.key.enc"), &env)?;
        write_json(&d.join("meta.json"), &VaultMeta { name: name.into() })?;
        fs::write(d.join("items/.keep"), "")?;
        Ok(id)
    }

    pub fn list_vaults(&self) -> Result<Vec<(String, VaultMeta)>> {
        let mut out = vec![];
        let d = self.root.join("vaults");
        if !d.exists() { return Ok(out); }
        for e in fs::read_dir(d)? {
            let e = e?;
            if e.path().join("meta.json").exists() {
                out.push((e.file_name().to_string_lossy().into(), read_json(&e.path().join("meta.json"))?));
            }
        }
        out.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        Ok(out)
    }

    /// Resolve a vault by name or id.
    pub fn resolve_vault(&self, q: &str) -> Result<String> {
        self.list_vaults()?
            .into_iter()
            .find(|(id, m)| m.name == q || id == q)
            .map(|(id, _)| id)
            .ok_or_else(|| anyhow!("no vault named '{q}'"))
    }

    fn vault_key(&self, vid: &str) -> Result<Key32> {
        let env: Envelope = read_json(&self.vdir(vid).join("vault.key.enc"))?;
        unwrap_key(&self.account_key, &env, &vault_key_aad(vid))
    }

    fn write_item(&self, vid: &str, iid: &str, version: u64, item: &Item) -> Result<()> {
        let vk = self.vault_key(vid)?;
        let mut pt = serde_json::to_vec(item)?;
        let env = seal(&vk, &pt, &item_aad(vid, iid, version));
        pt.zeroize();
        write_json(&self.vdir(vid).join("items").join(format!("{iid}.enc")), &ItemFile { version, envelope: env? })
    }

    pub fn add_item(&self, vid: &str, item: &Item) -> Result<String> {
        let iid = uuid::Uuid::new_v4().to_string();
        self.write_item(vid, &iid, 1, item)?;
        Ok(iid)
    }

    pub fn read_item(&self, vid: &str, iid: &str) -> Result<(u64, Item)> {
        let f: ItemFile = read_json(&self.vdir(vid).join("items").join(format!("{iid}.enc")))?;
        let vk = self.vault_key(vid)?;
        let mut pt = open(&vk, &f.envelope, &item_aad(vid, iid, f.version))?;
        let item: Item = serde_json::from_slice(&pt)?;
        pt.zeroize();
        Ok((f.version, item))
    }

    pub fn update_item(&self, vid: &str, iid: &str, item: &Item) -> Result<()> {
        let (v, _) = self.read_item(vid, iid)?;
        self.write_item(vid, iid, v + 1, item)
    }

    pub fn remove_item(&self, vid: &str, iid: &str) -> Result<()> {
        let p = self.vdir(vid).join("items").join(format!("{iid}.enc"));
        if !p.exists() { bail!("no item {iid}"); }
        fs::remove_file(p)?;
        Ok(())
    }

    pub fn list_items(&self, vid: &str) -> Result<Vec<(String, Item)>> {
        let mut out = vec![];
        for e in fs::read_dir(self.vdir(vid).join("items"))? {
            let p = e?.path();
            if p.extension().and_then(|s| s.to_str()) == Some("enc") {
                let iid = p.file_stem().unwrap().to_string_lossy().to_string();
                let (_, item) = self.read_item(vid, &iid)?;
                out.push((iid, item));
            }
        }
        out.sort_by(|a, b| a.1.title.to_lowercase().cmp(&b.1.title.to_lowercase()));
        Ok(out)
    }

    /// Find item(s) by id, id prefix, or case-insensitive title substring.
    pub fn find_items(&self, vid: &str, q: &str) -> Result<Vec<(String, Item)>> {
        let ql = q.to_lowercase();
        let all = self.list_items(vid)?;
        if let Some(exact) = all.iter().find(|(id, _)| id == q) { return Ok(vec![exact.clone()]); }
        Ok(all.into_iter().filter(|(id, it)| id.starts_with(q) || it.title.to_lowercase().contains(&ql)).collect())
    }

    pub fn resolve_item(&self, vid: &str, q: &str) -> Result<String> {
        let m = self.find_items(vid, q)?;
        match m.len() {
            0 => bail!("no item matches '{q}'"),
            1 => Ok(m[0].0.clone()),
            n => bail!("{n} items match '{q}'; use the item id"),
        }
    }
}

pub fn now() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{s}")
}
