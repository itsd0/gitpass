//! `gitpass doctor`: read-only health checks for vault integrity, permissions, git state,
//! browser integration, and local throttle/backup state.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::{Cli, Cmd};
use crate::cmd::common::unlock_readonly;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub title: String,
    pub status: CheckStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_checked: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_failed: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub status: CheckStatus,
    pub vault_path: String,
    pub checks: Vec<DoctorCheck>,
}

fn check_vault_files(root: &Path, no_unlock: bool) -> DoctorCheck {
    if !root.join("format.json").exists() {
        return DoctorCheck {
            name: "vault_files".into(),
            title: "Vault files".into(),
            status: CheckStatus::Fail,
            message: "vault is not initialized (format.json not found)".into(),
            items_checked: Some(0),
            items_failed: Some(vec![]),
            fix: Some("run `gitpass init` to initialize the vault".into()),
        };
    }

    if no_unlock {
        return match gitpass_core::store::read_format(root) {
            Ok(fmt) => DoctorCheck {
                name: "vault_files".into(),
                title: "Vault files".into(),
                status: CheckStatus::Warn,
                message: format!(
                    "skipped item decryption (--no-unlock specified); format v{} verified",
                    fmt.version
                ),
                items_checked: Some(0),
                items_failed: Some(vec![]),
                fix: Some("run `gitpass doctor` without --no-unlock to verify item decryption".into()),
            },
            Err(e) => DoctorCheck {
                name: "vault_files".into(),
                title: "Vault files".into(),
                status: CheckStatus::Fail,
                message: format!("format.json is corrupted: {e}"),
                items_checked: Some(0),
                items_failed: Some(vec![]),
                fix: Some("restore format.json from git history or backup".into()),
            },
        };
    }

    let s = match unlock_readonly(root) {
        Ok(s) => s,
        Err(e) => {
            return DoctorCheck {
                name: "vault_files".into(),
                title: "Vault files".into(),
                status: CheckStatus::Fail,
                message: format!("vault unlock failed: {e} (pass --no-unlock to skip item verification)"),
                items_checked: Some(0),
                items_failed: Some(vec![]),
                fix: Some("provide master password/secret key or pass --no-unlock".into()),
            };
        }
    };

    let manifest = match s.load_manifest() {
        Ok(m) => m,
        Err(e) => {
            return DoctorCheck {
                name: "vault_files".into(),
                title: "Vault files".into(),
                status: CheckStatus::Fail,
                message: format!("manifest failed to decrypt: {e}"),
                items_checked: Some(0),
                items_failed: Some(vec![]),
                fix: Some("restore manifest.enc from git history or backup".into()),
            };
        }
    };

    let problems = gitpass_core::store::verify(root, &manifest);
    let mut items_checked = 0;
    let mut items_failed = Vec::new();

    for (vid, ve) in &manifest.vaults {
        for iid in ve.items.keys() {
            items_checked += 1;
            if s.read_record(vid, iid).is_err() && !items_failed.contains(iid) {
                items_failed.push(iid.clone());
            }
        }
    }

    if !items_failed.is_empty() {
        DoctorCheck {
            name: "vault_files".into(),
            title: "Vault files".into(),
            status: CheckStatus::Fail,
            message: format!("{} of {items_checked} item(s) failed decryption", items_failed.len()),
            items_checked: Some(items_checked),
            items_failed: Some(items_failed),
            fix: Some("restore corrupted item(s) from git history or backup".into()),
        }
    } else if !problems.is_empty() {
        DoctorCheck {
            name: "vault_files".into(),
            title: "Vault files".into(),
            status: CheckStatus::Fail,
            message: format!("integrity verification failed: {}", problems.join("; ")),
            items_checked: Some(items_checked),
            items_failed: Some(vec![]),
            fix: Some("restore affected file(s) from git history or backup".into()),
        }
    } else {
        DoctorCheck {
            name: "vault_files".into(),
            title: "Vault files".into(),
            status: CheckStatus::Ok,
            message: format!(
                "all {items_checked} item(s) and manifest decrypted and verified (AAD/padding valid)"
            ),
            items_checked: Some(items_checked),
            items_failed: Some(vec![]),
            fix: None,
        }
    }
}

