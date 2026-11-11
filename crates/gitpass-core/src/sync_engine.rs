//! Shared sync engine (issue #452): the [`RemoteRepo`] trait, the pure three-way
//! planner [`plan_sync`], and the in-memory fake ([`MemoryRemoteRepo`]) used by
//! engine tests.
//!
//! The engine is transport-independent: it plans over decrypted [`VaultSnapshot`]s
//! (local, base, remote) and never touches the network or the filesystem. A
//! transport (#453: GitHub API; the git CLI; mobile) resolves the three snapshots
//! and applies the resulting [`SyncPlan`]:
//!
//! * local — this replica's current state, e.g. [`Session::snapshot_sync`](crate::store::Session::snapshot_sync).
//! * base — the last-synced state both sides descended from: the merge-base
//!   manifest for git histories, or the locally stored last-synced commit for
//!   transports without a merge-base (see #453). Deletes only propagate when the
//!   base is the true last-synced state; a caller that passes `base == local`
//!   gets an add-only plan and must not expect delete propagation.
//! * remote — the other side's state, decrypted with this account's keys.
//!
//! # Delete handling
//!
//! Deletes propagate by base-snapshot comparison (the issue's allowed
//! alternative to persistent tombstones): an item in `base` but missing on one
//! side is a delete on that side. It wins when the other side did not touch the
//! item since `base`; a delete against a concurrent edit is a conflict and the
//! edited copy is kept (fail-safe: no data loss). Soft-deleted items
//! (`deleted_at`, still in the manifest) are ordinary records and merge by
//! field clock through [`merge_records`](crate::merge::merge_records); only
//! hard deletes (purged, absent from the manifest) go through this table.
//!
//! # Vault keys
//!
//! A vault-key rotation is adopted only when the adopting side made no change
//! to that vault since `base` (clean pull, mirroring `git pull`); the merged
//! items and blobs are then re-sealed under the new key by the applier. Any
//! other key divergence is a [`SyncConflictKind::VaultKey`] conflict and the
//! local key is kept — rotating on one side while editing on the other needs a
//! sync in between, and guessing would corrupt sealed blobs.
//!
//! # Epochs
//!
//! The merged manifest's epoch is `max(local, remote, base) + 1` — strictly
//! above both sides, so adopting it can never look like a rollback. When the
//! plan is empty the epoch stays at the local epoch (nothing is re-sealed).
//!
//! # Allowed paths
//!
//! [`is_allowed_path`] is the single rule set for what may be committed or
//! accepted from a remote tree. It is re-exported here so the mobile bridge
//! (`gitpass-ios-ffi`, and `gitpass-mobile-ffi` after #450) exposes exactly
//! this function instead of each platform keeping its own copy; remote paths
//! are untrusted and anything outside the allowlist is rejected before
//! writing. The Swift (`SyncService.isSyncable`) and Kotlin
//! (`EncryptedFirstPull.allowedFile`) copies are migrated to the bridge export
//! in a follow-up, after #450's bridge rename lands, to avoid conflicting
//! with PR #459.
//!
//! # What this module does NOT do
//!
//! No I/O, no secrets in logs or errors, no transport, no UI. Applying a plan
//! to a live vault is [`Session::apply_sync_plan`](crate::store::Session::apply_sync_plan);
//! resolving a remote tree into a snapshot is the transport's job (#453).

use crate::device::{DeviceRegistry, UNKNOWN_DEVICE};
use crate::gen_history::GeneratorHistory;
use crate::merge::{merge_records, ItemRecord};
use crate::store::{Manifest, VaultMeta};
use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};

/// The single allowed-path rule for vault trees: what may be committed, and
/// what may be accepted from an untrusted remote tree. Re-exported from
/// [`crate::remote`] — one rule set in core, used by every platform.
pub use crate::remote::is_allowed_path;

/// A decrypted view of one vault replica at one point in time. Built by
/// [`Session::snapshot_sync`](crate::store::Session::snapshot_sync) for local
/// state; transports build the base/remote snapshots the same way after
/// decrypting the blobs they fetched. Raw key/blob bytes are opaque and must
/// never be logged.
#[derive(Debug, Clone, PartialEq)]
pub struct VaultSnapshot {
    pub manifest: Manifest,
    /// (vault_id, item_id) -> decrypted record.
    pub items: BTreeMap<(String, String), ItemRecord>,
    /// vault_id -> decrypted meta.
    pub vault_meta: BTreeMap<String, VaultMeta>,
    /// vault_id -> raw `vault.key.enc` bytes.
    pub vault_key_blobs: BTreeMap<String, Vec<u8>>,
    /// (vault_id, repo-relative path under the vault dir) -> raw bytes, for
    /// `files/<fid>.enc` attachments and `members/<fpr>.enc` share envelopes.
    pub vault_blobs: BTreeMap<(String, String), Vec<u8>>,
    pub generator_history: GeneratorHistory,
    /// None when the replica predates `devices.enc`.
    pub devices: Option<DeviceRegistry>,
    /// (identity.pub bytes, identity.enc bytes) when the account has an identity.
    pub identity: Option<(Vec<u8>, Vec<u8>)>,
}

