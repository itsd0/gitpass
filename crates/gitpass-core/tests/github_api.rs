//! GitHub API transport + API sync driver tests (issue #453). Everything runs
//! against an in-process fake of the GitHub Git Data API ([`MockGitHub`]):
//! no network, never a real repository, fake token, temp-dir vaults with
//! fake data. Test vaults use the KDF floor (8192 KiB, t=1) only to keep the
//! suite fast; production KDF parameters are untouched.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use gitpass_core::crypto::KdfParams;
use gitpass_core::github_api::*;
use gitpass_core::store::{self, Item, Session};
use gitpass_core::sync_engine::{git_blob_sha, git_tree_id, RemoteRepo};
use gitpass_core::sync_remote::{local_vault_files, sync_with_remote, SyncOptions};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const PW: &str = "correct horse battery";
const TOKEN: &str = "github_pat_FAKE_TEST_TOKEN_0123456789";

// ---------- fake GitHub ----------

type Hook = Box<dyn FnMut(&mut State)>;

#[derive(Default)]
struct State {
    blobs: HashMap<String, Vec<u8>>,
    trees: HashMap<String, Vec<(String, String)>>,
    commits: HashMap<String, (String, Vec<String>)>,
    head: Option<String>,
    counter: u64,
    // Fault injection.
    truncated: bool,
    corrupt_blobs: bool,
    oversize_blob_size: bool,
    redirect: bool,
    rate_limited: bool,
    fail: Option<HttpError>,
    extra_entry: Option<(String, String)>, // (path, mode) added to tree listings
    before_patch: Option<Hook>,
    requests: Vec<String>,
}

#[derive(Clone, Default)]
struct MockGitHub(Rc<RefCell<State>>);

fn ok(status: u16, v: Value) -> Result<HttpResponse, HttpError> {
    Ok(HttpResponse { status, body: serde_json::to_vec(&v).unwrap(), ..Default::default() })
}
fn status(code: u16) -> Result<HttpResponse, HttpError> {
    // Bodies are deliberately "noisy": the transport must never echo them.
    ok(code, json!({"message": "BODY_SHOULD_NOT_LEAK"}))
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
    fn files_at(&self, commit: &str) -> Vec<(String, Vec<u8>)> {
        let (tree, _) = &self.commits[commit];
        let mut v: Vec<_> = self.trees[tree].iter().map(|(p, s)| (p.clone(), self.blobs[s].clone())).collect();
        v.sort();
        v
    }
}