fn check_permissions(root: &Path) -> DoctorCheck {
    let mut failures = Vec::new();
    let mut warnings = Vec::new();
    let mut fixes = Vec::new();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(root) {
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                failures.push(format!(
                    "vault directory {} has permissions {:04o} (expected 0700; group/world readable)",
                    root.display(),
                    mode
                ));
                fixes.push(format!("chmod 700 {}", root.display()));
            }
        }
        for kf in ["secret.key", ".gitpass-device-key.json"] {
            let p = root.join(kf);
            if p.exists() {
                if let Ok(meta) = fs::metadata(&p) {
                    let mode = meta.permissions().mode() & 0o777;
                    if mode & 0o077 != 0 {
                        failures.push(format!(
                            "key file {} has permissions {:04o} (expected 0600; group/world readable)",
                            p.display(),
                            mode
                        ));
                        fixes.push(format!("chmod 600 {}", p.display()));
                    }
                }
            }
        }
    }

    let canon = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let canon_str = canon.to_string_lossy();
    let cloud_sync_indicators = [
        "Dropbox",
        "Google Drive",
        "GoogleDrive",
        "OneDrive",
        "Mobile Documents/com~apple~CloudDocs",
        "iCloud Drive",
        "Box Sync",
        "pCloud",
        "Nextcloud",
        "ownCloud",
    ];
    if cloud_sync_indicators.iter().any(|ind| canon_str.contains(ind)) {
        warnings.push(format!(
            "vault directory is inside cloud-synced folder ({canon_str}); third-party sync can corrupt git history"
        ));
        fixes.push("move vault to a dedicated local directory like ~/.gitpass and use git remote for syncing".into());
    }

    if !failures.is_empty() {
        DoctorCheck {
            name: "permissions".into(),
            title: "Permissions".into(),
            status: CheckStatus::Fail,
            message: failures.join("; "),
            items_checked: None,
            items_failed: None,
            fix: Some(fixes.join("; ")),
        }
    } else if !warnings.is_empty() {
        DoctorCheck {
            name: "permissions".into(),
            title: "Permissions".into(),
            status: CheckStatus::Warn,
            message: warnings.join("; "),
            items_checked: None,
            items_failed: None,
            fix: Some(fixes.join("; ")),
        }
    } else {
        DoctorCheck {
            name: "permissions".into(),
            title: "Permissions".into(),
            status: CheckStatus::Ok,
            message: "vault directory and key files have safe permissions (0700/0600)".into(),
            items_checked: None,
            items_failed: None,
            fix: None,
        }
    }
}

