//! Git CLI transport for the shared sync engine (issue #454): a
//! [`RemoteRepo`] over the vault's own git repository and its `origin`
//! remote, so desktop and CLI `sync --merge` run the same planner
//! ([`crate::sync_engine::plan_sync`]) and driver
//! ([`crate::sync_remote::sync_with_remote`]) as the GitHub API transport.
//!
//! * Keeps the existing git setup: whatever `origin` is (SSH, HTTPS with the
//!   user's credential helper, a local path), the repo's identity config,
//!   `.gitignore`/`.gitattributes`, and the current local branch name on the
//!   remote (same branch `git push origin HEAD` used).
//! * Reads never touch the working tree: `fetch`, `ls-tree`, `cat-file`.
//! * Commits are built in a throw-away index (`GIT_INDEX_FILE`) from the
//!   exact bytes the driver passes, then `commit-tree`; the resulting tree
//!   id is checked against [`git_tree_id`].
//! * The ref update is a plain, never-forced `git push <commit>:refs/heads/<branch>`
//!   of a commit whose only parent is the expected head: the server accepts
//!   it only as a fast-forward, i.e. only if the branch is still at
//!   `expected_old` (compare-and-swap). A rejection surfaces as
//!   [`RefUpdateConflict`] so the driver re-fetches and re-plans.
//! * The merge base is `git merge-base HEAD <remote head>`, so deletes
//!   propagate even for vaults that never ran the API driver before.
//! * [`RemoteRepo::finish`] moves the local branch and index onto the synced
//!   head (`git reset --mixed`); the working tree already holds exactly that
//!   tree, so the repository ends clean.

use crate::remote::{classify_git_error, git_cmd, is_allowed_path};
use crate::sync_engine::{git_tree_id, RefUpdateConflict, RemoteRepo, TreeEntry};
use anyhow::{anyhow, bail, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// [`RemoteRepo`] over `<root>/.git` and its `origin` remote.
pub struct GitCliRemote {
    root: PathBuf,
    branch: String,
}

fn valid_oid(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// Run git with optional stdin and extra env; returns (success, stdout bytes, stderr text).
fn git_raw(root: &Path, args: &[&str], stdin: Option<&[u8]>, env: &[(&str, &Path)]) -> Result<(bool, Vec<u8>, String)> {
    let mut c = Command::new("git");
    c.arg("-C").arg(root).args(args).env("GIT_TERMINAL_PROMPT", "0");
    for (k, v) in env {
        c.env(k, v);
    }
    c.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow!("{}: git is not installed", crate::remote::E_GIT_MISSING)
        } else {
            e.into()
        }
    })?;
    if let Some(input) = stdin {
        // Write on a thread so a large stdout cannot deadlock against stdin.
        let mut pipe = child.stdin.take().expect("stdin piped");
        let data = input.to_vec();
        let writer = std::thread::spawn(move || pipe.write_all(&data));
        let out = child.wait_with_output()?;
        writer.join().map_err(|_| anyhow!("git stdin writer panicked"))??;
        return Ok((out.status.success(), out.stdout, String::from_utf8_lossy(&out.stderr).into_owned()));
    }
    let out = child.wait_with_output()?;
    Ok((out.status.success(), out.stdout, String::from_utf8_lossy(&out.stderr).into_owned()))
}

fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    let (ok, out) = git_cmd(root, args)?;
    if !ok {
        bail!("git {} failed", args.first().copied().unwrap_or(""));
    }
    Ok(out.trim().to_string())
}

impl GitCliRemote {
    /// Open the vault repository at `root`; requires a git repo with an
    /// `origin` remote and a named (possibly unborn) local branch.
    pub fn open(root: &Path) -> Result<Self> {
        if !root.join(".git").exists() {
            bail!("the vault is not a git repository; connect a remote first");
        }
        let (has_origin, _) = git_cmd(root, &["remote", "get-url", "origin"])?;
        if !has_origin {
            bail!("no 'origin' remote configured");
        }
        let branch = git_text(root, &["symbolic-ref", "--short", "-q", "HEAD"])
            .map_err(|_| anyhow!("the vault repository is not on a branch (detached HEAD); check out a branch first"))?;
        if branch.is_empty() || branch.starts_with('-') || branch.contains("..") {
            bail!("unsupported local branch name");
        }
        Ok(Self { root: root.to_path_buf(), branch })
    }

