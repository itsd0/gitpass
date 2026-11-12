//! #477: the shared WebAuthn authenticator (extension host, desktop agent, Android provider).
//! Temporary vaults and fake secrets only.
use gitpass_core::crypto::KdfParams;
use gitpass_core::passkey::{b64, unb64};
use gitpass_core::store::{self, Session, UnlockOpts};
use gitpass_core::webauthn::{self as wa, ErrorKind};
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use sha2::{Digest, Sha256};

const CERT: [u8; 32] = [0xAB; 32];
const PKG: &str = "com.example.app";

fn session() -> (tempfile::TempDir, Session) {
    let d = tempfile::tempdir().unwrap();
    let fast = KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 };
    let sk = store::init(d.path(), "fake-pw", fast.clone()).unwrap();
    let s = Session::unlock_with(d.path(), "fake-pw", &sk, &UnlockOpts { accept_rollback: false, target_kdf: fast }).unwrap();
    s.create_vault("Personal").unwrap();
    (d, s)
}

fn fp(cert: &[u8]) -> String { cert.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":") }
fn dal(pkg: &str, cert: &[u8], relation: &str) -> String {
    serde_json::json!([{"relation": [relation], "target": {"namespace": "android_app", "package_name": pkg, "sha256_cert_fingerprints": [fp(cert)]}}]).to_string()
}
fn kind(e: anyhow::Error) -> ErrorKind { wa::error_kind(&e).unwrap_or_else(|| panic!("not a webauthn error: {e}")) }

#[test]
fn web_rp_rules() {
    assert_eq!(wa::verify_web("https://login.example.com", None).unwrap().rp_id(), "login.example.com");
    let rp = wa::verify_web("https://login.example.com/", Some("Example.COM.")).unwrap();
    assert_eq!((rp.rp_id(), rp.origin()), ("example.com", "https://login.example.com"));
    assert_eq!(kind(wa::verify_web("https://example.com.evil.io", Some("example.com")).unwrap_err()), ErrorKind::OriginMismatch);
    assert_eq!(kind(wa::verify_web("https://evil.co.uk", Some("co.uk")).unwrap_err()), ErrorKind::OriginMismatch, "public suffix rpId");
    assert_eq!(kind(wa::verify_web("http://example.com", None).unwrap_err()), ErrorKind::OriginMismatch, "plain http");
    assert_eq!(kind(wa::verify_web("android:apk-key-hash:AAAA", Some("example.com")).unwrap_err()), ErrorKind::OriginMismatch, "app origins never pass the web rule");
    assert!(wa::verify_web("http://localhost:8080", None).is_ok());
}

#[test]
fn app_rp_and_asset_links() {
    assert_eq!(wa::app_rp_host("Login.Example.com.").unwrap(), "login.example.com");
    for bad in ["co.uk", "com", "localhost", "1.2.3.4", "example.com:443", "example.com/path", "user@example.com", "", "exa mple.com"] {
        assert!(wa::app_rp_host(bad).is_err(), "{bad}");
    }
    let good = dal(PKG, &CERT, wa::GET_LOGIN_CREDS);
    assert!(wa::asset_links_allow(&good, PKG, &CERT));
    // lower-case / no-colon fingerprints are the same certificate
    assert!(wa::asset_links_allow(&good.to_lowercase().replace("com.example.app", PKG), PKG, &CERT));
    assert!(!wa::asset_links_allow(&good, "com.example.app.evil", &CERT), "other package");
    assert!(!wa::asset_links_allow(&good, PKG, &[0xAC; 32]), "other signing certificate");
    assert!(!wa::asset_links_allow(&dal(PKG, &CERT, "delegate_permission/common.handle_all_urls"), PKG, &CERT), "only get_login_creds counts");
    let web_target = serde_json::json!([{"relation": [wa::GET_LOGIN_CREDS], "target": {"namespace": "web", "site": "https://example.com"}}]).to_string();
    assert!(!wa::asset_links_allow(&web_target, PKG, &CERT));
    assert!(!wa::asset_links_allow("not json", PKG, &CERT));
    assert!(!wa::asset_links_allow("", PKG, &CERT));
    assert!(!wa::asset_links_allow(&good, PKG, &CERT[..31]));

    let rp = wa::verify_app(PKG, &CERT, Some("example.com"), &good).unwrap();
    assert_eq!(rp.rp_id(), "example.com");
    assert_eq!(rp.origin(), format!("android:apk-key-hash:{}", b64(&CERT)));
    assert_eq!(kind(wa::verify_app(PKG, &CERT, None, &good).unwrap_err()), ErrorKind::BadRequest, "apps must name the rpId");
    assert_eq!(kind(wa::verify_app("com.other.app", &CERT, Some("example.com"), &good).unwrap_err()), ErrorKind::OriginMismatch);
    assert_eq!(kind(wa::verify_app(PKG, &CERT, Some("example.com"), "").unwrap_err()), ErrorKind::OriginMismatch, "no DAL file, no passkey");
    assert_eq!(kind(wa::verify_app("not a package", &CERT, Some("example.com"), &good).unwrap_err()), ErrorKind::OriginMismatch);
}