fn check_git(root: &Path, online: bool) -> DoctorCheck {
    if !root.join(".git").exists() {
        return DoctorCheck {
            name: "git".into(),
            title: "Git repository".into(),
            status: CheckStatus::Warn,
            message: "vault directory is not a git repository".into(),
            items_checked: None,
            items_failed: None,
            fix: Some("run `git init` in the vault directory or `gitpass remote add <url>`".into()),
        };
    }
    if !gitpass_core::remote::git_available() {
        return DoctorCheck {
            name: "git".into(),
            title: "Git repository".into(),
            status: CheckStatus::Warn,
            message: "git is not installed or not in PATH".into(),
            items_checked: None,
            items_failed: None,
            fix: Some("install git (https://git-scm.com)".into()),
        };
    }

    let (has_remote, _) = {
        let o = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["remote", "get-url", "origin"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .output();
        match o {
            Ok(out) if out.status.success() => (true, String::from_utf8_lossy(&out.stdout).trim().to_string()),
            _ => (false, String::new()),
        }
    };

    if online && has_remote {
        let _ = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["fetch", "--quiet", "origin"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .output();
    }

    let status_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain=v1"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output();
    let status_str = status_out
        .as_ref()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let lines: Vec<&str> = status_str.lines().filter(|l| !l.trim().is_empty()).collect();

    let has_conflict = lines.iter().any(|l| {
        let p = l.get(..2).unwrap_or("");
        matches!(p, "UU" | "AA" | "DD" | "UD" | "DU" | "AU" | "UA")
    }) || root.join(".git/MERGE_HEAD").exists();

    if has_conflict {
        return DoctorCheck {
            name: "git".into(),
            title: "Git repository".into(),
            status: CheckStatus::Fail,
            message: "vault repository has unresolved merge conflicts".into(),
            items_checked: None,
            items_failed: None,
            fix: Some("resolve git conflicts or run `gitpass sync --merge`".into()),
        };
    }

    let branch = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "HEAD".into());

    let (ahead, behind) = {
        let o = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-list", "--left-right", "--count", "HEAD...@{u}"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .output();
        if let Ok(out) = o {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                let mut it = s.split_whitespace().filter_map(|x| x.parse::<u64>().ok());
                (it.next().unwrap_or(0), it.next().unwrap_or(0))
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        }
    };

    if !lines.is_empty() {
        return DoctorCheck {
            name: "git".into(),
            title: "Git repository".into(),
            status: CheckStatus::Warn,
            message: format!("{} uncommitted change(s) in vault repository (branch {})", lines.len(), branch),
            items_checked: None,
            items_failed: None,
            fix: Some("commit or stash local changes in the vault repository".into()),
        };
    }

    if !has_remote {
        return DoctorCheck {
            name: "git".into(),
            title: "Git repository".into(),
            status: CheckStatus::Warn,
            message: format!("clean repository on branch {branch}, but no git remote configured"),
            items_checked: None,
            items_failed: None,
            fix: Some("run `gitpass remote add <url>` to configure remote sync".into()),
        };
    }

    DoctorCheck {
        name: "git".into(),
        title: "Git repository".into(),
        status: CheckStatus::Ok,
        message: format!("clean, branch {branch}, remote origin configured ({ahead} ahead, {behind} behind)"),
        items_checked: None,
        items_failed: None,
        fix: None,
    }
}

fn check_browser_integration() -> DoctorCheck {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));

    struct Target {
        name: &'static str,
        cli_arg: &'static str,
        installed: bool,
        manifest_path: PathBuf,
    }

    #[cfg(target_os = "macos")]
    let targets = {
        let chrome_data = home.join("Library/Application Support/Google/Chrome");
        let chrome_manifest = chrome_data.join("NativeMessagingHosts/com.gitpass.host.json");
        let chrome_installed = chrome_manifest.exists()
            || (chrome_data.exists()
                && (Path::new("/Applications/Google Chrome.app").exists()
                    || home.join("Applications/Google Chrome.app").exists()));

        let chromium_data = home.join("Library/Application Support/Chromium");
        let chromium_manifest = chromium_data.join("NativeMessagingHosts/com.gitpass.host.json");
        let chromium_installed = chromium_manifest.exists()
            || (chromium_data.exists()
                && (Path::new("/Applications/Chromium.app").exists()
                    || home.join("Applications/Chromium.app").exists()));

        let brave_data = home.join("Library/Application Support/BraveSoftware/Brave-Browser");
        let brave_manifest = brave_data.join("NativeMessagingHosts/com.gitpass.host.json");
        let brave_installed = brave_manifest.exists()
            || (brave_data.exists()
                && (Path::new("/Applications/Brave Browser.app").exists()
                    || home.join("Applications/Brave Browser.app").exists()));

        vec![
            Target {
                name: "Google Chrome",
                cli_arg: "chrome",
                installed: chrome_installed,
                manifest_path: chrome_manifest,
            },
            Target {
                name: "Chromium",
                cli_arg: "chromium",
                installed: chromium_installed,
                manifest_path: chromium_manifest,
            },
            Target {
                name: "Brave",
                cli_arg: "brave",
                installed: brave_installed,
                manifest_path: brave_manifest,
            },
        ]
    };

    #[cfg(all(not(target_os = "macos"), not(windows)))]
    let targets = {
        let chrome_data = home.join(".config/google-chrome");
        let chrome_manifest = chrome_data.join("NativeMessagingHosts/com.gitpass.host.json");
        let chrome_installed = chrome_manifest.exists()
            || (chrome_data.exists()
                && (Command::new("which").arg("google-chrome").output().map(|o| o.status.success()).unwrap_or(false)
                    || Command::new("which").arg("google-chrome-stable").output().map(|o| o.status.success()).unwrap_or(false)));

        let chromium_data = home.join(".config/chromium");
        let chromium_manifest = chromium_data.join("NativeMessagingHosts/com.gitpass.host.json");
        let chromium_installed = chromium_manifest.exists()
            || (chromium_data.exists()
                && (Command::new("which").arg("chromium").output().map(|o| o.status.success()).unwrap_or(false)
                    || Command::new("which").arg("chromium-browser").output().map(|o| o.status.success()).unwrap_or(false)));

        let brave_data = home.join(".config/BraveSoftware/Brave-Browser");
        let brave_manifest = brave_data.join("NativeMessagingHosts/com.gitpass.host.json");
        let brave_installed = brave_manifest.exists()
            || (brave_data.exists()
                && (Command::new("which").arg("brave-browser").output().map(|o| o.status.success()).unwrap_or(false)
                    || Command::new("which").arg("brave").output().map(|o| o.status.success()).unwrap_or(false)));

        vec![
            Target {
                name: "Google Chrome",
                cli_arg: "chrome",
                installed: chrome_installed,
                manifest_path: chrome_manifest,
            },
            Target {
                name: "Chromium",
                cli_arg: "chromium",
                installed: chromium_installed,
                manifest_path: chromium_manifest,
            },
            Target {
                name: "Brave",
                cli_arg: "brave",
                installed: brave_installed,
                manifest_path: brave_manifest,
            },
        ]
    };

    #[cfg(windows)]
    let targets: Vec<Target> = Vec::new();

    let installed_targets: Vec<&Target> = targets.iter().filter(|t| t.installed).collect();
    if installed_targets.is_empty() {
        return DoctorCheck {
            name: "browser".into(),
            title: "Browser integration".into(),
            status: CheckStatus::Ok,
            message: "no Chromium browsers detected on this system".into(),
            items_checked: None,
            items_failed: None,
            fix: None,
        };
    }

    let mut broken = Vec::new();
    let mut missing = Vec::new();
    let mut valid = Vec::new();
    let mut fixes = Vec::new();

    for t in installed_targets {
        if !t.manifest_path.exists() {
            missing.push(t.name);
            fixes.push(format!("run `gitpass-host install {} <extension-id>` to register native host", t.cli_arg));
        } else {
            match fs::read_to_string(&t.manifest_path) {
                Ok(content) => {
                    match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(v) => {
                            if let Some(bin_path) = v.get("path").and_then(|p| p.as_str()) {
                                let p = Path::new(bin_path);
                                if p.exists() {
                                    valid.push(format!("{}: manifest points to {}", t.name, p.display()));
                                } else {
                                    broken.push(format!("{}: manifest points to missing binary '{}'", t.name, p.display()));
                                    fixes.push(format!("re-install with `gitpass-host install {} <extension-id>` or build gitpass-host", t.cli_arg));
                                }
                            } else {
                                broken.push(format!("{}: manifest has missing or invalid 'path' field", t.name));
                                fixes.push(format!("re-install with `gitpass-host install {} <extension-id>`", t.cli_arg));
                            }
                        }
                        Err(_) => {
                            broken.push(format!("{}: manifest is invalid JSON: {}", t.name, t.manifest_path.display()));
                            fixes.push(format!("re-install with `gitpass-host install {} <extension-id>`", t.cli_arg));
                        }
                    }
                }
                Err(e) => {
                    broken.push(format!("{}: unable to read manifest {}: {e}", t.name, t.manifest_path.display()));
                }
            }
        }
    }

    if !broken.is_empty() {
        DoctorCheck {
            name: "browser".into(),
            title: "Browser integration".into(),
            status: CheckStatus::Fail,
            message: broken.join("; "),
            items_checked: None,
            items_failed: None,
            fix: Some(fixes.join("; ")),
        }
    } else if !missing.is_empty() {
        DoctorCheck {
            name: "browser".into(),
            title: "Browser integration".into(),
            status: CheckStatus::Warn,
            message: format!("native host manifest missing for: {}", missing.join(", ")),
            items_checked: None,
            items_failed: None,
            fix: Some(fixes.join("; ")),
        }
    } else {
        DoctorCheck {
            name: "browser".into(),
            title: "Browser integration".into(),
            status: CheckStatus::Ok,
            message: valid.join("; "),
            items_checked: None,
            items_failed: None,
            fix: None,
        }
    }
}

