use std::process::Command;

#[test]
fn shell_completions_for_all_supported_shells() {
    let td = tempfile::tempdir().unwrap();
    let nonexistent_vault = td.path().join("nonexistent_vault_dir");
    assert!(!nonexistent_vault.exists());

    let shells = ["zsh", "bash", "fish"];
    for shell in shells {
        let output = Command::new(env!("CARGO_BIN_EXE_gitpass"))
            .args(["completions", shell])
            .env("GITPASS_PATH", &nonexistent_vault)
            .env("HOME", td.path())
            .output()
            .expect("failed to execute gitpass completions");

        assert!(
            output.status.success(),
            "gitpass completions {shell} failed with exit code: {:?}",
            output.status.code()
        );

        let stdout = String::from_utf8(output.stdout).expect("valid utf-8 output");
        assert!(!stdout.is_empty(), "completions script for {shell} was empty");

        // Must mention key subcommands and their long flags
        for subcmd in ["gen", "get", "copy", "run", "totp", "archive", "unarchive", "undelete", "mv"] {
            assert!(
                stdout.contains(subcmd),
                "completions for {shell} missing subcommand '{subcmd}'"
            );
        }
        for flag in ["copy", "clear-secs", "category", "tag", "untag", "favorite", "unfavorite", "to"] {
            let has_flag = match shell {
                "fish" => stdout.contains(&format!("-l {flag}")),
                _ => stdout.contains(&format!("--{flag}")),
            };
            assert!(
                has_flag,
                "completions for {shell} missing flag '{flag}'"
            );
        }

        // Vault directory must not be created or touched
        assert!(
            !nonexistent_vault.exists(),
            "vault directory was created while generating completions for {shell}"
        );
    }

    // Alias `completion` should also work identically
    let alias_output = Command::new(env!("CARGO_BIN_EXE_gitpass"))
        .args(["completion", "zsh"])
        .env("GITPASS_PATH", &nonexistent_vault)
        .env("HOME", td.path())
        .output()
        .expect("failed to execute gitpass completion alias");

    assert!(alias_output.status.success());
    assert!(!alias_output.stdout.is_empty());
    assert!(!nonexistent_vault.exists());
}

#[test]
fn unknown_shell_exits_nonzero_with_clear_message() {
    let td = tempfile::tempdir().unwrap();
    let nonexistent_vault = td.path().join("nonexistent_vault_dir");

    let output = Command::new(env!("CARGO_BIN_EXE_gitpass"))
        .args(["completions", "unknown"])
        .env("GITPASS_PATH", &nonexistent_vault)
        .env("HOME", td.path())
        .output()
        .expect("failed to execute gitpass completions");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid value 'unknown'") || stderr.contains("possible values"),
        "unexpected error message: {stderr}"
    );
    assert!(!nonexistent_vault.exists());

    // Missing shell argument also exits non-zero
    let missing_arg = Command::new(env!("CARGO_BIN_EXE_gitpass"))
        .args(["completions"])
        .env("GITPASS_PATH", &nonexistent_vault)
        .env("HOME", td.path())
        .output()
        .expect("failed to execute gitpass completions");

    assert!(!missing_arg.status.success());
    assert!(!nonexistent_vault.exists());
}
