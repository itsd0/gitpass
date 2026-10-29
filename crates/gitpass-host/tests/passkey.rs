//! passkey_create / passkey_get in standalone (fallback) mode, with real ES256 verification.
mod common;
use common::*;
use gitpass_core::passkey::unb64;
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const ORIGIN: &str = "https://login.example.com";

fn create(h: &mut gitpass_host::handler::Host, extra: Value) -> Value {
    let mut q = json!({"cmd":"passkey_create","source":"popup","approved":true,"origin":ORIGIN,"rp_id":"example.com",
        "challenge":"Y2hhbGxlbmdlLTE","user_id":"dXNlci0x","user_name":"alice@example.com","algorithms":[-7,-257]});
    for (k, v) in extra.as_object().unwrap() { q[k] = v.clone(); }
    ok(h, q)
}

/// Pull the COSE x/y out of attestation authData (fixed layout for our EC2 key) -> verifying key.
fn key_from_auth_data(ad: &[u8]) -> (Vec<u8>, VerifyingKey) {
    assert_eq!(ad[32] & 0x41, 0x41, "UP+AT set");
    let n = u16::from_be_bytes([ad[53], ad[54]]) as usize;
    let cred = ad[55..55 + n].to_vec();
    let cose = &ad[55 + n..];
    // a5 01 02 03 26 20 01 21 58 20 <x32> 22 58 20 <y32>
    assert_eq!(&cose[..10], &[0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21, 0x58, 0x20]);
    let x = &cose[10..42]; assert_eq!(&cose[42..45], &[0x22, 0x58, 0x20]); let y = &cose[45..77];
    let mut sec1 = vec![4u8]; sec1.extend_from_slice(x); sec1.extend_from_slice(y);
    (cred, VerifyingKey::from_sec1_bytes(&sec1).unwrap())
}

