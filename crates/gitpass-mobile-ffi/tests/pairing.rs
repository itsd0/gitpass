use gitpass_mobile_ffi::{
    pairing_host_begin, pairing_host_cancel, pairing_host_confirm, pairing_host_push, pairing_joiner_cancel,
    pairing_joiner_code, pairing_joiner_endpoint, pairing_joiner_finish, pairing_joiner_push, pairing_joiner_start,
    pairing_test_vector, pairing_test_vector_open, PairingHostInput,
};
use serde_json::json;

fn addresses() -> String {
    json!(["127.0.0.1"]).to_string()
}

#[test]
fn bridge_round_trip_hides_secrets_and_checks_state_dir() {
    let opened = pairing_test_vector_open().unwrap();
    assert_eq!(opened.repo_owner(), "fake-owner");
    assert_eq!(opened.repo_name(), "fake-vault");
    assert_eq!(opened.branch(), "main");
    assert_eq!(opened.token_kind(), "fine_grained_pat");
    assert_eq!(opened.sync_token(), "github_pat_FAKE_pairing_test_vector");
    assert!(opened.secret_key().starts_with("A3-"));
    let rendered = format!("{opened:?} {opened}");
    assert!(!rendered.contains("github_pat_"));
    assert!(!rendered.contains(&opened.secret_key()));
    let json = pairing_test_vector().unwrap();
    assert!(json.contains("gitpass-pair:2:qr:"));
    assert!(!json.contains("FAKE-master-password"));
    assert!(!json.contains("password"));

    let host_dir = tempfile::tempdir().unwrap();
    let phone_dir = tempfile::tempdir().unwrap();
    let host_state = host_dir.path().to_str().unwrap().to_string();
    let phone_state = phone_dir.path().to_str().unwrap().to_string();
    let input = PairingHostInput::new(
        host_state.clone(),
        "00000000-0000-4000-8000-000000000001".into(),
        "fake-owner".into(),
        "fake-vault".into(),
        "main".into(),
        "A3-7DPGPV-RCYDPG-PVRCY-DPGPV-RCYD3".into(),
        "github_pat_FAKE_bridge_roundtrip".into(),
        addresses(),
        27621,
    );
    let view = pairing_host_begin(input).unwrap();
    assert!(view.qr.starts_with("gitpass-pair:2:qr:"));
    assert!(!view.qr.contains("github_pat_"));
    assert!(!format!("{view:?}").contains("code"));
    let other = tempfile::tempdir().unwrap();
    let wrong_dir = pairing_host_cancel(other.path().to_str().unwrap().into()).unwrap_err();
    assert_eq!(wrong_dir.to_string(), "no pairing in progress");

    let commit = pairing_joiner_start(view.qr.clone(), phone_state.clone()).unwrap();
    let endpoint = pairing_joiner_endpoint().unwrap();
    assert_eq!(endpoint.port, 27621);
    assert_eq!(endpoint.addresses, vec!["127.0.0.1".to_string()]);
    let reveal = pairing_host_push(host_state.clone(), commit).unwrap();
    assert!(reveal.error.is_empty());
    let open = pairing_joiner_push(reveal.outbound).unwrap();
    assert!(open.error.is_empty());
    let ready = pairing_host_push(host_state.clone(), open.outbound).unwrap();
    assert!(ready.error.is_empty());
    let shown = pairing_joiner_push(ready.outbound).unwrap();
    assert!(shown.error.is_empty());
    assert_eq!(shown.phase, "code");
    let code = pairing_joiner_code().unwrap();
    assert_eq!(code.len(), 6);
    let wrong = if code == "000000" { "000001" } else { "000000" };
    let err = pairing_host_confirm(host_state.clone(), wrong.into()).unwrap();
    assert_eq!(err.error, "pairing verification code does not match");
    assert!(!err.error.contains("github_pat_"));
    let grant = pairing_host_confirm(host_state, code).unwrap();
    assert!(grant.error.is_empty());
    assert!(!String::from_utf8_lossy(&grant.outbound).contains("github_pat_"));
    let done = pairing_joiner_push(grant.outbound).unwrap();
    assert!(done.error.is_empty());
    assert_eq!(done.phase, "done");
    let payload = pairing_joiner_finish(phone_state).unwrap();
    assert_eq!(payload.sync_token(), "github_pat_FAKE_bridge_roundtrip");
    assert!(!format!("{payload:?}").contains("github_pat_"));

    pairing_joiner_start(view.qr, phone_dir.path().to_str().unwrap().into()).unwrap_err();
    pairing_joiner_cancel();
}
