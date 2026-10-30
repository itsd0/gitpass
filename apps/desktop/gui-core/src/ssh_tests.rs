//! SSH agent tests against the REAL OpenSSH client tools (`ssh-add`, `ssh-keygen -Y`) on a temp vault.
//! Skipped (with a message) when the tools are not installed.
use crate::agent::{AgentUi, Agent, read_frame, write_frame};
use crate::api::*;
use gitpass_core::crypto::KdfParams;
use gitpass_core::store::{self, UnlockOpts};
use serde_json::{json, Value};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }
struct Ui { allow: AtomicBool, asked: AtomicUsize }
impl AgentUi for Ui {
    fn request_unlock(&self, _: &str) {}
    fn open_item(&self, _: Option<&str>, _: Option<&str>) {}
    fn approve(&self, _: &str, _: &str) -> bool { self.asked.fetch_add(1, Ordering::SeqCst); self.allow.load(Ordering::SeqCst) }
}
struct Env { _d: tempfile::TempDir, _c: tempfile::TempDir, sock: tempfile::TempDir, app: Arc<App>, ui: Arc<Ui>, vault: String, dir: String, sk: String }
fn env() -> Env {
    let d = tempfile::tempdir().unwrap();
    let c = tempfile::tempdir().unwrap();
    let sock = tempfile::Builder::new().prefix("gps").tempdir_in("/tmp").unwrap();
    let sk = store::init(d.path(), "correct horse", fast()).unwrap().encode();
    let app = Arc::new(App::new(c.path().to_path_buf(), Arc::new(MemClipboard::default())));
    app.set_unlock_opts(UnlockOpts { accept_rollback: false, target_kdf: fast() });
    let dir = d.path().display().to_string();
    app.unlock(&dir, "correct horse", Some(&sk)).unwrap();
    let vault = app.list_vaults().unwrap()[0].id.clone();
    Env { _d: d, _c: c, sock, app, ui: Arc::new(Ui { allow: AtomicBool::new(true), asked: AtomicUsize::new(0) }), vault, dir, sk }
}
fn have_tools() -> bool { Command::new("ssh-add").arg("-h").output().is_ok() && Command::new("ssh-keygen").arg("-?").output().is_ok() }
fn ssh_add_l(sock: &std::path::Path) -> (i32, String) {
    let o = Command::new("ssh-add").arg("-L").env("SSH_AUTH_SOCK", sock).output().unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr))
}
/// `ssh-keygen -Y sign` with only the public key file: the private key operation goes to the agent.
fn y_sign(sock: &std::path::Path, dir: &std::path::Path, pubkey: &str) -> Result<String, String> {
    let pk = dir.join("k.pub"); std::fs::write(&pk, format!("{pubkey}\n")).unwrap();
    let msg = dir.join("msg.txt"); std::fs::write(&msg, b"hello from gitpass\n").unwrap();
    let _ = std::fs::remove_file(dir.join("msg.txt.sig"));
    let o = Command::new("ssh-keygen").args(["-Y", "sign", "-n", "file", "-f"]).arg(&pk).arg(&msg).env("SSH_AUTH_SOCK", sock).output().unwrap();
    if !o.status.success() { return Err(String::from_utf8_lossy(&o.stderr).to_string()); }
    // verify with ssh-keygen too
    let allowed = dir.join("allowed"); std::fs::write(&allowed, format!("test@gitpass {pubkey}\n")).unwrap();
    let v = Command::new("ssh-keygen").args(["-Y", "verify", "-n", "file", "-I", "test@gitpass", "-f"]).arg(&allowed).arg("-s").arg(dir.join("msg.txt.sig"))
        .stdin(std::fs::File::open(&msg).unwrap()).output().unwrap();
    if !v.status.success() { return Err(format!("verify failed: {}", String::from_utf8_lossy(&v.stderr))); }
    Ok(String::from_utf8_lossy(&v.stdout).to_string())
}