impl HttpClient for MockGitHub {
    fn send(&self, req: &HttpRequest<'_>) -> Result<HttpResponse, HttpError> {
        assert_eq!(req.authorization, format!("Bearer {TOKEN}"));
        assert!(!req.path.contains("FAKE_TEST_TOKEN"), "token must never be in a URL");
        assert!(req.path.starts_with("/repos/"));
        if req.cancel.load(Ordering::SeqCst) {
            return Err(HttpError::Cancelled);
        }
        let mut s = self.0.borrow_mut();
        s.requests.push(format!("{} {}", req.method.as_str(), req.path));
        if let Some(e) = s.fail {
            return Err(e);
        }
        if s.redirect {
            return status(302);
        }
        if s.rate_limited {
            return Ok(HttpResponse { status: 403, body: b"{}".to_vec(), retry_after: Some(60), rate_limit_remaining: Some(0), ..Default::default() });
        }
        let body: Value = req.body.map(|b| serde_json::from_slice(b).unwrap()).unwrap_or(Value::Null);
        let path = req.path.strip_prefix("/repos/o/vault").expect("fixed test repo only");
        let empty = s.commits.is_empty();
        let resp = match (req.method, path) {
            (HttpMethod::Get, "") => ok(200, json!({"full_name": "o/vault"})),
            (HttpMethod::Get, "/git/ref/heads/main") => match (&s.head, empty) {
                (Some(h), _) => ok(200, json!({"ref": "refs/heads/main", "object": {"type": "commit", "sha": h}})),
                (None, true) => status(409),
                (None, false) => status(404),
            },
            (HttpMethod::Get, p) if p.starts_with("/git/commits/") => {
                let sha = &p["/git/commits/".len()..];
                match s.commits.get(sha) {
                    Some((tree, parents)) => ok(200, json!({"sha": sha, "tree": {"sha": tree}, "parents": parents.iter().map(|p| json!({"sha": p})).collect::<Vec<_>>()})),
                    None => status(404),
                }
            }
            (HttpMethod::Get, p) if p.starts_with("/git/trees/") => {
                let sha = p["/git/trees/".len()..].strip_suffix("?recursive=1").expect("recursive listing");
                let Some(entries) = s.trees.get(sha) else { return status(404) };
                let mut dirs = std::collections::BTreeSet::new();
                let mut out = vec![];
                for (path, blob) in entries {
                    let mut acc = String::new();
                    for c in path.split('/').collect::<Vec<_>>().split_last().unwrap().1 {
                        if !acc.is_empty() {
                            acc.push('/');
                        }
                        acc.push_str(c);
                        dirs.insert(acc.clone());
                    }
                    let size = if s.oversize_blob_size { 64 * 1024 * 1024 } else { s.blobs[blob].len() };
                    out.push(json!({"path": path, "mode": "100644", "type": "blob", "sha": blob, "size": size}));
                }
                for d in dirs {
                    out.push(json!({"path": d, "mode": "040000", "type": "tree", "sha": git_blob_sha(d.as_bytes())}));
                }
                if let Some((p, mode)) = &s.extra_entry {
                    out.push(json!({"path": p, "mode": mode, "type": "blob", "sha": git_blob_sha(b"x"), "size": 1}));
                }
                ok(200, json!({"sha": sha, "truncated": s.truncated, "tree": out}))
            }
            (HttpMethod::Get, p) if p.starts_with("/git/blobs/") => {
                let sha = &p["/git/blobs/".len()..];
                let Some(b) = s.blobs.get(sha) else { return status(404) };
                let mut bytes = b.clone();
                if s.corrupt_blobs && !bytes.is_empty() {
                    bytes[0] ^= 1;
                }
                // GitHub wraps base64 at 60 columns with LF.
                let enc = B64.encode(&bytes);
                let wrapped: Vec<String> = enc.as_bytes().chunks(60).map(|c| String::from_utf8(c.to_vec()).unwrap()).collect();
                ok(200, json!({"sha": sha, "encoding": "base64", "size": b.len(), "content": wrapped.join("\n")}))
            }
            (HttpMethod::Post, "/git/blobs") => {
                if empty {
                    return status(409);
                }
                let bytes = B64.decode(body["content"].as_str().unwrap()).unwrap();
                let sha = git_blob_sha(&bytes);
                s.blobs.insert(sha.clone(), bytes);
                ok(201, json!({"sha": sha}))
            }
            (HttpMethod::Post, "/git/trees") => {
                if empty {
                    return status(409);
                }
                assert!(body.get("base_tree").is_none(), "full trees only");
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
            (HttpMethod::Post, "/git/commits") => {
                if empty {
                    return status(409);
                }
                let tree = body["tree"].as_str().unwrap().to_string();
                let parents: Vec<String> = body["parents"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect();
                if !s.trees.contains_key(&tree) || parents.iter().any(|p| !s.commits.contains_key(p)) {
                    return status(422);
                }
                let sha = s.new_commit(&tree, parents.clone());
                ok(201, json!({"sha": sha, "tree": {"sha": tree}, "parents": parents.iter().map(|p| json!({"sha": p})).collect::<Vec<_>>()}))
            }
            (HttpMethod::Put, "/contents/format.json") => {
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
            (HttpMethod::Post, "/git/refs") => {
                if s.head.is_some() {
                    return status(422);
                }
                let sha = body["sha"].as_str().unwrap().to_string();
                s.head = Some(sha.clone());
                ok(201, json!({"object": {"sha": sha}}))
            }
            (HttpMethod::Patch, "/git/refs/heads/main") => {
                assert_eq!(body["force"], json!(false), "never force");
                if let Some(mut hook) = s.before_patch.take() {
                    hook(&mut s);
                }
                let new = body["sha"].as_str().unwrap().to_string();
                let cur = s.head.clone().unwrap();
                if !s.commits.contains_key(&new) || !s.is_ancestor(&cur, &new) {
                    return status(422); // not a fast-forward
                }
                s.head = Some(new.clone());
                ok(200, json!({"object": {"sha": new}}))
            }
            (m, p) => panic!("unexpected request {m:?} {p}"),
        };
        resp
    }
}

fn api(mock: &MockGitHub) -> GitHubApi<MockGitHub> {
    api_with(mock, Limits::default(), Arc::new(AtomicBool::new(false)))
}
fn api_with(mock: &MockGitHub, limits: Limits, cancel: Arc<AtomicBool>) -> GitHubApi<MockGitHub> {
    GitHubApi::new(mock.clone(), TOKEN.into(), GitHubRepoRef::new("o", "vault", "main").unwrap(), limits, cancel).unwrap()
}

// ---------- vault helpers ----------

fn fast() -> KdfParams {
    KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 }
}
fn item(t: &str, pw: &str) -> Item {
    let mut i = Item::default();
    i.title = t.into();
    i.password = Some(pw.into());
    i.updated_at = "1700000000".into();
    i
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

struct Fixture {
    _dir: tempfile::TempDir,
    a: std::path::PathBuf,
    b: std::path::PathBuf,
    sk: gitpass_core::crypto::SecretKey,
    vid: String,
    items: Vec<String>,
    mock: MockGitHub,
}

/// Device A creates a vault with two items and syncs it to an empty fake
/// repo; device B is a fresh copy of A's files with no local sync state.
fn two_devices() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    fs::create_dir_all(&a).unwrap();
    let sk = store::init(&a, PW, fast()).unwrap();
    gitpass_core::remote::ensure_gitignore(&a).unwrap();
    let s = Session::unlock(&a, PW, &sk).unwrap();
    let vid = s.create_vault("Personal").unwrap();
    let i1 = s.add_item(&vid, &item("One", "pw-one")).unwrap();
    let i2 = s.add_item(&vid, &item("Two", "pw-two")).unwrap();
    let mock = MockGitHub::default();
    let mut repo = api(&mock);
    let id = repo.remote_id();
    let out = sync_with_remote(&s, &mut repo, &id, &SyncOptions::default()).unwrap();
    assert!(out.pushed);
    drop(s);
    let b = dir.path().join("b");
    copy_dir(&a, &b);
    fs::remove_file(b.join(".gitpass-state.json")).unwrap();
    Fixture { _dir: dir, a, b, sk, vid, items: vec![i1, i2], mock }
}

/// Unlock without maintenance writes (device touch, purge, KDF upgrade) so a
/// test sync only moves what the test changed.
fn open(root: &Path, sk: &gitpass_core::crypto::SecretKey) -> Session {
    Session::unlock_with_effects(root, PW, sk, &store::UnlockOpts { accept_rollback: false, target_kdf: fast() }, None, store::UnlockEffects::NONE).unwrap()
}

fn sync(root: &Path, sk: &gitpass_core::crypto::SecretKey, mock: &MockGitHub) -> anyhow::Result<gitpass_core::sync_remote::SyncOutcome> {
    let s = open(root, sk);
    let mut repo = api(mock);
    let id = repo.remote_id();
    sync_with_remote(&s, &mut repo, &id, &SyncOptions::default())
}

fn remote_files(mock: &MockGitHub) -> Vec<(String, Vec<u8>)> {
    let st = mock.0.borrow();
    st.files_at(st.head.as_ref().unwrap())
}

fn no_leak(e: &anyhow::Error) {
    let text = format!("{e:#} {e:?}");
    assert!(!text.contains("FAKE_TEST_TOKEN"), "token leaked: {text}");
    assert!(!text.contains("BODY_SHOULD_NOT_LEAK"), "body leaked: {text}");
    assert!(!text.contains("/tmp") && !text.contains(".tmp"), "path leaked: {text}");
}

// ---------- tests ----------

#[test]
fn happy_path_push_to_empty_repo_then_pull_and_delete_propagation() {
    let f = two_devices();
    // The empty repo was bootstrapped (format.json only) and then fast-forwarded to the full tree.
    {
        let st = f.mock.0.borrow();
        let head = st.head.clone().unwrap();
        assert_eq!(st.commits[&head].1.len(), 1, "real commit sits on the bootstrap commit");
        let boot = &st.commits[&head].1[0];
        assert_eq!(st.files_at(boot).iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), vec!["format.json"]);
    }
    assert_eq!(remote_files(&f.mock), local_vault_files(&f.a).unwrap());
    // Local-only state (sync base, epoch, device key, secret key) is never pushed.
    assert!(remote_files(&f.mock).iter().all(|(p, _)| !p.contains(".gitpass-") && p != "secret.key"));
    let st = fs::read_to_string(f.a.join(".gitpass-state.json")).unwrap();
    assert!(st.contains("github:o/vault#main") && !st.contains("FAKE_TEST_TOKEN"));

    // B: first sync without a recorded base is a union: nothing changes, base recorded.
    let out = sync(&f.b, &f.sk, &f.mock).unwrap();
    assert!(!out.pushed && !out.local_changed, "{out:?}");

    // A edits item 1 and hard-deletes item 2, syncs.
    let s = Session::unlock(&f.a, PW, &f.sk).unwrap();
    let mut one = s.read_item(&f.vid, &f.items[0]).unwrap().1;
    one.password = Some("pw-one-new".into());
    s.update_item(&f.vid, &f.items[0], &one).unwrap();
    s.remove_item(&f.vid, &f.items[1]).unwrap();
    drop(s);
    assert!(sync(&f.a, &f.sk, &f.mock).unwrap().pushed);

    // B pulls: the edit arrives and the delete propagates (recorded base).
    let out = sync(&f.b, &f.sk, &f.mock).unwrap();
    assert!(out.local_changed);
    let s = Session::unlock(&f.b, PW, &f.sk).unwrap();
    assert_eq!(s.read_item(&f.vid, &f.items[0]).unwrap().1.password.as_deref(), Some("pw-one-new"));
    assert!(s.read_record(&f.vid, &f.items[1]).is_err(), "delete must propagate");
    assert!(store::verify(&f.b, &s.load_manifest().unwrap()).is_empty());
    drop(s);
    // Converged: B's files are exactly the remote tree after one more no-op round.
    let out = sync(&f.b, &f.sk, &f.mock).unwrap();
    assert!(!out.local_changed);
    assert_eq!(remote_files(&f.mock), local_vault_files(&f.b).unwrap());
    let out = sync(&f.a, &f.sk, &f.mock).unwrap();
    assert!(!out.pushed);
    assert_eq!(local_vault_files(&f.a).unwrap(), local_vault_files(&f.b).unwrap());
}

#[test]
fn diverged_devices_merge_and_converge() {
    let f = two_devices();
    sync(&f.b, &f.sk, &f.mock).unwrap();
    let s = Session::unlock(&f.a, PW, &f.sk).unwrap();
    let mut one = s.read_item(&f.vid, &f.items[0]).unwrap().1;
    one.password = Some("from-a".into());
    s.update_item(&f.vid, &f.items[0], &one).unwrap();
    let a_new = s.add_item(&f.vid, &item("A-only", "x")).unwrap();
    drop(s);
    let s = Session::unlock(&f.b, PW, &f.sk).unwrap();
    let mut one = s.read_item(&f.vid, &f.items[0]).unwrap().1;
    one.title = "renamed-by-b".into();
    s.update_item(&f.vid, &f.items[0], &one).unwrap();
    let b_new = s.add_item(&f.vid, &item("B-only", "y")).unwrap();
    drop(s);

    assert!(sync(&f.a, &f.sk, &f.mock).unwrap().pushed);
    let out = sync(&f.b, &f.sk, &f.mock).unwrap();
    assert!(out.pushed && out.local_changed && out.conflicts.is_empty(), "{out:?}");
    sync(&f.a, &f.sk, &f.mock).unwrap();
    for root in [&f.a, &f.b] {
        let s = Session::unlock(root, PW, &f.sk).unwrap();
        let one = s.read_item(&f.vid, &f.items[0]).unwrap().1;
        assert_eq!((one.title.as_str(), one.password.as_deref()), ("renamed-by-b", Some("from-a")));
        assert!(s.read_item(&f.vid, &a_new).is_ok() && s.read_item(&f.vid, &b_new).is_ok());
    }
    assert_eq!(local_vault_files(&f.a).unwrap(), remote_files(&f.mock));
    assert_eq!(local_vault_files(&f.b).unwrap(), remote_files(&f.mock));
}

#[test]
fn rollback_to_an_older_remote_commit_is_refused() {
    let f = two_devices();
    sync(&f.b, &f.sk, &f.mock).unwrap();
    let old_head = f.mock.0.borrow().head.clone().unwrap();
    let s = Session::unlock(&f.a, PW, &f.sk).unwrap();
    s.add_item(&f.vid, &item("Three", "pw-three")).unwrap();
    drop(s);
    sync(&f.a, &f.sk, &f.mock).unwrap();
    sync(&f.b, &f.sk, &f.mock).unwrap();
    let b_before = local_vault_files(&f.b).unwrap();
    // An attacker (or a bad force-push elsewhere) rewinds the branch.
    f.mock.0.borrow_mut().head = Some(old_head);
    let e = sync(&f.b, &f.sk, &f.mock).unwrap_err();
    assert!(format!("{e:#}").contains("rollback"), "{e:#}");
    no_leak(&e);
    assert_eq!(local_vault_files(&f.b).unwrap(), b_before, "nothing applied");
    // An emptied remote after a recorded sync is refused too.
    {
        let mut st = f.mock.0.borrow_mut();
        st.head = None;
    }
    let e = sync(&f.b, &f.sk, &f.mock).unwrap_err();
    assert!(format!("{e:#}").contains("rollback"), "{e:#}");
}

#[test]
fn ref_update_race_refetches_and_replans() {
    let f = two_devices();
    sync(&f.b, &f.sk, &f.mock).unwrap();
    let base = f.mock.0.borrow().head.clone().unwrap();
    // A pushes a change; then the ref is put back so B still sees the old head,
    // and A's commit "lands" exactly while B's ref update is in flight.
    let s = Session::unlock(&f.a, PW, &f.sk).unwrap();
    let a_new = s.add_item(&f.vid, &item("A-concurrent", "x")).unwrap();
    drop(s);
    sync(&f.a, &f.sk, &f.mock).unwrap();
    let a_head = f.mock.0.borrow().head.clone().unwrap();
    {
        let mut st = f.mock.0.borrow_mut();
        st.head = Some(base);
        st.before_patch = Some(Box::new(move |st: &mut State| st.head = Some(a_head.clone())));
    }
    let s = Session::unlock(&f.b, PW, &f.sk).unwrap();
    let b_new = s.add_item(&f.vid, &item("B-concurrent", "y")).unwrap();
    drop(s);
    let out = sync(&f.b, &f.sk, &f.mock).unwrap();
    assert_eq!(out.attempts, 2, "first CAS must fail, second succeed");
    assert!(out.pushed);
    let s = Session::unlock(&f.b, PW, &f.sk).unwrap();
    assert!(s.read_item(&f.vid, &a_new).is_ok() && s.read_item(&f.vid, &b_new).is_ok());
    drop(s);
    assert_eq!(local_vault_files(&f.b).unwrap(), remote_files(&f.mock));
    // Every ref update was a non-forced PATCH (asserted by the mock).
}

#[test]
fn repeated_races_give_up_after_bounded_attempts() {
    let f = two_devices();
    let s = Session::unlock(&f.a, PW, &f.sk).unwrap();
    s.add_item(&f.vid, &item("Three", "x")).unwrap();
    let mut repo = api(&f.mock);
    let id = repo.remote_id();
    // Before every PATCH another writer moves the branch again.
    fn arm(m: &MockGitHub) {
        m.0.borrow_mut().before_patch = Some(Box::new(|st: &mut State| {
            let head = st.head.clone().unwrap();
            let tree = st.commits[&head].0.clone();
            let c = st.new_commit(&tree, vec![head]);
            st.head = Some(c);
        }));
    }
    let mut attempts = 0;
    let opts = SyncOptions { max_attempts: 2, ..Default::default() };
    // Re-arm the race for each attempt by wrapping the transport.
    struct Rearm<'a>(&'a mut GitHubApi<MockGitHub>, MockGitHub, &'a mut u32);
    impl RemoteRepo for Rearm<'_> {
        fn head(&self) -> anyhow::Result<Option<String>> { self.0.head() }
        fn list_tree(&self, c: &str) -> anyhow::Result<Vec<gitpass_core::sync_engine::TreeEntry>> { self.0.list_tree(c) }
        fn get_blob(&self, sha: &str) -> anyhow::Result<Vec<u8>> { self.0.get_blob(sha) }
        fn create_commit(&mut self, p: Option<&str>, t: &[(String, Vec<u8>)], m: &str) -> anyhow::Result<String> { self.0.create_commit(p, t, m) }
        fn update_ref(&mut self, n: &str, o: Option<&str>) -> anyhow::Result<()> {
            *self.2 += 1;
            arm(&self.1);
            self.0.update_ref(n, o)
        }
    }
    let mock = f.mock.clone();
    let e = sync_with_remote(&s, &mut Rearm(&mut repo, mock, &mut attempts), &id, &opts).unwrap_err();
    assert!(format!("{e:#}").contains("kept moving"), "{e:#}");
    assert_eq!(attempts, 2);
}

