use gitpass_core::crypto::KdfParams;
use gitpass_core::gen_history::GEN_HISTORY_FILE;
use gitpass_core::store::{self, Session};
use tempfile::tempdir;

fn fast() -> KdfParams { KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 } }

#[test]
fn test_session_generator_history_lifecycle() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("vault");
    let secret = store::init(&root, "test-master-pw-1", fast()).unwrap();
    let s = Session::unlock(&root, "test-master-pw-1", &secret).unwrap();

    // Initially no generator_history.enc file
    assert!(!root.join(GEN_HISTORY_FILE).exists());
    let h0 = s.read_generator_history().unwrap();
    assert!(h0.entries.is_empty());
    assert!(s.load_manifest().unwrap().generator_history_hash.is_none());

    // Record a password from desktop
    let e1 = s.record_generated_password("FAKE-pass-1", "desktop", None).unwrap();
    assert_eq!(e1.value, "FAKE-pass-1");
    assert_eq!(e1.source, "desktop");
    assert!(root.join(GEN_HISTORY_FILE).exists());

    // Manifest binds generator_history_hash and verifies
    let m1 = s.load_manifest().unwrap();
    assert!(m1.generator_history_hash.is_some());
    assert!(store::verify(&root, &m1).is_empty());

    // Record a password from extension with host
    let e2 = s.record_generated_password("FAKE-pass-2", "extension", Some("github.com".into())).unwrap();
    assert_eq!(e2.value, "FAKE-pass-2");
    assert_eq!(e2.source, "extension");
    assert_eq!(e2.host.as_deref(), Some("github.com"));

    // Read back: newest first (e2, then e1)
    let h1 = s.read_generator_history().unwrap();
    assert_eq!(h1.entries.len(), 2);
    assert_eq!(h1.entries[0].id, e2.id);
    assert_eq!(h1.entries[0].value, "FAKE-pass-2");
    assert_eq!(h1.entries[1].id, e1.id);
    assert_eq!(h1.entries[1].value, "FAKE-pass-1");

    // Delete single entry
    assert!(s.delete_generator_history_entry(&e1.id).unwrap());
    let h2 = s.read_generator_history().unwrap();
    assert_eq!(h2.entries.len(), 1);
    assert_eq!(h2.entries[0].id, e2.id);
    assert!(store::verify(&root, &s.load_manifest().unwrap()).is_empty());

    // Clear history
    s.clear_generator_history().unwrap();
    let h3 = s.read_generator_history().unwrap();
    assert!(h3.entries.is_empty());
    assert!(h3.cleared_at.is_some());
    assert!(store::verify(&root, &s.load_manifest().unwrap()).is_empty());
}

#[test]
fn test_generator_history_survives_account_key_rotation() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("vault");
    let secret = store::init(&root, "test-master-pw-1", fast()).unwrap();
    let mut s = Session::unlock(&root, "test-master-pw-1", &secret).unwrap();

    s.record_generated_password("FAKE-key-rotate-pass", "cli", None).unwrap();
    assert_eq!(s.read_generator_history().unwrap().entries.len(), 1);

    // Rotate account key
    s.rotate_account_key("test-master-pw-1", &secret).unwrap();

    // Can still read generator history after rotation
    let h = s.read_generator_history().unwrap();
    assert_eq!(h.entries.len(), 1);
    assert_eq!(h.entries[0].value, "FAKE-key-rotate-pass");
    assert!(store::verify(&root, &s.load_manifest().unwrap()).is_empty());
}

#[test]
fn test_tampered_generator_history_rejected() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("vault");
    let secret = store::init(&root, "test-master-pw-1", fast()).unwrap();
    let s = Session::unlock(&root, "test-master-pw-1", &secret).unwrap();
    s.record_generated_password("FAKE-tamper-pass", "desktop", None).unwrap();

    // Tamper with generator_history.enc file
    std::fs::write(root.join(GEN_HISTORY_FILE), b"corrupted payload").unwrap();
    let problems = store::verify(&root, &s.load_manifest().unwrap());
    assert!(!problems.is_empty());
    assert!(problems[0].contains("generator_history.enc does not match manifest hash"));
}
