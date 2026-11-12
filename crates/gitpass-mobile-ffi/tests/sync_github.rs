//! Mobile two-way sync through the bridge (#455): `Vault::sync_github` on the
//! shared core engine, with an in-process fake of the GitHub Git Data API as
//! the app-supplied [`HttpTransport`]. No network, never a real repository,
//! fake token, temp-dir vaults with fake data. Test vaults use the KDF floor
//! (8192 KiB, t=1) only to keep the suite fast; production KDF parameters are
//! untouched (`open_vault` never upgrades or lowers them).
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use gitpass_core::crypto::KdfParams;
use gitpass_core::sync_engine::{git_blob_sha, git_tree_id};
use gitpass_mobile_ffi::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const PW: &str = "correct horse battery";
const TOKEN: &str = "github_pat_FAKE_TEST_TOKEN_0123456789";

#[derive(Default)]
struct State {
    blobs: HashMap<String, Vec<u8>>,
    trees: HashMap<String, Vec<(String, String)>>,
    commits: HashMap<String, (String, Vec<String>)>,
    head: Option<String>,
    counter: u64,
    status_override: Option<u16>,
    fail: Option<HttpTransportError>,
    requests: Vec<String>,
}

impl State {
    fn new_commit(&mut self, tree: &str, parents: Vec<String>) -> String {
        self.counter += 1;
        let sha = git_blob_sha(format!("commit {tree} {parents:?} {}", self.counter).as_bytes());
        self.commits.insert(sha.clone(), (tree.to_string(), parents));
        sha
    }
    fn is_ancestor(&self, anc: &str, of: &str) -> bool {
        let mut stack = vec![of.to_string()];
        while let Some(c) = stack.pop() {
            if c == anc {
                return true;
            }
            if let Some((_, ps)) = self.commits.get(&c) {
                stack.extend(ps.iter().cloned());
            }
        }
        false
    }
}

/// Fake GitHub behind the app transport interface.
#[derive(Clone, Default)]
struct FakeGitHub(Arc<Mutex<State>>);

fn ok(status: u16, v: Value) -> Result<HttpTransportResponse, HttpTransportError> {
    Ok(HttpTransportResponse { status, body: serde_json::to_vec(&v).unwrap(), retry_after: None, rate_limit_remaining: None, rate_limit_reset: None })
}
fn status(code: u16) -> Result<HttpTransportResponse, HttpTransportError> {
    // Noisy body: it must never reach an error message.
    ok(code, json!({"message": "BODY_SHOULD_NOT_LEAK"}))
}

