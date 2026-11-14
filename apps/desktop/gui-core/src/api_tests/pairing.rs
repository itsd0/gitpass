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

use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const PASSWORD: &str = "correct horse";

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

fn leaked(bytes: &[u8], secret: &str) -> bool {
    contains(bytes, PASSWORD.as_bytes())
        || contains(bytes, TOKEN.as_bytes())
        || contains(bytes, secret.as_bytes())
        || contains(bytes, b"github_pat_")
}

fn refused(port: u16) -> bool {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    match TcpStream::connect_timeout(&addr, Duration::from_millis(400)) {
        Ok(_) => false,
        Err(e) => e.kind() == ErrorKind::ConnectionRefused,
    }
}

struct PhoneOut {
    grant_ok: bool,
    leaked: bool,
    error: Option<String>,
}

fn drive_phone(qr: String, secret: String, code_tx: mpsc::Sender<String>) -> PhoneOut {
    let mut seen = Vec::new();
    let result = (|| -> Result<bool, String> {
        let now = pairing::now_unix();
        let mut joiner = JoinerSession::from_qr(&qr, now, &mut pairing::ConsumedPairings::default())
            .map_err(|e| e.to_string())?;
        let host = joiner.addresses().first().cloned().ok_or_else(|| "pairing could not connect".to_string())?;
        let addr: SocketAddr = format!("{host}:{}", joiner.port()).parse().map_err(|_| "pairing could not connect".to_string())?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
            .map_err(|_| "pairing could not connect".to_string())?;
        stream.set_read_timeout(Some(Duration::from_secs(3))).map_err(|_| "pairing failed".to_string())?;
        stream.set_nodelay(true).map_err(|_| "pairing failed".to_string())?;
        let mut buf = [0u8; 8192];
        let mut outbound = joiner.push(&[], now).outbound;
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        loop {
            if !outbound.is_empty() {
                stream.write_all(&outbound).map_err(|_| "pairing connection closed".to_string())?;
                seen.extend_from_slice(&outbound);
                outbound.clear();
            }
            if joiner.phase() == "code" {
                let code = joiner.code().map_err(|e| e.to_string())?.to_string();
                code_tx.send(code).map_err(|_| "pairing failed".to_string())?;
            }
            if joiner.phase() == "done" {
                let payload = joiner.take_payload().map_err(|e| e.to_string())?;
                let ok = payload.sync_token.as_str() == TOKEN
                    && payload.secret_key.encode() == secret
                    && payload.repo_owner == "fake-owner"
                    && payload.repo_name == "fake-vault"
                    && payload.branch == "main";
                return Ok(ok);
            }
            if std::time::Instant::now() >= deadline {
                return Err("pairing timed out".into());
            }
            let n = match stream.read(&mut buf) {
                Ok(0) => return Err("pairing connection closed".into()),
                Ok(n) => n,
                Err(_) => return Err("pairing timed out".into()),
            };
            seen.extend_from_slice(&buf[..n]);
            let step = joiner.push(&buf[..n], now);
            if let Some(e) = step.error {
                return Err(e.to_string());
            }
            outbound = step.outbound;
            if joiner.phase() == "failed" {
                return Err("pairing failed".into());
            }
        }
    })();
    let leaked = leaked(&seen, &secret) || leaked(qr.as_bytes(), &secret);
    match result {
        Ok(grant_ok) => PhoneOut { grant_ok, leaked, error: None },
        Err(error) => PhoneOut { grant_ok: false, leaked, error: Some(error) },
    }
}

#[test]
fn loopback_exchange_grants_then_refuses_another_connection() {
    let e = env();
    configure_sync(e._vault.path());
    let prepared = e.app.pairing_host_listen_on(&[IpAddr::V4(Ipv4Addr::LOCALHOST)]).unwrap();
    assert!(prepared.qr.starts_with("gitpass-pair:2:qr:"));
    assert!(!prepared.qr.contains(TOKEN));
    assert!(!prepared.qr.contains(&e.sk));
    assert!(!prepared.qr.contains(PASSWORD));
    assert!(!format!("{prepared:?}").contains("code:"));
    let port = prepared.port;
    let (tx, rx) = mpsc::channel();
    let phone = thread::spawn({
        let qr = prepared.qr.clone();
        let sk = e.sk.clone();
        move || drive_phone(qr, sk, tx)
    });
    e.app.pairing_host_accept(Duration::from_secs(3)).unwrap();
    let code = rx.recv_timeout(Duration::from_secs(3)).expect("phone reached the code");
    let grant = e.app.pairing_host_confirm(&code).unwrap();
    assert!(grant.error.is_none(), "grant step failed");
    assert!(!String::from_utf8_lossy(&grant.outbound).contains(TOKEN));
    assert!(!String::from_utf8_lossy(&grant.outbound).contains(PASSWORD));
    let done = phone.join().expect("phone thread");
    assert!(!done.leaked, "secret bytes appeared on the pairing connection");
    assert!(done.error.is_none(), "phone reported a fixed pairing error");
    assert!(done.grant_ok, "grant did not open to the configured sync target");
    assert!(refused(port), "listener stayed open after success");
    let mut consumed = pairing::load_consumed(e._cfg.path()).unwrap();
    assert_eq!(
        JoinerSession::from_qr(&prepared.qr, pairing::now_unix(), &mut consumed).unwrap_err(),
        pairing::PairingError::Replay
    );
}