impl VaultSnapshot {
    /// Vault ids in manifest order (BTreeMap order: sorted by id).
    pub fn vault_ids(&self) -> BTreeSet<&str> {
        self.manifest.vaults.keys().map(String::as_str).collect()
    }
    /// Item ids of `vault_id` per the manifest (presence, not content).
    pub fn item_ids(&self, vault_id: &str) -> BTreeSet<&str> {
        self.manifest
            .vaults
            .get(vault_id)
            .map(|ve| ve.items.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// Content equality: everything except the manifest's epoch, hashes and
    /// per-item versions (per-replica revision counters, not content). Two
    /// replicas are in sync when their contents are equal even if their
    /// epochs or item versions differ — comparing whole snapshots would
    /// report "changed" forever after any re-seal.
    pub fn content_eq(&self, other: &Self) -> bool {
        if self.items != other.items
            || self.vault_meta != other.vault_meta
            || self.vault_key_blobs != other.vault_key_blobs
            || self.vault_blobs != other.vault_blobs
            || self.generator_history != other.generator_history
            || self.devices != other.devices
            || self.identity != other.identity
        {
            return false;
        }
        // Vault/item shape from the manifests, ignoring versions and hashes.
        manifest_shape(self) == manifest_shape(other)
    }
}

/// Vault/item shape of a snapshot's manifest, ignoring versions and hashes
/// (per-replica revision counters and content hashes, not content).
type VaultShape<'a> = BTreeMap<&'a str, (u32, BTreeSet<&'a str>, BTreeSet<&'a str>, BTreeSet<&'a str>)>;

fn manifest_shape(s: &VaultSnapshot) -> VaultShape<'_> {
    s.manifest
        .vaults
        .iter()
        .map(|(vid, ve)| {
            (
                vid.as_str(),
                (
                    ve.key_version,
                    ve.items.keys().map(String::as_str).collect(),
                    ve.members.keys().map(String::as_str).collect(),
                    ve.files.keys().map(String::as_str).collect(),
                ),
            )
        })
        .collect()
}

/// One step the applier takes to bring the local replica to the merged state.
/// Actions are local-relative and ordered by phase; the applier executes them
/// in the order given (vaults, then keys/meta, then items/blobs).
///
/// `CreateVault` dwarfs the other variants, but plans are small (one entry
/// per changed vault/item) and built once per sync, so the enum stays
/// unboxed; see the `large_enum_variant` precedent in `gitpass-cli`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum SyncAction {
    /// Adopt a vault that exists only on the other side (or that the local
    /// side deleted while the remote side changed — see [`SyncConflictKind::EditVsDelete`]).
    CreateVault {
        vault_id: String,
        meta: VaultMeta,
        key_blob: Vec<u8>,
        items: Vec<(String, ItemRecord)>,
        blobs: Vec<(String, Vec<u8>)>,
    },
    /// The vault was deleted on one side and untouched on the other since base.
    DeleteVault { vault_id: String },
    /// Adopt the remote vault key (clean pull only — see module docs). The
    /// applier re-seals meta, items and blobs under the new key.
    SetVaultKey { vault_id: String, key_blob: Vec<u8> },
    /// Write the merged vault meta (rename/description/icon/color).
    UpdateVaultMeta { vault_id: String, meta: VaultMeta },
    /// Write a merged or adopted record (version is bumped by the applier).
    UpsertItem { vault_id: String, item_id: String, record: ItemRecord },
    /// Hard-delete an item the other side deleted (and this side did not edit).
    DeleteItem { vault_id: String, item_id: String },
    /// Write an adopted attachment/share blob verbatim.
    UpsertBlob { vault_id: String, path: String, bytes: Vec<u8> },
    /// Drop a blob the other side deleted (and this side did not change).
    DeleteBlob { vault_id: String, path: String },
    /// Adopt the remote identity (local side has none).
    SetIdentity { pub_blob: Vec<u8>, enc_blob: Vec<u8> },
}

/// Why a sync decision kept the local value instead of taking the remote one.
/// Conflict details never contain secrets — only ids, field names and kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncConflictKind {
    /// Equal field clocks, different values; the local value was kept.
    /// `fields` holds the conflicting field/clock keys.
    EqualClock { fields: Vec<String> },
    /// Remote deleted the item/vault, local edited it since base; kept local.
    DeleteVsEdit,
    /// Local deleted the item/vault, remote edited it since base; kept remote.
    EditVsDelete,
    /// Both sides changed the vault meta differently since base; kept local.
    VaultMeta,
    /// Vault-key divergence outside the clean-pull case; kept local.
    VaultKey,
    /// Both sides hold a different identity; kept local.
    Identity,
    /// Same blob path, different bytes, both added/changed; kept local.
    Blob,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyncConflict {
    /// "" for account-level conflicts (identity).
    pub vault_id: String,
    pub item_id: Option<String>,
    pub kind: SyncConflictKind,
}

impl SyncConflict {
    /// One-line, secret-free description for logs and reports.
    pub fn describe(&self) -> String {
        let what = match &self.item_id {
            Some(i) => format!("{}/{}", self.vault_id, i),
            None if self.vault_id.is_empty() => "account".to_string(),
            None => self.vault_id.clone(),
        };
        let kind = match &self.kind {
            SyncConflictKind::EqualClock { fields } => format!("equal-clock conflict on {}", fields.join(", ")),
            SyncConflictKind::DeleteVsEdit => "remote deleted, local edited — kept local".to_string(),
            SyncConflictKind::EditVsDelete => "local deleted, remote edited — kept remote".to_string(),
            SyncConflictKind::VaultMeta => "vault meta changed on both sides — kept local".to_string(),
            SyncConflictKind::VaultKey => "vault key changed on both sides — kept local".to_string(),
            SyncConflictKind::Identity => "identity differs on both sides — kept local".to_string(),
            SyncConflictKind::Blob => "blob changed on both sides — kept local".to_string(),
        };
        format!("{what}: {kind}")
    }
}

/// The result of [`plan_sync`]: the actions that bring the local replica to
/// the merged state, the merged account-level state, and the conflict report.
/// Applying the plan and then pushing the merged tree makes the remote side
/// converge to the same state.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncPlan {
    /// Account the plan was computed for; the applier refuses a plan for a
    /// different account.
    pub account_id: String,
    /// Local epoch the plan was computed from; the applier refuses a plan
    /// whose local epoch no longer matches the vault on disk.
    pub local_epoch: u64,
    pub remote_epoch: u64,
    /// Epoch the merged manifest is re-sealed with: strictly above both sides
    /// when applying the plan changes the local vault, else the local epoch.
    pub new_epoch: u64,
    pub actions: Vec<SyncAction>,
    /// Merged generator history, present only when it differs from local.
    pub generator_history: Option<GeneratorHistory>,
    /// Merged device registry, present only when it differs from local.
    pub devices: Option<DeviceRegistry>,
    pub conflicts: Vec<SyncConflict>,
    /// True when local made no change since base, so the merged state is the
    /// remote state wholesale and a transport may fast-forward instead.
    pub fast_forward: bool,
    /// True when local and remote contents differ, so a sync round-trip
    /// (apply + push) is needed — even when the local side needs no changes
    /// itself (local-ahead: the push carries local's newer content).
    pub changed: bool,
    /// True when applying this plan changes the local vault (actions,
    /// generator history or devices). When false the applier is a no-op
    /// even if [`changed`](Self::changed) is true.
    pub local_changes: bool,
}