#[test]
fn real_ssh_add_and_ssh_keygen_sign_against_agent() {
    if !have_tools() { eprintln!("SKIP: ssh-add/ssh-keygen not installed"); return; }
    let e = env();
    let a = crate::ssh_agent::start(e.app.clone(), e.ui.clone(), &e.sock.path().join("gitpass")).unwrap();
    let k = e.app.ssh_generate(&e.vault, "Deploy key", "deploy@test").unwrap();
    assert!(k.public_key.starts_with("ssh-ed25519 ") && k.fingerprint.starts_with("SHA256:"));
    // imported ecdsa key (made by the real ssh-keygen, encrypted with a passphrase)
    let kd = tempfile::tempdir().unwrap();
    let kf = kd.path().join("id_ecdsa");
    assert!(Command::new("ssh-keygen").args(["-q", "-t", "ecdsa", "-b", "256", "-N", "pass phrase", "-C", "ecdsa@test", "-f"]).arg(&kf).status().unwrap().success());
    let pem = std::fs::read_to_string(&kf).unwrap();
    assert!(e.app.ssh_import(&e.vault, "", &pem, None).is_err(), "encrypted key needs passphrase");
    assert!(e.app.ssh_import(&e.vault, "", &pem, Some("wrong")).is_err());
    let imp = e.app.ssh_import(&e.vault, "", &pem, Some("pass phrase")).unwrap();
    let real_pub = std::fs::read_to_string(kf.with_extension("pub")).unwrap();
    assert_eq!(imp.public_key, real_pub.trim(), "public key matches ssh-keygen's .pub");
    let fp = Command::new("ssh-keygen").arg("-lf").arg(kf.with_extension("pub")).output().unwrap();
    assert!(String::from_utf8_lossy(&fp.stdout).contains(&imp.fingerprint), "fingerprint matches ssh-keygen -l");
    assert_eq!(imp.item.title, "ecdsa@test");

    // imported RSA key (2048 bits)
    let kf_rsa = kd.path().join("id_rsa");
    assert!(Command::new("ssh-keygen").args(["-q", "-t", "rsa", "-b", "2048", "-N", "", "-C", "rsa@test", "-f"]).arg(&kf_rsa).status().unwrap().success());
    let rsa_pem = std::fs::read_to_string(&kf_rsa).unwrap();
    let imp_rsa = e.app.ssh_import(&e.vault, "", &rsa_pem, None).unwrap();
    assert_eq!(imp_rsa.key_type, "ssh-rsa");
    let real_rsa_pub = std::fs::read_to_string(kf_rsa.with_extension("pub")).unwrap();
    assert_eq!(imp_rsa.public_key, real_rsa_pub.trim());
    let fp_rsa = Command::new("ssh-keygen").arg("-lf").arg(kf_rsa.with_extension("pub")).output().unwrap();
    assert!(String::from_utf8_lossy(&fp_rsa.stdout).contains(&imp_rsa.fingerprint));

    let (code, out) = ssh_add_l(&a.socket_path);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(k.public_key.split(' ').nth(1).unwrap()), "{out}");
    assert!(out.contains(imp.public_key.split(' ').nth(1).unwrap()), "{out}");
    assert!(out.contains(imp_rsa.public_key.split(' ').nth(1).unwrap()), "{out}");
    y_sign(&a.socket_path, kd.path(), &k.public_key).expect("ed25519 sign+verify");
    y_sign(&a.socket_path, kd.path(), &imp.public_key).expect("ecdsa sign+verify");
    y_sign(&a.socket_path, kd.path(), &imp_rsa.public_key).expect("rsa sign+verify");

    // per-use approval
    e.app.set_settings(Settings { ssh_approve_each_use: true, ..e.app.get_settings() }).unwrap();
    e.ui.allow.store(false, Ordering::SeqCst);
    assert!(y_sign(&a.socket_path, kd.path(), &k.public_key).is_err(), "denied prompt -> no signature");
    e.ui.allow.store(true, Ordering::SeqCst);
    y_sign(&a.socket_path, kd.path(), &k.public_key).expect("approved prompt");
    assert!(e.ui.asked.load(Ordering::SeqCst) >= 2);

    // locked: no identities, no signatures
    e.app.lock();
    let (code, out) = ssh_add_l(&a.socket_path);
    assert_ne!(code, 0); assert!(out.contains("no identities"), "{out}");
    assert!(y_sign(&a.socket_path, kd.path(), &k.public_key).is_err());
    // unlock again -> back
    e.app.unlock(&e.dir, "correct horse", Some(&e.sk)).unwrap();
    assert_eq!(ssh_add_l(&a.socket_path).0, 0);
    let info = e.app.ssh_agent_info(&a.socket_path, true);
    assert!(info.config_line.starts_with("Host *\n  IdentityAgent \"") && info.config_line.contains("ssh-agent.sock"));
}