fn expect_err(f: &Fixture, setup: impl FnOnce(&mut State), want: &str) {
    setup(&mut f.mock.0.borrow_mut());
    let e = sync(&f.b, &f.sk, &f.mock).unwrap_err();
    no_leak(&e);
    assert!(format!("{e:#}").contains(want), "want {want:?}, got {e:#}");
}

#[test]
fn truncated_tree_is_refused() {
    let f = two_devices();
    expect_err(&f, |s| s.truncated = true, "exceeds sync limits");
}

#[test]
fn oversize_blob_is_refused() {
    let f = two_devices();
    expect_err(&f, |s| s.oversize_blob_size = true, "exceeds sync limits");
}

#[test]
fn blob_sha_mismatch_is_refused() {
    let f = two_devices();
    expect_err(&f, |s| s.corrupt_blobs = true, "identity check");
}

#[test]
fn redirect_is_refused() {
    let f = two_devices();
    expect_err(&f, |s| s.redirect = true, "redirect rejected");
}

#[test]
fn transport_timeout_is_reported_without_details() {
    let f = two_devices();
    expect_err(&f, |s| s.fail = Some(HttpError::Timeout), "timed out");
}

#[test]
fn rate_limit_reports_wait_without_echoing_headers_or_body() {
    let f = two_devices();
    expect_err(&f, |s| s.rate_limited = true, "retry in 60 s");
}

