//! Sync a local vault with an API remote (issue #453): the driver that ties
//! a [`RemoteRepo`] transport (e.g. [`crate::github_api::GitHubApi`]) to the
//! shared engine ([`plan_sync`]) without a git checkout.
//!
//! One attempt:
//! 1. Read the remote head; fetch and verify the remote tree (every path on
//!    the allowlist, every blob id re-checked here, independent of the
//!    transport) and decrypt it with this session's keys
//!    ([`Session::snapshot_from_files`]). No key derivation runs on remote
//!    data.
//! 2. Base = the transport's merge base if it has one
//!    ([`RemoteRepo::merge_base`]: git uses `git merge-base`, #454), else
//!    this device's last-synced commit for this remote, stored in the
//!    local-only `.gitpass-state.json` ([`SyncBase`]); a recorded base's tree
//!    id is re-verified when fetched. Without either (first sync with this
//!    remote) the base is EMPTY: the plan is a union — nothing is deleted on
//!    either side — which is the fail-safe choice. Deletes propagate once a
//!    base exists.
//! 3. Rollback: the remote's manifest epoch must not be below the recorded
//!    base epoch (plan_sync additionally checks both sides against the base),
//!    and a remote that became empty after a recorded sync is refused.
//! 4. `format.json` / `keyset.enc` (KDF parameters and the wrapped account
//!    key) are not merged by the engine. A remote change to them that this
//!    side does not share is refused (re-pull with the desktop git client);
//!    a local-only change is pushed.
//! 5. Plan, apply locally ([`Session::apply_sync_plan`]), push the merged
//!    tree with a non-forced compare-and-swap ref update. If another writer
//!    moved the ref ([`RefUpdateConflict`]), re-fetch and re-plan, at most
//!    [`SyncOptions::max_attempts`] times. The base is recorded only after the
//!    remote is known to hold exactly the merged tree.
//!
//! Errors never contain tokens, file paths, remote bodies or decrypted data.

use crate::store::{self, Session, SyncBase};
use crate::sync_engine::{
    git_blob_sha, git_tree_id, is_allowed_path, plan_sync, RefUpdateConflict, RemoteRepo, SyncConflict,
    TreeEntry, VaultSnapshot,
};
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Files the engine does not merge: account KDF parameters and the wrapped
/// account key. Handled by the rule in step 4 of the module docs.
/// Repo-relative path and bytes of every committed vault file, sorted by path.
pub type VaultFiles = Vec<(String, Vec<u8>)>;

const ACCOUNT_FILES: [&str; 2] = ["format.json", "keyset.enc"];

/// Driver knobs.
#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// Attempts when the ref moves under us (re-fetch + re-plan each time).
    pub max_attempts: u32,
    /// Commit message for pushed commits. Never include item data.
    pub message: String,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self { max_attempts: 3, message: "gitpass sync".into() }
    }
}

/// What a sync did.
#[derive(Debug, Clone, Default)]
pub struct SyncOutcome {
    /// Remote head after the sync (== recorded base commit).
    pub head: String,
    /// A new commit was pushed.
    pub pushed: bool,
    /// Local vault was changed by the merge.
    pub local_changed: bool,
    /// Conflicts resolved fail-safe (local copy kept); see [`SyncConflict::describe`].
    pub conflicts: Vec<SyncConflict>,
    /// Attempts used (> 1 when the ref moved concurrently).
    pub attempts: u32,
    /// Items present on both sides with different content, with the
    /// field-level merge report from this device's viewpoint (`from_remote`
    /// = fields taken from the remote). Ids and field names only.
    pub merged: Vec<(String, String, crate::merge::MergeReport)>,
    /// Items this side had that the remote lacked and that survive the merge
    /// (pushed). Items the remote deleted since the base are not listed.
    pub local_only: Vec<(String, String)>,
}

