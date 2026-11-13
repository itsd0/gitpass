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

fn configure_remote_only(vault: &Path) {
    let git = |args: &[&str]| {
        let out = Command::new("git").arg("-C").arg(vault).args(args).output().unwrap();
        assert!(out.status.success(), "{args:?} {}", String::from_utf8_lossy(&out.stderr));
    };
    git(&["init", "-b", "main"]);
    git(&["remote", "add", "origin", "https://github.com/fake-owner/fake-vault.git"]);
}

#[test]
fn typed_token_is_used_in_memory_and_never_written() {
    let e = env();
    configure_remote_only(e._vault.path());
    let now = pairing::now_unix();
    let mut joiner = JoinerSession::begin(now).unwrap();
    // No stored token and none typed: a fixed error.
    let none = e.app.pairing_host_prepare_with(&joiner.offer(), Some("  ")).unwrap_err();
    assert_eq!(none.to_string(), "sync token is not configured");
    let prepared = e.app.pairing_host_prepare_with(&joiner.offer(), Some(TOKEN)).unwrap();
    let code = joiner.read_announce(&prepared.announce, now).unwrap();
    let grant = e.app.pairing_host_release(&code).unwrap();
    let payload = joiner
        .open(&grant, &code, now, &mut pairing::ConsumedPairings::default())
        .unwrap();
    assert_eq!(&payload.sync_token[..], TOKEN);
    assert_eq!(payload.repo_owner, "fake-owner");
    assert!(!e._vault.path().join(".gitpass-sync-token").exists());
    for dir in [e._vault.path(), e._cfg.path()] {
        for f in std::fs::read_dir(dir).unwrap().flatten() {
            if f.path().is_file() {
                let bytes = std::fs::read(f.path()).unwrap();
                assert!(!String::from_utf8_lossy(&bytes).contains(TOKEN), "token written to {:?}", f.file_name());
            }
        }
    }
}

#[test]
fn typed_classic_token_is_refused_without_echoing_it() {
    let e = env();
    configure_remote_only(e._vault.path());
    let joiner = JoinerSession::begin(pairing::now_unix()).unwrap();
    let err = e.app.pairing_host_prepare_with(&joiner.offer(), Some("ghp_FAKE0123456789abcdef")).unwrap_err();
    assert_eq!(err.to_string(), "pairing payload field rejected");
    assert!(!err.to_string().contains("ghp_"));
}

const OFFER_TEXT: &str = "gitpass-pair:1:offer:MzMzMzMzMzMzMzMzMzMzMw.AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA.1700000120";
const OFFER_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAALQAAAC0AQAAAAAVtjufAAABw0lEQVR4nNWYsZFtNwxDj3acQx1s/2W9DsAKjgOtA4+/E5uBrUABAwwvBQLkPfKrM1+/DMN/L37OufeeO8OZGe45567hYzQg0pC8QLfwf4MTEK53SuYFduvWA9zK93zff4Hz1/MbALdXGNIjdxP/CyxmZvgGesHu4R8P5E+hgXz26iPA/ZD5Zrwgi/2FKdC01Vgx7PHzyOVDDikDl3LDWn2wMWmDFGpTXcsf0yraqsWExf5FmiQpUlShm/mTWNqgoJDV/EuCtdE07aq+fQFthpvhEi7M6a4+U4MNtrW7/EEskgStuMmfL3JyEfl8mPM9mfwjnL/NP7WVkoCtySZ/khAgNikku/z3Ga5Eq22TTfxq0pCk0NQs6w/t41Assozfn7s10dcJm/mL1vokLim7+ZdXl/74cDf150lCYqO1gbDKnzzCpy19j7zK/6RaME1egF1/aVRtodBtfyRtIE1R3iet6nOewEX5Yxra1B9Fk1enxCy+79tfOMOHzD0HLqvzfxRg+AykQ2dW96PnK4+ib9pd7K+feGduO/RIy17+b39pTpzcHm+47uG//YWbud/Mj8rdVX+HvAGI5zbZ9JfzP/8/8DsOXqV2GQ3TNgAAAABJRU5ErkJggg==";
const OTHER_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAIQAAACEAQAAAAB5P74KAAABDUlEQVR4nL2WQYpdMQwES8Pfl28w9z/Wu0H7BJ3FT2ASCAw2iVa2QIXckpCn/G77gz/t/3pmZu3FXrNmZh1zbPHBFNvTfF4wMgJkjRccgF32+n7U3zyyB9Ydp2EHVkNzypkO+POyweeMQ9sWUmvb5pQj2EhS8JxDTG1BrMecVhKTEo/fNV27bFnwAB7OKQ3RtGmbC32sJRG40seCBXpTrw9YzV5J99TntJ+pwaqpV/rQprUG9FSfF+4lsENZ5LjuYhIttrmpOxgDIDdz0ULEN/FYZ7ABozfzbqsmjb3qZ/Q9YiAX/fy2fO7k4XQPvn4dxs8Nx/3z3hclBCF3+2ITgOVxPi++fFz63ah/5/kBf0XIq5Ta6Q4AAAAASUVORK5CYII=";

#[test]
fn scan_offer_returns_only_pairing_offers() {
    let e = env();
    assert_eq!(e.app.pairing_scan_offer(OFFER_PNG).unwrap(), OFFER_TEXT);
    assert_eq!(
        e.app.pairing_scan_offer(&format!("data:image/png;base64,{OFFER_PNG}")).unwrap(),
        OFFER_TEXT
    );
    let other = e.app.pairing_scan_offer(OTHER_PNG).unwrap_err().to_string();
    assert_eq!(other, "This QR code isn't a gitpass pairing code");
    assert!(!other.contains("example.invalid"));
    assert_eq!(e.app.pairing_scan_offer("bm90IGFuIGltYWdl").unwrap_err().to_string(), "No QR code found");
}