#[test]
fn third_wrong_code_closes_the_listener() {
    let e = env();
    configure_sync(e._vault.path());
    let prepared = e.app.pairing_host_listen_on(&[IpAddr::V4(Ipv4Addr::LOCALHOST)]).unwrap();
    let port = prepared.port;
    let qr = prepared.qr.clone();
    let (tx, rx) = mpsc::channel();
    let phone = thread::spawn({
        let sk = e.sk.clone();
        move || drive_phone(qr, sk, tx)
    });
    e.app.pairing_host_accept(Duration::from_secs(3)).unwrap();
    let code = rx.recv_timeout(Duration::from_secs(3)).expect("phone reached the code");
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
    let done = phone.join().expect("phone thread");
    assert!(!done.leaked, "secret bytes appeared on the pairing connection");
    assert!(!done.grant_ok, "a rejected code still opened a grant");
    let err = done.error.unwrap_or_default();
    assert!(err == "pairing failed" || err == "pairing connection closed", "phone error was not a fixed message");
    assert!(!err.contains(TOKEN) && !err.contains(&e.sk) && !err.contains("127.0.0.1"));
    assert!(refused(port), "listener stayed open after three wrong codes");
}

#[test]
fn cancel_and_expiry_stop_accepting() {
    let e = env();
    configure_sync(e._vault.path());
    let prepared = e.app.pairing_host_listen_on(&[IpAddr::V4(Ipv4Addr::LOCALHOST)]).unwrap();
    let port = prepared.port;
    e.app.pairing_host_cancel().unwrap();
    assert!(refused(port), "listener stayed open after cancel");
    assert_eq!(
        e.app.pairing_host_accept(Duration::from_millis(200)).unwrap_err().to_string(),
        "pairing failed"
    );

    let prepared = e.app.pairing_host_listen_on(&[IpAddr::V4(Ipv4Addr::LOCALHOST)]).unwrap();
    let port = prepared.port;
    e.app.pairing_host_expire_for_test();
    let err = e.app.pairing_host_accept(Duration::from_secs(1)).unwrap_err();
    assert_eq!(err.to_string(), "pairing session expired");
    assert!(!err.to_string().contains("127.0.0.1"));
    assert!(refused(port), "listener stayed open after expiry");
}

#[test]
fn oversized_and_truncated_frames_are_rejected_without_addresses() {
    let e = env();
    configure_sync(e._vault.path());
    let prepared = e.app.pairing_host_listen_on(&[IpAddr::V4(Ipv4Addr::LOCALHOST)]).unwrap();
    let port = prepared.port;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    thread::spawn(move || {
        if let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
            let _ = stream.write_all(&100_000u32.to_be_bytes());
            let mut buf = [0u8; 32];
            let _ = stream.read(&mut buf);
        }
    });
    let err = e.app.pairing_host_accept(Duration::from_secs(2)).unwrap_err();
    assert_eq!(err.to_string(), "pairing message too large");
    assert!(!err.to_string().contains("127.0.0.1"));
    assert!(!err.to_string().contains(TOKEN));
    assert!(refused(port), "listener stayed open after an oversized frame");

    let prepared = e.app.pairing_host_listen_on(&[IpAddr::V4(Ipv4Addr::LOCALHOST)]).unwrap();
    let port = prepared.port;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    thread::spawn(move || {
        if let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
            let _ = stream.write_all(&[0, 0, 0]);
            let _ = stream.shutdown(Shutdown::Both);
        }
    });
    let err = e.app.pairing_host_accept(Duration::from_secs(2)).unwrap_err();
    assert_eq!(err.to_string(), "pairing message truncated");
    assert!(!err.to_string().contains("127.0.0.1"));
    assert!(refused(port), "listener stayed open after a truncated frame");
}

#[test]
fn a_silent_peer_hits_the_read_timeout() {
    let e = env();
    configure_sync(e._vault.path());
    let prepared = e.app.pairing_host_listen_on(&[IpAddr::V4(Ipv4Addr::LOCALHOST)]).unwrap();
    let port = prepared.port;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let peer = thread::spawn(move || {
        let _hold = TcpStream::connect_timeout(&addr, Duration::from_secs(2));
        thread::sleep(Duration::from_secs(2));
    });
    let err = e.app.pairing_host_accept(Duration::from_millis(500)).unwrap_err();
    assert_eq!(err.to_string(), "pairing timed out");
    assert!(!err.to_string().contains("127.0.0.1"));
    assert!(!err.to_string().contains(TOKEN));
    let _ = peer.join();
    assert!(refused(port), "listener stayed open after a read timeout");
}

#[test]
fn lock_closes_the_listener() {
    let e = env();
    configure_sync(e._vault.path());
    let prepared = e.app.pairing_host_listen_on(&[IpAddr::V4(Ipv4Addr::LOCALHOST)]).unwrap();
    let port = prepared.port;
    e.app.lock();
    assert!(refused(port), "listener stayed open after lock");
}