#[test]
fn non_vault_paths_and_special_modes_in_remote_tree_are_refused() {
    for (path, mode, want) in [
        ("notes.txt", "100644", "allowlist"),
        ("secret.key", "100644", "allowlist"),
        ("vaults/x/../../secret.key", "100644", "allowlist"),
        ("format.json.enc", "120000", "non-regular"),
        ("sub", "160000", "non-regular"),
    ] {
        let f = two_devices();
        expect_err(&f, |s| s.extra_entry = Some((path.into(), mode.into())), want);
    }
}

#[test]
fn cancellation_and_overall_deadline_stop_requests() {
    let f = two_devices();
    let s = Session::unlock(&f.b, PW, &f.sk).unwrap();
    let cancel = Arc::new(AtomicBool::new(true));
    let before = f.mock.0.borrow().requests.len();
    let mut repo = api_with(&f.mock, Limits::default(), cancel);
    let id = repo.remote_id();
    let e = sync_with_remote(&s, &mut repo, &id, &SyncOptions::default()).unwrap_err();
    assert!(format!("{e:#}").contains("cancelled"), "{e:#}");
    assert_eq!(f.mock.0.borrow().requests.len(), before, "no request after cancel");
    let limits = Limits { overall_timeout: std::time::Duration::ZERO, ..Default::default() };
    let mut repo = api_with(&f.mock, limits, Default::default());
    let e = sync_with_remote(&s, &mut repo, &id, &SyncOptions::default()).unwrap_err();
    assert!(format!("{e:#}").contains("timed out"), "{e:#}");
    no_leak(&e);
}

