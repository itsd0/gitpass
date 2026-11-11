//! Shared encrypted cross-platform test vault fixtures (issue #451).
//! Every committed fixture must open (read-only) and decrypt to exactly
//! its committed `expected.json`, and re-running the generator must
//! reproduce that expected JSON. All data is FAKE; see
//! `tests/fixtures/shared/README.md`.
use gitpass_core::test_fixtures;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn shared_dir() -> std::path::PathBuf {
    test_fixtures::shared_fixtures_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
}

#[test]
fn committed_fixtures_match_expected() {
    let base = shared_dir();
    let secret = test_fixtures::fixture_secret_key();
    for v in test_fixtures::VERSIONS {
        let dir = base.join(test_fixtures::dir_name(v));
        let want: Value = serde_json::from_str(
            &fs::read_to_string(dir.join("expected.json")).unwrap(),
        )
        .unwrap();
        // Copy before opening: unlocking mutates device state and may
        // re-wrap the keyset; the committed fixture stays pristine.
        let tmp = tempfile::tempdir().unwrap();
        let copy = tmp.path().join(test_fixtures::dir_name(v));
        test_fixtures::copy_fixture(&dir, &copy).unwrap();
        let got = test_fixtures::dump_fixture(&copy, test_fixtures::FIXTURE_PASSWORD, &secret)
            .unwrap();
        assert_eq!(got["format_version"], json!(v), "v{v} format version");
        assert_eq!(got, want, "v{v} decrypted content must match expected.json");
    }
}

#[test]
fn generator_output_matches_committed() {
    // Regenerating must reproduce the committed expected.json for every
    // version. This is the CI drift check: if the generator or the
    // format changes, regenerate the fixtures and commit the result.
    test_fixtures::check_all(&shared_dir()).unwrap();
}

#[test]
fn fixtures_are_self_describing() {
    // The README documents the test-only credentials and the
    // regeneration command; a fixture without it is easy to mistake
    // for real vault data.
    let readme = fs::read_to_string(shared_dir().join("README.md")).unwrap();
    assert!(readme.contains(test_fixtures::FIXTURE_PASSWORD));
    assert!(readme.contains("test-only"));
    for v in test_fixtures::VERSIONS {
        let dir = shared_dir().join(test_fixtures::dir_name(v));
        assert!(dir.join("format.json").exists(), "v{v} format.json");
        assert!(dir.join("keyset.enc").exists(), "v{v} keyset.enc");
        assert!(dir.join("manifest.enc").exists(), "v{v} manifest.enc");
        assert!(dir.join("secret.key.txt").exists(), "v{v} secret.key.txt");
        // No device-local state is committed.
        for n in [
            ".gitpass.lock",
            ".gitpass-device.json",
            ".gitpass-device-key.json",
            ".gitpass-state.json",
            ".gitpass-throttle.json",
        ] {
            assert!(!dir.join(n).exists(), "v{v} must not commit {n}");
        }
    }
}

/// The shared allowed-path table (#452) matches core's single rule; the
/// bridge, Android and iOS tests check the same table through the bridge.
#[test]
fn shared_allowed_path_table_matches_core_rule() {
    let table: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(shared_dir().join("allowed_paths.json")).unwrap()).unwrap();
    let allowed = table["allowed"].as_array().unwrap();
    let rejected = table["rejected"].as_array().unwrap();
    assert!(allowed.len() >= 10 && rejected.len() >= 20);
    for p in allowed {
        let p = p.as_str().unwrap();
        assert!(gitpass_core::remote::is_allowed_path(p), "must allow {p:?}");
    }
    for p in rejected {
        let p = p.as_str().unwrap();
        assert!(!gitpass_core::remote::is_allowed_path(p), "must reject {p:?}");
    }
}