/// Merge two device registries: union by device id. Revoked wins (fail-closed),
/// `last_seen_at` takes the max, `created_at` the min, a real name wins over an
/// empty/`unknown-device` name, and a present public key wins over an absent one.
/// `None` on a side means that replica predates `devices.enc`.
pub fn merge_device_registries(
    local: Option<&DeviceRegistry>,
    remote: Option<&DeviceRegistry>,
) -> Option<DeviceRegistry> {
    match (local, remote) {
        (None, None) => None,
        (Some(l), None) => Some(l.clone()),
        (None, Some(r)) => Some(r.clone()),
        (Some(l), Some(r)) => {
            let mut out = DeviceRegistry { format: l.format.max(r.format), devices: vec![] };
            for d in l.devices.iter().chain(r.devices.iter()) {
                match out.devices.iter_mut().find(|x| x.id == d.id) {
                    None => out.devices.push(d.clone()),
                    Some(x) => {
                        if d.revoked {
                            x.revoked = true;
                            x.revoked_at = match (&x.revoked_at, &d.revoked_at) {
                                (Some(a), Some(b)) => Some(if a <= b { a.clone() } else { b.clone() }),
                                (a, b) => a.clone().or_else(|| b.clone()),
                            };
                        }
                        if d.last_seen_at > x.last_seen_at { x.last_seen_at = d.last_seen_at.clone(); }
                        if d.created_at < x.created_at { x.created_at = d.created_at.clone(); }
                        if (x.name.trim().is_empty() || x.name == UNKNOWN_DEVICE) && !d.name.trim().is_empty() {
                            x.name = d.name.clone();
                        }
                        if x.public_key.is_none() { x.public_key = d.public_key.clone(); }
                    }
                }
            }
            if out.devices.iter().any(|d| d.public_key.is_some()) {
                out.format = out.format.max(crate::device::DEVICES_FORMAT);
            }
            out.devices.sort_by(|a, b| a.id.cmp(&b.id));
            Some(out)
        }
    }
}

// ---------- planning ----------

struct VaultView<'a> {
    meta: Option<&'a VaultMeta>,
    key_blob: Option<&'a [u8]>,
    items: BTreeMap<&'a str, &'a ItemRecord>,
    blobs: BTreeMap<&'a str, &'a [u8]>,
}

fn vault_view<'a>(s: &'a VaultSnapshot, vid: &str) -> Option<VaultView<'a>> {
    if !s.manifest.vaults.contains_key(vid) {
        return None;
    }
    let mut items = BTreeMap::new();
    for ((v, iid), rec) in &s.items {
        if v == vid {
            items.insert(iid.as_str(), rec);
        }
    }
    let mut blobs = BTreeMap::new();
    for ((v, path), bytes) in &s.vault_blobs {
        if v == vid {
            blobs.insert(path.as_str(), bytes.as_slice());
        }
    }
    Some(VaultView {
        meta: s.vault_meta.get(vid),
        key_blob: s.vault_key_blobs.get(vid).map(Vec::as_slice),
        items,
        blobs,
    })
}

impl<'a> PartialEq for VaultView<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.meta == other.meta && self.key_blob == other.key_blob && self.items == other.items && self.blobs == other.blobs
    }
}

/// Pure three-way plan. `now_secs` feeds the generator-history merge cutoff.
/// Refuses snapshots from different accounts: merging those would mix vaults.
pub fn plan_sync(
    local: &VaultSnapshot,
    base: &VaultSnapshot,
    remote: &VaultSnapshot,
    now_secs: u64,
) -> Result<SyncPlan> {
    let acct = &local.manifest.account_id;
    if remote.manifest.account_id != *acct || base.manifest.account_id != *acct {
        bail!("sync plan refuses to merge snapshots from different accounts");
    }
    let mut plan = SyncPlan {
        account_id: acct.clone(),
        local_epoch: local.manifest.epoch,
        remote_epoch: remote.manifest.epoch,
        new_epoch: local
            .manifest
            .epoch
            .max(remote.manifest.epoch)
            .max(base.manifest.epoch)
            + 1,
        actions: vec![],
        generator_history: None,
        devices: None,
        conflicts: vec![],
        fast_forward: local.content_eq(base),
        changed: !local.content_eq(remote),
        local_changes: false,
    };

    let mut vids: BTreeSet<&str> = BTreeSet::new();
    vids.extend(local.vault_ids());
    vids.extend(base.vault_ids());
    vids.extend(remote.vault_ids());
    for vid in vids {
        plan_vault(local, base, remote, vid, &mut plan);
    }

    let merged_gh = local.generator_history.merge(&remote.generator_history, now_secs);
    if merged_gh != local.generator_history {
        plan.generator_history = Some(merged_gh);
    }
    let merged_dev = merge_device_registries(local.devices.as_ref(), remote.devices.as_ref());
    if merged_dev.as_ref() != local.devices.as_ref() {
        plan.devices = merged_dev;
    }
    match (&local.identity, &remote.identity) {
        (None, Some((pub_blob, enc_blob))) => plan.actions.push(SyncAction::SetIdentity {
            pub_blob: pub_blob.clone(),
            enc_blob: enc_blob.clone(),
        }),
        (Some(l), Some(r)) if l != r => plan.conflicts.push(SyncConflict {
            vault_id: String::new(),
            item_id: None,
            kind: SyncConflictKind::Identity,
        }),
        _ => {}
    }

    plan.local_changes =
        !plan.actions.is_empty() || plan.generator_history.is_some() || plan.devices.is_some();
    if !plan.local_changes {
        plan.new_epoch = local.manifest.epoch;
    }
    Ok(plan)
}

fn plan_vault(
    local: &VaultSnapshot,
    base: &VaultSnapshot,
    remote: &VaultSnapshot,
    vid: &str,
    plan: &mut SyncPlan,
) {
    let l = vault_view(local, vid);
    let b = vault_view(base, vid);
    let r = vault_view(remote, vid);
    match (&l, &b, &r) {
        (Some(_), Some(_), Some(_)) => plan_vault_merge(vid, l.as_ref().unwrap(), b.as_ref().unwrap(), r.as_ref().unwrap(), plan),
        (Some(lv), Some(bv), None) => {
            // Remote deleted the vault.
            if lv == bv {
                plan.actions.push(SyncAction::DeleteVault { vault_id: vid.to_string() });
            } else {
                plan.conflicts.push(SyncConflict {
                    vault_id: vid.to_string(),
                    item_id: None,
                    kind: SyncConflictKind::DeleteVsEdit,
                });
            }
        }
        (Some(lv), None, Some(rv)) => {
            // Both created the vault independently (same id, no shared base).
            plan_vault_both_created(vid, lv, rv, plan);
        }
        (Some(_lv), None, None) => {
            // Local-only vault: already present locally, nothing to apply;
            // the remote side adopts it from the pushed merged tree.
        }
        (None, Some(_), Some(rv)) => {
            // Local deleted the vault.
            if rv == b.as_ref().unwrap() {
                // Remote untouched: the delete stands; nothing to do locally.
            } else {
                plan.conflicts.push(SyncConflict {
                    vault_id: vid.to_string(),
                    item_id: None,
                    kind: SyncConflictKind::EditVsDelete,
                });
                plan.actions.push(SyncAction::CreateVault {
                    vault_id: vid.to_string(),
                    meta: rv.meta.cloned().unwrap_or_else(|| VaultMeta {
                        name: vid.to_string(),
                        description: None,
                        icon: None,
                        color: None,
                    }),
                    key_blob: rv.key_blob.unwrap_or(&[]).to_vec(),
                    items: rv.items.iter().map(|(iid, rec)| (iid.to_string(), (*rec).clone())).collect(),
                    blobs: rv.blobs.iter().map(|(p, b)| (p.to_string(), b.to_vec())).collect(),
                });
            }
        }
        (None, Some(_), None) => {} // Deleted on both sides: stays deleted.
        (None, None, Some(rv)) => {
            plan.actions.push(SyncAction::CreateVault {
                vault_id: vid.to_string(),
                meta: rv.meta.cloned().unwrap_or_else(|| VaultMeta {
                    name: vid.to_string(),
                    description: None,
                    icon: None,
                    color: None,
                }),
                key_blob: rv.key_blob.unwrap_or(&[]).to_vec(),
                items: rv.items.iter().map(|(iid, rec)| (iid.to_string(), (*rec).clone())).collect(),
                blobs: rv.blobs.iter().map(|(p, b)| (p.to_string(), b.to_vec())).collect(),
            });
        }
        (None, None, None) => {}
    }
}

