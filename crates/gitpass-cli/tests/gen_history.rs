//! Integration tests for `gitpass gen history` and `--no-history`. All test data is FAKE.
use std::path::Path;
use std::process::{Command, Stdio};

fn gp(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_gitpass"))
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

fn out(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).trim_end_matches('\n').to_string()
}

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn test_cli_gen_history_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");

    // Initialize vault with fast kdf
    let init_out = gp(&root, &["init", "--save-secret-key"]);
    assert!(init_out.status.success(), "init failed: {}", err(&init_out));

    // Initially history is empty
    let h0 = gp(&root, &["gen", "history"]);
    assert!(h0.status.success());
    assert!(out(&h0).contains("(no generated passwords in history)"));

    let h0_json = gp(&root, &["gen", "history", "--json"]);
    assert!(h0_json.status.success());
    assert_eq!(out(&h0_json), "[]");

    // Generate a password (records to history)
    let g1 = gp(&root, &["gen", "24"]);
    assert!(g1.status.success(), "gen failed: {}", err(&g1));
    let pw1 = out(&g1);
    assert_eq!(pw1.len(), 24);

    // List history masked: must NOT reveal pw1, but must show masked dots
    let h1_masked = gp(&root, &["gen", "history"]);
    assert!(h1_masked.status.success());
    let masked_text = out(&h1_masked);
    assert!(masked_text.contains("••••••••"));
    assert!(masked_text.contains("[cli]"));
    assert!(!masked_text.contains(&pw1), "history without --reveal must not leak password");

    // List history with --reveal: must show pw1
    let h1_reveal = gp(&root, &["gen", "history", "--reveal"]);
    assert!(h1_reveal.status.success());
    let reveal_text = out(&h1_reveal);
    assert!(reveal_text.contains(&pw1));
    assert!(reveal_text.contains("[cli]"));

    // List history JSON without reveal
    let h1_json = gp(&root, &["gen", "history", "--json"]);
    assert!(h1_json.status.success());
    let json_text = out(&h1_json);
    assert!(json_text.contains("••••••••"));
    assert!(!json_text.contains(&pw1));

    // List history JSON with --reveal
    let h1_json_rev = gp(&root, &["gen", "history", "--json", "--reveal"]);
    assert!(h1_json_rev.status.success());
    assert!(out(&h1_json_rev).contains(&pw1));

    // Generate with --no-history: output should NOT appear in history
    let g2 = gp(&root, &["gen", "18", "--no-history"]);
    assert!(g2.status.success());
    let pw2 = out(&g2);
    assert_eq!(pw2.len(), 18);

    let h2_reveal = gp(&root, &["gen", "history", "--reveal"]);
    assert!(h2_reveal.status.success());
    let h2_text = out(&h2_reveal);
    assert!(!h2_text.contains(&pw2), "password generated with --no-history must not be recorded");

    // Clear history
    let clr = gp(&root, &["gen", "history", "clear"]);
    assert!(clr.status.success());
    let h3 = gp(&root, &["gen", "history"]);
    assert!(out(&h3).contains("(no generated passwords in history)"));
}
