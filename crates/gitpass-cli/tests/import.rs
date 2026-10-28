//! `gitpass import` CLI tests against a throwaway vault. All data FAKE.
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn gp(root: &Path, args: &[&str], stdin: Option<&str>) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_gitpass"));
    c.args(args)
        .env("GITPASS_PATH", root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut ch = c.spawn().unwrap();
    {
        let mut i = ch.stdin.take().unwrap();
        if let Some(s) = stdin {
            i.write_all(s.as_bytes()).unwrap();
        }
    }
    ch.wait_with_output().unwrap()
}

#[test]
fn cli_import_apple_passwords_csv() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let csv_file = d.path().join("apple-export.csv");
    std::fs::write(
        &csv_file,
        "Title,URL,Username,Password,Notes,OTPAuth\n\
         Apple Service,https://apple.example.invalid,alice@example.invalid,FAKE-pw-apple,\"Important notes\nwith newline\",otpauth://totp/Apple:alice@example.invalid?secret=JBSWY3DPEHPK3PXP&issuer=Apple\n\
         Other Login,https://other.example.invalid,bob@example.invalid,FAKE-pw-other,Single line note,\n",
    )
    .unwrap();

    // 1. Dry run
    let o = gp(
        &root,
        &[
            "import",
            "--from",
            "apple-passwords-csv",
            "Personal",
            csv_file.to_str().unwrap(),
            "--dry-run",
        ],
        None,
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("[dry run] parsed 2, would add 2, 0 duplicate(s) skipped"));

    // 2. Real import using alias "safari"
    let o = gp(
        &root,
        &[
            "import",
            "--from",
            "safari",
            "Personal",
            csv_file.to_str().unwrap(),
        ],
        None,
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("parsed 2, added 2, 0 duplicate(s) skipped"));

    // 3. Verify imported item fields
    let o = gp(&root, &["get", "Personal", "Apple Service", "--reveal"], None);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let get_out = String::from_utf8_lossy(&o.stdout);
    assert!(get_out.contains("alice@example.invalid"));
    assert!(get_out.contains("FAKE-pw-apple"));
    assert!(get_out.contains("https://apple.example.invalid"));
    assert!(get_out.contains("Important notes"));

    // 4. Second import without --allow-duplicates skips duplicates
    let o = gp(
        &root,
        &[
            "import",
            "--from",
            "apple-passwords",
            "Personal",
            csv_file.to_str().unwrap(),
        ],
        None,
    );
    assert!(o.status.success());
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("parsed 2, added 0, 2 duplicate(s) skipped"));
}