/// Both sides created the same vault id independently: merge against an empty
/// base. Meta/key conflicts keep local; items and blobs merge per key.
fn plan_vault_both_created(vid: &str, l: &VaultView, r: &VaultView, plan: &mut SyncPlan) {
    if l.meta != r.meta {
        plan.conflicts.push(SyncConflict {
            vault_id: vid.to_string(),
            item_id: None,
            kind: SyncConflictKind::VaultMeta,
        });
    }
    if l.key_blob != r.key_blob {
        plan.conflicts.push(SyncConflict {
            vault_id: vid.to_string(),
            item_id: None,
            kind: SyncConflictKind::VaultKey,
        });
    }
    let empty_items: BTreeMap<&str, &ItemRecord> = BTreeMap::new();
    let empty_blobs: BTreeMap<&str, &[u8]> = BTreeMap::new();
    plan_items(vid, &l.items, &empty_items, &r.items, plan);
    plan_blobs(vid, &l.blobs, &empty_blobs, &r.blobs, plan);
}

/// The (Some, Some, Some) case: merge one vault's meta, key, items and blobs.
fn plan_vault_merge(vid: &str, l: &VaultView, b: &VaultView, r: &VaultView, plan: &mut SyncPlan) {
    // --- vault key ---
    let mut key_adopted = false;
    if l.key_blob != r.key_blob {
        if l == b {
            // Clean pull: this side changed nothing in the vault since base,
            // so the remote rotation (and its re-sealed files) can be adopted.
            plan.actions.push(SyncAction::SetVaultKey {
                vault_id: vid.to_string(),
                key_blob: r.key_blob.unwrap_or(&[]).to_vec(),
            });
            key_adopted = true;
        } else if r.key_blob == b.key_blob {
            // Only local rotated: keep local, nothing to do.
        } else {
            plan.conflicts.push(SyncConflict {
                vault_id: vid.to_string(),
                item_id: None,
                kind: SyncConflictKind::VaultKey,
            });
        }
    }
    // --- vault meta ---
    let merged_meta = match (l.meta, b.meta, r.meta) {
        (Some(lm), _, Some(rm)) if lm == rm => Some(lm.clone()),
        (_, Some(bm), Some(rm)) if l.meta == Some(bm) => Some(rm.clone()),
        (Some(lm), _, _) if r.meta == b.meta => Some(lm.clone()),
        (Some(lm), Some(bm), Some(rm)) if lm != bm && rm != bm && lm != rm => {
            plan.conflicts.push(SyncConflict {
                vault_id: vid.to_string(),
                item_id: None,
                kind: SyncConflictKind::VaultMeta,
            });
            Some(lm.clone())
        }
        (Some(lm), _, _) => Some(lm.clone()),
        (None, _, Some(rm)) => Some(rm.clone()),
        (None, _, None) => None,
    };
    if let Some(meta) = merged_meta {
        if Some(&meta) != l.meta || key_adopted {
            // Re-seal even when the meta is unchanged if the key changed.
            plan.actions.push(SyncAction::UpdateVaultMeta { vault_id: vid.to_string(), meta });
        }
    }
    // --- items ---
    plan_items(vid, &l.items, &b.items, &r.items, plan);
    // A key adoption re-seals every item and blob under the new key, even
    // ones whose content did not change.
    if key_adopted {
        for (iid, rec) in &r.items {
            if !plan.actions.iter().any(|a| matches!(a, SyncAction::UpsertItem { vault_id: v, item_id: i, .. } if v == vid && i == iid)) {
                plan.actions.push(SyncAction::UpsertItem {
                    vault_id: vid.to_string(),
                    item_id: iid.to_string(),
                    record: (*rec).clone(),
                });
            }
        }
        for (path, bytes) in &r.blobs {
            if !plan.actions.iter().any(|a| matches!(a, SyncAction::UpsertBlob { vault_id: v, path: p, .. } if v == vid && p == path)) {
                plan.actions.push(SyncAction::UpsertBlob {
                    vault_id: vid.to_string(),
                    path: path.to_string(),
                    bytes: bytes.to_vec(),
                });
            }
        }
    }
    // --- blobs ---
    plan_blobs(vid, &l.blobs, &b.blobs, &r.blobs, plan);
}

/// Three-way per-item merge. `base` is empty for a both-created vault.
fn plan_items(
    vid: &str,
    l: &BTreeMap<&str, &ItemRecord>,
    b: &BTreeMap<&str, &ItemRecord>,
    r: &BTreeMap<&str, &ItemRecord>,
    plan: &mut SyncPlan,
) {
    let mut iids: BTreeSet<&str> = BTreeSet::new();
    iids.extend(l.keys().copied());
    iids.extend(b.keys().copied());
    iids.extend(r.keys().copied());
    for iid in iids {
        match (l.get(iid), b.get(iid), r.get(iid)) {
            (Some(lr), _, Some(rr)) => {
                if lr == rr {
                    continue;
                }
                let (merged, rep) = merge_records(lr, rr);
                if rep.changed_local() || lr.clocks.is_none() {
                    plan.actions.push(SyncAction::UpsertItem {
                        vault_id: vid.to_string(),
                        item_id: iid.to_string(),
                        record: merged,
                    });
                }
                if rep.has_conflicts() {
                    let mut fields: Vec<String> = rep.conflicts.iter().map(|f| f.to_string()).collect();
                    fields.extend(rep.ext_conflicts.iter().cloned());
                    plan.conflicts.push(SyncConflict {
                        vault_id: vid.to_string(),
                        item_id: Some(iid.to_string()),
                        kind: SyncConflictKind::EqualClock { fields },
                    });
                }
            }
            (Some(lr), Some(br), None) => {
                // Remote deleted it.
                if lr == br {
                    plan.actions.push(SyncAction::DeleteItem { vault_id: vid.to_string(), item_id: iid.to_string() });
                } else {
                    plan.conflicts.push(SyncConflict {
                        vault_id: vid.to_string(),
                        item_id: Some(iid.to_string()),
                        kind: SyncConflictKind::DeleteVsEdit,
                    });
                }
            }
            (Some(_lr), None, None) => {
                // Local-only item: already present locally, nothing to apply;
                // the remote side adopts it from the pushed merged tree.
            }
            (None, Some(br), Some(rr)) => {
                // Local deleted it.
                if rr == br {
                    // Remote untouched: the delete stands; nothing to do locally.
                } else {
                    plan.conflicts.push(SyncConflict {
                        vault_id: vid.to_string(),
                        item_id: Some(iid.to_string()),
                        kind: SyncConflictKind::EditVsDelete,
                    });
                    plan.actions.push(SyncAction::UpsertItem {
                        vault_id: vid.to_string(),
                        item_id: iid.to_string(),
                        record: (*rr).clone(),
                    });
                }
            }
            (None, Some(_), None) => {} // Deleted on both sides.
            (None, None, Some(rr)) => {
                plan.actions.push(SyncAction::UpsertItem {
                    vault_id: vid.to_string(),
                    item_id: iid.to_string(),
                    record: (*rr).clone(),
                });
            }
            (None, None, None) => {}
        }
    }
}