#[test]
fn several_loopback_addresses_share_one_port() {
    let e = env();
    configure_sync(e._vault.path());
    let ips = [IpAddr::V4(Ipv4Addr::LOCALHOST), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2))];
    let prepared = e.app.pairing_host_listen_on(&ips).unwrap();
    assert!(prepared.port >= 49152, "shared port was not in the high range");
    assert_eq!(prepared.addresses.len(), 2);
    assert!(!prepared.qr.contains(TOKEN));
    assert!(!prepared.qr.contains(PASSWORD));
    e.app.pairing_host_cancel().unwrap();
}

#[test]
fn lan_listen_error_stays_fixed() {
    let e = env();
    configure_sync(e._vault.path());
    match e.app.pairing_host_listen() {
        Ok(prepared) => {
            assert!(prepared.qr.starts_with("gitpass-pair:2:qr:"));
            assert!(!prepared.qr.contains(TOKEN));
            assert!(!prepared.qr.contains(&e.sk));
            assert!(!prepared.qr.contains(PASSWORD));
            e.app.pairing_host_cancel().unwrap();
        }
        Err(err) => {
            let text = err.to_string();
            assert!(
                text == "pairing needs a local network address" || text == "pairing listener could not start",
                "listen error was not a fixed message"
            );
            assert!(!text.contains(TOKEN) && !text.contains(&e.sk));
        }
    }
}

const TYPED: &str = "github_pat_FAKE_typed_only";

fn configure_remote(vault: &Path) {
    let git = |args: &[&str]| {
        let out = Command::new("git").arg("-C").arg(vault).args(args).output().unwrap();
        assert!(out.status.success(), "git command failed");
    };
    git(&["init", "-b", "main"]);
    git(&["remote", "add", "origin", "https://github.com/fake-owner/fake-vault.git"]);
}

#[test]
fn a_typed_token_is_used_once_and_not_written() {
    let e = env();
    configure_remote(e._vault.path());
    let path = e._vault.path().join(".gitpass-sync-token");
    assert!(!path.exists());
    let now = pairing::now_unix();
    let prepared = e
        .app
        .pairing_host_begin_with(&["127.0.0.1".into()], 27621, Some(TYPED))
        .unwrap();
    assert!(!path.exists(), "typed token was written to disk");
    assert!(prepared.qr.starts_with("gitpass-pair:2:qr:"));
    assert!(!prepared.qr.contains(TYPED));
    assert!(!prepared.qr.contains(PASSWORD));
    assert!(!prepared.qr.contains(&e.sk));
    let mut joiner = JoinerSession::from_qr(&prepared.qr, now, &mut pairing::ConsumedPairings::default()).unwrap();
    let code = exchange(&e.app, &mut joiner, now);
    let grant = e.app.pairing_host_confirm(&code).unwrap();
    assert!(grant.error.is_none(), "grant step failed");
    assert!(!String::from_utf8_lossy(&grant.outbound).contains(TYPED));
    assert!(!String::from_utf8_lossy(&grant.outbound).contains(PASSWORD));
    let opened = joiner.push(&grant.outbound, now);
    assert!(opened.error.is_none(), "phone could not open the grant");
    let payload = joiner.take_payload().unwrap();
    assert!(payload.sync_token.as_str() == TYPED, "grant token mismatch");
    assert!(payload.secret_key.encode() == e.sk, "grant secret mismatch");
    assert!(!path.exists(), "typed token was written after the grant");
    e.app.pairing_host_cancel().unwrap();
}

#[test]
fn a_stored_token_wins_and_a_classic_token_is_not_echoed() {
    let e = env();
    configure_sync(e._vault.path());
    let prepared = e
        .app
        .pairing_host_begin_with(&["127.0.0.1".into()], 27622, Some(TYPED))
        .unwrap();
    assert!(!prepared.qr.contains(TYPED));
    assert!(!prepared.qr.contains(TOKEN));
    let now = pairing::now_unix();
    let mut joiner = JoinerSession::from_qr(&prepared.qr, now, &mut pairing::ConsumedPairings::default()).unwrap();
    let code = exchange(&e.app, &mut joiner, now);
    let grant = e.app.pairing_host_confirm(&code).unwrap();
    let opened = joiner.push(&grant.outbound, now);
    assert!(opened.error.is_none(), "phone could not open the grant");
    let payload = joiner.take_payload().unwrap();
    assert!(payload.sync_token.as_str() == TOKEN, "stored token was not used");
    assert!(!String::from_utf8_lossy(&grant.outbound).contains(TYPED));

    let e = env();
    configure_remote(e._vault.path());
    let err = e
        .app
        .pairing_host_begin_with(&["127.0.0.1".into()], 27623, Some("ghp_FAKE0123456789"))
        .unwrap_err();
    let text = err.to_string();
    assert_eq!(text, "pairing payload field rejected");
    assert!(!text.contains("ghp_"));
    assert!(!e._vault.path().join(".gitpass-sync-token").exists());
}
