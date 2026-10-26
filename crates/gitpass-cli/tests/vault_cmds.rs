//! `gitpass vault rename` / `vault rm` against a throwaway vault. All data FAKE.
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn gp(root: &Path, args: &[&str], stdin: Option<&str>) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_gitpass"));
    c.args(args).env("GITPASS_PATH", root).env("GITPASS_PASSWORD", "fake-master-pw-123").env("GITPASS_DEVICE_NAME", "test")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut ch = c.spawn().unwrap();
    { let mut i = ch.stdin.take().unwrap(); if let Some(s) = stdin { i.write_all(s.as_bytes()).unwrap(); } }
    ch.wait_with_output().unwrap()
}
fn list(root: &Path) -> String { String::from_utf8_lossy(&gp(root, &["vault", "list"], None).stdout).to_string() }

#[test]
fn rename_and_rm() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Dev"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Keep"], None).status.success());
    assert!(gp(&root, &["vault", "rename", "Dev", "Scratch"], None).status.success());
    let l = list(&root); assert!(l.contains("Scratch") && !l.contains("Dev"), "{l}");
    assert!(!gp(&root, &["vault", "rename", "Scratch", "Keep"], None).status.success(), "duplicate name");
    // wrong confirmation -> nothing deleted
    let o = gp(&root, &["vault", "rm", "Scratch"], Some("nope\n"));
    assert!(!o.status.success()); assert!(list(&root).contains("Scratch"));
    let o = gp(&root, &["vault", "rm", "Scratch"], Some("Scratch\n"));
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!list(&root).contains("Scratch"));
    // remaining vault(s): deleting down to zero is refused
    let names: Vec<String> = list(&root).lines().filter_map(|l| l.split_whitespace().next().map(String::from)).collect();
    let mut last_err = None;
    for n in &names { let o = gp(&root, &["vault", "rm", n, "--yes"], None); if !o.status.success() { last_err = Some(String::from_utf8_lossy(&o.stderr).to_string()); } }
    assert!(last_err.unwrap_or_default().contains("last vault"));
}