/// Three-way merge for opaque vault blobs (attachments, member envelopes):
/// byte equality decides; a delete wins over an untouched side, a delete
/// against a changed side keeps the changed bytes and reports a conflict.
fn plan_blobs(
    vid: &str,
    l: &BTreeMap<&str, &[u8]>,
    b: &BTreeMap<&str, &[u8]>,
    r: &BTreeMap<&str, &[u8]>,
    plan: &mut SyncPlan,
) {
    let mut paths: BTreeSet<&str> = BTreeSet::new();
    paths.extend(l.keys().copied());
    paths.extend(b.keys().copied());
    paths.extend(r.keys().copied());
    for path in paths {
        match (l.get(path), b.get(path), r.get(path)) {
            (Some(lb), _, Some(rb)) => {
                if lb == rb {
                    continue;
                }
                match b.get(path) {
                    // Only remote changed it: adopt the remote bytes.
                    Some(bb) if *bb == *lb => plan.actions.push(SyncAction::UpsertBlob {
                        vault_id: vid.to_string(),
                        path: path.to_string(),
                        bytes: rb.to_vec(),
                    }),
                    // Only local changed it: keep local, nothing to do.
                    Some(bb) if *bb == *rb => {}
                    // Both changed it differently (or both added different
                    // bytes): blobs carry no clocks, so keep local.
                    _ => plan.conflicts.push(SyncConflict {
                        vault_id: vid.to_string(),
                        item_id: None,
                        kind: SyncConflictKind::Blob,
                    }),
                }
            }
            (Some(lb), Some(bb), None) => {
                if lb == bb {
                    plan.actions.push(SyncAction::DeleteBlob { vault_id: vid.to_string(), path: path.to_string() });
                } else {
                    plan.conflicts.push(SyncConflict {
                        vault_id: vid.to_string(),
                        item_id: None,
                        kind: SyncConflictKind::Blob,
                    });
                }
            }
            (Some(_), None, None) => {}
            (None, Some(bb), Some(rb)) => {
                if rb == bb {
                    // Local deleted it, remote untouched: stays deleted.
                } else {
                    plan.conflicts.push(SyncConflict {
                        vault_id: vid.to_string(),
                        item_id: None,
                        kind: SyncConflictKind::Blob,
                    });
                    plan.actions.push(SyncAction::UpsertBlob {
                        vault_id: vid.to_string(),
                        path: path.to_string(),
                        bytes: rb.to_vec(),
                    });
                }
            }
            (None, Some(_), None) => {}
            (None, None, Some(rb)) => {
                plan.actions.push(SyncAction::UpsertBlob {
                    vault_id: vid.to_string(),
                    path: path.to_string(),
                    bytes: rb.to_vec(),
                });
            }
            (None, None, None) => {}
        }
    }
}

// ---------- remote repositories ----------

/// One entry of a remote tree listing: repo-relative path and blob id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: String,
    /// Git blob id (sha1 of `blob <len>\0<bytes>`), verified by transports
    /// against the fetched bytes.
    pub sha: String,
}

/// The minimal remote-repo surface the sync engine needs. Transports
/// implement it — the git CLI (#452 follow-up), the GitHub API (#453), or a
/// test fake. Ids are opaque strings; `create_commit` takes the full tree
/// (path -> bytes) so API transports can create blobs + tree + commit without
/// a local checkout. `update_ref` is a compare-and-swap: it must fail when
/// the current head is not `expected_old`, so a concurrent push is never
/// silently overwritten (never force-push).
pub trait RemoteRepo {
    /// Current head commit id, or None for an empty remote.
    fn head(&self) -> Result<Option<String>>;
    /// (path, blob id) for every blob in the tree at `commit`.
    fn list_tree(&self, commit: &str) -> Result<Vec<TreeEntry>>;
    /// Blob bytes for `sha`; the caller verifies the id matches the bytes.
    fn get_blob(&self, sha: &str) -> Result<Vec<u8>>;
    /// Create a commit with `parent` (None for a root commit) whose tree is
    /// exactly `tree`, and return the new commit id.
    fn create_commit(&mut self, parent: Option<&str>, tree: &[(String, Vec<u8>)], message: &str) -> Result<String>;
    /// Move the head to `new_head`, failing unless the current head equals
    /// `expected_old` (None = the remote must currently be empty).
    fn update_ref(&mut self, new_head: &str, expected_old: Option<&str>) -> Result<()>;
}

/// Git blob id: sha1 of `blob <len>\0<bytes>`. Same function the Android
/// first-pull client uses (`gitBlobSha`); transports verify every fetched
/// blob against the id the tree advertised.
pub fn git_blob_sha(bytes: &[u8]) -> String {
    use sha1::Digest;
    let mut h = sha1::Sha1::new();
    h.update(format!("blob {}\0", bytes.len()).as_bytes());
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn git_tree_sha(entries: &[(String, String)]) -> String {
    // entries: (path, blob sha), sorted by path; tree object format is
    // `100644 <path>\0<20-byte sha>` per entry.
    use sha1::Digest;
    let mut sorted = entries.to_vec();
    sorted.sort();
    let mut body = Vec::new();
    for (path, sha) in &sorted {
        body.extend_from_slice(format!("100644 {path}\0").as_bytes());
        let raw = (0..sha.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&sha[i..i + 2], 16).unwrap_or(0))
            .collect::<Vec<u8>>();
        body.extend_from_slice(&raw);
    }
    let mut h = sha1::Sha1::new();
    h.update(format!("tree {}\0", body.len()).as_bytes());
    h.update(&body);
    format!("{:x}", h.finalize())
}