/// All vault files under `root` that may be committed, as (path, bytes),
/// sorted. Local-only state (`.gitpass-*`, `secret.key`, …) is never
/// included: [`is_allowed_path`] is the single rule set.
pub fn local_vault_files(root: &Path) -> Result<VaultFiles> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir)? {
            let e = e?;
            let ft = e.file_type()?;
            let p = e.path();
            let Ok(rel) = p.strip_prefix(root) else { continue };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if ft.is_dir() {
                // Skip hidden/local-only directories (.git, backups, temp dirs).
                if !e.file_name().to_string_lossy().starts_with('.') {
                    stack.push(p);
                }
            } else if ft.is_file() && is_allowed_path(&rel) {
                let bytes = fs::read(&p)?;
                if rel.ends_with("/.keep") && !bytes.is_empty() {
                    bail!("a vault placeholder file is not empty; refusing to sync");
                }
                out.push((rel, bytes));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Remote files at `commit`, verified: allowlisted paths only, every blob's
/// bytes hash to the advertised id. Returns (files, tree id).
fn fetch_tree<R: RemoteRepo>(repo: &R, commit: &str) -> Result<(VaultFiles, String)> {
    let entries: Vec<TreeEntry> = repo.list_tree(commit)?;
    let mut files = Vec::with_capacity(entries.len());
    for e in &entries {
        if !is_allowed_path(&e.path) {
            bail!("remote tree contains a file outside the vault allowlist; refusing to sync");
        }
        let bytes = repo.get_blob(&e.sha)?;
        if git_blob_sha(&bytes) != e.sha {
            bail!("remote blob failed its identity check; refusing to sync");
        }
        files.push((e.path.clone(), bytes));
    }
    files.sort();
    let tree = tree_id_of(&files)?;
    Ok((files, tree))
}

fn tree_id_of(files: &[(String, Vec<u8>)]) -> Result<String> {
    git_tree_id(&files.iter().map(|(p, b)| (p.clone(), git_blob_sha(b))).collect::<Vec<_>>())
}

/// Empty base for a first sync: same account, no vaults, epoch 0. Every
/// difference then reads as an add on one side (union, no deletes).
fn empty_base(local: &VaultSnapshot) -> VaultSnapshot {
    let mut manifest = local.manifest.clone();
    manifest.vaults.clear();
    manifest.epoch = 0;
    VaultSnapshot {
        manifest,
        items: BTreeMap::new(),
        vault_meta: BTreeMap::new(),
        vault_key_blobs: BTreeMap::new(),
        vault_blobs: BTreeMap::new(),
        generator_history: Default::default(),
        devices: None,
        identity: None,
    }
}

fn account_files_check(
    local: &[(String, Vec<u8>)],
    base: Option<&[(String, Vec<u8>)]>,
    remote: &[(String, Vec<u8>)],
) -> Result<()> {
    let get = |files: &[(String, Vec<u8>)], name: &str| files.iter().find(|(p, _)| p == name).map(|(_, b)| b.clone());
    for f in ACCOUNT_FILES {
        let (l, r) = (get(local, f), get(remote, f));
        if l == r {
            continue;
        }
        // Local-only change (remote still at base): ours is pushed.
        if base.is_some_and(|b| get(b, f) == r) {
            continue;
        }
        bail!(
            "the remote changed the account password/KDF settings on another device; pull it with the desktop git sync first (nothing was changed)"
        );
    }
    Ok(())
}

/// Sync `session`'s vault with `repo`. `remote_id` names the remote stably
/// (e.g. [`crate::github_api::GitHubApi::remote_id`]); the recorded base is
/// only used for the same id. The session must be a normal unlock of the
/// local vault (integrity + rollback checks already ran).
pub fn sync_with_remote<R: RemoteRepo>(
    session: &Session,
    repo: &mut R,
    remote_id: &str,
    opts: &SyncOptions,
) -> Result<SyncOutcome> {
    let root = session.root.clone();
    let account = session.fmt.account_id.clone();
    let mut outcome = SyncOutcome::default();
    for attempt in 1..=opts.max_attempts.max(1) {
        outcome.attempts = attempt;
        let recorded = store::sync_base(&root, &account, remote_id);
        let head = repo.head()?;

        let Some(head) = head else {
            // Empty remote: only a first sync may populate it.
            if recorded.is_some() {
                bail!("the remote is empty but this device synced with it before (possible reset/rollback); refusing to sync");
            }
            let files = local_vault_files(&root)?;
            let tree = tree_id_of(&files)?;
            let commit = repo.create_commit(None, &files, &opts.message)?;
            match repo.update_ref(&commit, None) {
                Ok(()) => {}
                Err(e) if e.downcast_ref::<RefUpdateConflict>().is_some() => continue,
                Err(e) => return Err(e),
            }
            repo.finish(&commit)?;
            let epoch = session.epoch()?;
            store::set_sync_base(&root, &account, &SyncBase { remote: remote_id.into(), commit: commit.clone(), tree, epoch })?;
            outcome.head = commit;
            outcome.pushed = true;
            return Ok(outcome);
        };

        let (remote_files, remote_tree) = fetch_tree(repo, &head)?;
        let remote_snap = session.snapshot_from_files(&remote_files)?;
        if let Some(b) = &recorded {
            if remote_snap.manifest.epoch < b.epoch {
                bail!(
                    "remote epoch {} is below this device's last-synced epoch {}: possible rollback; refusing to sync",
                    remote_snap.manifest.epoch,
                    b.epoch
                );
            }
        }
        // Base: a transport merge base (git) wins over the recorded base;
        // the recorded tree id is re-verified only for the recorded commit.
        let hint = repo.merge_base(&head)?;
        let base_commit = hint.clone().or_else(|| recorded.as_ref().map(|b| b.commit.clone()));
        let recorded_tree = |c: &str| recorded.as_ref().filter(|b| b.commit == c).map(|b| b.tree.clone());
        let (base_files, base_snap) = match base_commit {
            Some(c) if c == head => {
                if recorded_tree(&c).is_some_and(|t| t != remote_tree) {
                    bail!("remote tree does not match the recorded last-synced tree; refusing to sync");
                }
                (Some(remote_files.clone()), remote_snap.clone())
            }
            Some(c) => {
                let (files, tree) = fetch_tree(repo, &c)?;
                if recorded_tree(&c).is_some_and(|t| t != tree) {
                    bail!("the last-synced commit's tree does not match the recorded tree; refusing to sync");
                }
                let snap = session.snapshot_from_files(&files)?;
                (Some(files), snap)
            }
            None => (None, empty_base(&session.snapshot_sync()?)),
        };

        let local_files = local_vault_files(&root)?;
        account_files_check(&local_files, base_files.as_deref(), &remote_files)?;

        let local_snap = session.snapshot_sync()?;
        let plan = plan_sync(&local_snap, &base_snap, &remote_snap, store::now_secs())?;
        outcome.conflicts = plan.conflicts.clone();
        outcome.merged.clear();
        outcome.local_only.clear();
        for ((vid, iid), local) in &local_snap.items {
            match remote_snap.items.get(&(vid.clone(), iid.clone())) {
                Some(remote) if remote != local => {
                    let (_, r) = crate::merge::merge_records(local, remote);
                    outcome.merged.push((vid.clone(), iid.clone(), r));
                }
                Some(_) => {}
                None => {
                    let deleted_remotely = plan
                        .actions
                        .iter()
                        .any(|a| matches!(a, crate::sync_engine::SyncAction::DeleteItem { vault_id, item_id } if vault_id == vid && item_id == iid)
                            || matches!(a, crate::sync_engine::SyncAction::DeleteVault { vault_id } if vault_id == vid));
                    if !deleted_remotely {
                        outcome.local_only.push((vid.clone(), iid.clone()));
                    }
                }
            }
        }
        let remote_epoch = remote_snap.manifest.epoch;
        // Fast-forward: this side has nothing the remote lacks (no local
        // content change since the base, or the same content) and the remote
        // is not behind us. Adopt the verified remote files as-is instead of
        // re-sealing and pushing an equivalent commit (which would bounce
        // between devices forever).
        let nothing_local = !plan.changed || (plan.fast_forward && plan.conflicts.is_empty());
        if nothing_local && remote_epoch >= local_snap.manifest.epoch {
            if local_files != remote_files {
                session.adopt_remote_files(&remote_files, remote_epoch)?;
                outcome.local_changed = true;
            }
        } else if plan.local_changes {
            session.apply_sync_plan(&plan)?;
            outcome.local_changed = true;
        }

        let merged = local_vault_files(&root)?;
        let merged_tree = tree_id_of(&merged)?;
        let new_head = if merged_tree == remote_tree {
            head.clone()
        } else {
            let commit = repo.create_commit(Some(&head), &merged, &opts.message)?;
            match repo.update_ref(&commit, Some(&head)) {
                Ok(()) => {}
                Err(e) if e.downcast_ref::<RefUpdateConflict>().is_some() => continue,
                Err(e) => return Err(e),
            }
            outcome.pushed = true;
            commit
        };
        repo.finish(&new_head)?;
        let epoch = session.epoch()?;
        store::set_sync_base(&root, &account, &SyncBase { remote: remote_id.into(), commit: new_head.clone(), tree: merged_tree, epoch })?;
        outcome.head = new_head;
        return Ok(outcome);
    }
    bail!("the remote kept moving while syncing ({} attempts); try again later", opts.max_attempts.max(1))
}
