//! CLI unlock failure throttling tests. All test data fake, temp dirs only.
use std::process::Command;

fn gp_with_pw(root: &std::path::Path, args: &[&str], pw: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_gitpass"))
        .args(args)
        .env("GITPASS_PATH", root)
        .env("GITPASS_PASSWORD", pw)
        .env("GITPASS_DEVICE_NAME", "test-device")
        .output()
        .unwrap()
}

#[test]
fn cli_unlock_throttled_after_three_wrong_passwords() {
    gitpass_core::unlock_throttle::reset_memory();
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    let init_out = gp_with_pw(&root, &["init", "--save-secret-key"], "correct-horse-battery");
    assert!(init_out.status.success());

    // 1st wrong attempt
    let out1 = gp_with_pw(&root, &["unlock"], "wrong-1");
    assert!(!out1.status.success());
    let err1 = String::from_utf8_lossy(&out1.stderr);
    assert!(err1.contains("unlock failed"), "{err1}");

    // 2nd wrong attempt
    let out2 = gp_with_pw(&root, &["unlock"], "wrong-2");
    assert!(!out2.status.success());

    // 3rd wrong attempt
    let out3 = gp_with_pw(&root, &["unlock"], "wrong-3");
    assert!(!out3.status.success());

    // 4th attempt: throttled!
    let out4 = gp_with_pw(&root, &["unlock"], "correct-horse-battery");
    assert!(!out4.status.success());
    let err4 = String::from_utf8_lossy(&out4.stderr);
    assert!(err4.contains("throttled: try again in "), "{err4}");
}
