//! #450: version handshake and side-effect-free mobile unlock.
//!
//! All vaults are temporary and every secret is fake.

use gitpass_core::crypto::KdfParams;
use gitpass_core::store::{self, Session, UnlockOpts};
use gitpass_mobile_ffi::{
    default_unlock_options, ffi_version, format_version, open_vault, open_vault_with, FfiVersion,
    FormatVersion, UnlockOptions, FFI_API_LEVEL,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const PASSWORD: &str = "FAKE correct horse battery staple";

/// Local, untracked per-device state that an unlock may legitimately write: unlock throttle counter,
/// last-seen manifest epoch (anti-rollback), and the local device id. None of them is synced.
const LOCAL_STATE_ALLOWLIST: &[&str] = &[
    gitpass_core::unlock_throttle::THROTTLE_FILE,
    ".gitpass-state.json",
    gitpass_core::device::LOCAL_DEVICE_FILE,
];

/// Weaker-than-default but still above the core floor: lets the test prove that a default unlock does
/// NOT upgrade the KDF (only the opt-in path does).
fn weak_kdf() -> KdfParams {
    KdfParams {
        alg: "argon2id".into(),
        m_kib: 8192,
        t: 1,
        p: 1,
    }
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                walk(root, &path, out);
            } else if !LOCAL_STATE_ALLOWLIST.contains(&name.as_str()) {
                out.insert(name, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// A vault that a full (desktop-style) unlock WOULD change: weak KDF, an item in the trash for longer than
/// the 30-day purge window, and a device registry. Returns (root dir guard, root, secret, vault id, item id).
fn maintenance_due_vault() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    String,
    String,
    String,
) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let sk = store::init(&root, PASSWORD, weak_kdf()).unwrap();
    let opts = UnlockOpts {
        accept_rollback: false,
        target_kdf: weak_kdf(),
    };
    let s = Session::unlock_device(&root, PASSWORD, &sk, &opts, Some("FAKE desktop")).unwrap();
    let vid = s
        .create_vault_full("FAKE Personal", None, None, None)
        .unwrap();
    let mut item = store::Item::default();
    item.title = "FAKE old trash".into();
    let rec = gitpass_core::ItemRecord::new(item);
    let iid = s.add_record(&vid, rec).unwrap();
    let (_v, mut r) = s.read_record(&vid, &iid).unwrap();
    r.deleted_at = Some(store::now_secs() - 31 * 24 * 3600);
    s.update_record(&vid, &iid, r).unwrap();
    drop(s);
    (dir, root, sk.encode(), vid, iid)
}

#[test]
fn ffi_version_matches_workspace_version() {
    let manifest =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml")).unwrap();
    let workspace_version = manifest
        .split("[workspace.package]")
        .nth(1)
        .and_then(|sec| sec.lines().find(|l| l.trim_start().starts_with("version")))
        .and_then(|l| l.split('"').nth(1))
        .expect("workspace.package.version");
    assert_eq!(
        ffi_version(),
        FfiVersion {
            crate_version: workspace_version.to_string(),
            api_level: FFI_API_LEVEL
        }
    );
    assert_eq!(FFI_API_LEVEL, 3);
}

#[test]
fn format_version_matches_core_constants() {
    assert_eq!(
        format_version(),
        FormatVersion {
            min_read: gitpass_core::MIN_READ_FORMAT,
            current_write: gitpass_core::FORMAT_VERSION
        }
    );
    assert!(format_version().min_read <= format_version().current_write);
}

#[test]
fn default_mobile_unlock_leaves_every_vault_file_byte_identical() {
    assert_eq!(default_unlock_options(), UnlockOptions::default());
    let (_dir, root, secret, vid, iid) = maintenance_due_vault();
    let before = snapshot(&root);
    assert!(before.contains_key("devices.enc") && before.contains_key("keyset.enc"));
    let root_s = root.to_str().unwrap().to_string();

    // Both entry points: the plain `open_vault` and an explicit default options record.
    let v = open_vault(root_s.clone(), PASSWORD.into(), secret.clone()).unwrap();
    // The expired trash item is still readable: nothing was purged.
    assert!(v.get_item(vid.clone(), iid.clone()).unwrap().deleted);
    drop(v);
    assert_eq!(
        before,
        snapshot(&root),
        "open_vault must not write any tracked vault file"
    );

    let v = open_vault_with(
        root_s.clone(),
        PASSWORD.into(),
        secret.clone(),
        default_unlock_options(),
    )
    .unwrap();
    drop(v);
    assert_eq!(
        before,
        snapshot(&root),
        "default unlock options must not write any tracked vault file"
    );

    // A phone that has never unlocked this vault (no local device id yet) must not register itself either.
    fs::remove_file(root.join(gitpass_core::device::LOCAL_DEVICE_FILE)).unwrap();
    drop(open_vault(root_s, PASSWORD.into(), secret).unwrap());
    assert_eq!(
        before,
        snapshot(&root),
        "first unlock on a new device must not write any tracked vault file"
    );
    let fmt: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("format.json")).unwrap()).unwrap();
    assert_eq!(
        fmt["kdf"]["m_kib"], 8192,
        "KDF must not be upgraded by a default mobile unlock"
    );
}

