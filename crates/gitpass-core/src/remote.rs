//! `gitpass remote add|show|remove`: connect the vault folder to a (private) git remote.
//!
//! Safety rules:
//! - only encrypted / public vault files may ever be committed: every staged path is checked against an
//!   allowlist before a commit is made, and local-only secrets (secret.key, device key/state/lock files,
//!   GUI settings, backups) are always in `.gitignore`;
//! - a remote that already holds a different gitpass account (or non-gitpass content) is refused;
//! - never force-push. Diverged histories are left for `sync --merge`.
//!
//! Auth relies entirely on the user's existing git credentials (SSH agent/keys or a credential helper such as
//! the macOS keychain or `gh auth setup-git`). Tokens embedded in URLs are rejected so they are never stored.
use crate::fsutil;
use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

/// Lines that must be present in the vault `.gitignore` (local-only files; secrets or per-device state).
pub const REQUIRED_IGNORES: &[&str] = &[
    "secret.key", "*.tmp", ".gitpass.lock", ".gitpass-state.json", ".gitpass-throttle.json", ".gitpass-device.json", ".gitpass-device-key.json",
    ".gitpass-revoke-pending.json", ".gitpass-backup-*/", ".gitpass-migrate-tmp/", ".gitpass-migrating",
    ".gitpass-settings.json", "*.new", ".DS_Store",
];

/// Error-category markers (prefixes of the error message) that the GUI maps to localized guidance.
pub const E_GIT_MISSING: &str = "GIT_NOT_INSTALLED";
pub const E_AUTH: &str = "AUTH_FAILED";
pub const E_NOT_EMPTY: &str = "REMOTE_NOT_EMPTY";
pub const E_WRONG_ACCOUNT: &str = "WRONG_ACCOUNT";
pub const E_BAD_URL: &str = "INVALID_URL";
pub const E_SECRET_STAGED: &str = "SECRET_STAGED";
pub const E_ALREADY: &str = "ALREADY_CONNECTED";
pub const E_DIVERGED: &str = "DIVERGED";

pub(crate) fn git_cmd(dir: &Path, args: &[&str]) -> Result<(bool, String)> {
    let o = Command::new("git").arg("-C").arg(dir).args(args)
        .env("GIT_TERMINAL_PROMPT", "0") // never block on an interactive username/password prompt
        .output()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { anyhow::anyhow!("{E_GIT_MISSING}: git is not installed (install Xcode Command Line Tools: xcode-select --install, or https://git-scm.com)") } else { e.into() })?;
    Ok((o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))))
}
fn git_ok(dir: &Path, args: &[&str]) -> Result<String> {
    let (ok, out) = git_cmd(dir, args)?;
    if !ok { bail!("git {} failed:\n{}", args.join(" "), out.trim()); }
    Ok(out)
}

/// Is `git` callable?
pub fn git_available() -> bool { Command::new("git").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) }