impl HttpTransport for FakeGitHub {
    fn send(&self, req: HttpTransportRequest) -> Result<HttpTransportResponse, HttpTransportError> {
        assert!(!format!("{req:?}").contains("FAKE_TEST_TOKEN"), "token must never be in a request record");
        assert!(!req.path.contains("FAKE_TEST_TOKEN"), "token must never be in a URL");
        assert!(req.path.starts_with("/repos/"));
        assert!(req.max_response_bytes > 0 && req.timeout_ms > 0);
        let mut s = self.0.lock().unwrap();
        s.requests.push(format!("{} {}", req.method, req.path));
        if let Some(e) = s.fail {
            return Err(e);
        }
        if let Some(code) = s.status_override {
            return status(code);
        }
        let body: Value = req.body.as_ref().map(|b| serde_json::from_slice(b).unwrap()).unwrap_or(Value::Null);
        let path = req.path.strip_prefix("/repos/o/vault").expect("fixed test repo only").to_string();
        let empty = s.commits.is_empty();
        match (req.method.as_str(), path.as_str()) {
            ("GET", "") => ok(200, json!({"full_name": "o/vault"})),
            ("GET", "/git/ref/heads/main") => match (&s.head, empty) {
                (Some(h), _) => ok(200, json!({"ref": "refs/heads/main", "object": {"type": "commit", "sha": h}})),
                (None, true) => status(409),
                (None, false) => status(404),
            },
            ("GET", p) if p.starts_with("/git/commits/") => {
                let sha = &p["/git/commits/".len()..];
                match s.commits.get(sha) {
                    Some((tree, parents)) => ok(200, json!({"sha": sha, "tree": {"sha": tree}, "parents": parents.iter().map(|p| json!({"sha": p})).collect::<Vec<_>>()})),
                    None => status(404),
                }
            }
            ("GET", p) if p.starts_with("/git/trees/") => {
                let sha = p["/git/trees/".len()..].strip_suffix("?recursive=1").expect("recursive listing");
                let Some(entries) = s.trees.get(sha) else { return status(404) };
                let mut dirs = std::collections::BTreeSet::new();
                let mut out = vec![];
                for (path, blob) in entries {
                    let parts: Vec<_> = path.split('/').collect();
                    let mut acc = String::new();
                    for c in &parts[..parts.len() - 1] {
                        if !acc.is_empty() {
                            acc.push('/');
                        }
                        acc.push_str(c);
                        dirs.insert(acc.clone());
                    }
                    out.push(json!({"path": path, "mode": "100644", "type": "blob", "sha": blob, "size": s.blobs[blob].len()}));
                }
                for d in dirs {
                    out.push(json!({"path": d, "mode": "040000", "type": "tree", "sha": git_blob_sha(d.as_bytes())}));
                }
                ok(200, json!({"sha": sha, "truncated": false, "tree": out}))
            }
            ("GET", p) if p.starts_with("/git/blobs/") => {
                let sha = &p["/git/blobs/".len()..];
                let Some(b) = s.blobs.get(sha) else { return status(404) };
                let enc = B64.encode(b);
                let wrapped: Vec<String> = enc.as_bytes().chunks(60).map(|c| String::from_utf8(c.to_vec()).unwrap()).collect();
                ok(200, json!({"sha": sha, "encoding": "base64", "size": b.len(), "content": wrapped.join("\n")}))
            }
            ("POST", "/git/blobs") => {
                if empty {
                    return status(409);
                }
                let bytes = B64.decode(body["content"].as_str().unwrap()).unwrap();
                let sha = git_blob_sha(&bytes);
                s.blobs.insert(sha.clone(), bytes);
                ok(201, json!({"sha": sha}))
            }
            ("POST", "/git/trees") => {
                let mut pairs = vec![];
                for e in body["tree"].as_array().unwrap() {
                    assert_eq!(e["mode"], "100644");
                    let sha = e["sha"].as_str().unwrap().to_string();
                    if !s.blobs.contains_key(&sha) {
                        return status(422);
                    }
                    pairs.push((e["path"].as_str().unwrap().to_string(), sha));
                }
                let id = git_tree_id(&pairs).unwrap();
                s.trees.insert(id.clone(), pairs);
                ok(201, json!({"sha": id}))
            }
            ("POST", "/git/commits") => {
                let tree = body["tree"].as_str().unwrap().to_string();
                let parents: Vec<String> = body["parents"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect();
                if !s.trees.contains_key(&tree) || parents.iter().any(|p| !s.commits.contains_key(p)) {
                    return status(422);
                }
                let sha = s.new_commit(&tree, parents.clone());
                ok(201, json!({"sha": sha, "tree": {"sha": tree}, "parents": parents.iter().map(|p| json!({"sha": p})).collect::<Vec<_>>()}))
            }
            ("PUT", "/contents/format.json") => {
                if s.head.is_some() {
                    return status(422);
                }
                let bytes = B64.decode(body["content"].as_str().unwrap()).unwrap();
                let blob = git_blob_sha(&bytes);
                s.blobs.insert(blob.clone(), bytes);
                let pairs = vec![("format.json".to_string(), blob.clone())];
                let tree = git_tree_id(&pairs).unwrap();
                s.trees.insert(tree.clone(), pairs);
                let c = s.new_commit(&tree, vec![]);
                s.head = Some(c.clone());
                ok(201, json!({"content": {"sha": blob}, "commit": {"sha": c}}))
            }
            ("PATCH", "/git/refs/heads/main") => {
                assert_eq!(body["force"], json!(false), "never force");
                let new = body["sha"].as_str().unwrap().to_string();
                let cur = s.head.clone().unwrap();
                if !s.commits.contains_key(&new) || !s.is_ancestor(&cur, &new) {
                    return status(422);
                }
                s.head = Some(new.clone());
                ok(200, json!({"object": {"sha": new}}))
            }
            (m, p) => panic!("unexpected request {m} {p}"),
        }
    }
}

fn fast() -> KdfParams {
    KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 }
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for e in fs::read_dir(src).unwrap().filter_map(|e| e.ok()) {
        let (s, d) = (e.path(), dst.join(e.file_name()));
        if e.file_type().unwrap().is_dir() {
            copy_dir(&s, &d);
        } else {
            fs::copy(&s, &d).unwrap();
        }
    }
}

