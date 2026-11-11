use super::*;
use gitpass_core::pairing::{self, JoinerSession};

const TOKEN: &str = "FAKE-github-pat-desktop-host";

#[test]
fn host_release_requires_the_code_and_lock_drops_the_session() {
    let e = env();
    let now = pairing::now_unix();
    let mut joiner = JoinerSession::begin(now).unwrap();
    let prepared = e
        .app
        .pairing_host_prepare(&joiner.offer(), "fake-owner", "fake-vault", "main", TOKEN)
        .unwrap();
    let bad = e
        .app
        .pairing_host_prepare("not-a-frame", "fake-owner", "fake-vault", "main", TOKEN);
    let bad = bad.unwrap_err();
    assert_eq!(bad.to_string(), "unrecognized pairing message");
    assert!(!bad.to_string().contains(TOKEN));
    let code = joiner.read_announce(&prepared.announce, now).unwrap();
    assert_eq!(code, prepared.code);
    assert!(!prepared.announce.contains(TOKEN));
    assert!(!prepared.announce.contains(&e.sk));
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
        .open(
            &grant,
            &code,
            now,
            &mut pairing::ConsumedPairings::default(),
        )
        .unwrap();
    assert_eq!(payload.secret_key.encode(), e.sk);
    assert_eq!(&payload.sync_token[..], TOKEN);
    assert_eq!(payload.repo_owner, "fake-owner");
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
fn lock_cancels_an_in_progress_host_so_the_offer_can_be_retried() {
    let e = env();
    let now = pairing::now_unix();
    let joiner = JoinerSession::begin(now).unwrap();
    let offer = joiner.offer();
    e.app
        .pairing_host_prepare(&offer, "fake-owner", "fake-vault", "main", TOKEN)
        .unwrap();
    e.app.lock();
    let locked = e.app.pairing_host_release("000000").unwrap_err();
    assert_eq!(locked.to_string(), "vault is locked");
    assert!(!locked.to_string().contains(TOKEN));
    e.app
        .unlock(
            &e._vault.path().display().to_string(),
            "correct horse",
            Some(&e.sk),
        )
        .unwrap();
    let again = e
        .app
        .pairing_host_prepare(&offer, "fake-owner", "fake-vault", "main", TOKEN)
        .unwrap();
    assert_eq!(again.code.len(), 6);
    e.app.pairing_host_cancel().unwrap();
    assert_eq!(
        e.app
            .pairing_host_release(&again.code)
            .unwrap_err()
            .to_string(),
        "no pairing in progress"
    );
}