#[test]
fn request_and_byte_limits_are_enforced() {
    let f = two_devices();
    let s = Session::unlock(&f.b, PW, &f.sk).unwrap();
    for limits in [
        Limits { max_requests: 3, ..Default::default() },
        Limits { max_total_bytes: 512, ..Default::default() },
        Limits { max_entries: 2, ..Default::default() },
    ] {
        let mut repo = api_with(&f.mock, limits, Default::default());
        let id = repo.remote_id();
        let e = sync_with_remote(&s, &mut repo, &id, &SyncOptions::default()).unwrap_err();
        assert!(format!("{e:#}").contains("exceeds sync limits"), "{e:#}");
    }
}

#[test]
fn remote_password_change_is_not_silently_overwritten() {
    let f = two_devices();
    sync(&f.b, &f.sk, &f.mock).unwrap();
    // Another device rewrapped the keyset (password change): simulate by pushing
    // A's vault after a password change.
    let mut s = Session::unlock(&f.a, PW, &f.sk).unwrap();
    s.change_password("new password 123", &f.sk, fast()).unwrap();
    drop(s);
    let s = Session::unlock(&f.a, "new password 123", &f.sk).unwrap();
    let mut repo = api(&f.mock);
    let id = repo.remote_id();
    assert!(sync_with_remote(&s, &mut repo, &id, &SyncOptions::default()).unwrap().pushed);
    drop(s);
    // B (old password, unchanged keyset) must not push its old keyset back.
    let before = f.mock.0.borrow().head.clone();
    let e = sync(&f.b, &f.sk, &f.mock).unwrap_err();
    assert!(format!("{e:#}").contains("password/KDF"), "{e:#}");
    assert_eq!(f.mock.0.borrow().head, before);
}