#[test]
fn ssh_agent_activity_records_signed_denied_refused_and_clears_on_lock() {
    use crate::ssh_activity::SshActivityResult;
    use gitpass_core::ssh::{SSH_AGENTC_SIGN_REQUEST, SSH_AGENT_FAILURE, SSH_AGENT_SIGN_RESPONSE};
    use std::io::{Read, Write};

    fn sign(s: &mut std::os::unix::net::UnixStream, blob: &[u8], data: &[u8], flags: u32) -> u8 {
        let mut body = vec![SSH_AGENTC_SIGN_REQUEST];
        body.extend_from_slice(&(blob.len() as u32).to_be_bytes());
        body.extend_from_slice(blob);
        body.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(data);
        body.extend_from_slice(&flags.to_be_bytes());
        s.write_all(&(body.len() as u32).to_be_bytes()).unwrap();
        s.write_all(&body).unwrap();
        s.flush().unwrap();
        let mut len = [0u8; 4];
        s.read_exact(&mut len).unwrap();
        let mut resp = vec![0u8; u32::from_be_bytes(len) as usize];
        s.read_exact(&mut resp).unwrap();
        resp[0]
    }

    let e = env();
    let a = crate::ssh_agent::start(e.app.clone(), e.ui.clone(), &e.sock.path().join("gitpass")).unwrap();
    let mut s = std::os::unix::net::UnixStream::connect(&a.socket_path).unwrap();
    let k = e.app.ssh_generate(&e.vault, "Deploy key", "deploy@example.invalid").unwrap();
    let blob = gitpass_core::ssh::public_blob_from_openssh(&k.public_key).unwrap();
    let payload = b"payload-that-must-not-be-logged";

    assert_eq!(sign(&mut s, &blob, payload, 0), SSH_AGENT_SIGN_RESPONSE);
    let act = e.app.ssh_agent_activity();
    assert_eq!(act.len(), 1);
    let first = &act[0];
    assert_eq!(first.key_title, "Deploy key");
    assert_eq!(first.fingerprint, k.fingerprint);
    assert_eq!(first.key_type, "ssh-ed25519");
    assert_eq!(first.algorithm, "ed25519");
    assert_eq!(first.result, SshActivityResult::Signed);
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        assert!(first.pid.is_some(), "peer pid is captured from the socket");
        assert!(first.client.is_some(), "peer process name is captured from the socket");
    }
    let json = serde_json::to_string(&act).unwrap();
    assert!(!json.contains("payload-that-must-not-be-logged"), "signed data never enters the log: {json}");

    e.app.set_settings(Settings { ssh_approve_each_use: true, ..e.app.get_settings() }).unwrap();
    e.ui.allow.store(false, Ordering::SeqCst);
    assert_eq!(sign(&mut s, &blob, b"data", 0), SSH_AGENT_FAILURE);
    assert_eq!(e.app.ssh_agent_activity()[0].result, SshActivityResult::Denied);
    e.ui.allow.store(true, Ordering::SeqCst);

    assert_eq!(sign(&mut s, b"not-a-key", b"data", 0), SSH_AGENT_FAILURE);
    let refused = &e.app.ssh_agent_activity()[0];
    assert_eq!(refused.result, SshActivityResult::Refused);
    assert_eq!(refused.key_title, "");

    e.app.lock();
    assert!(e.app.ssh_agent_activity().is_empty(), "locking clears the activity log");

    e.app.unlock(&e.dir, "correct horse", Some(&e.sk)).unwrap();
    assert_eq!(sign(&mut s, &blob, b"data", 0), SSH_AGENT_SIGN_RESPONSE);
    assert_eq!(e.app.ssh_agent_activity().len(), 1);
    e.app.clear_ssh_agent_activity();
    assert!(e.app.ssh_agent_activity().is_empty(), "clear button empties the log");
}

#[test]
fn ssh_activity_clears_on_idle_auto_lock() {
    use gitpass_core::ssh::KeyProvider;
    let e = env();
    let keys = crate::ssh_agent::AppKeys { app: e.app.clone(), ui: e.ui.clone() };
    let k = e.app.ssh_generate(&e.vault, "Idle key", "idle@example.invalid").unwrap();
    let blob = gitpass_core::ssh::public_blob_from_openssh(&k.public_key).unwrap();
    keys.sign(&blob, b"data", 0).unwrap();
    assert_eq!(e.app.ssh_agent_activity().len(), 1);

    e.app.state.set_auto_lock_duration(std::time::Duration::from_secs(1));
    e.app.state.backdate_activity_wall(std::time::Duration::from_secs(5));
    assert!(e.app.state.check_idle(), "idle timeout locks the vault");
    assert!(e.app.ssh_agent_activity().is_empty(), "idle auto-lock clears the activity log");
}

