//! Test Git commit and tag signing via `gitpass ssh git-signing` and the SSH agent.
//! Uses a temp HOME, temp Git repo, and temp vault only. Never touches real vault or ~/.gitconfig.
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_gitpass")
}

fn gp(home: &Path, root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .args(args)
        .env("HOME", home)
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

#[test]
fn git_signing_cli_workflow_and_git_verify_commit() {
    let d = tempfile::tempdir().unwrap();
    let home = d.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let root = home.join(".gitpass");

    // 1. Initialize vault
    assert!(gp(&home, &root, &["init", "--save-secret-key"]).status.success());
    assert!(gp(&home, &root, &["vault", "create", "Dev"]).status.success());

    // 2. Generate Ed25519 key
    let o = gp(
        &home,
        &root,
        &[
            "ssh",
            "generate",
            "Dev",
            "--title",
            "Deploy Key",
            "--comment",
            "deployer@example.invalid",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));

    // 3. Import 2048-bit RSA key
    let rsa_key_file = d.path().join("id_rsa_2048");
    assert!(Command::new("ssh-keygen")
        .args(["-q", "-t", "rsa", "-b", "2048", "-N", "", "-C", "rsa@example.invalid", "-f"])
        .arg(&rsa_key_file)
        .status()
        .unwrap()
        .success());
    let o = gp(
        &home,
        &root,
        &[
            "ssh",
            "import",
            "Dev",
            rsa_key_file.to_str().unwrap(),
            "--title",
            "Deploy RSA",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));

    // 4. Import ECDSA key (to test refusal)
    let ecdsa_key_file = d.path().join("id_ecdsa_256");
    assert!(Command::new("ssh-keygen")
        .args(["-q", "-t", "ecdsa", "-b", "256", "-N", "", "-C", "ecdsa@example.invalid", "-f"])
        .arg(&ecdsa_key_file)
        .status()
        .unwrap()
        .success());
    let o = gp(
        &home,
        &root,
        &[
            "ssh",
            "import",
            "Dev",
            ecdsa_key_file.to_str().unwrap(),
            "--title",
            "Deploy ECDSA",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));

    // 5. ECDSA key is refused for git-signing
    let o = gp(&home, &root, &["ssh", "git-signing", "Dev", "Deploy ECDSA"]);
    assert!(!o.status.success(), "expected ECDSA git-signing to be rejected");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains("Git commit signing supports Ed25519 and RSA keys"),
        "error must be clear about supported keys: {err}"
    );

    // 6. Test --print (the default) includes export SSH_AUTH_SOCK and does NOT write files
    let o = gp(&home, &root, &["ssh", "git-signing", "Dev", "Deploy Key"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let print_out = String::from_utf8_lossy(&o.stdout);
    assert!(print_out.contains("export SSH_AUTH_SOCK="), "{print_out}");
    assert!(print_out.contains("gpg.format=ssh"), "{print_out}");
    assert!(print_out.contains("user.signingkey=\"key::ssh-ed25519 AAAA"), "{print_out}");
    assert!(print_out.contains("commit.gpgsign=true"), "{print_out}");
    assert!(print_out.contains("tag.gpgsign=true"), "{print_out}");
    assert!(print_out.contains("gpg.ssh.allowedSignersFile=~/.ssh/allowed_signers"), "{print_out}");

    // Verify nothing written yet
    let allowed_signers = home.join(".ssh/allowed_signers");
    assert!(!allowed_signers.exists(), "allowed_signers should not exist in --print mode");
    assert!(!home.join(".gitconfig").exists(), "~/.gitconfig should not exist in --print mode");

    // 7. Test --apply without --yes aborts on non-interactive prompt
    let o = gp(&home, &root, &["ssh", "git-signing", "Dev", "Deploy Key", "--apply"]);
    assert!(!o.status.success(), "expected failure without --yes in non-interactive environment");

    // 8. Test --apply --yes applies configuration to ~/.gitconfig and allowed_signers
    let o = gp(&home, &root, &["ssh", "git-signing", "Dev", "Deploy Key", "--apply", "--yes"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(allowed_signers.exists(), "allowed_signers must be created");
    let signers_content = std::fs::read_to_string(&allowed_signers).unwrap();
    assert!(signers_content.starts_with("deployer@example.invalid ssh-ed25519 AAAA"));

    // Verify ~/.gitconfig was written
    let gitconfig_content = std::fs::read_to_string(home.join(".gitconfig")).unwrap();
    assert!(gitconfig_content.contains("format = ssh"));
    assert!(gitconfig_content.contains("gpgsign = true"));

    // 9. Start gitpass ssh agent on a temp socket
    let sock = d.path().join("agent.sock");
    let mut child = Command::new(bin())
        .args(["ssh", "agent", "--socket", sock.to_str().unwrap()])
        .env("HOME", &home)
        .env("GITPASS_PATH", &root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test-device")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut _stream = wait_for_socket(&mut child, &sock, Duration::from_secs(30));
    let _guard = ChildGuard(child);

    // 10. Create a temp Git repository and make a signed commit using instructions from --print
    let repo_dir = d.path().join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();
    assert!(Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .status()
        .unwrap()
        .success());

    assert!(Command::new("git")
        .args(["config", "user.name", "Test Deployer"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .status()
        .unwrap()
        .success());

    assert!(Command::new("git")
        .args(["config", "user.email", "deployer@example.invalid"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .status()
        .unwrap()
        .success());

    std::fs::write(repo_dir.join("file.txt"), "hello ed25519 commit signing\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "file.txt"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .status()
        .unwrap()
        .success());

    // Extract the printed export SSH_AUTH_SOCK instruction
    let o = gp(
        &home,
        &root,
        &[
            "ssh",
            "git-signing",
            "Dev",
            "Deploy Key",
            "--socket",
            sock.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let print_inst = String::from_utf8_lossy(&o.stdout);
    let auth_sock_line = print_inst
        .lines()
        .find(|l| l.starts_with("export SSH_AUTH_SOCK="))
        .expect("must output export SSH_AUTH_SOCK instruction");
    let extracted_sock = auth_sock_line
        .trim_start_matches("export SSH_AUTH_SOCK=\"")
        .trim_end_matches('"');
    assert_eq!(extracted_sock, sock.to_str().unwrap());

    // Without SSH_AUTH_SOCK (or pointing to macOS default / unset), signing fails
    let failed_commit = Command::new("git")
        .args(["commit", "-m", "should fail without agent"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .env_remove("SSH_AUTH_SOCK")
        .output()
        .unwrap();
    assert!(
        !failed_commit.status.success(),
        "git commit without SSH_AUTH_SOCK must fail because ssh-keygen cannot find key"
    );

    // Commit using the exact SSH_AUTH_SOCK path parsed from printed instructions
    let commit_out = Command::new("git")
        .args(["commit", "-m", "signed commit with ed25519"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .env("SSH_AUTH_SOCK", extracted_sock)
        .output()
        .unwrap();
    assert!(
        commit_out.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&commit_out.stderr)
    );

    // Verify commit
    let verify_out = Command::new("git")
        .args(["verify-commit", "HEAD"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .env("SSH_AUTH_SOCK", extracted_sock)
        .output()
        .unwrap();
    let verify_combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&verify_out.stdout),
        String::from_utf8_lossy(&verify_out.stderr)
    );
    assert!(
        verify_out.status.success(),
        "git verify-commit failed: {verify_combined}"
    );
    assert!(
        verify_combined.contains("Good \"git\" signature for deployer@example.invalid"),
        "expected good signature verification, got: {verify_combined}"
    );

    // 11. Test RSA commit and tag signing, plus warning behavior when SSH_AUTH_SOCK is inactive
    // A. Applying when SSH_AUTH_SOCK != sock emits a warning on stderr
    let o = Command::new(bin())
        .args([
            "ssh",
            "git-signing",
            "Dev",
            "Deploy RSA",
            "--apply",
            "--yes",
            "--socket",
            sock.to_str().unwrap(),
            "--email",
            "rsa@example.invalid",
        ])
        .env("HOME", &home)
        .env("GITPASS_PATH", &root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test-device")
        .env_remove("SSH_AUTH_SOCK")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let warn_err = String::from_utf8_lossy(&o.stderr);
    assert!(
        warn_err.contains("Warning: SSH_AUTH_SOCK does not point to the gitpass SSH agent."),
        "expected warning when SSH_AUTH_SOCK is not pointing to gitpass: {warn_err}"
    );

    // B. Applying when SSH_AUTH_SOCK == sock does NOT emit warning
    let o = Command::new(bin())
        .args([
            "ssh",
            "git-signing",
            "Dev",
            "Deploy RSA",
            "--apply",
            "--yes",
            "--socket",
            sock.to_str().unwrap(),
            "--email",
            "rsa@example.invalid",
        ])
        .env("HOME", &home)
        .env("GITPASS_PATH", &root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test-device")
        .env("SSH_AUTH_SOCK", &sock)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let clean_err = String::from_utf8_lossy(&o.stderr);
    assert!(
        !clean_err.contains("Warning: SSH_AUTH_SOCK does not point"),
        "expected no warning when SSH_AUTH_SOCK is active: {clean_err}"
    );

    assert!(Command::new("git")
        .args(["config", "user.email", "rsa@example.invalid"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .status()
        .unwrap()
        .success());

    std::fs::write(repo_dir.join("file2.txt"), "hello rsa commit signing\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "file2.txt"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .status()
        .unwrap()
        .success());

    let commit_rsa_out = Command::new("git")
        .args(["commit", "-m", "signed commit with rsa"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .env("SSH_AUTH_SOCK", &sock)
        .output()
        .unwrap();
    assert!(
        commit_rsa_out.status.success(),
        "git commit (RSA) failed: {}",
        String::from_utf8_lossy(&commit_rsa_out.stderr)
    );

    let verify_rsa_out = Command::new("git")
        .args(["verify-commit", "HEAD"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .env("SSH_AUTH_SOCK", &sock)
        .output()
        .unwrap();
    let verify_rsa_combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&verify_rsa_out.stdout),
        String::from_utf8_lossy(&verify_rsa_out.stderr)
    );
    assert!(
        verify_rsa_out.status.success(),
        "git verify-commit (RSA) failed: {verify_rsa_combined}"
    );
    assert!(
        verify_rsa_combined.contains("Good \"git\" signature for rsa@example.invalid"),
        "expected good RSA signature verification, got: {verify_rsa_combined}"
    );

    // 12. Test tag signing
    let tag_out = Command::new("git")
        .args(["tag", "-s", "v1.0.0", "-m", "release v1.0.0"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .env("SSH_AUTH_SOCK", &sock)
        .output()
        .unwrap();
    assert!(
        tag_out.status.success(),
        "git tag -s failed: {}",
        String::from_utf8_lossy(&tag_out.stderr)
    );

    let verify_tag_out = Command::new("git")
        .args(["verify-tag", "v1.0.0"])
        .current_dir(&repo_dir)
        .env("HOME", &home)
        .env("SSH_AUTH_SOCK", &sock)
        .output()
        .unwrap();
    let verify_tag_combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&verify_tag_out.stdout),
        String::from_utf8_lossy(&verify_tag_out.stderr)
    );
    assert!(
        verify_tag_out.status.success(),
        "git verify-tag failed: {verify_tag_combined}"
    );
    assert!(
        verify_tag_combined.contains("Good \"git\" signature for rsa@example.invalid"),
        "expected good RSA tag signature, got: {verify_tag_combined}"
    );
}