fn check_local_state(root: &Path) -> DoctorCheck {
    let mut fails = Vec::new();
    let mut warns = Vec::new();
    let mut fixes = Vec::new();

    let tp = root.join(".gitpass-throttle.json");
    if tp.exists() {
        match fs::read(&tp) {
            Ok(bytes) => {
                match serde_json::from_slice::<gitpass_core::unlock_throttle::ThrottleRecord>(&bytes) {
                    Ok(rec) => {
                        if let Some(remaining) = gitpass_core::unlock_throttle::seconds_remaining(root) {
                            if remaining > 0 {
                                warns.push(format!(
                                    "vault unlock currently throttled: {remaining} s remaining ({} consecutive failures)",
                                    rec.failures
                                ));
                                fixes.push("wait for throttle countdown to expire, or unlock with Touch ID".into());
                            }
                        }
                    }
                    Err(_) => {
                        fails.push(format!("corrupted throttle file: {}", tp.display()));
                        fixes.push(format!("remove corrupted throttle file: rm {}", tp.display()));
                    }
                }
            }
            Err(e) => {
                fails.push(format!("unable to read throttle file {}: {e}", tp.display()));
            }
        }
    }

    let mut routine_backups = Vec::new();
    let mut migration_backups = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(rest) = name.strip_prefix(".gitpass-backup-") {
                    if !path.is_dir() {
                        warns.push(format!("backup {} is not a directory", path.display()));
                    } else if rest.starts_with(|c: char| c.is_ascii_digit()) {
                        routine_backups.push(path);
                    } else {
                        migration_backups.push(path);
                    }
                }
            }
        }
    }

    if routine_backups.len() > 10 {
        warns.push(format!("{} routine backups found; backups may not be pruning properly", routine_backups.len()));
        fixes.push("routine backups prune automatically during vault writes or can be pruned manually".into());
    }

    let total_backups = routine_backups.len() + migration_backups.len();
    if !fails.is_empty() {
        DoctorCheck {
            name: "local_state".into(),
            title: "Local state".into(),
            status: CheckStatus::Fail,
            message: fails.join("; "),
            items_checked: None,
            items_failed: None,
            fix: Some(fixes.join("; ")),
        }
    } else if !warns.is_empty() {
        DoctorCheck {
            name: "local_state".into(),
            title: "Local state".into(),
            status: CheckStatus::Warn,
            message: warns.join("; "),
            items_checked: None,
            items_failed: None,
            fix: Some(fixes.join("; ")),
        }
    } else {
        DoctorCheck {
            name: "local_state".into(),
            title: "Local state".into(),
            status: CheckStatus::Ok,
            message: format!("unlock throttle clean, {} backup(s) found", total_backups),
            items_checked: None,
            items_failed: None,
            fix: None,
        }
    }
}

