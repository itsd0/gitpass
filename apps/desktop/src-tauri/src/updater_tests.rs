use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri_plugin_updater::UpdaterExt;

const TEST_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3QgcHVia2V5ClJXUXhNak0wTlRZM09JTVBhS0htdjQrT3AzWTZ4R2dNaEcxM1N5TTN3VTlNSnRMZjZ3SEllNk9zCg==";
const TEST_PUBKEY_UNTRUSTED: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3QgcHVia2V5ClJXUTROelkxTkRNeU1hRi9BZjMyVU82SFRnQTc4NFIxUDdqb3pCaEMrVUYyc2IrS2pQbWQ2dCt4Cg==";
const TEST_PAYLOAD: &[u8] = b"gitpass-updater-test-artifact-payload-v1";

// Valid Ed25519 Minisign signature for TEST_PAYLOAD with trusted comment containing version:1.0.0
const SIG_V1: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3Qgc2lnClJXUXhNak0wTlRZM09CbHU5cUl6ZWJSd1NJcnVGZmd0b0FqR3o2QVczQ0VnbitjWlhXeGdBVmpzVjdaTmpEbnRpTmg5ZStSQ1R5eHhwUXpRSTl5TzlWM0V0ektLdnJZdUpnZz0KdHJ1c3RlZCBjb21tZW50OiB0aW1lc3RhbXA6MTcwMDAwMDAwMAlmaWxlOnJlbGVhc2UudGFyLmd6CXZlcnNpb246MS4wLjAKWHBGakQ3QnRINUxHYWljQnFUa21PeXNlYW1WTnNZekNmTGM4YllUNkVKQVNnalJGT2xFZXFPTFNHb0FuTnA2LzZGc1V6STlxYnlSVFhTcTRLcXNmQVE9PQo=";

// Valid Ed25519 Minisign signature for TEST_PAYLOAD with trusted comment containing version:2.0.0
const SIG_V2: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3Qgc2lnClJXUXhNak0wTlRZM09CbHU5cUl6ZWJSd1NJcnVGZmd0b0FqR3o2QVczQ0VnbitjWlhXeGdBVmpzVjdaTmpEbnRpTmg5ZStSQ1R5eHhwUXpRSTl5TzlWM0V0ektLdnJZdUpnZz0KdHJ1c3RlZCBjb21tZW50OiB0aW1lc3RhbXA6MTcwMDAwMDAwMAlmaWxlOnJlbGVhc2UudGFyLmd6CXZlcnNpb246Mi4wLjAKRzNWN3dwRUlWanlHMEJzaVdrZ2ZJbU1aU25WOGN3RnJTOHdPUU9aL29jTE5XRUt0bUZ3KzloZ0lsY0xWb3ZVUTBRSDhhN2lFTnROQk84QzcyT1RNRFE9PQo=";

// Signature created with TEST_PUBKEY_UNTRUSTED for version:1.0.0
const SIG_BAD_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3Qgc2lnClJXUTROelkxTkRNeU1RTllwL3dmNk96YnhQNkptWFdlM1V1bWpOdlRsQXc2VzA0djdrL1pLUmV3MUx3dzVNbmw0ZjIzR3hGK2xTTkJKcWZmQWNLa2NCWXlBRmI2WE9OcUNRbz0KdHJ1c3RlZCBjb21tZW50OiB0aW1lc3RhbXA6MTcwMDAwMDAwMAlmaWxlOnJlbGVhc2UudGFyLmd6CXZlcnNpb246MS4wLjAKR3dFeFg0eHRlUGNkaFgyZnlPRlk4eDdzMkZ1WlMvSmRVejJ5cTJhb3pPVDkxaWNOcTBBaWpJaVlOOU5lOThQVWhuNGFiV1BKbjFFdkVjRjFIeU9vQUE9PQo=";

struct MockHttpServer {
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockHttpServer {
    fn start<F>(handler: F) -> Self
    where
        F: Fn(SocketAddr, &str) -> (u16, &'static str, Vec<u8>) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local port");
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));
        let handler = Arc::new(handler);
        let s_clone = shutdown.clone();

