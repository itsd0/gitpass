use super::*;
use gitpass_core::pairing::{self, JoinerSession};
use std::path::Path;
use std::process::Command;

const TOKEN: &str = "github_pat_FAKE_desktop_host";

fn configure_sync(vault: &Path) {
    let git = |args: &[&str]| {
        let out = Command::new("git").arg("-C").arg(vault).args(args).output().unwrap();
        assert!(out.status.success(), "{args:?} {}", String::from_utf8_lossy(&out.stderr));
    };
    git(&["init", "-b", "main"]);
    git(&["remote", "add", "origin", "https://github.com/fake-owner/fake-vault.git"]);
    std::fs::write(vault.join(".gitpass-sync-token"), format!("{TOKEN}\n")).unwrap();
}

fn exchange(app: &App, joiner: &mut JoinerSession, now: i64) -> String {
    let commit = joiner.push(&[], now);
    assert!(commit.error.is_none());
    let reveal = app.pairing_host_push(&commit.outbound).unwrap();
    assert!(reveal.error.is_none());
    let open = joiner.push(&reveal.outbound, now);
    assert!(open.error.is_none());
    let ready = app.pairing_host_push(&open.outbound).unwrap();
    assert!(ready.error.is_none());
    let shown = joiner.push(&ready.outbound, now);
    assert!(shown.error.is_none());
    joiner.code().unwrap().to_string()
}

#[test]
fn host_confirm_requires_the_phones_code_and_lock_drops_the_session() {
    let e = env();
    configure_sync(e._vault.path());
    let now = pairing::now_unix();
    let prepared = e.app.pairing_host_begin(&["127.0.0.1".into()], 27611).unwrap();
    assert!(prepared.qr.starts_with("gitpass-pair:2:qr:"));
    assert!(!prepared.qr.contains(TOKEN));
    assert!(!prepared.qr.contains(&e.sk));
    assert!(!format!("{prepared:?}").contains("code:"));
    let mut joiner = JoinerSession::from_qr(&prepared.qr, now, &mut pairing::ConsumedPairings::default()).unwrap();
    let code = exchange(&e.app, &mut joiner, now);
    let wrong = if code == "000000" { "000001" } else { "000000" };
    let err = e.app.pairing_host_confirm(wrong).unwrap();
    assert_eq!(err.error.as_deref(), Some("pairing verification code does not match"));
    assert!(!err.error.as_deref().unwrap().contains(TOKEN));
    let grant = e.app.pairing_host_confirm(&code).unwrap();
    assert!(grant.error.is_none());
    let text = String::from_utf8_lossy(&grant.outbound);
    assert!(!text.contains(TOKEN));
    assert!(!text.contains(&e.sk));
    assert!(!text.contains("correct horse"));
    let opened = joiner.push(&grant.outbound, now);
    assert!(opened.error.is_none());
    let payload = joiner.take_payload().unwrap();
    assert_eq!(payload.secret_key.encode(), e.sk);
    assert_eq!(&payload.sync_token[..], TOKEN);
    assert_eq!(payload.repo_owner, "fake-owner");
    assert_eq!(payload.repo_name, "fake-vault");
    assert_eq!(payload.branch, "main");
    assert!(!format!("{payload:?}").contains(TOKEN));
    assert!(!format!("{payload:?}").contains(&e.sk));
    let stored = std::fs::read_to_string(e._cfg.path().join(pairing::CONSUMED_FILE)).unwrap();
    assert!(!stored.contains(TOKEN));
    assert!(!stored.contains(&e.sk));
    assert_eq!(
        e.app.pairing_host_confirm(&code).unwrap_err().to_string(),
        "no pairing in progress"
    );
}

#[test]
fn three_wrong_codes_mark_the_session_used() {
    let e = env();
    configure_sync(e._vault.path());
    let now = pairing::now_unix();
    let prepared = e.app.pairing_host_begin(&["127.0.0.1".into()], 27612).unwrap();
    let qr = prepared.qr.clone();
    let mut joiner = JoinerSession::from_qr(&qr, now, &mut pairing::ConsumedPairings::default()).unwrap();
    let code = exchange(&e.app, &mut joiner, now);
    let wrong = if code == "000000" { "000001" } else { "000000" };
    assert_eq!(
        e.app.pairing_host_confirm(wrong).unwrap().error.as_deref(),
        Some("pairing verification code does not match")
    );
    assert_eq!(
        e.app.pairing_host_confirm(wrong).unwrap().error.as_deref(),
        Some("pairing verification code does not match")
    );
    let burned = e.app.pairing_host_confirm(wrong).unwrap();
    assert_eq!(burned.error.as_deref(), Some("too many wrong pairing codes"));
    assert!(!burned.outbound.is_empty());
    let mut consumed = pairing::load_consumed(e._cfg.path()).unwrap();
    assert_eq!(
        JoinerSession::from_qr(&qr, now, &mut consumed).unwrap_err(),
        pairing::PairingError::Replay
    );
}

#[test]
fn classic_token_file_is_refused_without_echoing_it() {
    let e = env();
    configure_sync(e._vault.path());
    std::fs::write(e._vault.path().join(".gitpass-sync-token"), "ghp_FAKE0123456789\n").unwrap();
    let err = e.app.pairing_host_begin(&["127.0.0.1".into()], 27613).unwrap_err();
    assert_eq!(err.to_string(), "pairing payload field rejected");
    assert!(!err.to_string().contains("ghp_"));
}

#[test]
fn lock_cancels_an_in_progress_host_so_a_new_qr_can_start() {
    let e = env();
    configure_sync(e._vault.path());
    e.app.pairing_host_begin(&["127.0.0.1".into()], 27614).unwrap();
    e.app.lock();
    let locked = e.app.pairing_host_confirm("000000").unwrap_err();
    assert_eq!(locked.to_string(), "vault is locked");
    assert!(!locked.to_string().contains(TOKEN));
    e.app
        .unlock(&e._vault.path().display().to_string(), "correct horse", Some(&e.sk))
        .unwrap();
    let again = e.app.pairing_host_begin(&["127.0.0.1".into()], 27615).unwrap();
    assert!(again.qr.starts_with("gitpass-pair:2:qr:"));
    e.app.pairing_host_cancel().unwrap();
    assert_eq!(
        e.app.pairing_host_confirm("000000").unwrap_err().to_string(),
        "no pairing in progress"
    );
}
