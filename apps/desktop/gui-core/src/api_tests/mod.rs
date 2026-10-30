//! Contract API tests against a temp vault. All data is FAKE.
use crate::api::*;
use gitpass_core::audit::PwnedRangeClient;
use gitpass_core::crypto::KdfParams;
use gitpass_core::import::{self, ImportFormat};
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::store::{self, UnlockOpts};
use gitpass_pasteboard::PasteboardMarkers;
use gitpass_core::generate::Mode;
use std::sync::Arc;
use base64::{engine::general_purpose::STANDARD as B64, Engine};

fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }

struct MockHibp;
impl PwnedRangeClient for MockHibp {
    fn range(&self, prefix: &str) -> anyhow::Result<String> {
        // SHA1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8
        Ok(if prefix == "5BAA6" { "1E4C9B93F3F0682250B6CF8331B7EE68FD8:9999\r\n0000000000000000000000000000000000A:0".into() } else { String::new() })
    }
}

struct Env { _vault: tempfile::TempDir, _cfg: tempfile::TempDir, app: App, clip: Arc<MemClipboard>, sk: String, vault: String }

fn env() -> Env {
    let d = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), "correct horse", fast()).unwrap().encode();
    let clip = Arc::new(MemClipboard::default());
    let app = App::new(cfg.path().to_path_buf(), clip.clone()).with_hibp(Box::new(MockHibp));
    app.set_unlock_opts(UnlockOpts { accept_rollback: false, target_kdf: fast() });
    let st = app.unlock(&d.path().display().to_string(), "correct horse", Some(&sk)).unwrap();
    assert!(st.unlocked && st.account.is_some());
    // unlock auto-creates the default "Personal" vault for a fresh account
    let vs = app.list_vaults().unwrap();
    assert_eq!(vs.len(), 1); assert_eq!(vs[0].name, "Personal");
    let v = vs[0].clone();
    Env { vault: v.id, _vault: d, _cfg: cfg, app, clip, sk }
}
fn fld(id: &str, kind: FieldKind, v: Option<&str>) -> Field { Field { id: id.into(), label: id.into(), kind, value: v.map(Into::into), masked: None, diff: None } }
fn login(e: &Env, title: &str, user: &str, pw: &str) -> ItemSummary {
    e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::Login, title: title.into(), url: Some("https://example.invalid".into()),
        sections: vec![Section { id: "login".into(), title: None, fields: vec![fld("username", FieldKind::Text, Some(user)), fld("password", FieldKind::Concealed, Some(pw))] }],
        notes: None, tags: Some(vec!["work".into()]), favorite: None, related_item_ids: None }).unwrap()
}
fn card(e: &Env) -> ItemSummary {
    e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::CreditCard, title: "Example Bank".into(), url: None,
        sections: vec![Section { id: "card".into(), title: Some("Card".into()), fields: vec![fld("number", FieldKind::Concealed, Some("4111 1111 1111 1111")),
            fld("cvv", FieldKind::Concealed, Some("123")), fld("expiry", FieldKind::MonthYear, Some("12/30"))] }],
        notes: Some("fake".into()), tags: None, favorite: Some(true), related_item_ids: None }).unwrap()
}
fn api_cred(e: &Env, title: &str, user: &str, cred: &str, expires: &str) -> ItemSummary {
    e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::ApiCredential, title: title.into(), url: Some("https://api.example.invalid".into()),
        sections: vec![Section { id: "main".into(), title: None, fields: vec![
            fld("username", FieldKind::Text, Some(user)), fld("credential", FieldKind::Concealed, Some(cred)),
            fld("type", FieldKind::Text, Some("token")), fld("expires", FieldKind::Date, Some(expires))] }],
        notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap()
}

struct MemBio(std::sync::Mutex<Option<(String, String, String)>>);
impl BiometricStore for MemBio {
    fn supported(&self) -> bool { true }
    fn store(&self, v: &str, p: &str, k: &str) -> anyhow::Result<()> { *self.0.lock().unwrap() = Some((v.into(), p.into(), k.into())); Ok(()) }
    fn load(&self, v: &str, _: &str) -> anyhow::Result<(zeroize::Zeroizing<String>, zeroize::Zeroizing<String>)> {
        let g = self.0.lock().unwrap(); let (vv, p, k) = g.clone().ok_or_else(|| anyhow::anyhow!("none"))?;
        assert_eq!(vv, v); Ok((p.into(), k.into()))
    }
    fn delete(&self, _: &str) -> anyhow::Result<()> { *self.0.lock().unwrap() = None; Ok(()) }
}

mod account;
mod cards;
mod clipboard;
mod favicon;
mod import_export;
mod items;
mod settings;
mod tools;
mod watchtower;
