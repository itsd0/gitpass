//! Docs drift check: every visible `gitpass` subcommand must appear in docs/CLI.md.
use std::path::Path;
use std::process::Command;

fn help(home: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_gitpass"))
        .args(args)
        .arg("--help")
        .env("HOME", home)
        .env("GITPASS_PATH", home.join("vault"))
        .output()
        .expect("run gitpass --help");
    assert!(
        out.status.success(),
        "gitpass {args:?} --help failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("help output is UTF-8")
}

fn visible_subcommands(help: &str) -> Vec<String> {
    let Some(start) = help.lines().position(|l| l.trim() == "Commands:") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in help.lines().skip(start + 1) {
        if line.trim().is_empty() {
            continue;
        }
        // Command rows are indented by exactly two spaces; section headers are not.
        if !line.starts_with("  ") || line.starts_with("   ") {
            break;
        }
        let name = line.split_whitespace().next().unwrap();
        if name != "help" {
            out.push(name.to_string());
        }
    }
    out
}

#[test]
fn cli_docs_cover_every_visible_subcommand() {
    let td = tempfile::tempdir().unwrap();
    let docs_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/CLI.md");
    let docs = std::fs::read_to_string(&docs_path).expect("docs/CLI.md must exist");

    let top = visible_subcommands(&help(td.path(), &[]));
    assert!(top.len() >= 25, "unexpectedly few subcommands: {top:?}");

    let mut missing = Vec::new();
    let mut nested = 0;
    for cmd in &top {
        if !docs.contains(&format!("gitpass {cmd}")) {
            missing.push(cmd.clone());
        }
        for sub in visible_subcommands(&help(td.path(), &[cmd])) {
            nested += 1;
            let path = format!("{cmd} {sub}");
            if !docs.contains(&format!("gitpass {path}")) {
                missing.push(path);
            }
        }
    }
    assert!(nested >= 20, "only {nested} nested subcommands found; help parsing looks broken");

    // `completions` is hidden from `--help` but still a supported command.
    if !docs.contains("gitpass completions") {
        missing.push("completions".into());
    }

    assert!(missing.is_empty(), "docs/CLI.md is missing: {missing:?}");
}