fn login(title: &str, pw: &str) -> ItemInput {
    ItemInput {
        title: title.into(),
        username: Some("user".into()),
        password: Some(pw.into()),
        url: None,
        notes: None,
        totp: None,
        category: "login".into(),
        tags: vec![],
        favorite: false,
        sections: vec![],
    }
}

fn target() -> GitHubSyncTarget {
    GitHubSyncTarget { owner: "o".into(), repo: "vault".into(), branch: "main".into() }
}

fn open(root: &Path, sk: &str) -> Arc<Vault> {
    open_vault(root.to_str().unwrap().into(), PW.into(), sk.into()).unwrap()
}

fn sync(v: &Vault, gh: &FakeGitHub) -> Result<GitHubSyncReport, GitpassError> {
    v.sync_github(target(), TOKEN.into(), Arc::new(gh.clone()), SyncCancellation::new())
}

fn msg(e: GitpassError) -> String {
    let GitpassError::Generic(m) = e;
    assert!(!m.contains("FAKE_TEST_TOKEN") && !m.contains("BODY_SHOULD_NOT_LEAK") && !m.contains("/tmp"), "leak: {m}");
    m
}

struct Devices {
    _dir: tempfile::TempDir,
    a: PathBuf,
    b: PathBuf,
    sk: String,
    vid: String,
    one: String,
    two: String,
    gh: FakeGitHub,
}

/// Device A (a fresh vault with two items) syncs to an empty fake repo; B is a
/// copy of A's files without local sync state, like a first-pulled phone.
fn devices() -> Devices {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    fs::create_dir_all(&a).unwrap();
    let sk = gitpass_core::store::init(&a, PW, fast()).unwrap().encode();
    let va = open(&a, &sk);
    let vid = va.create_vault("Personal".into(), None).unwrap();
    let one = va.create_item(vid.clone(), login("One", "pw-one")).unwrap();
    let two = va.create_item(vid.clone(), login("Two", "pw-two")).unwrap();
    let gh = FakeGitHub::default();
    let r = sync(&va, &gh).unwrap();
    assert!(r.pushed, "first sync initializes the empty repository");
    drop(va);
    let b = dir.path().join("b");
    copy_dir(&a, &b);
    fs::remove_file(b.join(".gitpass-state.json")).unwrap();
    Devices { _dir: dir, a, b, sk, vid, one, two, gh }
}

#[test]
fn first_sync_from_a_pulled_copy_is_a_no_op() {
    let d = devices();
    let before = d.gh.0.lock().unwrap().head.clone();
    let r = sync(&open(&d.b, &d.sk), &d.gh).unwrap();
    assert!(!r.pushed && !r.local_changed && r.conflicts.is_empty(), "{r:?}");
    assert_eq!(d.gh.0.lock().unwrap().head, before);
    assert_eq!(Some(r.head), before);
}

#[test]
fn edits_and_deletes_flow_both_ways() {
    let d = devices();
    // Phone (B): edit One, trash Two, add Three; sync.
    let vb = open(&d.b, &d.sk);
    sync(&vb, &d.gh).unwrap();
    vb.update_item(d.vid.clone(), d.one.clone(), login("One", "pw-one-phone")).unwrap();
    vb.delete_item(d.vid.clone(), d.two.clone()).unwrap();
    let three = vb.create_item(d.vid.clone(), login("Three", "pw-three")).unwrap();
    let r = sync(&vb, &d.gh).unwrap();
    assert!(r.pushed && r.conflicts.is_empty(), "{r:?}");
    assert!(r.local_only_items >= 1);

    // Desktop-like device (A) syncs: everything arrives.
    let va = open(&d.a, &d.sk);
    let r = sync(&va, &d.gh).unwrap();
    assert!(r.local_changed && !r.pushed, "{r:?}");
    assert_eq!(va.get_item(d.vid.clone(), d.one.clone()).unwrap().password.as_deref(), Some("pw-one-phone"));
    assert!(va.get_item(d.vid.clone(), d.two.clone()).unwrap().deleted);
    assert_eq!(va.get_item(d.vid.clone(), three.clone()).unwrap().title, "Three");

    // A restores Two and edits Three; the phone receives both.
    va.restore_item(d.vid.clone(), d.two.clone()).unwrap();
    va.update_item(d.vid.clone(), three.clone(), login("Three", "pw-three-desk")).unwrap();
    assert!(sync(&va, &d.gh).unwrap().pushed);
    let r = sync(&vb, &d.gh).unwrap();
    assert!(r.local_changed, "{r:?}");
    assert!(!vb.get_item(d.vid.clone(), d.two.clone()).unwrap().deleted);
    assert_eq!(vb.get_item(d.vid.clone(), three).unwrap().password.as_deref(), Some("pw-three-desk"));
}