/// Validate a remote URL: `https://host/owner/repo(.git)`, `ssh://[user@]host[:port]/path`, or scp-style
/// `user@host:owner/repo(.git)`. Any git host is accepted. Credentials embedded in https URLs are refused.
/// `allow_local` additionally accepts absolute paths and `file://` (used by tests and local mirrors).
pub fn validate_url(url: &str, allow_local: bool) -> Result<()> {
    let u = url.trim();
    let bad = |why: &str| -> Result<()> { bail!("{E_BAD_URL}: {why}: {u:?}. Use https://github.com/YOU/REPO.git or git@github.com:YOU/REPO.git") };
    if u.is_empty() { return bad("empty URL"); }
    if u.starts_with('-') || u.chars().any(|c| c.is_whitespace() || c.is_control()) { return bad("invalid characters"); }
    // Local paths: `/…` on Unix, `C:\…` on Windows (Path::is_absolute is platform-specific).
    if allow_local && (u.starts_with('/') || u.starts_with("file://") || std::path::Path::new(u).is_absolute()) { return Ok(()); }
    let path_ok = |p: &str| { let p = p.trim_matches('/'); !p.is_empty() && !p.contains("..") && p.split('/').all(|s| !s.is_empty()) };
    let host_ok = |h: &str| !h.is_empty() && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':' || c == '[' || c == ']');
    if let Some(rest) = u.strip_prefix("https://") {
        let (auth_host, path) = rest.split_once('/').unwrap_or((rest, ""));
        if auth_host.contains('@') { return bad("do not put a username or token in the URL; gitpass uses your git credentials"); }
        if !host_ok(auth_host) || !path_ok(path) { return bad("not a repository URL"); }
        return Ok(());
    }
    if let Some(rest) = u.strip_prefix("ssh://") {
        let (auth_host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = auth_host.rsplit_once('@').map(|x| x.1).unwrap_or(auth_host);
        if auth_host.matches('@').count() > 1 || auth_host.contains(':') && auth_host.split_once('@').map(|x| x.0.contains(':')).unwrap_or(false) { return bad("invalid user"); }
        if !host_ok(host) || !path_ok(path) { return bad("not a repository URL"); }
        return Ok(());
    }
    if u.contains("://") { return bad("unsupported scheme (use https:// or SSH)"); }
    // scp-like: user@host:path
    if let Some((uh, path)) = u.split_once(':') {
        if let Some((user, host)) = uh.split_once('@') {
            if !user.is_empty() && user.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') && host_ok(host) && !host.contains(':') && path_ok(path) {
                return Ok(());
            }
        }
    }
    bad("not a recognised git URL")
}

/// Make sure every [`REQUIRED_IGNORES`] line is in `<root>/.gitignore`.
pub fn ensure_gitignore(root: &Path) -> Result<()> {
    for n in REQUIRED_IGNORES { crate::device::ensure_ignored(root, n)?; }
    ensure_gitattributes(root)
}

/// `.gitattributes` content: every vault file is byte-exact (hashes in manifest.enc), so git must never
/// convert line endings (Windows `core.autocrlf=true` would otherwise corrupt `.enc` files on checkout).
pub const GITATTRIBUTES: &str = "* -text\n";

/// Make sure `<root>/.gitattributes` disables line-ending conversion for all files.
pub fn ensure_gitattributes(root: &Path) -> Result<()> {
    let p = root.join(".gitattributes");
    let cur = std::fs::read_to_string(&p).unwrap_or_default();
    if cur.lines().any(|l| l.trim() == "* -text") { return Ok(()); }
    let mut s = cur;
    if !s.is_empty() && !s.ends_with('\n') { s.push('\n'); }
    s.push_str(GITATTRIBUTES);
    std::fs::write(&p, s)?;
    Ok(())
}

/// Is `path` (repo-relative, `/`-separated) allowed to be committed?
pub fn is_allowed_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if path.split('/').any(|c| c.starts_with(".gitpass-") || c == ".gitpass.lock" || c == "secret.key") { return false; }
    match path {
        ".gitignore" | ".gitattributes" | "format.json" | "identity.pub" => true,
        _ if name == ".keep" => path.starts_with("vaults/"), // empty placeholder dirs (content checked in check_staged)
        _ => name.ends_with(".enc") && !name.starts_with('.') && !path.contains(".."),
    }
}

/// Paths currently staged (added/modified/renamed) in the index.
pub fn staged_paths(root: &Path) -> Result<Vec<String>> {
    let out = git_ok(root, &["diff", "--cached", "--name-only", "--diff-filter=ACMR", "-z"])?;
    Ok(out.split('\0').filter(|s| !s.is_empty()).map(String::from).collect())
}

