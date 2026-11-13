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

#[test]
fn host_release_requires_the_phones_code_and_lock_drops_the_session() {
    let e = env();
    configure_sync(e._vault.path());
    let now = pairing::now_unix();
    let mut joiner = JoinerSession::begin(now).unwrap();
    let prepared = e.app.pairing_host_prepare(&joiner.offer()).unwrap();
    let bad = e.app.pairing_host_prepare("not-a-frame").unwrap_err();
    assert_eq!(bad.to_string(), "unrecognized pairing message");
    assert!(!bad.to_string().contains(TOKEN));
    // The code comes from the phone, not from the desktop result.
    let code = joiner.read_announce(&prepared.announce, now).unwrap();
    assert_eq!(code.len(), 6);
    assert!(!prepared.announce.contains(TOKEN));
    assert!(!prepared.announce.contains(&e.sk));
    assert!(!format!("{prepared:?}").contains("code:"));
    let wrong = if code == "000000" { "000001" } else { "000000" };
    let err = e.app.pairing_host_release(wrong).unwrap_err();
    assert_eq!(err.to_string(), "pairing verification code does not match");
    assert!(!err.to_string().contains(TOKEN));
    assert!(!err.to_string().contains(&e.sk));
    let grant = e.app.pairing_host_release(&code).unwrap();
    assert!(!grant.contains(TOKEN));
    assert!(!grant.contains(&e.sk));
    assert!(!grant.contains("correct horse"));
    let payload = joiner
        .open(&grant, &code, now, &mut pairing::ConsumedPairings::default())
        .unwrap();
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
        e.app.pairing_host_release(&code).unwrap_err().to_string(),
        "no pairing in progress"
    );
}

#[test]
fn three_wrong_codes_mark_the_offer_used() {
    let e = env();
    configure_sync(e._vault.path());
    let now = pairing::now_unix();
    let mut joiner = JoinerSession::begin(now).unwrap();
    let offer = joiner.offer();
    let prepared = e.app.pairing_host_prepare(&offer).unwrap();
    let code = joiner.read_announce(&prepared.announce, now).unwrap();
    let wrong = if code == "000000" { "000001" } else { "000000" };
    assert_eq!(
        e.app.pairing_host_release(wrong).unwrap_err().to_string(),
        "pairing verification code does not match"
    );
    assert_eq!(
        e.app.pairing_host_release(wrong).unwrap_err().to_string(),
        "pairing verification code does not match"
    );
    let burned = e.app.pairing_host_release(wrong).unwrap_err();
    assert_eq!(burned.to_string(), "too many wrong pairing codes");
    assert!(!burned.to_string().contains(TOKEN));
    let again = e.app.pairing_host_prepare(&offer).unwrap_err();
    assert_eq!(again.to_string(), "pairing session already used");
}

#[test]
fn classic_token_file_is_refused_without_echoing_it() {
    let e = env();
    configure_sync(e._vault.path());
    std::fs::write(e._vault.path().join(".gitpass-sync-token"), "ghp_FAKE0123456789\n").unwrap();
    let now = pairing::now_unix();
    let joiner = JoinerSession::begin(now).unwrap();
    let err = e.app.pairing_host_prepare(&joiner.offer()).unwrap_err();
    assert_eq!(err.to_string(), "pairing payload field rejected");
    assert!(!err.to_string().contains("ghp_"));
}

#[test]
fn lock_cancels_an_in_progress_host_so_the_offer_can_be_retried() {
    let e = env();
    configure_sync(e._vault.path());
    let now = pairing::now_unix();
    let joiner = JoinerSession::begin(now).unwrap();
    let offer = joiner.offer();
    e.app.pairing_host_prepare(&offer).unwrap();
    e.app.lock();
    let locked = e.app.pairing_host_release("000000").unwrap_err();
    assert_eq!(locked.to_string(), "vault is locked");
    assert!(!locked.to_string().contains(TOKEN));
    e.app
        .unlock(&e._vault.path().display().to_string(), "correct horse", Some(&e.sk))
        .unwrap();
    let again = e.app.pairing_host_prepare(&offer).unwrap();
    assert!(again.announce.starts_with("gitpass-pair:1:announce:"));
    e.app.pairing_host_cancel().unwrap();
    assert_eq!(
        e.app.pairing_host_release("000000").unwrap_err().to_string(),
        "no pairing in progress"
    );
}