#[test]
fn desktop_delete_vs_phone_edit_keeps_the_edit_and_reports_it() {
    let d = devices();
    let vb = open(&d.b, &d.sk);
    sync(&vb, &d.gh).unwrap();
    // Desktop (A) permanently deletes One and syncs (desktop `item rm`).
    {
        let sk = gitpass_core::crypto::SecretKey::parse(&d.sk).unwrap();
        let opts = gitpass_core::store::UnlockOpts { accept_rollback: false, target_kdf: fast() };
        let s = gitpass_core::store::Session::unlock_with_effects(&d.a, PW, &sk, &opts, None, gitpass_core::store::UnlockEffects::NONE).unwrap();
        s.remove_item(&d.vid, &d.one).unwrap();
    }
    assert!(sync(&open(&d.a, &d.sk), &d.gh).unwrap().pushed);
    // Phone edited One meanwhile.
    vb.update_item(d.vid.clone(), d.one.clone(), login("One", "pw-phone")).unwrap();
    let r = sync(&vb, &d.gh).unwrap();
    assert_eq!(r.conflicts.len(), 1, "{r:?}");
    assert!(r.conflicts[0].contains(&d.one) && r.conflicts[0].contains("kept local") && !r.conflicts[0].contains("pw-"), "{:?}", r.conflicts);
    assert_eq!(vb.get_item(d.vid.clone(), d.one.clone()).unwrap().password.as_deref(), Some("pw-phone"));
    assert!(r.pushed);
    // The edit wins on the desktop too.
    let va = open(&d.a, &d.sk);
    sync(&va, &d.gh).unwrap();
    assert_eq!(va.get_item(d.vid.clone(), d.one.clone()).unwrap().password.as_deref(), Some("pw-phone"));
    let _ = &d.two;
}

#[test]
fn errors_are_fixed_text_and_cancellation_stops_before_any_request() {
    let d = devices();
    let vb = open(&d.b, &d.sk);
    let n = d.gh.0.lock().unwrap().requests.len();

    let c = SyncCancellation::new();
    c.cancel();
    let m = msg(vb.sync_github(target(), TOKEN.into(), Arc::new(d.gh.clone()), c).unwrap_err());
    assert!(m.contains("cancelled"), "{m}");
    assert_eq!(d.gh.0.lock().unwrap().requests.len(), n, "no request after cancel");

    // Classic tokens are refused before any request (fine-grained PAT only).
    let m = msg(vb.sync_github(target(), "ghp_classic".into(), Arc::new(d.gh.clone()), SyncCancellation::new()).unwrap_err());
    assert!(m.contains("invalid GitHub sync settings"), "{m}");
    let bad = GitHubSyncTarget { owner: "o/../x".into(), repo: "vault".into(), branch: "main".into() };
    assert!(msg(vb.sync_github(bad, TOKEN.into(), Arc::new(d.gh.clone()), SyncCancellation::new()).unwrap_err()).contains("invalid"));
    assert_eq!(d.gh.0.lock().unwrap().requests.len(), n);

    d.gh.0.lock().unwrap().status_override = Some(401);
    assert!(msg(sync(&vb, &d.gh).unwrap_err()).contains("token"));
    d.gh.0.lock().unwrap().status_override = Some(404);
    assert!(msg(sync(&vb, &d.gh).unwrap_err()).contains("access"));
    d.gh.0.lock().unwrap().status_override = None;
    d.gh.0.lock().unwrap().fail = Some(HttpTransportError::Transport);
    assert!(msg(sync(&vb, &d.gh).unwrap_err()).contains("network"));
    d.gh.0.lock().unwrap().fail = Some(HttpTransportError::Redirect);
    assert!(msg(sync(&vb, &d.gh).unwrap_err()).contains("redirect"));
    d.gh.0.lock().unwrap().fail = None;
    // Still works afterwards, nothing half-applied.
    assert!(sync(&vb, &d.gh).unwrap().conflicts.is_empty());
}

