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

    let (code, out) = ssh_add_l(&a.socket_path);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(k.public_key.split(' ').nth(1).unwrap()), "{out}");
    assert!(out.contains(imp.public_key.split(' ').nth(1).unwrap()), "{out}");
    y_sign(&a.socket_path, kd.path(), &k.public_key).expect("ed25519 sign+verify");
    y_sign(&a.socket_path, kd.path(), &imp.public_key).expect("ecdsa sign+verify");

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