const CREATE: &str = r#"{"rp":{"id":"example.com","name":"Example"},"user":{"id":"dXNlci0x","name":"alice@example.com","displayName":"Alice"},
  "challenge":"Y2hhbGxlbmdlLWNyZWF0ZQ","pubKeyCredParams":[{"type":"public-key","alg":-8},{"type":"public-key","alg":-7}],
  "excludeCredentials":[],"authenticatorSelection":{"residentKey":"required","userVerification":"required"}}"#;

#[test]
fn request_parsing() {
    let c = wa::parse_create_options(CREATE).unwrap();
    assert_eq!(c.rp_id.as_deref(), Some("example.com"));
    assert_eq!(c.rp_name.as_deref(), Some("Example"));
    assert_eq!(c.user_id, b"user-1");
    assert_eq!(c.user_name, "alice@example.com");
    assert_eq!(c.algorithms, Some(vec![-8, -7]));
    assert_eq!(c.challenge, "Y2hhbGxlbmdlLWNyZWF0ZQ");
    let g = wa::parse_get_options(r#"{"challenge":"YWJj","rpId":"Example.com","allowCredentials":[{"type":"public-key","id":"AQID"}],"userVerification":"preferred"}"#).unwrap();
    assert_eq!((g.rp_id.as_deref(), g.allow.clone()), (Some("example.com"), vec![vec![1, 2, 3]]));
    assert!(wa::parse_get_options(r#"{"challenge":"YWJj"}"#).unwrap().rp_id.is_none());
    for bad in ["", "[]", "{}", r#"{"challenge":"!!"}"#, r#"{"challenge":""}"#, r#"{"challenge":"YWJj","allowCredentials":[{"type":"public-key"}]}"#, r#"{"challenge":"YWJj","allowCredentials":"x"}"#] {
        assert_eq!(kind(wa::parse_get_options(bad).unwrap_err()), ErrorKind::BadRequest, "{bad}");
    }
    for bad in [r#"{"user":{"id":"YQ"},"challenge":"YWJj"}"#, r#"{"rp":{},"challenge":"YWJj","user":{}}"#, r#"{"rp":{},"user":{"id":"YQ"}}"#] {
        assert_eq!(kind(wa::parse_create_options(bad).unwrap_err()), ErrorKind::BadRequest, "{bad}");
    }
    assert!(wa::parse_get_options(&format!(r#"{{"challenge":"YWJj","pad":"{}"}}"#, "x".repeat(70_000))).is_err(), "size limit");
}

fn verify(spki: &[u8], ad: &[u8], hash: &[u8], sig: &[u8]) -> bool {
    let vk = VerifyingKey::from_public_key_der(spki).unwrap();
    let mut msg = ad.to_vec();
    msg.extend_from_slice(hash);
    vk.verify(&msg, &Signature::from_der(sig).unwrap()).is_ok()
}

#[test]
fn app_register_then_assert_with_built_and_supplied_client_data() {
    let (_d, s) = session();
    let links = dal(PKG, &CERT, wa::GET_LOGIN_CREDS);
    let c = wa::parse_create_options(CREATE).unwrap();
    let rp = wa::verify_app(PKG, &CERT, c.rp_id.as_deref(), &links).unwrap();
    let reg = wa::create(&s, &rp, &c, None).unwrap();
    let cdj: serde_json::Value = serde_json::from_slice(&reg.client_data_json).unwrap();
    assert_eq!(cdj["type"], "webauthn.create");
    assert_eq!(cdj["origin"], rp.origin());
    assert_eq!(cdj["challenge"], c.challenge);
    let j = reg.to_webauthn_json();
    assert_eq!(j["type"], "public-key");
    assert_eq!(j["id"], b64(&reg.credential_id));
    assert_eq!(j["response"]["publicKeyAlgorithm"], -7);
    assert_eq!(unb64(j["response"]["attestationObject"].as_str().unwrap()).unwrap(), reg.attestation_object);

    // exclusion and unsupported algorithms
    let mut again = c.clone();
    again.exclude = vec![reg.credential_id.clone()];
    assert_eq!(kind(wa::create(&s, &rp, &again, None).unwrap_err()), ErrorKind::Exists);
    let mut eddsa = c.clone();
    eddsa.algorithms = Some(vec![-8]);
    assert_eq!(kind(wa::create(&s, &rp, &eddsa, None).unwrap_err()), ErrorKind::NotSupported);

    let get = wa::parse_get_options(r#"{"challenge":"c2lnbi1pbg","rpId":"example.com"}"#).unwrap();
    let cands = wa::candidates(&s, &rp, &get.allow).unwrap();
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].user_name, "alice@example.com");

    let a = wa::get(&s, &rp, &get, Some(&reg.credential_id), None).unwrap();
    assert_eq!(a.sign_count, 1);
    assert_eq!(a.user_handle, b"user-1");
    assert_eq!(&a.authenticator_data[..32], Sha256::digest(b"example.com").as_slice(), "rpIdHash");
    assert!(verify(&reg.public_key, &a.authenticator_data, &Sha256::digest(&a.client_data_json), &a.signature));
    let cdj: serde_json::Value = serde_json::from_slice(&a.client_data_json).unwrap();
    assert_eq!((cdj["type"].as_str(), cdj["origin"].as_str()), (Some("webauthn.get"), Some(rp.origin())));
    let j = a.to_webauthn_json();
    assert_eq!(j["response"]["userHandle"], b64(b"user-1"));

    // Privileged browser path: the caller supplies SHA-256(clientDataJSON).
    let browser = wa::verify_web("https://www.example.com", Some("example.com")).unwrap();
    let hash = Sha256::digest(b"browser-built clientDataJSON");
    let b = wa::get(&s, &browser, &get, None, Some(hash.as_slice())).unwrap();
    assert_eq!(b.sign_count, 2, "sign count increases");
    assert!(verify(&reg.public_key, &b.authenticator_data, &hash, &b.signature));
    assert_eq!(kind(wa::get(&s, &browser, &get, None, Some(&[1, 2, 3])).unwrap_err()), ErrorKind::BadRequest);

    // Never across RPs or outside allowCredentials.
    let other = wa::verify_web("https://example.org", None).unwrap();
    assert_eq!(kind(wa::get(&s, &other, &get, None, None).unwrap_err()), ErrorKind::NotFound);
    let mut only_other = get.clone();
    only_other.allow = vec![vec![9; 32]];
    assert_eq!(kind(wa::get(&s, &rp, &only_other, Some(&reg.credential_id), None).unwrap_err()), ErrorKind::NotFound);
}