#[test]
fn foreign_file_on_the_remote_is_refused_without_local_changes() {
    let d = devices();
    {
        let mut s = d.gh.0.lock().unwrap();
        let head = s.head.clone().unwrap();
        let (tree, _) = s.commits[&head].clone();
        let mut pairs = s.trees[&tree].clone();
        let blob = git_blob_sha(b"hello");
        s.blobs.insert(blob.clone(), b"hello".to_vec());
        pairs.push(("notes.txt".into(), blob));
        let t = git_tree_id(&pairs).unwrap();
        s.trees.insert(t.clone(), pairs);
        let c = s.new_commit(&t, vec![head]);
        s.head = Some(c);
    }
    let vb = open(&d.b, &d.sk);
    let before = vb.list_items(d.vid.clone()).unwrap().len();
    let m = msg(sync(&vb, &d.gh).unwrap_err());
    assert!(m.contains("refusing"), "{m}");
    assert_eq!(vb.list_items(d.vid.clone()).unwrap().len(), before);
}

fn pull(root: &Path, gh: &FakeGitHub) -> Result<String, GitpassError> {
    pull_github_snapshot(root.to_str().unwrap().into(), target(), TOKEN.into(), Arc::new(gh.clone()), SyncCancellation::new())
}

#[test]
fn first_pull_downloads_a_verifiable_snapshot_that_syncs_as_a_no_op() {
    let d = devices();
    let c = d._dir.path().join("pulled");
    let head = pull(&c, &d.gh).unwrap();
    assert_eq!(Some(head.clone()), d.gh.0.lock().unwrap().head);
    assert!(!c.join(".gitpass-state.json").exists(), "local state is never pulled");
    validate_first_pull_snapshot(c.to_str().unwrap().into(), PW.into(), d.sk.clone()).unwrap();
    let vc = open(&c, &d.sk);
    assert_eq!(vc.list_items(d.vid.clone()).unwrap().len(), 2);
    let r = sync(&vc, &d.gh).unwrap();
    assert!(!r.pushed && !r.local_changed && r.conflicts.is_empty(), "{r:?}");
}

#[test]
fn first_pull_is_read_only_and_refuses_unsafe_destinations_and_remotes() {
    let d = devices();
    let writes_before = d.gh.0.lock().unwrap().requests.iter().filter(|r| !r.starts_with("GET")).count();
    // An existing vault (or any non-empty directory) is never overwritten.
    let before = fs::read(d.b.join("manifest.enc")).unwrap();
    assert!(msg(pull(&d.b, &d.gh).unwrap_err()).contains("refusing"));
    assert_eq!(fs::read(d.b.join("manifest.enc")).unwrap(), before);
    // An empty repository: nothing to pull, nothing written.
    let empty = FakeGitHub::default();
    let e = d._dir.path().join("empty");
    assert!(msg(pull(&e, &empty).unwrap_err()).contains("no vault"));
    assert!(!e.exists());
    // Cancelled before start: no request.
    let n = d.gh.0.lock().unwrap().requests.len();
    let cancel = SyncCancellation::new();
    cancel.cancel();
    let x = d._dir.path().join("x");
    let m = msg(pull_github_snapshot(x.to_str().unwrap().into(), target(), TOKEN.into(), Arc::new(d.gh.clone()), cancel).unwrap_err());
    assert!(m.contains("cancelled"), "{m}");
    assert_eq!(d.gh.0.lock().unwrap().requests.len(), n);
    assert!(!x.exists());
    // Classic token refused up front.
    let m = msg(pull_github_snapshot(x.to_str().unwrap().into(), target(), "ghp_classic".into(), Arc::new(d.gh.clone()), SyncCancellation::new()).unwrap_err());
    assert!(m.contains("invalid"), "{m}");
    // A foreign file on the remote: refused, nothing written.
    {
        let mut s = d.gh.0.lock().unwrap();
        let head = s.head.clone().unwrap();
        let (tree, _) = s.commits[&head].clone();
        let mut pairs = s.trees[&tree].clone();
        let blob = git_blob_sha(b"hello");
        s.blobs.insert(blob.clone(), b"hello".to_vec());
        pairs.push(("notes.txt".into(), blob));
        let t = git_tree_id(&pairs).unwrap();
        s.trees.insert(t.clone(), pairs);
        let c = s.new_commit(&t, vec![head]);
        s.head = Some(c);
    }
    let f = d._dir.path().join("foreign");
    assert!(msg(pull(&f, &d.gh).unwrap_err()).contains("refusing"));
    assert!(!f.exists());
    let writes_after = d.gh.0.lock().unwrap().requests.iter().filter(|r| !r.starts_with("GET")).count();
    assert_eq!(writes_before, writes_after, "first pull never writes to the remote");
}