        let handle = thread::spawn(move || {
            while !s_clone.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buf = [0u8; 4096];
                        if let Ok(n) = stream.read(&mut buf) {
                            if n > 0 {
                                let req = String::from_utf8_lossy(&buf[..n]);
                                let path = req.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/");
                                let (status, content_type, body) = handler(addr, path);
                                let status_text = match status {
                                    200 => "OK",
                                    204 => "No Content",
                                    404 => "Not Found",
                                    _ => "Custom",
                                };
                                let response = format!(
                                    "HTTP/1.1 {} {}
Content-Type: {}
Content-Length: {}
Connection: close

",
                                    status, status_text, content_type, body.len()
                                );
                                let _ = stream.write_all(response.as_bytes());
                                let _ = stream.write_all(&body);
                                let _ = stream.flush();
                                let _ = stream.shutdown(std::net::Shutdown::Write);
                                let mut discard = [0u8; 1024];
                                while let Ok(n) = stream.read(&mut discard) {
                                    if n == 0 { break; }
                                }
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            shutdown,
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}/{}", self.addr, path.trim_start_matches('/'))
    }
}

impl Drop for MockHttpServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn create_app(current_version: &str, pubkey: &str) -> tauri::App<tauri::test::MockRuntime> {
    let mut ctx = mock_context(noop_assets());
    ctx.package_info_mut().version = current_version.parse().unwrap();
    ctx.config_mut().plugins.0.insert(
        "updater".to_string(),
        serde_json::json!({
            "pubkey": pubkey
        }),
    );
    mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(ctx)
        .expect("build mock app")
}

fn release_json(version: &str, target: &str, download_url: &str, signature: &str) -> Vec<u8> {
    serde_json::json!({
        "version": version,
        "notes": "Test release notes",
        "pub_date": "2026-10-28T00:00:00Z",
        "platforms": {
            target: {
                "url": download_url,
                "signature": signature
            }
        }
    })
    .to_string()
    .into_bytes()
}

#[test]
fn update_check_and_signature_verification_succeeds() {
    let target = "test-target";
    let target_str = target.to_string();
    let server = MockHttpServer::start(move |addr, path| {
        if path == "/latest.json" {
            let download_url = format!("http://{}/download/release.tar.gz", addr);
            let json = release_json("1.0.0", &target_str, &download_url, SIG_V1);
            (200, "application/json", json)
        } else if path == "/download/release.tar.gz" {
            (200, "application/octet-stream", TEST_PAYLOAD.to_vec())
        } else {
            (404, "text/plain", b"not found".to_vec())
        }
    });

    let app = create_app("0.6.2", TEST_PUBKEY);
    let endpoint: tauri::Url = server.url("latest.json").parse().unwrap();
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .unwrap()
        .target(target)
        .build()
        .expect("build updater");

    let update = tauri::async_runtime::block_on(updater.check())
        .expect("check succeeds")
        .expect("update available");

    assert_eq!(update.version, "1.0.0");
    assert_eq!(update.current_version, "0.6.2");
    assert_eq!(update.body.as_deref(), Some("Test release notes"));

    let downloaded = tauri::async_runtime::block_on(update.download(|_, _| {}, || {}))
        .expect("download and signature verification succeeds");
    assert_eq!(downloaded, TEST_PAYLOAD);

    // Also verify multi-step upgrade to 2.0.0 using SIG_V2
    let server_v2 = MockHttpServer::start({
        let target_str = target.to_string();
        move |addr, path| {
            if path == "/latest.json" {
                let download_url = format!("http://{}/download/release-v2.tar.gz", addr);
                let json = release_json("2.0.0", &target_str, &download_url, SIG_V2);
                (200, "application/json", json)
            } else if path == "/download/release-v2.tar.gz" {
                (200, "application/octet-stream", TEST_PAYLOAD.to_vec())
            } else {
                (404, "text/plain", b"not found".to_vec())
            }
        }
    });

    let endpoint_v2: tauri::Url = server_v2.url("latest.json").parse().unwrap();
    let updater_v2 = app
        .updater_builder()
        .endpoints(vec![endpoint_v2])
        .unwrap()
        .target(target)
        .build()
        .expect("build updater v2");

    let update_v2 = tauri::async_runtime::block_on(updater_v2.check())
        .expect("check succeeds")
        .expect("v2 update available");
    assert_eq!(update_v2.version, "2.0.0");
    let downloaded_v2 = tauri::async_runtime::block_on(update_v2.download(|_, _| {}, || {}))
        .expect("v2 download and verification succeeds");
    assert_eq!(downloaded_v2, TEST_PAYLOAD);
}

#[test]
fn downgrade_and_same_version_rejected() {
    let target = "test-target";
    let target_str = target.to_string();

    // 1. Remote version is older than current app version (0.5.0 vs 0.6.2)
    let server_downgrade = MockHttpServer::start({
        let target_str = target_str.clone();
        move |addr, path| {
            if path == "/latest.json" {
                let download_url = format!("http://{}/download/release.tar.gz", addr);
                let json = release_json("0.5.0", &target_str, &download_url, SIG_V1);
                (200, "application/json", json)
            } else {
                (404, "text/plain", b"not found".to_vec())
            }
        }
    });

    let app = create_app("0.6.2", TEST_PUBKEY);
    let endpoint: tauri::Url = server_downgrade.url("latest.json").parse().unwrap();
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .unwrap()
        .target(target)
        .build()
        .expect("build updater");

    let update = tauri::async_runtime::block_on(updater.check()).expect("check succeeds");
    assert!(update.is_none(), "downgrade version must be rejected as no update");

    // 2. Remote version is identical to current app version (0.6.2 vs 0.6.2)
    let server_same = MockHttpServer::start({
        let target_str = target_str.clone();
        move |addr, path| {
            if path == "/latest.json" {
                let download_url = format!("http://{}/download/release.tar.gz", addr);
                let json = release_json("0.6.2", &target_str, &download_url, SIG_V1);
                (200, "application/json", json)
            } else {
                (404, "text/plain", b"not found".to_vec())
            }
        }
    });

    let endpoint_same: tauri::Url = server_same.url("latest.json").parse().unwrap();
    let updater_same = app
        .updater_builder()
        .endpoints(vec![endpoint_same])
        .unwrap()
        .target(target)
        .build()
        .expect("build updater");

    let update_same = tauri::async_runtime::block_on(updater_same.check()).expect("check succeeds");
    assert!(update_same.is_none(), "same version must report up-to-date (None)");
}

#[test]
fn bad_signature_rejected_during_verification() {
    let target = "test-target";

    // Release signed with untrusted / unknown private key
    let server = MockHttpServer::start({
        let target_str = target.to_string();
        move |addr, path| {
            if path == "/latest.json" {
                let download_url = format!("http://{}/download/release.tar.gz", addr);
                let json = release_json("1.0.0", &target_str, &download_url, SIG_BAD_KEY);
                (200, "application/json", json)
            } else if path == "/download/release.tar.gz" {
                (200, "application/octet-stream", TEST_PAYLOAD.to_vec())
            } else {
                (404, "text/plain", b"not found".to_vec())
            }
        }
    });

    let app = create_app("0.6.2", TEST_PUBKEY);
    let endpoint: tauri::Url = server.url("latest.json").parse().unwrap();
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .unwrap()
        .target(target)
        .build()
        .expect("build updater");

    let update = tauri::async_runtime::block_on(updater.check())
        .expect("check succeeds")
        .expect("update candidate found");

    let download_res = tauri::async_runtime::block_on(update.download(|_, _| {}, || {}));
    assert!(download_res.is_err(), "untrusted key signature must fail verification");
    let err_msg = download_res.unwrap_err().to_string();
    assert!(
        err_msg.contains("signature") || err_msg.contains("key"),
        "error must indicate signature failure: {err_msg}"
    );

    // Conversely, an app configured with TEST_PUBKEY_UNTRUSTED rejects an update signed by TEST_PUBKEY
    let app_untrusted = create_app("0.6.2", TEST_PUBKEY_UNTRUSTED);
    let server_v1 = MockHttpServer::start({
        let target_str = target.to_string();
        move |addr, path| {
            if path == "/latest.json" {
                let download_url = format!("http://{}/download/release.tar.gz", addr);
                let json = release_json("1.0.0", &target_str, &download_url, SIG_V1);
                (200, "application/json", json)
            } else if path == "/download/release.tar.gz" {
                (200, "application/octet-stream", TEST_PAYLOAD.to_vec())
            } else {
                (404, "text/plain", b"not found".to_vec())
            }
        }
    });

    let endpoint_v1: tauri::Url = server_v1.url("latest.json").parse().unwrap();
    let updater_untrusted = app_untrusted
        .updater_builder()
        .endpoints(vec![endpoint_v1])
        .unwrap()
        .target(target)
        .build()
        .expect("build updater");

    let update_cand = tauri::async_runtime::block_on(updater_untrusted.check())
        .expect("check succeeds")
        .expect("update candidate found");
    let download_res2 = tauri::async_runtime::block_on(update_cand.download(|_, _| {}, || {}));
    assert!(download_res2.is_err(), "mismatched public key must fail verification");
}

#[test]
fn tampered_payload_rejected_during_verification() {
    let target = "test-target";
    let target_str = target.to_string();

    // Valid signature for TEST_PAYLOAD, but server serves modified bytes
    let server = MockHttpServer::start(move |addr, path| {
        if path == "/latest.json" {
            let download_url = format!("http://{}/download/release.tar.gz", addr);
            let json = release_json("1.0.0", &target_str, &download_url, SIG_V1);
            (200, "application/json", json)
        } else if path == "/download/release.tar.gz" {
            (200, "application/octet-stream", b"tampered-malicious-payload".to_vec())
        } else {
            (404, "text/plain", b"not found".to_vec())
        }
    });

    let app = create_app("0.6.2", TEST_PUBKEY);
    let endpoint: tauri::Url = server.url("latest.json").parse().unwrap();
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .unwrap()
        .target(target)
        .build()
        .expect("build updater");

    let update = tauri::async_runtime::block_on(updater.check())
        .expect("check succeeds")
        .expect("update candidate found");

    let download_res = tauri::async_runtime::block_on(update.download(|_, _| {}, || {}));
    assert!(download_res.is_err(), "tampered artifact must fail verification");
}

#[test]
fn signed_version_mismatch_rejected() {
    let target = "test-target";
    let target_str = target.to_string();

    // latest.json announces version 2.0.0, but the signature trusted comment was signed for version 1.0.0
    let server = MockHttpServer::start(move |addr, path| {
        if path == "/latest.json" {
            let download_url = format!("http://{}/download/release.tar.gz", addr);
            let json = release_json("2.0.0", &target_str, &download_url, SIG_V1);
            (200, "application/json", json)
        } else if path == "/download/release.tar.gz" {
            (200, "application/octet-stream", TEST_PAYLOAD.to_vec())
        } else {
            (404, "text/plain", b"not found".to_vec())
        }
    });

    let app = create_app("0.6.2", TEST_PUBKEY);
    let endpoint: tauri::Url = server.url("latest.json").parse().unwrap();
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .unwrap()
        .target(target)
        .build()
        .expect("build updater");

    let update = tauri::async_runtime::block_on(updater.check())
        .expect("check succeeds")
        .expect("update candidate found");

    let download_res = tauri::async_runtime::block_on(update.download(|_, _| {}, || {}));
    assert!(download_res.is_err(), "version mismatch between announcement and signature must fail");
    let err_msg = download_res.unwrap_err().to_string();
    assert!(
        err_msg.contains("mismatch") || err_msg.contains("version"),
        "error must indicate signed version mismatch: {err_msg}"
    );
}

#[test]
fn release_204_no_content_handled() {
    let server = MockHttpServer::start(|_, path| {
        if path == "/latest.json" {
            (204, "application/json", Vec::new())
        } else {
            (404, "text/plain", b"not found".to_vec())
        }
    });

    let app = create_app("0.6.2", TEST_PUBKEY);
    let endpoint: tauri::Url = server.url("latest.json").parse().unwrap();
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .unwrap()
        .target("test-target")
        .build()
        .expect("build updater");

    let update = tauri::async_runtime::block_on(updater.check()).expect("check succeeds");
    assert!(update.is_none(), "204 No Content must return None");
}

#[test]
fn release_404_not_found_handled() {
    let server = MockHttpServer::start(|_, _| (404, "text/plain", b"not found".to_vec()));

    let app = create_app("0.6.2", TEST_PUBKEY);
    let endpoint: tauri::Url = server.url("latest.json").parse().unwrap();
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .unwrap()
        .target("test-target")
        .build()
        .expect("build updater");

    let check_res = tauri::async_runtime::block_on(updater.check());
    assert!(check_res.is_err(), "404 Not Found returns error which client handles as up-to-date");
}

#[test]
fn corrupted_signature_rejected() {
    let target = "test-target";
    let target_str = target.to_string();

    let server = MockHttpServer::start(move |addr, path| {
        if path == "/latest.json" {
            let download_url = format!("http://{}/download/release.tar.gz", addr);
            let json = release_json("1.0.0", &target_str, &download_url, "not-valid-base64!!!");
            (200, "application/json", json)
        } else if path == "/download/release.tar.gz" {
            (200, "application/octet-stream", TEST_PAYLOAD.to_vec())
        } else {
            (404, "text/plain", b"not found".to_vec())
        }
    });

    let app = create_app("0.6.2", TEST_PUBKEY);
    let endpoint: tauri::Url = server.url("latest.json").parse().unwrap();
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .unwrap()
        .target(target)
        .build()
        .expect("build updater");

    let update = tauri::async_runtime::block_on(updater.check())
        .expect("check succeeds")
        .expect("update candidate found");

    let download_res = tauri::async_runtime::block_on(update.download(|_, _| {}, || {}));
    assert!(download_res.is_err(), "corrupted signature must fail download verification");
}