#[test]
fn first_sync_with_another_account_is_refused() {
    let f = two_devices();
    let d = tempfile::tempdir().unwrap();
    let sk2 = store::init(d.path(), PW, fast()).unwrap();
    let e = sync(d.path(), &sk2, &f.mock).unwrap_err();
    assert!(format!("{e:#}").contains("different account"), "{e:#}");
    no_leak(&e);
}

// ---------- git CLI round trip ----------

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git is required for this test");
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

#[test]
fn git_cli_vault_and_api_transport_produce_identical_trees() {
    let f = two_devices();
    let s = Session::unlock(&f.a, PW, &f.sk).unwrap();
    let mut i = item("With attachment", "pw");
    i.notes = Some("n".into());
    s.add_item(&f.vid, &i).unwrap();
    drop(s);
    sync(&f.a, &f.sk, &f.mock).unwrap();
    let api_tree = {
        let st = f.mock.0.borrow();
        st.commits[st.head.as_ref().unwrap()].0.clone()
    };

    // Desktop direction: `git add -A` in the vault (the git CLI transport's
    // staging rule, .gitignore respected) yields the same tree id.
    git(&f.a, &["init", "-q"]);
    git(&f.a, &["add", "-A"]);
    gitpass_core::remote::check_staged(&f.a).unwrap();
    assert_eq!(git(&f.a, &["write-tree"]), api_tree, "git CLI tree == API tree");
    let staged: Vec<String> = git(&f.a, &["ls-files"]).lines().map(String::from).collect();
    let api_paths: Vec<String> = remote_files(&f.mock).into_iter().map(|(p, _)| p).collect();
    assert_eq!(staged, api_paths);

    // Mobile/API direction: materialize the API tree into a fresh git repo.
    let d = tempfile::tempdir().unwrap();
    for (p, b) in remote_files(&f.mock) {
        let dst = d.path().join(&p);
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        fs::write(dst, b).unwrap();
    }
    git(d.path(), &["init", "-q"]);
    git(d.path(), &["add", "-A"]);
    assert_eq!(git(d.path(), &["write-tree"]), api_tree);

    // And the API transport reads a tree pushed by git exactly as git wrote it.
    let repo = api(&f.mock);
    let head = repo.head().unwrap().unwrap();
    let listed: Vec<(String, String)> = repo.list_tree(&head).unwrap().into_iter().map(|e| (e.path, e.sha)).collect();
    let git_listed: Vec<(String, String)> = git(&f.a, &["ls-files", "-s"])
        .lines()
        .map(|l| {
            let (meta, path) = l.split_once('\t').unwrap();
            (path.to_string(), meta.split(' ').nth(1).unwrap().to_string())
        })
        .collect();
    let mut listed_sorted = listed.clone();
    listed_sorted.sort();
    assert_eq!(listed_sorted, git_listed);
}