/// Abort if any staged file is not an expected encrypted/public vault file. On failure the index is reset
/// (nothing is unstaged from the working tree; files stay on disk).
pub fn check_staged(root: &Path) -> Result<()> {
    let mut bad = vec![];
    for p in staged_paths(root)? {
        let ok = is_allowed_path(&p) && (!p.ends_with("/.keep") || git_ok(root, &["cat-file", "-s", &format!(":{p}")])?.trim() == "0");
        if !ok { bad.push(p); }
    }
    if !bad.is_empty() {
        let _ = git_cmd(root, &["reset", "-q"]);
        bail!("{E_SECRET_STAGED}: refusing to commit non-vault/plaintext file(s): {}. Add them to .gitignore or remove them.", bad.join(", "));
    }
    Ok(())
}

/// Turn a failed git network operation into a categorized, actionable error.
pub fn classify_git_error(op: &str, out: &str) -> anyhow::Error {
    let l = out.to_lowercase();
    let auth = ["authentication failed", "permission denied", "could not read username", "could not read from remote repository",
        "repository not found", "terminal prompts disabled", "host key verification failed", "invalid username or password", "403"];
    if auth.iter().any(|m| l.contains(m)) {
        return anyhow::anyhow!("{E_AUTH}: {op} failed: git could not authenticate or the repository does not exist / you lack access.\n\
            Fixes: use the SSH URL (git@github.com:YOU/REPO.git) with an SSH key added to GitHub, or run `gh auth login` (and `gh auth setup-git`) in Terminal, then try again.\n\
            git said: {}", out.trim());
    }
    if l.contains("non-fast-forward") || l.contains("fetch first") || l.contains("rejected") {
        return anyhow::anyhow!("{E_NOT_EMPTY}: {op} was rejected because the remote has commits this vault does not have; gitpass never force-pushes. Use an empty repository.\ngit said: {}", out.trim());
    }
    anyhow::anyhow!("{op} failed:\n{}", out.trim())
}

fn has_commits(root: &Path) -> Result<bool> { Ok(git_cmd(root, &["rev-parse", "--verify", "-q", "HEAD"])?.0) }

fn ensure_identity(root: &Path) -> Result<()> {
    let (ok, out) = git_cmd(root, &["config", "user.email"])?;
    if !ok || out.trim().is_empty() {
        git_ok(root, &["config", "user.email", "gitpass@localhost"])?;
        git_ok(root, &["config", "user.name", "gitpass"])?;
    }
    Ok(())
}

/// Stage everything (respecting .gitignore), verify the staged set, commit if non-empty.
pub fn commit_checked(root: &Path, msg: &str) -> Result<bool> {
    git_ok(root, &["add", "-A"])?;
    check_staged(root)?;
    let (clean, _) = git_cmd(root, &["diff", "--cached", "--quiet"])?;
    if clean { return Ok(false); }
    ensure_identity(root)?;
    git_ok(root, &["commit", "-q", "-m", msg])?;
    Ok(true)
}

/// Owner part of a hosted git remote URL: `https://github.com/itsd0/gitpass-vault.git` and
/// `git@github.com:itsd0/gitpass-vault.git` both give `itsd0`. Local paths / file URLs give None.
pub fn remote_owner(url: &str) -> Option<String> {
    let u = url.trim();
    let rest = if let Some((scheme, r)) = u.split_once("://") {
        if scheme.eq_ignore_ascii_case("file") { return None; }
        r.split_once('/').map(|(_, p)| p)?
    } else if let Some((h, p)) = u.split_once(':') {
        if h.contains('/') || !h.contains('@') && !h.contains('.') { return None; } // a path, or a Windows drive
        p
    } else { return None };
    let segs: Vec<&str> = rest.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 { return None; }
    let o = segs[segs.len() - 2];
    (!o.is_empty() && o.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))).then(|| o.to_string())
}