#[test]
fn create_then_get_signature_verifies() {
    let mut f = v5_vault();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    let c = create(&mut f.host, json!({}));
    let cdj: Value = serde_json::from_slice(&unb64(c["client_data_json"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(cdj["type"], "webauthn.create"); assert_eq!(cdj["origin"], ORIGIN); assert_eq!(cdj["challenge"], "Y2hhbGxlbmdlLTE");
    let ad = unb64(c["authenticator_data"].as_str().unwrap()).unwrap();
    assert_eq!(&ad[..32], Sha256::digest(b"example.com").as_slice());
    let (cred, vk) = key_from_auth_data(&ad);
    assert_eq!(cred, unb64(c["credential_id"].as_str().unwrap()).unwrap());
    let spki = VerifyingKey::from_public_key_der(&unb64(c["public_key"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(spki, vk);

    for round in 1..=2u32 {
        let g = ok(&mut f.host, json!({"cmd":"passkey_get","source":"popup","approved":true,"origin":ORIGIN,"rp_id":"example.com",
            "challenge":"Y2hhbGxlbmdlLTI","allow_credentials":[c["credential_id"]]}));
        let ad = unb64(g["authenticator_data"].as_str().unwrap()).unwrap();
        let cdj = unb64(g["client_data_json"].as_str().unwrap()).unwrap();
        assert_eq!(u32::from_be_bytes(ad[33..37].try_into().unwrap()), round, "signCount increments");
        let mut msg = ad.clone(); msg.extend_from_slice(&Sha256::digest(&cdj));
        let sig = Signature::from_der(&unb64(g["signature"].as_str().unwrap()).unwrap()).unwrap();
        vk.verify(&msg, &sig).expect("assertion signature verifies");
        msg[40] ^= 1; assert!(vk.verify(&msg, &sig).is_err());
        assert_eq!(unb64(g["user_handle"].as_str().unwrap()).unwrap(), b"user-1");
    }
    // the item is listed as a passkey and its private key never leaves via list/detail
    let l = ok(&mut f.host, json!({"cmd":"list_items","category":"passkey"}));
    assert_eq!(l["items"].as_array().unwrap().len(), 1);
    let d = ok(&mut f.host, json!({"cmd":"get_item","item_id":c["item_id"]}));
    let pkf = d["item"]["fields"].as_array().unwrap().iter().find(|x| x["name"] == "private_key").unwrap().clone();
    assert_eq!(pkf["concealed"], true); assert!(pkf.get("value").is_none() || pkf["value"].is_null());
}

#[test]
fn policy_approval_rpid_exclude() {
    let mut f = v5_vault();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    let base = json!({"cmd":"passkey_create","origin":ORIGIN,"challenge":"YQ","user_id":"dQ"});
    assert_eq!(err(&mut f.host, base.clone()).0, "denied");
    let mut q = base.clone(); q["source"] = json!("content"); q["approved"] = json!(true);
    assert_eq!(err(&mut f.host, q).0, "denied", "only the popup may approve");
    let mut q = base.clone(); q["source"] = json!("popup"); q["approved"] = json!(true); q["rp_id"] = json!("evil.com");
    assert_eq!(err(&mut f.host, q).0, "origin_mismatch");
    let mut q = base.clone(); q["source"] = json!("popup"); q["approved"] = json!(true); q["rp_id"] = json!("com");
    assert_eq!(err(&mut f.host, q).0, "origin_mismatch", "public suffix rpId refused");
    let mut q = base.clone(); q["source"] = json!("popup"); q["approved"] = json!(true); q["origin"] = json!("http://example.com");
    assert_eq!(err(&mut f.host, q).0, "origin_mismatch");
    let mut q = base.clone(); q["source"] = json!("popup"); q["approved"] = json!(true); q["algorithms"] = json!([-257]);
    assert_eq!(err(&mut f.host, q).0, "not_supported");
    let c = create(&mut f.host, json!({}));
    let mut q = base.clone(); q["source"] = json!("popup"); q["approved"] = json!(true); q["rp_id"] = json!("example.com"); q["exclude_credentials"] = json!([c["credential_id"]]);
    assert_eq!(err(&mut f.host, q).0, "exists");
    assert_eq!(err(&mut f.host, json!({"cmd":"passkey_get","source":"popup","approved":true,"origin":"https://other.org","challenge":"YQ"})).0, "not_found");
    assert_eq!(err(&mut f.host, json!({"cmd":"passkey_get","origin":ORIGIN,"rp_id":"example.com","challenge":"YQ"})).0, "denied");
    ok(&mut f.host, json!({"cmd":"lock"}));
    assert_eq!(err(&mut f.host, json!({"cmd":"passkey_get","source":"popup","approved":true,"origin":ORIGIN,"rp_id":"example.com","challenge":"YQ"})).0, "locked");
    // localhost works for local demo pages
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    let l = ok(&mut f.host, json!({"cmd":"passkey_create","source":"popup","approved":true,"origin":"http://localhost:8123","challenge":"YQ","user_id":"dQ","user_name":"x"}));
    assert_eq!(l["rp_id"], "localhost");

    // passkey_query returns real username without requiring popup approval
    let q = ok(&mut f.host, json!({"cmd":"passkey_query","origin":ORIGIN,"rp_id":"example.com"}));
    let matches = q["matching"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["user_name"], "alice@example.com");

    // query for unknown rp returns empty matching list
    let q_empty = ok(&mut f.host, json!({"cmd":"passkey_query","origin":"https://other.example","rp_id":"other.example"}));
    assert_eq!(q_empty["matching"].as_array().unwrap().len(), 0);
}


#[test]
fn multiple_passkeys_query_choice_and_last_used() {
    let mut f = v5_vault();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    let c1 = create(&mut f.host, json!({"user_id":"dXNlci0x","user_name":"alice@example.com"}));
    let c2 = create(&mut f.host, json!({"user_id":"dXNlci0y","user_name":"bob@example.com"}));

    // Query returns both passkeys with vault_name
    let q = ok(&mut f.host, json!({"cmd":"passkey_query","origin":ORIGIN,"rp_id":"example.com"}));
    let matches = q["matching"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    assert_ne!(c1["credential_id"], c2["credential_id"]);
    assert_eq!(matches[0]["vault_name"], "Personal");
    assert_eq!(matches[1]["vault_name"], "Personal");

    // Passkey get with credential_id chooses specific passkey
    let g2 = ok(&mut f.host, json!({
        "cmd":"passkey_get",
        "source":"popup",
        "approved":true,
        "origin":ORIGIN,
        "rp_id":"example.com",
        "challenge":"Y2hhbGxlbmdlLTI",
        "credential_id": c2["credential_id"]
    }));
    assert_eq!(g2["credential_id"], c2["credential_id"]);
    assert_eq!(unb64(g2["user_handle"].as_str().unwrap()).unwrap(), b"user-2");

    // Query now returns c2 first because it was most recently used!
    let q2 = ok(&mut f.host, json!({"cmd":"passkey_query","origin":ORIGIN,"rp_id":"example.com"}));
    let matches2 = q2["matching"].as_array().unwrap();
    assert_eq!(matches2[0]["credential_id"], c2["credential_id"]);
    assert_eq!(matches2[0]["user_name"], "bob@example.com");
    assert!(matches2[0]["last_used"].as_u64().unwrap() > 0);

    // Invalid base64url credential_id rejected with bad_request
    let (err_code, _) = err(&mut f.host, json!({
        "cmd":"passkey_get",
        "source":"popup",
        "approved":true,
        "origin":ORIGIN,
        "rp_id":"example.com",
        "challenge":"Y2hhbGxlbmdlLTI",
        "credential_id": "not base64url!!!"
    }));
    assert_eq!(err_code, "bad_request");

    // Unknown credential_id returns not_found
    let (err_code, _) = err(&mut f.host, json!({
        "cmd":"passkey_get",
        "source":"popup",
        "approved":true,
        "origin":ORIGIN,
        "rp_id":"example.com",
        "challenge":"Y2hhbGxlbmdlLTI",
        "credential_id": "AAAA"
    }));
    assert_eq!(err_code, "not_found");
}

#[test]
fn saved_passkey_shows_in_site_match_only_when_requested() {
    let mut f = v5_vault();
    ok(&mut f.host, json!({"cmd":"unlock","password":PW}));
    let c = create(&mut f.host, json!({}));
    let id = c["item_id"].as_str().unwrap().to_string();
    let has = |v: &Value| v["items"].as_array().unwrap().iter().any(|i| i["id"] == id.as_str() && i["category"] == "passkey");
    // Popup site list (include_passkeys): the rp's own host, subdomains of the rpId, not other sites.
    for o in ["https://example.com", ORIGIN, "https://www.example.com"] {
        assert!(has(&ok(&mut f.host, json!({"cmd":"match","origin":o,"include_passkeys":true}))), "{o}");
    }
    assert!(!has(&ok(&mut f.host, json!({"cmd":"match","origin":"https://other.example","include_passkeys":true}))));
    assert!(!has(&ok(&mut f.host, json!({"cmd":"match","origin":"https://notexample.com","include_passkeys":true}))));
    // Fill flows (no flag) keep getting logins only.
    assert!(!has(&ok(&mut f.host, json!({"cmd":"match","origin":ORIGIN}))));
}

#[test]
fn rp_matches_origin_rules() {
    use gitpass_host::passkey::rp_matches_origin as m;
    assert!(m("github.com", "https://github.com"));
    assert!(m("github.com", "https://www.github.com"));
    assert!(m("GitHub.com.", "https://github.com"));
    assert!(!m("www.github.com", "https://github.com"));
    assert!(!m("github.com", "https://evilgithub.com"));
    assert!(!m("com", "https://github.com"));
    assert!(!m("", "https://github.com"));
    assert!(!m("github.com", "http://github.com"));
}
