//! #457: pairing test vector and a joiner/host round trip through the bridge.
//! Every secret is fake. The process holds one joiner and one host, so this file
//! takes a lock around the session calls.
use gitpass_mobile_ffi::{
    pairing_host_accept, pairing_host_release, pairing_joiner_begin, pairing_joiner_code,
    pairing_joiner_open, pairing_test_vector, pairing_test_vector_open, PairingHostInput,
};
use std::sync::Mutex;

static SESSIONS: Mutex<()> = Mutex::new(());

#[test]
fn vector_and_bridge_round_trip_keep_secrets_out_of_errors() {
    let _guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    let opened = pairing_test_vector_open().unwrap();
    assert_eq!(opened.repo_owner, "fake-owner");
    assert_eq!(opened.repo_name, "fake-vault");
    assert_eq!(opened.branch, "main");
    assert_eq!(opened.account_id, "00000000-0000-4000-8000-000000000001");
    assert_eq!(opened.sync_token, "FAKE-github-pat-pairing-test-vector");
    assert!(opened.secret_key.starts_with("A3-"));
    let json = pairing_test_vector().unwrap();
    assert!(json.contains("gitpass-pair:1:offer:"));
    assert!(json.contains("gitpass-pair:1:announce:"));
    assert!(json.contains("gitpass-pair:1:grant:"));
    assert!(!json.contains("password"));
    assert!(!json.contains("FAKE-master-password"));

    // Host and joiner keep separate replay records; they are different devices.
    let host_dir = tempfile::tempdir().unwrap();
    let joiner_dir = tempfile::tempdir().unwrap();
    let host_state = host_dir.path().to_str().unwrap().to_string();
    let joiner_state = joiner_dir.path().to_str().unwrap().to_string();
    let offer = pairing_joiner_begin().unwrap();
    let token = "FAKE-github-pat-bridge-roundtrip";
    let view = pairing_host_accept(PairingHostInput {
        state_dir: host_state.clone(),
        offer,
        account_id: opened.account_id.clone(),
        repo_owner: "fake-owner".into(),
        repo_name: "fake-vault".into(),
        branch: "main".into(),
        secret_key: opened.secret_key.clone(),
        sync_token: token.into(),
    })
    .unwrap();
    let bad = pairing_host_accept(PairingHostInput {
        state_dir: host_state.clone(),
        offer: "not-a-frame".into(),
        account_id: opened.account_id.clone(),
        repo_owner: "fake-owner".into(),
        repo_name: "fake-vault".into(),
        branch: "main".into(),
        secret_key: opened.secret_key.clone(),
        sync_token: token.into(),
    });
    let bad = bad.unwrap_err();
    assert_eq!(bad.to_string(), "unrecognized pairing message");
    assert!(!bad.to_string().contains(token));
    let code = pairing_joiner_code(view.announce.clone()).unwrap();
    assert_eq!(code, view.code);
    let wrong = if code == "000000" { "000001" } else { "000000" };
    let err = pairing_host_release(host_state.clone(), wrong.into()).unwrap_err();
    assert_eq!(err.to_string(), "pairing verification code does not match");
    assert!(!err.to_string().contains(token));
    assert!(!err.to_string().contains(&opened.secret_key));
    let grant = pairing_host_release(host_state, code.clone()).unwrap();
    assert!(!grant.contains(token));
    assert!(!grant.contains(&opened.secret_key));
    let payload = pairing_joiner_open(joiner_state, grant, code).unwrap();
    assert_eq!(payload.sync_token, token);
    assert_eq!(payload.secret_key, opened.secret_key);
    assert_eq!(payload.repo_owner, "fake-owner");
}