#[test]
fn opt_in_options_still_perform_maintenance() {
    let (_dir, root, secret, vid, iid) = maintenance_due_vault();
    // Unlock as a phone that has never been registered (no local device id yet).
    fs::remove_file(root.join(gitpass_core::device::LOCAL_DEVICE_FILE)).unwrap();
    let before = snapshot(&root);
    let options = UnlockOptions {
        register_device: true,
        device_name: Some("FAKE phone".into()),
        purge_expired_trash: true,
        upgrade_kdf: true,
    };
    let v = open_vault_with(
        root.to_str().unwrap().into(),
        PASSWORD.into(),
        secret,
        options,
    )
    .unwrap();
    assert!(
        v.get_item(vid, iid).is_err(),
        "expired trash is purged when opted in"
    );
    drop(v);
    let after = snapshot(&root);
    assert_ne!(
        before.get("devices.enc"),
        after.get("devices.enc"),
        "device registered when opted in"
    );
    assert_ne!(
        before.get("keyset.enc"),
        after.get("keyset.enc"),
        "keyset re-wrapped when opted in"
    );
    let fmt: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("format.json")).unwrap()).unwrap();
    // Upgraded to (at least) the core default; never lowered.
    assert!(fmt["kdf"]["m_kib"].as_u64().unwrap() >= u64::from(KdfParams::default().m_kib));
}

#[test]
fn side_effect_free_unlock_still_rejects_rollback() {
    let (_dir, root, secret, vid, _iid) = maintenance_due_vault();
    let root_s = root.to_str().unwrap().to_string();
    // Capture an older manifest, then advance the vault and record the newer epoch on this device.
    let old_manifest = fs::read(root.join("manifest.enc")).unwrap();
    let v = open_vault(root_s.clone(), PASSWORD.into(), secret.clone()).unwrap();
    v.create_item(
        vid,
        gitpass_mobile_ffi::ItemInput {
            title: "FAKE newer".into(),
            username: None,
            password: None,
            url: None,
            notes: None,
            totp: None,
            category: "login".into(),
            tags: vec![],
            favorite: false,
            sections: vec![],
        },
    )
    .unwrap();
    drop(v);
    open_vault(root_s.clone(), PASSWORD.into(), secret.clone()).unwrap();
    // Roll the manifest back: the side-effect-free path must refuse, exactly like the full unlock.
    fs::write(root.join("manifest.enc"), old_manifest).unwrap();
    let e = open_vault(root_s, PASSWORD.into(), secret)
        .err()
        .expect("rollback must be refused");
    assert!(e.to_string().contains("integrity check FAILED"), "{e}");
}

#[test]
fn side_effect_free_unlock_still_counts_failures_for_throttling() {
    let (_dir, root, secret, _vid, _iid) = maintenance_due_vault();
    let root_s = root.to_str().unwrap().to_string();
    let e = open_vault(root_s.clone(), "FAKE wrong password".into(), secret.clone())
        .err()
        .unwrap();
    // Errors never echo the password or Secret Key.
    assert!(!e.to_string().contains("FAKE wrong password") && !e.to_string().contains(&secret));
    let throttle =
        fs::read_to_string(root.join(gitpass_core::unlock_throttle::THROTTLE_FILE)).unwrap();
    let rec: serde_json::Value = serde_json::from_str(&throttle).unwrap();
    assert_eq!(
        rec["failures"], 1,
        "failed side-effect-free unlock must be throttled"
    );
    open_vault(root_s, PASSWORD.into(), secret).unwrap();
}

#[test]
fn each_unlock_option_is_independent() {
    let (_dir, root, secret, vid, iid) = maintenance_due_vault();
    fs::remove_file(root.join(gitpass_core::device::LOCAL_DEVICE_FILE)).unwrap();
    let before = snapshot(&root);
    let only_purge = UnlockOptions {
        purge_expired_trash: true,
        ..UnlockOptions::default()
    };
    let v = open_vault_with(
        root.to_str().unwrap().into(),
        PASSWORD.into(),
        secret,
        only_purge,
    )
    .unwrap();
    assert!(v.get_item(vid, iid).is_err(), "purge ran");
    drop(v);
    let after = snapshot(&root);
    assert_eq!(
        before.get("devices.enc"),
        after.get("devices.enc"),
        "no device registration"
    );
    assert_eq!(
        before.get("keyset.enc"),
        after.get("keyset.enc"),
        "no KDF upgrade"
    );
    assert_eq!(
        before.get("format.json"),
        after.get("format.json"),
        "no KDF upgrade"
    );
}