fn git_commit_sha(tree_sha: &str, parent: Option<&str>, message: &str) -> String {
    use sha1::Digest;
    let mut body = format!("tree {tree_sha}\n");
    if let Some(p) = parent {
        body.push_str(&format!("parent {p}\n"));
    }
    body.push_str("author gitpass <gitpass@localhost> 0 +0000\n");
    body.push_str("committer gitpass <gitpass@localhost> 0 +0000\n");
    body.push_str(&format!("\n{message}"));
    let mut h = sha1::Sha1::new();
    h.update(format!("commit {}\0", body.len()).as_bytes());
    h.update(body.as_bytes());
    format!("{:x}", h.finalize())
}

struct MemoryCommit {
    tree: Vec<TreeEntry>,
}

/// In-memory [`RemoteRepo`] for engine tests: content-addressed blobs and
/// commits with real git object ids, so tests exercise the same id
/// verification transports do. Not for production use.
#[derive(Default)]
pub struct MemoryRemoteRepo {
    blobs: BTreeMap<String, Vec<u8>>,
    commits: BTreeMap<String, MemoryCommit>,
    head: Option<String>,
}

impl MemoryRemoteRepo {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RemoteRepo for MemoryRemoteRepo {
    fn head(&self) -> Result<Option<String>> {
        Ok(self.head.clone())
    }

    fn list_tree(&self, commit: &str) -> Result<Vec<TreeEntry>> {
        let c = self
            .commits
            .get(commit)
            .ok_or_else(|| anyhow::anyhow!("unknown commit {commit}"))?;
        Ok(c.tree.clone())
    }

    fn get_blob(&self, sha: &str) -> Result<Vec<u8>> {
        let b = self.blobs.get(sha).ok_or_else(|| anyhow::anyhow!("unknown blob {sha}"))?;
        if git_blob_sha(b) != sha {
            bail!("blob {sha} failed identity check");
        }
        Ok(b.clone())
    }