#[test]
fn agent_socket_passkey_needs_approval() {
    let e = env();
    let sd = tempfile::Builder::new().prefix("gpk").tempdir_in("/tmp").unwrap();
    let agent = Agent::start(e.app.clone(), e.ui.clone(), &sd.path().join("gitpass")).unwrap();
    let mut s = std::os::unix::net::UnixStream::connect(&agent.socket_path).unwrap();
    let mut n = 0;
    let mut call = |q: Value| -> Value { n += 1; let mut q = q; q["id"] = json!(n); write_frame(&mut s, &q).unwrap(); loop { let v = read_frame(&mut s).unwrap().unwrap(); if v.get("event").is_none() { return v; } } };
    assert_eq!(call(json!({"cmd":"hello","token":agent.token()}))["ok"], true);
    let q = json!({"cmd":"passkey_create","origin":"https://example.com","challenge":"YQ","user_id":"dQ","user_name":"a"});
    e.ui.allow.store(false, Ordering::SeqCst);
    let r = call(q.clone()); assert_eq!(r["error"], "denied", "{r}");
    e.ui.allow.store(true, Ordering::SeqCst);
    let r = call(q.clone()); assert_eq!(r["ok"], true, "app prompt approved: {r}");
    let r = call(json!({"cmd":"passkey_get","source":"popup","approved":true,"origin":"https://example.com","challenge":"Yg"}));
    assert_eq!(r["ok"], true, "{r}"); assert!(r["result"]["signature"].is_string());
    let r = call(json!({"cmd":"passkey_get","source":"popup","approved":true,"origin":"https://evil.com","rp_id":"example.com","challenge":"Yg"}));
    assert_eq!(r["error"], "origin_mismatch", "{r}");
}