/// Friendly account name for UIs; never the account UUID. Order: optional `git config gitpass.displayName`
/// in the vault repo (local, plaintext, set with `git -C <vault> config gitpass.displayName "..."`), then the
/// owner of the `origin` remote, then the OS full user name, then "gitpass".
pub fn display_name(root: &Path) -> String {
    if root.join(".git").exists() {
        if let Ok((true, v)) = git_cmd(root, &["config", "--get", "gitpass.displayName"]) {
            let v = v.trim(); if !v.is_empty() { return v.chars().take(64).collect(); }
        }
        if let Ok((true, u)) = git_cmd(root, &["remote", "get-url", "origin"]) {
            if let Some(o) = remote_owner(&u) { return o; }
        }
    }
    os_full_name().unwrap_or_else(|| "gitpass".to_string())
}

fn os_full_name() -> Option<String> {
    #[cfg(target_os = "macos")]
    { let o = Command::new("id").arg("-F").output().ok()?; let n = String::from_utf8_lossy(&o.stdout).trim().to_string(); if o.status.success() && !n.is_empty() { return Some(n); } }
    None
}

/// Info about the configured remote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteInfo { pub url: Option<String>, pub branch: Option<String>, pub ahead: u64, pub behind: u64, pub is_repo: bool }

pub fn show(root: &Path) -> Result<RemoteInfo> {
    if !root.join(".git").exists() { return Ok(RemoteInfo::default()); }
    let mut i = RemoteInfo { is_repo: true, ..Default::default() };
    let (ok, out) = git_cmd(root, &["remote", "get-url", "origin"])?;
    if ok && !out.trim().is_empty() { i.url = Some(out.trim().to_string()); }
    let (ok, out) = git_cmd(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if ok { i.branch = Some(out.trim().to_string()); }
    let (ok, out) = git_cmd(root, &["rev-list", "--left-right", "--count", "HEAD...@{u}"])?;
    if ok {
        let mut it = out.split_whitespace().filter_map(|x| x.parse::<u64>().ok());
        i.ahead = it.next().unwrap_or(0); i.behind = it.next().unwrap_or(0);
    }
    Ok(i)
}

/// Remove `origin` (local git history and files are kept).
pub fn remove(root: &Path) -> Result<()> {
    if !root.join(".git").exists() { bail!("not connected: {} is not a git repository", root.display()); }
    let (ok, _) = git_cmd(root, &["remote", "get-url", "origin"])?;
    if !ok { bail!("not connected: no 'origin' remote"); }
    git_ok(root, &["remote", "remove", "origin"])?;
    Ok(())
}

fn remote_account_id(root: &Path, rev: &str) -> Option<String> {
    let (ok, out) = git_cmd(root, &["show", &format!("{rev}:format.json")]).ok()?;
    if !ok { return None; }
    serde_json::from_str::<serde_json::Value>(&out).ok()?.get("account_id")?.as_str().map(String::from)
}

/// Connect the vault at `root` to `url` and push it. Returns a human-readable log.
pub fn add(root: &Path, url: &str, allow_local: bool) -> Result<String> {
    let url = url.trim();
    validate_url(url, allow_local)?;
    if !git_available() { bail!("{E_GIT_MISSING}: git is not installed (install Xcode Command Line Tools: xcode-select --install, or https://git-scm.com)"); }
    let fmt = crate::store::read_format(root)?;
    let mut log = String::new();
    if !root.join(".git").exists() {
        git_ok(root, &["init", "-q", "-b", "main"])?;
        log.push_str("initialized git repository (branch main)\n");
    } else if git_cmd(root, &["remote", "get-url", "origin"])?.0 {
        bail!("{E_ALREADY}: this vault already has an 'origin' remote; disconnect it first (gitpass remote remove)");
    }
    ensure_gitignore(root)?;
    // Drop anything already tracked that must stay local (e.g. a repo created by hand before).
    let tracked = git_ok(root, &["ls-files", "-z"])?;
    for p in tracked.split('\0').filter(|s| !s.is_empty() && !is_allowed_path(s)) {
        git_ok(root, &["rm", "-q", "--cached", "--", p])?;
        log.push_str(&format!("untracked local-only file {p}\n"));
    }
    git_ok(root, &["remote", "add", "origin", url])?;
    let res = connect_inner(root, &fmt.account_id, &mut log);
    if res.is_err() { let _ = git_cmd(root, &["remote", "remove", "origin"]); }
    res.map(|_| log)
}

fn connect_inner(root: &Path, account_id: &str, log: &mut String) -> Result<()> {
    let _g = fsutil::lock(root)?;
    let (ok, out) = git_cmd(root, &["ls-remote", "--heads", "origin"])?;
    if !ok { return Err(classify_git_error("connecting to the remote", &out)); }
    let remote_empty = out.trim().is_empty();
    if remote_empty {
        log.push_str(if commit_checked(root, "gitpass: encrypted vault")? { "committed encrypted vault files\n" } else { "nothing new to commit\n" });
        if !has_commits(root)? { bail!("nothing to push: the vault has no files"); }
        let (ok, out) = git_cmd(root, &["push", "-u", "origin", "HEAD:main"])?;
        if !ok { return Err(classify_git_error("git push", &out)); }
        git_cmd(root, &["branch", "--set-upstream-to=origin/main"]).ok();
        log.push_str("pushed to origin/main\n");
        return Ok(());
    }
    // Remote has content: fetch it and make sure it is this account's vault.
    let (ok, out) = git_cmd(root, &["fetch", "-q", "origin"])?;
    if !ok { return Err(classify_git_error("git fetch", &out)); }
    let (has_main, _) = git_cmd(root, &["rev-parse", "--verify", "-q", "origin/main"])?;
    let rref = if has_main { "origin/main".to_string() } else {
        let (_, o) = git_cmd(root, &["ls-remote", "--symref", "origin", "HEAD"])?;
        let b = o.lines().find_map(|l| l.strip_prefix("ref: refs/heads/").and_then(|r| r.split_whitespace().next()).map(String::from));
        match b { Some(b) => format!("origin/{b}"), None => bail!("{E_NOT_EMPTY}: the remote is not empty and has no main branch; use an empty private repository") }
    };
    match remote_account_id(root, &rref) {
        None => bail!("{E_NOT_EMPTY}: the remote repository is not empty and does not contain a gitpass vault. Create a new EMPTY private repository (no README, .gitignore or license) and try again."),
        Some(a) if a != account_id => bail!("{E_WRONG_ACCOUNT}: the remote holds a different gitpass account ({a}); this vault is account {account_id}. Refusing to mix accounts; use a different repository."),
        Some(_) => {}
    }
    log.push_str("remote already holds this account's vault\n");
    let branch = rref.trim_start_matches("origin/").to_string();
    if !has_commits(root)? {
        // Local folder never committed: only adopt the remote history if the files are identical.
        git_ok(root, &["reset", "-q", "--mixed", &rref])?;
        let st = git_ok(root, &["status", "--porcelain"])?;
        if !st.trim().is_empty() {
            // undo: leave the working tree untouched, drop the adopted ref
            let _ = git_cmd(root, &["update-ref", "-d", "HEAD"]);
            let _ = git_cmd(root, &["rm", "-q", "-r", "--cached", "."]);
            bail!("{E_DIVERGED}: the remote already contains this account's vault with different contents. Nothing was changed. To use the remote copy on this device, clone it into an empty folder (git clone URL ~/.gitpass) instead; gitpass never overwrites or force-pushes.");
        }
        git_ok(root, &["branch", &format!("--set-upstream-to={rref}")])?;
        log.push_str("local files match the remote; connected\n");
        return Ok(());
    }
    commit_checked(root, "gitpass: local changes")?;
    let head = git_ok(root, &["rev-parse", "HEAD"])?.trim().to_string();
    let theirs = git_ok(root, &["rev-parse", &rref])?.trim().to_string();
    let (ok, base) = git_cmd(root, &["merge-base", "HEAD", &rref])?;
    let base = if ok { base.trim().to_string() } else { String::new() };
    git_cmd(root, &["branch", &format!("--set-upstream-to={rref}")]).ok();
    if base == theirs {
        let (ok, out) = git_cmd(root, &["push", "-u", "origin", &format!("HEAD:{branch}")])?;
        if !ok { return Err(classify_git_error("git push", &out)); }
        log.push_str("pushed local commits\n");
    } else if base == head {
        git_ok(root, &["merge", "-q", "--ff-only", &rref])?;
        log.push_str("fast-forwarded to the remote\n");
    } else if base.is_empty() {
        let _ = git_cmd(root, &["branch", "--unset-upstream"]);
        bail!("{E_DIVERGED}: local and remote histories are unrelated. Nothing was pushed. Clone the remote into an empty folder instead.");
    } else {
        log.push_str("local and remote have diverged; run Sync (merge) to combine them\n");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn remote_owner_forms() {
        assert_eq!(super::remote_owner("https://github.com/itsd0/gitpass-vault.git").as_deref(), Some("itsd0"));
        assert_eq!(super::remote_owner("git@github.com:itsd0/gitpass-vault.git").as_deref(), Some("itsd0"));
        assert_eq!(super::remote_owner("ssh://git@gitlab.com/team/sub/vault").as_deref(), Some("sub"));
        assert_eq!(super::remote_owner("/tmp/bare.git"), None);
        assert_eq!(super::remote_owner("file:///tmp/x/bare.git"), None);
        assert_eq!(super::remote_owner("https://github.com/vault.git"), None);
    }
    #[test]
    fn display_name_prefers_git_config_and_never_uuid() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        assert!(std::process::Command::new("git").arg("-C").arg(r).arg("init").arg("-q").status().unwrap().success());
        super::git_cmd(r, &["remote", "add", "origin", "https://github.com/itsd0/gitpass-vault.git"]).unwrap();
        assert_eq!(super::display_name(r), "itsd0");
        super::git_cmd(r, &["config", "gitpass.displayName", "Naing"]).unwrap();
        assert_eq!(super::display_name(r), "Naing");
    }

    use super::*;
    #[test]
    fn urls() {
        for u in ["https://github.com/me/vault.git", "https://github.com/me/vault", "git@github.com:me/vault.git", "git@gitlab.example.org:grp/sub/vault.git",
                  "ssh://git@github.com/me/vault.git", "ssh://git@host.example:2222/me/v.git", "https://codeberg.org/me/v"] {
            assert!(validate_url(u, false).is_ok(), "{u}");
        }
        for u in ["", "github.com/me/v", "http://github.com/me/v", "https://tok@github.com/me/v", "https://u:p@github.com/me/v.git", "-uupload-pack=x",
                  "https://github.com/", "git@github.com:", "ftp://x/y", "/tmp/repo", "file:///tmp/r", "https://github.com/me/v x", "ext::sh -c x"] {
            assert!(validate_url(u, false).is_err(), "{u}");
        }
        assert!(validate_url("/tmp/repo.git", true).is_ok());
    }
    #[test]
    fn allowlist() {
        for p in [".gitignore", "format.json", "identity.pub", "keyset.enc", "manifest.enc", "devices.enc", "identity.enc", "vaults/abc/meta.enc", "vaults/a/items/b.enc", "vaults/a/files/c.enc", "vaults/a/items/.keep"] {
            assert!(is_allowed_path(p), "{p}");
        }
        for p in ["secret.key", ".gitpass-device-key.json", ".gitpass-device.json", ".gitpass-state.json", ".gitpass-throttle.json", ".gitpass.lock", ".gitpass-settings.json",
                  ".gitpass-backup-1/keyset.enc", "notes.txt", "export.csv", "vaults/a/items/b.json", "x.enc.tmp", ".hidden.enc", ".keep", "x/.keep"] {
            assert!(!is_allowed_path(p), "{p}");
        }
    }
}
