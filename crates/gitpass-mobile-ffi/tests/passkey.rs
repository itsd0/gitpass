//! #477: the Android passkey provider's bridge calls. Temporary vaults, fake secrets only.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use gitpass_core::crypto::KdfParams;
use gitpass_core::store;
use gitpass_mobile_ffi::{open_vault, passkey_app_rp_host, passkey_request_info, passkey_verify_caller, PasskeyCaller, PasskeyRequestInfo};
use serde_json::Value;

const PASSWORD: &str = "FAKE correct horse battery staple";
const PKG: &str = "com.example.app";
const CERT: [u8; 32] = [0x5A; 32];
const CREATE: &str = r#"{"rp":{"id":"example.com","name":"Example"},"user":{"id":"dXNlci0x","name":"alice","displayName":"Alice"},"challenge":"Y3JlYXRl","pubKeyCredParams":[{"type":"public-key","alg":-7}]}"#;
const GET: &str = r#"{"rpId":"example.com","challenge":"Z2V0","userVerification":"required"}"#;

fn dal(pkg: &str) -> String {
    let fp = CERT.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":");
    format!(r#"[{{"relation":["delegate_permission/common.get_login_creds"],"target":{{"namespace":"android_app","package_name":"{pkg}","sha256_cert_fingerprints":["{fp}"]}}}}]"#)
}
fn app(pkg: &str, links: String) -> PasskeyCaller { PasskeyCaller::App { package: pkg.into(), cert_sha256: CERT.to_vec(), asset_links_json: links } }
fn browser(origin: &str) -> PasskeyCaller { PasskeyCaller::Browser { origin: origin.into() } }

#[test]
fn begin_step_helpers_need_no_vault() {
    assert_eq!(
        passkey_request_info(CREATE.into(), true).unwrap(),
        PasskeyRequestInfo { rp_id: Some("example.com".into()), rp_name: Some("Example".into()), user_name: Some("alice".into()) }
    );
    assert_eq!(passkey_request_info(GET.into(), false).unwrap().rp_id.as_deref(), Some("example.com"));
    assert!(passkey_request_info("{}".into(), false).is_err());
    assert_eq!(passkey_app_rp_host("Example.com".into()).unwrap(), "example.com");
    assert!(passkey_app_rp_host("co.uk".into()).is_err());
    assert_eq!(passkey_verify_caller(app(PKG, dal(PKG)), Some("example.com".into())).unwrap(), "example.com");
    assert!(passkey_verify_caller(app("com.evil.app", dal(PKG)), Some("example.com".into())).is_err());
    assert!(passkey_verify_caller(browser("https://accounts.example.com"), Some("example.com".into())).is_ok());
    assert!(passkey_verify_caller(browser("https://example.com.evil.io"), Some("example.com".into())).is_err());
}

#[test]
fn register_list_assert_and_refuse_other_callers() {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PASSWORD, KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 }).unwrap();
    let v = open_vault(d.path().to_string_lossy().into(), PASSWORD.into(), sk.encode()).unwrap();
    v.create_vault("Personal".into(), None).unwrap();

    assert!(v.passkey_register(app("com.evil.app", dal(PKG)), CREATE.into(), None).is_err(), "an app the site does not list cannot create");
    let reg: Value = serde_json::from_str(&v.passkey_register(app(PKG, dal(PKG)), CREATE.into(), None).unwrap()).unwrap();
    let id = reg["id"].as_str().unwrap().to_string();
    let cdj: Value = serde_json::from_slice(&B64.decode(reg["response"]["clientDataJSON"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(cdj["origin"], format!("android:apk-key-hash:{}", B64.encode(CERT)));

    let entries = v.passkey_entries(app(PKG, dal(PKG)), GET.into()).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!((entries[0].credential_id.as_str(), entries[0].user_name.as_str()), (id.as_str(), "alice"));
    assert!(v.passkey_entries(app("com.evil.app", dal("com.evil.app").replace("com.evil.app", PKG)), GET.into()).is_err());
    assert!(v.passkey_entries(app(PKG, String::new()), GET.into()).is_err(), "DAL fetch failed: refused");
    assert!(v.passkey_entries(browser("https://example.com.evil.io"), GET.into()).is_err());
    let other_rp = GET.replace("example.com", "example.org");
    assert!(v.passkey_entries(browser("https://example.org"), other_rp.clone()).unwrap().is_empty());

    let a: Value = serde_json::from_str(&v.passkey_assert(app(PKG, dal(PKG)), GET.into(), id.clone(), None).unwrap()).unwrap();
    assert_eq!(a["id"], id);
    assert_eq!(a["response"]["userHandle"], "dXNlci0x");
    let b: Value = serde_json::from_str(&v.passkey_assert(browser("https://www.example.com"), GET.into(), id.clone(), Some(vec![7; 32])).unwrap()).unwrap();
    assert_eq!(b["id"], id);
    assert!(v.passkey_assert(browser("https://example.org"), other_rp, id.clone(), None).is_err(), "never signs for another RP");
    assert!(v.passkey_assert(app("com.evil.app", dal(PKG)), GET.into(), id, None).is_err());
}