    fn create_commit(&mut self, parent: Option<&str>, tree: &[(String, Vec<u8>)], message: &str) -> Result<String> {
        if let Some(p) = parent {
            if !self.commits.contains_key(p) {
                bail!("unknown parent commit {p}");
            }
        }
        let mut entries: Vec<TreeEntry> = vec![];
        for (path, bytes) in tree {
            if !is_allowed_path(path) {
                bail!("refusing to store non-vault path {path}");
            }
            let sha = git_blob_sha(bytes);
            self.blobs.insert(sha.clone(), bytes.clone());
            entries.push(TreeEntry { path: path.clone(), sha });
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let tree_sha = git_tree_sha(&entries.iter().map(|e| (e.path.clone(), e.sha.clone())).collect::<Vec<_>>());
        let id = git_commit_sha(&tree_sha, parent, message);
        self.commits.insert(id.clone(), MemoryCommit { tree: entries });
        Ok(id)
    }

    fn update_ref(&mut self, new_head: &str, expected_old: Option<&str>) -> Result<()> {
        if !self.commits.contains_key(new_head) {
            bail!("unknown commit {new_head}");
        }
        if self.head.as_deref() != expected_old {
            bail!(
                "ref update refused: head is {:?}, expected {:?}; another writer moved the ref (never force-push)",
                self.head,
                expected_old
            );
        }
        self.head = Some(new_head.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{Device, DeviceRegistry};
    use crate::gen_history::{GeneratorHistory, GeneratorHistoryEntry};
    use crate::merge::FieldClocks;
    use crate::store::{Item, ItemEntry, VaultEntry};

    fn meta(name: &str) -> VaultMeta {
        VaultMeta { name: name.into(), description: None, icon: None, color: None }
    }

    fn record(title: &str, password: &str, clock: u64) -> ItemRecord {
        let mut r = ItemRecord::new(Item {
            title: title.into(),
            username: Some("user".into()),
            password: Some(password.into()),
            url: None,
            notes: None,
            totp: None,
            updated_at: "1700000000".into(),
        });
        r.clocks = Some(FieldClocks::all(clock));
        r
    }

    fn record_title(title: &str, clock: u64) -> ItemRecord {
        record(title, "pw", clock)
    }

    /// Build a snapshot with the given epoch and vaults. Each vault is
    /// (vault_id, &[(item_id, record)]); meta is the vault id, key blob is
    /// `key-<vault_id>`.
    fn snap(epoch: u64, vaults: &[(&str, &[(&str, ItemRecord)])]) -> VaultSnapshot {
        let mut manifest = Manifest {
            format: 5,
            account_id: "acct-test".into(),
            epoch,
            keyset_hash: "k".into(),
            vaults: BTreeMap::new(),
            devices_hash: None,
            identity: None,
            generator_history_hash: None,
        };
        let mut items = BTreeMap::new();
        let mut metas = BTreeMap::new();
        let mut keys = BTreeMap::new();
        for (vid, vitems) in vaults {
            let mut ve = VaultEntry {
                key_version: 1,
                key_hash: "kh".into(),
                meta_hash: "mh".into(),
                items: BTreeMap::new(),
                members: BTreeMap::new(),
                files: BTreeMap::new(),
            };
            for (iid, rec) in *vitems {
                ve.items.insert(iid.to_string(), ItemEntry { version: 1, hash: "h".into() });
                items.insert((vid.to_string(), iid.to_string()), rec.clone());
            }
            manifest.vaults.insert(vid.to_string(), ve);
            metas.insert(vid.to_string(), meta(vid));
            keys.insert(vid.to_string(), format!("key-{vid}").into_bytes());
        }
        VaultSnapshot {
            manifest,
            items,
            vault_meta: metas,
            vault_key_blobs: keys,
            vault_blobs: BTreeMap::new(),
            generator_history: GeneratorHistory::default(),
            devices: None,
            identity: None,
        }
    }

    fn upserts(plan: &SyncPlan) -> Vec<(&str, &str)> {
        plan.actions
            .iter()
            .filter_map(|a| match a {
                SyncAction::UpsertItem { vault_id, item_id, .. } => Some((vault_id.as_str(), item_id.as_str())),
                _ => None,
            })
            .collect()
    }

    fn has_delete_item(plan: &SyncPlan, vid: &str, iid: &str) -> bool {
        plan.actions.iter().any(|a| matches!(a, SyncAction::DeleteItem { vault_id: v, item_id: i, .. } if v == vid && i == iid))
    }

    fn has_conflict(plan: &SyncPlan, kind: &SyncConflictKind) -> bool {
        plan.conflicts.iter().any(|c| &c.kind == kind)
    }

    #[test]
    fn fast_forward_adopts_remote() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let local = base.clone();
        let mut remote = base.clone();
        remote.manifest.epoch = 2;
        remote.items.insert(("v1".into(), "b".into()), record_title("B", 1000));
        remote.manifest.vaults.get_mut("v1").unwrap().items.insert("b".into(), ItemEntry { version: 1, hash: "h".into() });
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert!(plan.fast_forward);
        assert!(plan.changed);
        assert!(plan.local_changes);
        assert_eq!(upserts(&plan), vec![("v1", "b")]);
        assert_eq!(plan.new_epoch, 3);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn local_ahead_needs_push_but_no_local_change() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let mut local = base.clone();
        local.manifest.epoch = 2;
        local.items.insert(("v1".into(), "b".into()), record_title("B", 1000));
        local.manifest.vaults.get_mut("v1").unwrap().items.insert("b".into(), ItemEntry { version: 1, hash: "h".into() });
        let remote = base.clone();
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert!(!plan.fast_forward);
        assert!(plan.changed); // the push still has to happen
        assert!(!plan.local_changes); // ...but nothing to write locally
        assert!(plan.actions.is_empty());
        assert_eq!(plan.new_epoch, 2); // no re-seal
    }

    #[test]
    fn identical_snapshots_are_a_no_op() {
        let s = snap(5, &[("v1", &[("a", record_title("A", 1000))])]);
        let plan = plan_sync(&s, &s, &s, 1700000000).unwrap();
        assert!(!plan.changed);
        assert!(!plan.local_changes);
        assert_eq!(plan.new_epoch, 5);
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn diverged_edits_merge_by_field_clock() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let mut local = base.clone();
        let mut lrec = record("A", "local-pw", 2000);
        lrec.clocks = Some(FieldClocks { password: 2000, ..FieldClocks::all(1000) });
        local.items.insert(("v1".into(), "a".into()), lrec);
        let mut remote = base.clone();
        let mut rrec = record("Remote title", "pw", 2000);
        rrec.clocks = Some(FieldClocks { title: 2000, ..FieldClocks::all(1000) });
        remote.items.insert(("v1".into(), "a".into()), rrec);
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert_eq!(upserts(&plan), vec![("v1", "a")]);
        assert!(plan.conflicts.is_empty());
        let merged = match &plan.actions[0] {
            SyncAction::UpsertItem { record, .. } => record,
            _ => panic!("expected upsert"),
        };
        assert_eq!(merged.item.title, "Remote title");
        assert_eq!(merged.item.password.as_deref(), Some("local-pw"));
    }

    #[test]
    fn remote_delete_of_untouched_item_propagates() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000)), ("b", record_title("B", 1000))])]);
        let local = base.clone();
        let mut remote = base.clone();
        remote.items.remove(&("v1".into(), "b".into()));
        remote.manifest.vaults.get_mut("v1").unwrap().items.remove("b");
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert!(has_delete_item(&plan, "v1", "b"));
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn local_delete_of_untouched_item_stands() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000)), ("b", record_title("B", 1000))])]);
        let mut local = base.clone();
        local.items.remove(&("v1".into(), "b".into()));
        local.manifest.vaults.get_mut("v1").unwrap().items.remove("b");
        let remote = base.clone();
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert!(!has_delete_item(&plan, "v1", "b"));
        assert!(plan.conflicts.is_empty());
        assert!(plan.changed); // the delete still has to reach the remote
        assert!(!plan.local_changes);
    }

    #[test]
    fn local_delete_vs_remote_edit_keeps_remote() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let mut local = base.clone();
        local.items.remove(&("v1".into(), "a".into()));
        local.manifest.vaults.get_mut("v1").unwrap().items.remove("a");
        let mut remote = base.clone();
        remote.items.insert(("v1".into(), "a".into()), record("A edited", "pw", 2000));
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert!(has_conflict(&plan, &SyncConflictKind::EditVsDelete));
        assert_eq!(upserts(&plan), vec![("v1", "a")]); // edited copy resurrected
    }

    #[test]
    fn remote_delete_vs_local_edit_keeps_local() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let mut local = base.clone();
        local.items.insert(("v1".into(), "a".into()), record("A edited", "pw", 2000));
        let mut remote = base.clone();
        remote.items.remove(&("v1".into(), "a".into()));
        remote.manifest.vaults.get_mut("v1").unwrap().items.remove("a");
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert!(has_conflict(&plan, &SyncConflictKind::DeleteVsEdit));
        assert!(upserts(&plan).is_empty());
        assert!(!has_delete_item(&plan, "v1", "a"));
    }

    #[test]
    fn both_deleted_stays_deleted() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let mut local = base.clone();
        local.items.remove(&("v1".into(), "a".into()));
        local.manifest.vaults.get_mut("v1").unwrap().items.remove("a");
        let mut remote = base.clone();
        remote.items.remove(&("v1".into(), "a".into()));
        remote.manifest.vaults.get_mut("v1").unwrap().items.remove("a");
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert!(plan.actions.is_empty());
        assert!(plan.conflicts.is_empty());
        assert!(!plan.changed);
    }

    #[test]
    fn local_only_vault_needs_push_not_create() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let mut local = base.clone();
        local.manifest.vaults.insert(
            "v2".into(),
            VaultEntry { key_version: 1, key_hash: "kh".into(), meta_hash: "mh".into(), items: [("x".to_string(), ItemEntry { version: 1, hash: "h".into() })].into(), members: BTreeMap::new(), files: BTreeMap::new() },
        );
        local.vault_meta.insert("v2".into(), meta("Second"));
        local.vault_key_blobs.insert("v2".into(), b"key-v2".to_vec());
        local.items.insert(("v2".into(), "x".into()), record_title("X", 1000));
        let remote = base.clone();
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        // The vault already exists locally: no local action, but the push
        // still has to carry it to the remote side.
        assert!(!plan.actions.iter().any(|a| matches!(a, SyncAction::CreateVault { .. })));
        assert!(plan.changed);
        assert!(!plan.local_changes);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn remote_only_vault_is_created() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let local = base.clone();
        let mut remote = base.clone();
        remote.manifest.vaults.insert(
            "v2".into(),
            VaultEntry { key_version: 1, key_hash: "kh".into(), meta_hash: "mh".into(), items: [("x".to_string(), ItemEntry { version: 1, hash: "h".into() })].into(), members: BTreeMap::new(), files: BTreeMap::new() },
        );
        remote.vault_meta.insert("v2".into(), meta("Second"));
        remote.vault_key_blobs.insert("v2".into(), b"key-v2".to_vec());
        remote.items.insert(("v2".into(), "x".into()), record_title("X", 1000));
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert!(plan.actions.iter().any(|a| matches!(a, SyncAction::CreateVault { vault_id, .. } if vault_id == "v2")));
    }

    #[test]
    fn equal_clock_conflict_keeps_local() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let mut local = base.clone();
        local.items.insert(("v1".into(), "a".into()), record("Local title", "pw", 2000));
        let mut remote = base.clone();
        remote.items.insert(("v1".into(), "a".into()), record("Remote title", "pw", 2000));
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert!(plan.conflicts.iter().any(|c| matches!(&c.kind, SyncConflictKind::EqualClock { fields } if fields.iter().any(|f| f == "title"))));
        assert!(upserts(&plan).is_empty()); // merged == local, nothing to write
        assert!(plan.changed); // remote still holds the other title
    }

    #[test]
    fn epoch_is_strictly_above_both_sides() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let local = snap(10, &[("v1", &[("a", record_title("A", 1000))])]);
        let mut remote = snap(7, &[("v1", &[("a", record_title("A", 1000))])]);
        remote.items.insert(("v1".into(), "b".into()), record_title("B", 1000));
        remote.manifest.vaults.get_mut("v1").unwrap().items.insert("b".into(), ItemEntry { version: 1, hash: "h".into() });
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert_eq!(plan.new_epoch, 11);
    }

    #[test]
    fn cross_account_snapshots_are_refused() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let local = base.clone();
        let mut remote = base.clone();
        remote.manifest.account_id = "other-acct".into();
        assert!(plan_sync(&local, &base, &remote, 1700000000).is_err());
    }

    #[test]
    fn remote_key_rotation_is_adopted_on_clean_pull() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let local = base.clone();
        let mut remote = base.clone();
        remote.vault_key_blobs.insert("v1".into(), b"key-v1-rotated".to_vec());
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert!(plan.actions.iter().any(|a| matches!(a, SyncAction::SetVaultKey { vault_id, .. } if vault_id == "v1")));
        // Meta is re-sealed under the new key, and the item is re-sealed too.
        assert!(plan.actions.iter().any(|a| matches!(a, SyncAction::UpdateVaultMeta { vault_id, .. } if vault_id == "v1")));
        assert_eq!(upserts(&plan), vec![("v1", "a")]);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn key_rotation_against_local_edits_conflicts() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let mut local = base.clone();
        local.items.insert(("v1".into(), "a".into()), record("A edited", "pw", 2000));
        let mut remote = base.clone();
        remote.vault_key_blobs.insert("v1".into(), b"key-v1-rotated".to_vec());
        let plan = plan_sync(&local, &base, &remote, 1700000000).unwrap();
        assert!(has_conflict(&plan, &SyncConflictKind::VaultKey));
        assert!(!plan.actions.iter().any(|a| matches!(a, SyncAction::SetVaultKey { .. })));
    }

    #[test]
    fn generator_history_merges() {
        let base = snap(1, &[("v1", &[("a", record_title("A", 1000))])]);
        let now = 1700000000u64;
        let mut local = base.clone();
        local.generator_history.entries.push(GeneratorHistoryEntry {
            id: "e1".into(),
            value: "local-pw".into(),
            created_at: now - 200,
            source: "cli".into(),
            host: None,
        });
        let mut remote = base.clone();
        remote.generator_history.entries.push(GeneratorHistoryEntry {
            id: "e2".into(),
            value: "remote-pw".into(),
            created_at: now - 100,
            source: "desktop".into(),
            host: None,
        });
        let plan = plan_sync(&local, &base, &remote, now).unwrap();
        let merged = plan.generator_history.expect("merged history");
        assert_eq!(merged.entries.len(), 2);
        assert_eq!(merged.entries[0].id, "e2");
        assert_eq!(merged.entries[1].id, "e1");
    }

    #[test]
    fn device_registries_merge_with_revoked_wins() {
        fn dev(id: &str, name: &str, seen: &str, revoked: bool) -> Device {
            Device {
                id: id.into(),
                name: name.into(),
                created_at: "2026-01-01T00:00:00Z".into(),
                last_seen_at: seen.into(),
                revoked,
                revoked_at: revoked.then(|| "2026-01-02T00:00:00Z".into()),
                public_key: None,
            }
        }
        let l = DeviceRegistry { format: 3, devices: vec![dev("d1", "mac", "2026-01-01T00:00:00Z", false)] };
        let r = DeviceRegistry {
            format: 3,
            devices: vec![
                dev("d1", "mac", "2026-01-03T00:00:00Z", true),
                dev("d2", "phone", "2026-01-02T00:00:00Z", false),
            ],
        };
        let merged = merge_device_registries(Some(&l), Some(&r)).unwrap();
        assert_eq!(merged.devices.len(), 2);
        let d1 = merged.get("d1").unwrap();
        assert!(d1.revoked);
        assert_eq!(d1.last_seen_at, "2026-01-03T00:00:00Z");
        assert!(merged.get("d2").is_some());
        assert!(merge_device_registries(None, Some(&r)).unwrap() == r);
        assert!(merge_device_registries(None, None).is_none());
    }

    #[test]
    fn allowed_path_is_the_single_core_rule() {
        for p in ["manifest.enc", "devices.enc", "vaults/v1/items/a.enc", ".gitignore", "format.json"] {
            assert!(is_allowed_path(p), "{p}");
        }
        for p in ["secret.key", ".gitpass-state.json", "notes.txt", "../escape.enc", "vaults/v1/items/a.json"] {
            assert!(!is_allowed_path(p), "{p}");
        }
    }

    #[test]
    fn memory_repo_round_trip_and_cas_ref_update() {
        let mut repo = MemoryRemoteRepo::new();
        assert_eq!(repo.head().unwrap(), None);
        let tree = vec![
            ("manifest.enc".to_string(), b"manifest-bytes".to_vec()),
            ("vaults/v1/items/a.enc".to_string(), b"item-bytes".to_vec()),
        ];
        let c1 = repo.create_commit(None, &tree, "sync").unwrap();
        assert_eq!(repo.head().unwrap(), None); // head moves only via update_ref
        repo.update_ref(&c1, None).unwrap();
        assert_eq!(repo.head().unwrap(), Some(c1.clone()));
        let listed = repo.list_tree(&c1).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(repo.get_blob(&listed[0].sha).unwrap(), b"manifest-bytes");
        assert_eq!(git_blob_sha(b"manifest-bytes"), listed[0].sha);
        // A stale expected_old is refused (never force-push).
        let c2 = repo.create_commit(Some(&c1), &tree, "sync 2").unwrap();
        assert!(repo.update_ref(&c2, Some("stale-head")).is_err());
        repo.update_ref(&c2, Some(&c1)).unwrap();
        assert_eq!(repo.head().unwrap(), Some(c2.clone()));
        // Unknown blobs/commits and non-vault paths are refused.
        assert!(repo.get_blob("0000000000000000000000000000000000000000").is_err());
        assert!(repo.list_tree("0000000000000000000000000000000000000000").is_err());
        assert!(repo
            .create_commit(Some(&c2), &[("secret.key".to_string(), b"x".to_vec())], "bad")
            .is_err());
    }
}
