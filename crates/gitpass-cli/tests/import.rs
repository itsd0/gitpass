//! `gitpass import` CLI tests against a throwaway vault. All data FAKE.
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn gp(root: &Path, args: &[&str], stdin: Option<&str>) -> std::process::Output {
    gp_env(root, args, stdin, &[])
}

fn gp_env(root: &Path, args: &[&str], stdin: Option<&str>, envs: &[(&str, &str)]) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_gitpass"));
    c.args(args)
        .env("GITPASS_PATH", root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test")
        .env_remove("GITPASS_IMPORT_PASSWORD")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        c.env(k, v);
    }
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
fn cli_import_lastpass_csv() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let csv_file = d.path().join("lastpass-export.csv");
    std::fs::write(
        &csv_file,
        "url,username,password,totp,extra,name,grouping,fav\n\
         https://service.example.invalid,alice@example.invalid,FAKE-pw-lp,otpauth://totp/Service:alice?secret=JBSWY3DPEHPK3PXP,Important notes,Service Login,Work,1\n\
         http://sn,,,,Server secret combination,Server Secrets,Infrastructure,0\n",
    )
    .unwrap();

    // 1. Dry run
    let o = gp(
        &root,
        &[
            "import",
            "Personal",
            "--from",
            "lastpass-csv",
            csv_file.to_str().unwrap(),
            "--dry-run",
        ],
        None,
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("[dry run] parsed 2, would add 2, 0 duplicate(s) skipped"));

    // 2. Real import with alias 'lastpass'
    let o = gp(
        &root,
        &[
            "import",
            "Personal",
            "--from",
            "lastpass",
            csv_file.to_str().unwrap(),
        ],
        None,
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("parsed 2, added 2, 0 duplicate(s) skipped"));

    // 3. Verify imported login
    let o = gp(&root, &["get", "Personal", "Service Login", "--reveal"], None);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let get_out = String::from_utf8_lossy(&o.stdout);
    assert!(get_out.contains("alice@example.invalid"));
    assert!(get_out.contains("FAKE-pw-lp"));
    assert!(get_out.contains("https://service.example.invalid"));
    assert!(get_out.contains("Important notes"));

    // 4. Verify secure note (url http://sn became note without url)
    let o = gp(&root, &["get", "Personal", "Server Secrets", "--reveal"], None);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let get_out = String::from_utf8_lossy(&o.stdout);
    assert!(get_out.contains("Server secret combination"));
    assert!(!get_out.contains("http://sn"));

    // 5. Re-import skips duplicates
    let o = gp(
        &root,
        &[
            "import",
            "Personal",
            "--from",
            "lastpass-csv",
            csv_file.to_str().unwrap(),
        ],
        None,
    );
    assert!(o.status.success());
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("parsed 2, added 0, 2 duplicate(s) skipped"));
}

#[test]
fn cli_import_protonpass_csv() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let csv_file = d.path().join("protonpass-export.csv");
    std::fs::write(
        &csv_file,
        "type,name,url,email,username,password,note,totp,createTime,modifyTime,vault\n\
         login,Direct User,https://direct.example.invalid,alice@example.invalid,alice_user,FAKE-pw-1,My note,,1700000000,1700001000,Work\n\
         login,Email Fallback,https://fallback.example.invalid,bob@example.invalid,,FAKE-pw-2,Empty username fallback,,1700002000,1700003000,Personal\n\
         note,Secret Key,,,,FAKE-secret,Server recovery note,,1700004000,1700005000,Home\n",
    )
    .unwrap();

    // 1. Dry run
    let o = gp(
        &root,
        &[
            "import",
            "Personal",
            "--from",
            "protonpass-csv",
            csv_file.to_str().unwrap(),
            "--dry-run",
        ],
        None,
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("[dry run] parsed 3, would add 3, 0 duplicate(s) skipped"));

    // 2. Real import with alias 'protonpass'
    let o = gp(
        &root,
        &[
            "import",
            "Personal",
            "--from",
            "protonpass",
            csv_file.to_str().unwrap(),
        ],
        None,
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("parsed 3, added 3, 0 duplicate(s) skipped"));

    // 3. Verify direct username
    let o = gp(&root, &["get", "Personal", "Direct User", "--reveal"], None);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let get_out = String::from_utf8_lossy(&o.stdout);
    assert!(get_out.contains("alice_user"));
    assert!(get_out.contains("FAKE-pw-1"));

    // 4. Verify email fallback when username is empty
    let o = gp(&root, &["get", "Personal", "Email Fallback", "--reveal"], None);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let get_out = String::from_utf8_lossy(&o.stdout);
    assert!(get_out.contains("bob@example.invalid"));
    assert!(get_out.contains("FAKE-pw-2"));

    // 5. Verify note
    let o = gp(&root, &["get", "Personal", "Secret Key", "--reveal"], None);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let get_out = String::from_utf8_lossy(&o.stdout);
    assert!(get_out.contains("Server recovery note"));

    // 6. Re-import skips duplicates
    let o = gp(
        &root,
        &[
            "import",
            "Personal",
            "--from",
            "protonpass-csv",
            csv_file.to_str().unwrap(),
        ],
        None,
    );
    assert!(o.status.success());
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("parsed 3, added 0, 3 duplicate(s) skipped"));
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

#[test]
fn cli_import_password_protected_bitwarden_json() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());

    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../gitpass-core/tests/fixtures/import/bitwarden-password-protected.json");
    let file = d.path().join("bitwarden-password-protected.json");
    std::fs::copy(&fixture, &file).unwrap();
    let file = file.to_str().unwrap();
    let export_pw = [("GITPASS_IMPORT_PASSWORD", "fake-export-password")];

    // Dry run with the export password from the environment.
    let o = gp_env(&root, &["import", "Personal", "--from", "bitwarden-json", file, "--dry-run"], None, &export_pw);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stdout).contains("[dry run] parsed 2, would add 2, 0 duplicate(s) skipped"));

    // Real import, then read a decrypted field back through `get`.
    let o = gp_env(&root, &["import", "Personal", "--from", "bitwarden-json", file], None, &export_pw);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stdout).contains("parsed 2, added 2, 0 duplicate(s) skipped"));
    let o = gp(&root, &["get", "Personal", "Example Mail", "--reveal"], None);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("alice@example.com") && out.contains("FAKE-BW-Pass-1"), "{out}");

    // A wrong export password fails with a clear error and imports nothing.
    let wrong = [("GITPASS_IMPORT_PASSWORD", "not-the-export-password")];
    let o = gp_env(&root, &["import", "Personal", "--from", "bitwarden-json", file, "--allow-duplicates"], None, &wrong);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("wrong export password"));
}