#[test]
fn ssh_agent_socket_wire_protocol_add_list_remove_and_sign() {
    let e = env();
    let a = crate::ssh_agent::start(e.app.clone(), e.ui.clone(), &e.sock.path().join("gitpass")).unwrap();
    let mut s = std::os::unix::net::UnixStream::connect(&a.socket_path).unwrap();

    let req_ids = |s: &mut std::os::unix::net::UnixStream| -> Vec<(Vec<u8>, String)> {
        use std::io::{Read, Write};
        let req = [0u8, 0, 0, 1, gitpass_core::ssh::SSH_AGENTC_REQUEST_IDENTITIES];
        s.write_all(&req).unwrap();
        s.flush().unwrap();
        let mut len_b = [0u8; 4];
        s.read_exact(&mut len_b).unwrap();
        let len = u32::from_be_bytes(len_b) as usize;
        let mut resp = vec![0u8; len];
        s.read_exact(&mut resp).unwrap();
        assert_eq!(resp[0], gitpass_core::ssh::SSH_AGENT_IDENTITIES_ANSWER);
        let n = u32::from_be_bytes(resp[1..5].try_into().unwrap()) as usize;
        let mut at = 5;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let blen = u32::from_be_bytes(resp[at..at + 4].try_into().unwrap()) as usize;
            at += 4;
            let blob = resp[at..at + blen].to_vec();
            at += blen;
            let clen = u32::from_be_bytes(resp[at..at + 4].try_into().unwrap()) as usize;
            at += 4;
            let comment = String::from_utf8(resp[at..at + clen].to_vec()).unwrap();
            at += clen;
            out.push((blob, comment));
        }
        out
    };

    let sign_req = |s: &mut std::os::unix::net::UnixStream, blob: &[u8], data: &[u8]| -> Result<Vec<u8>, u8> {
        use std::io::{Read, Write};
        let mut body = vec![gitpass_core::ssh::SSH_AGENTC_SIGN_REQUEST];
        body.extend_from_slice(&(blob.len() as u32).to_be_bytes());
        body.extend_from_slice(blob);
        body.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(data);
        body.extend_from_slice(&0u32.to_be_bytes());

        let len = (body.len() as u32).to_be_bytes();
        s.write_all(&len).unwrap();
        s.write_all(&body).unwrap();
        s.flush().unwrap();

        let mut len_b = [0u8; 4];
        s.read_exact(&mut len_b).unwrap();
        let resp_len = u32::from_be_bytes(len_b) as usize;
        let mut resp = vec![0u8; resp_len];
        s.read_exact(&mut resp).unwrap();

        if resp[0] == gitpass_core::ssh::SSH_AGENT_SIGN_RESPONSE {
            let sig_len = u32::from_be_bytes(resp[1..5].try_into().unwrap()) as usize;
            Ok(resp[5..5 + sig_len].to_vec())
        } else {
            Err(resp[0])
        }
    };

    // 1. Initial state: 0 identities
    let ids = req_ids(&mut s);
    assert_eq!(ids.len(), 0);

    // 2. Add identity via app (Ed25519)
    let k1 = e.app.ssh_generate(&e.vault, "Deploy key", "deploy@test.example").unwrap();
    let k1_blob = gitpass_core::ssh::public_blob_from_openssh(&k1.public_key).unwrap();
    let ids = req_ids(&mut s);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].1, "Deploy key");
    assert_eq!(&ids[0].0, &k1_blob);

    // 3. Sign request over socket succeeds and signature verifies
    let payload = b"commit 42: zero-knowledge gitpass ssh test payload";
    let sig = sign_req(&mut s, &k1_blob, payload).expect("sign succeeds");
    assert!(gitpass_core::ssh::verify_blob(&k1_blob, payload, &sig).is_ok());
    assert!(gitpass_core::ssh::verify_blob(&k1_blob, b"tampered payload", &sig).is_err());

    // 4. Sign request with unknown key returns failure (SSH_AGENT_FAILURE = 5)
    let bad_blob = b"not-a-valid-ssh-key-blob";
    assert_eq!(sign_req(&mut s, bad_blob, payload), Err(gitpass_core::ssh::SSH_AGENT_FAILURE));

    // 5. Add second identity
    let k2 = e.app.ssh_generate(&e.vault, "Personal key", "me@home.example").unwrap();
    let k2_blob = gitpass_core::ssh::public_blob_from_openssh(&k2.public_key).unwrap();
    let ids = req_ids(&mut s);
    assert_eq!(ids.len(), 2);
    let titles: Vec<&str> = ids.iter().map(|(_, c)| c.as_str()).collect();
    assert!(titles.contains(&"Deploy key") && titles.contains(&"Personal key"));

    let sig2 = sign_req(&mut s, &k2_blob, payload).expect("second key sign succeeds");
    assert!(gitpass_core::ssh::verify_blob(&k2_blob, payload, &sig2).is_ok());

    // 6. Remove first identity (k1) from vault
    e.app.delete_item(&e.vault, &k1.item.id).unwrap();
    let ids = req_ids(&mut s);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].1, "Personal key");

    // Signing with removed key now fails
    assert_eq!(sign_req(&mut s, &k1_blob, payload), Err(gitpass_core::ssh::SSH_AGENT_FAILURE));
    // Signing with remaining key still succeeds
    assert!(sign_req(&mut s, &k2_blob, payload).is_ok());

    // 7. Remove second identity (k2) from vault
    e.app.delete_item(&e.vault, &k2.item.id).unwrap();
    let ids = req_ids(&mut s);
    assert_eq!(ids.len(), 0);
    assert_eq!(sign_req(&mut s, &k2_blob, payload), Err(gitpass_core::ssh::SSH_AGENT_FAILURE));

    // 8. Add key again to test lock/unlock
    let k3 = e.app.ssh_generate(&e.vault, "Key 3", "k3@example").unwrap();
    let k3_blob = gitpass_core::ssh::public_blob_from_openssh(&k3.public_key).unwrap();
    assert_eq!(req_ids(&mut s).len(), 1);

    // Lock the vault
    e.app.lock();
    assert_eq!(req_ids(&mut s).len(), 0);
    assert_eq!(sign_req(&mut s, &k3_blob, payload), Err(gitpass_core::ssh::SSH_AGENT_FAILURE));

    // Unlock the vault again
    e.app.unlock(&e.dir, "correct horse", Some(&e.sk)).unwrap();
    assert_eq!(req_ids(&mut s).len(), 1);
    assert!(sign_req(&mut s, &k3_blob, payload).is_ok());

    // 9. Per-use approval
    e.app.set_settings(Settings { ssh_approve_each_use: true, ..e.app.get_settings() }).unwrap();
    e.ui.allow.store(false, Ordering::SeqCst);
    assert_eq!(sign_req(&mut s, &k3_blob, payload), Err(gitpass_core::ssh::SSH_AGENT_FAILURE));
    e.ui.allow.store(true, Ordering::SeqCst);
    assert!(sign_req(&mut s, &k3_blob, payload).is_ok());

    // 10. Check socket and directory permissions (0600 on socket, 0700 on parent dir)
    use std::os::unix::fs::PermissionsExt;
    let smeta = std::fs::metadata(&a.socket_path).unwrap();
    assert_eq!(smeta.permissions().mode() & 0o777, 0o600);
    let dmeta = std::fs::metadata(a.socket_path.parent().unwrap()).unwrap();
    assert_eq!(dmeta.permissions().mode() & 0o777, 0o700);
}
