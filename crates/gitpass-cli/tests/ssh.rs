//! SSH CLI commands (`gitpass ssh generate/import/list/public/config/agent`) and agent wire protocol
//! over a Unix domain socket against a throwaway vault. All test data is FAKE.
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_gitpass")
}

fn gp(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .args(args)
        .env("GITPASS_PATH", root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test-device")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap()
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for_socket(child: &mut Child, sock: &Path, timeout: Duration) -> UnixStream {
    let start = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut err);
            }
            panic!("agent process exited early with status {status:?}; stderr: {err}");
        }
        if sock.exists() {
            if let Ok(s) = UnixStream::connect(sock) {
                return s;
            }
        }
        if start.elapsed() > timeout {
            panic!("timed out waiting for agent socket at {}", sock.display());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn request_identities(s: &mut UnixStream) -> Vec<(Vec<u8>, String)> {
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
}

fn sign_request(s: &mut UnixStream, blob: &[u8], data: &[u8]) -> Result<Vec<u8>, u8> {
    let mut body = vec![gitpass_core::ssh::SSH_AGENTC_SIGN_REQUEST];
    body.extend_from_slice(&(blob.len() as u32).to_be_bytes());
    body.extend_from_slice(blob);
    body.extend_from_slice(&(data.len() as u32).to_be_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(&0u32.to_be_bytes()); // flags

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
}

#[test]
fn ssh_cli_lifecycle_and_agent_socket() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");

    // 1. Initialize vault and create test vault
    assert!(gp(&root, &["init", "--save-secret-key"]).status.success());
    assert!(gp(&root, &["vault", "create", "Dev"]).status.success());

    // 2. Generate Ed25519 key via CLI
    let o = gp(
        &root,
        &[
            "ssh",
            "generate",
            "Dev",
            "--title",
            "Deploy Key",
            "--comment",
            "deploy@example.invalid",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("id:"), "{out}");
    assert!(out.contains("public key:  ssh-ed25519 AAAA"), "{out}");
    assert!(out.contains("fingerprint: SHA256:"), "{out}");

    // Extract item id from generate output
    let gen_id = out
        .lines()
        .find(|l| l.starts_with("id:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();

    // 3. List SSH keys via CLI
    let o = gp(&root, &["ssh", "list", "Dev"]);
    assert!(o.status.success());
    let list_out = String::from_utf8_lossy(&o.stdout);
    assert!(list_out.contains(&gen_id));
    assert!(list_out.contains("ssh-ed25519"));
    assert!(list_out.contains("Deploy Key"));

    // 4. Public key line via CLI
    let o = gp(&root, &["ssh", "public", "Dev", "Deploy Key"]);
    assert!(o.status.success());
    let pub_out = String::from_utf8_lossy(&o.stdout);
    assert!(pub_out.starts_with("ssh-ed25519 AAAA"));
    assert!(pub_out.trim_end().ends_with("deploy@example.invalid"));

    // 5. Config line via CLI
    let sock = d.path().join("ssh-agent.sock");
    let o = gp(&root, &["ssh", "config", "--socket", sock.to_str().unwrap()]);
    assert!(o.status.success());
    let cfg_out = String::from_utf8_lossy(&o.stdout);
    assert!(cfg_out.contains("Host *\n  IdentityAgent "));
    assert!(cfg_out.contains("ssh-agent.sock"));

    // 6. Import an unencrypted OpenSSH private key file
    let imp_key = gitpass_core::ssh::SshKey::generate_ed25519("imported@example.invalid").unwrap();
    let imp_file = d.path().join("imported_id_ed25519");
    std::fs::write(&imp_file, imp_key.private_pem().unwrap().as_str()).unwrap();
    let o = gp(
        &root,
        &[
            "ssh",
            "import",
            "Dev",
            imp_file.to_str().unwrap(),
            "--title",
            "Imported Key",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));

    // List now shows both keys
    let o = gp(&root, &["ssh", "list", "Dev"]);
    let list_out = String::from_utf8_lossy(&o.stdout);
    assert!(list_out.contains("Deploy Key"));
    assert!(list_out.contains("Imported Key"));

    // 7. Run `gitpass ssh agent --socket <sock>` as background process
    let mut child = Command::new(bin())
        .args(["ssh", "agent", "--socket", sock.to_str().unwrap()])
        .env("GITPASS_PATH", &root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test-device")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Wait for agent to start listening
    let mut stream = wait_for_socket(&mut child, &sock, Duration::from_secs(30));
    let _guard = ChildGuard(child);

    // Request identities over the socket
    let ids = request_identities(&mut stream);
    assert_eq!(ids.len(), 2, "agent should advertise both vault keys");
    let titles: Vec<&str> = ids.iter().map(|(_, c)| c.as_str()).collect();
    assert!(titles.contains(&"Deploy Key") && titles.contains(&"Imported Key"));

    // Sign request over the socket
    let deploy_blob = ids.iter().find(|(_, c)| c == "Deploy Key").unwrap().0.clone();
    let payload = b"commit 101: signed over gitpass standalone ssh agent socket";
    let sig = sign_request(&mut stream, &deploy_blob, payload).expect("sign over socket succeeds");
    assert!(gitpass_core::ssh::verify_blob(&deploy_blob, payload, &sig).is_ok());
    assert!(gitpass_core::ssh::verify_blob(&deploy_blob, b"corrupted payload", &sig).is_err());

    // Sign with unknown key fails
    assert_eq!(
        sign_request(&mut stream, b"bogus-blob", payload),
        Err(gitpass_core::ssh::SSH_AGENT_FAILURE)
    );

    // Drop agent process
    drop(_guard);

    // 8. Remove an SSH key item
    let o = gp(&root, &["rm", "Dev", &gen_id]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));

    // List confirms Deploy Key is removed, Imported Key remains
    let o = gp(&root, &["ssh", "list", "Dev"]);
    let list_out = String::from_utf8_lossy(&o.stdout);
    assert!(!list_out.contains("Deploy Key"));
    assert!(list_out.contains("Imported Key"));
}