    /// Stable id for the recorded sync base. The origin URL is hashed, never
    /// stored: HTTPS remotes can embed credentials.
    pub fn remote_id(&self) -> String {
        use sha2::Digest;
        let url = git_cmd(&self.root, &["remote", "get-url", "origin"]).map(|(_, u)| u.trim().to_string()).unwrap_or_default();
        let h = sha2::Sha256::digest(url.as_bytes());
        let hex: String = h.iter().take(8).map(|b| format!("{b:02x}")).collect();
        format!("git:{hex}#{}", self.branch)
    }

    fn branch_ref(&self) -> String {
        format!("refs/heads/{}", self.branch)
    }
}

impl RemoteRepo for GitCliRemote {
    fn head(&self) -> Result<Option<String>> {
        let (ok, out) = git_cmd(&self.root, &["fetch", "-q", "origin", &self.branch_ref()])?;
        if !ok {
            if out.contains("couldn't find remote ref") {
                return Ok(None);
            }
            return Err(classify_git_error("git fetch", &out));
        }
        let sha = git_text(&self.root, &["rev-parse", "FETCH_HEAD"])?;
        if !valid_oid(&sha) {
            bail!("unexpected git object id");
        }
        Ok(Some(sha))
    }

    fn list_tree(&self, commit: &str) -> Result<Vec<TreeEntry>> {
        if !valid_oid(commit) {
            bail!("invalid commit id");
        }
        let (ok, out, _) = git_raw(&self.root, &["ls-tree", "-r", "-z", "--full-tree", commit], None, &[])?;
        if !ok {
            bail!("git ls-tree failed");
        }
        let mut entries = vec![];
        for rec in out.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let rec = std::str::from_utf8(rec).map_err(|_| anyhow!("remote tree contains a non-UTF-8 path; refusing to sync"))?;
            let (meta, path) = rec.split_once('\t').ok_or_else(|| anyhow!("unexpected git ls-tree output"))?;
            let mut parts = meta.split(' ');
            let (mode, kind, sha) = (parts.next(), parts.next(), parts.next());
            if mode != Some("100644") || kind != Some("blob") {
                bail!("remote tree contains a non-regular file; refusing to sync");
            }
            let sha = sha.filter(|s| valid_oid(s)).ok_or_else(|| anyhow!("unexpected git ls-tree output"))?;
            if !is_allowed_path(path) {
                bail!("remote tree contains a file outside the vault allowlist; refusing to sync");
            }
            entries.push(TreeEntry { path: path.to_string(), sha: sha.to_string() });
        }
        Ok(entries)
    }

    fn get_blob(&self, sha: &str) -> Result<Vec<u8>> {
        if !valid_oid(sha) {
            bail!("invalid blob id");
        }
        let (ok, out, _) = git_raw(&self.root, &["cat-file", "blob", sha], None, &[])?;
        if !ok {
            bail!("git cat-file failed");
        }
        Ok(out)
    }

    fn create_commit(&mut self, parent: Option<&str>, tree: &[(String, Vec<u8>)], message: &str) -> Result<String> {
        if parent.is_some_and(|p| !valid_oid(p)) {
            bail!("invalid parent id");
        }
        // Blob ids, and which objects the repository already has (one batch-check process).
        let mut pairs = Vec::with_capacity(tree.len());
        for (path, bytes) in tree {
            if !is_allowed_path(path) {
                bail!("refusing to commit a file outside the vault allowlist");
            }
            pairs.push((path.clone(), crate::sync_engine::git_blob_sha(bytes)));
        }
        let query: String = pairs.iter().map(|(_, s)| format!("{s}\n")).collect();
        let (ok, out, _) = git_raw(&self.root, &["cat-file", "--batch-check"], Some(query.as_bytes()), &[])?;
        if !ok {
            bail!("git cat-file --batch-check failed");
        }
        let present: std::collections::HashSet<String> = String::from_utf8_lossy(&out)
            .lines()
            .filter(|l| !l.ends_with(" missing"))
            .filter_map(|l| l.split(' ').next().map(str::to_string))
            .collect();
        for ((_, bytes), (_, sha)) in tree.iter().zip(&pairs) {
            if present.contains(sha) {
                continue;
            }
            let (ok, out, _) = git_raw(&self.root, &["hash-object", "-w", "--stdin"], Some(bytes), &[])?;
            if !ok || String::from_utf8_lossy(&out).trim() != sha {
                bail!("git hash-object failed or produced an unexpected id");
            }
        }
        // Build the tree in a private index so the user's index/working tree stay untouched.
        let tmp = tempfile::Builder::new().prefix("gitpass-index-").tempdir_in(self.root.join(".git"))?;
        let index = tmp.path().join("index");
        let info: String = pairs.iter().map(|(p, s)| format!("100644 {s}\t{p}\n")).collect();
        let env = [("GIT_INDEX_FILE", index.as_path())];
        let (ok, _, _) = git_raw(&self.root, &["update-index", "--add", "--index-info"], Some(info.as_bytes()), &env)?;
        if !ok {
            bail!("git update-index failed");
        }
        let (ok, out, _) = git_raw(&self.root, &["write-tree"], None, &env)?;
        let tree_id = String::from_utf8_lossy(&out).trim().to_string();
        if !ok || tree_id != git_tree_id(&pairs)? {
            bail!("git write-tree produced an unexpected tree");
        }
        crate::remote::ensure_identity(&self.root)?;
        let mut args = vec!["commit-tree", tree_id.as_str()];
        if let Some(p) = parent {
            args.extend(["-p", p]);
        }
        args.extend(["-m", message]);
        let commit = git_text(&self.root, &args)?;
        if !valid_oid(&commit) {
            bail!("git commit-tree produced an unexpected id");
        }
        Ok(commit)
    }

    fn update_ref(&mut self, new_head: &str, expected_old: Option<&str>) -> Result<()> {
        if !valid_oid(new_head) || expected_old.is_some_and(|o| !valid_oid(o)) {
            bail!("invalid commit id");
        }
        // Cheap early check; the server's fast-forward rule is the real CAS.
        if self.head()?.as_deref() != expected_old {
            return Err(RefUpdateConflict.into());
        }
        let refspec = format!("{new_head}:{}", self.branch_ref());
        let (ok, out) = git_cmd(&self.root, &["push", "-q", "origin", &refspec])?;
        if !ok {
            let l = out.to_lowercase();
            if l.contains("non-fast-forward") || l.contains("fetch first") || l.contains("[rejected]") || l.contains("stale info") {
                return Err(RefUpdateConflict.into());
            }
            return Err(classify_git_error("git push", &out));
        }
        Ok(())
    }

    fn merge_base(&self, remote_head: &str) -> Result<Option<String>> {
        if !valid_oid(remote_head) {
            bail!("invalid commit id");
        }
        let (ok, out) = git_cmd(&self.root, &["merge-base", "HEAD", remote_head])?;
        let sha = out.trim();
        Ok((ok && valid_oid(sha)).then(|| sha.to_string()))
    }

    fn finish(&mut self, head: &str) -> Result<()> {
        if !valid_oid(head) {
            bail!("invalid commit id");
        }
        // Branch + index onto the synced head; the working tree already matches it.
        git_text(&self.root, &["reset", "-q", "--mixed", head])?;
        // Keep `git status` / ahead-behind meaningful (what `push -u` used to set).
        let _ = git_cmd(&self.root, &["config", &format!("branch.{}.remote", self.branch), "origin"]);
        let _ = git_cmd(&self.root, &["config", &format!("branch.{}.merge", self.branch), &self.branch_ref()]);
        let _ = git_cmd(&self.root, &["update-ref", &format!("refs/remotes/origin/{}", self.branch), head]);
        Ok(())
    }
}