pub(crate) fn run(_cli: &Cli, root: &Path, cmd: &Cmd) -> Result<()> {
    let (json, no_unlock, online) = match cmd {
        Cmd::Doctor { json, no_unlock, online } => (*json, *no_unlock, *online),
        _ => unreachable!(),
    };

    let checks = vec![
        check_vault_files(root, no_unlock),
        check_permissions(root),
        check_git(root, online),
        check_browser_integration(),
        check_local_state(root),
    ];

    let has_fail = checks.iter().any(|c| c.status == CheckStatus::Fail);
    let has_warn = checks.iter().any(|c| c.status == CheckStatus::Warn);
    let overall_status = if has_fail {
        CheckStatus::Fail
    } else if has_warn {
        CheckStatus::Warn
    } else {
        CheckStatus::Ok
    };

    let report = DoctorReport {
        status: overall_status,
        vault_path: root.display().to_string(),
        checks,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("gitpass doctor (vault: {})
", root.display());
        for check in &report.checks {
            let symbol = match check.status {
                CheckStatus::Ok => "[✓]",
                CheckStatus::Warn => "[!]",
                CheckStatus::Fail => "[✗]",
            };
            println!("{symbol} {}: {}", check.title, check.message);
            if let Some(items) = &check.items_failed {
                for iid in items {
                    println!("    failing item: {iid}");
                }
            }
            if let Some(fix) = &check.fix {
                println!("    fix: {fix}");
            }
        }
        println!();
        let ok_count = report.checks.iter().filter(|c| c.status == CheckStatus::Ok).count();
        let warn_count = report.checks.iter().filter(|c| c.status == CheckStatus::Warn).count();
        let fail_count = report.checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
        if fail_count == 0 && warn_count == 0 {
            println!("All {} checks passed.", report.checks.len());
        } else if fail_count > 0 && warn_count == 0 {
            println!("{fail_count} check(s) failed, {ok_count} passed.");
        } else if fail_count > 0 && warn_count > 0 {
            println!("{fail_count} check(s) failed, {warn_count} warning(s), {ok_count} passed.");
        } else {
            println!("{warn_count} warning(s), {ok_count} passed.");
        }
    }

    match report.status {
        CheckStatus::Ok => Ok(()),
        CheckStatus::Warn => std::process::exit(2),
        CheckStatus::Fail => std::process::exit(1),
    }
}
