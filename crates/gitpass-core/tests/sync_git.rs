//! Desktop/CLI `sync --merge` on the shared sync engine (issue #454): the
//! git CLI transport (`GitCliRemote`) against local bare repositories, and
//! desktop ↔ mobile-style API sync where the "GitHub API" is a fake backed
//! by the same local bare repository (git plumbing). No network, temp dirs,
//! fake data; test vaults use the KDF floor (8192 KiB, t=1) only for speed.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use gitpass_core::crypto::{KdfParams, SecretKey};
use gitpass_core::github_api::*;
use gitpass_core::store::{self, Item, Session, UnlockEffects, UnlockOpts};
use gitpass_core::sync::sync_merge;
use gitpass_core::sync_engine::{RemoteRepo, TreeEntry};
use gitpass_core::sync_git::GitCliRemote;
use gitpass_core::sync_remote::{local_vault_files, sync_with_remote, SyncOptions};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PW: &str = "correct horse battery";
fn fast() -> KdfParams {
    KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 }
}
fn opts() -> UnlockOpts {
    UnlockOpts { accept_rollback: false, target_kdf: fast() }
}
fn item(t: &str) -> Item {
    Item { title: t.into(), username: None, password: Some(format!("pw-{t}")), url: None, notes: None, totp: None, updated_at: "1".into() }
}

fn git_in(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}
fn identity(dir: &Path) {
    for (k, v) in [("user.email", "t@t"), ("user.name", "t")] {
        git_in(dir, &["config", k, v]);
    }
}

struct World {
    _d: tempfile::TempDir,
    bare: PathBuf,
    a: PathBuf,
    sk: SecretKey,
    vid: String,
    items: Vec<String>,
}

/// Desktop A: a vault with two items, connected to an empty bare repo and synced.
fn world() -> World {
    let d = tempfile::tempdir().unwrap();
    let bare = d.path().join("remote.git");
    fs::create_dir_all(&bare).unwrap();
    git_in(&bare, &["init", "--bare", "-q", "-b", "main"]);
    let a = d.path().join("a");
    fs::create_dir_all(&a).unwrap();
    let sk = store::init(&a, PW, fast()).unwrap();
    let s = Session::unlock_with(&a, PW, &sk, &opts()).unwrap();
    let vid = s.create_vault("Personal").unwrap();
    let items = vec![s.add_item(&vid, &item("One")).unwrap(), s.add_item(&vid, &item("Two")).unwrap()];
    drop(s);
    git_in(&a, &["init", "-q", "-b", "main"]);
    identity(&a);
    git_in(&a, &["remote", "add", "origin", bare.to_str().unwrap()]);
    let r = sync_merge(&a, PW, &sk, &opts()).unwrap();
    assert!(r.log.contains("pushed"), "{}", r.log);
    World { _d: d, bare, a, sk, vid, items }
}

/// Device B: a git clone of the remote.
fn clone(w: &World, name: &str) -> PathBuf {
    let p = w._d.path().join(name);
    let o = Command::new("git").args(["clone", "-q", w.bare.to_str().unwrap(), p.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success());
    identity(&p);
    p
}

fn open(root: &Path, sk: &SecretKey) -> Session {
    Session::unlock_with_effects(root, PW, sk, &opts(), None, UnlockEffects::NONE).unwrap()
}

fn assert_clean_and_synced(root: &Path, bare: &Path) {
    assert_eq!(git_in(root, &["status", "--porcelain"]), "", "working tree must be clean after sync");
    assert_eq!(git_in(root, &["rev-parse", "HEAD"]), git_in(bare, &["rev-parse", "refs/heads/main"]));
}

#[test]
fn remote_delete_propagates_instead_of_resurrecting() {
    let w = world();
    let b = clone(&w, "b");
    let s = open(&w.a, &w.sk);
    s.remove_item(&w.vid, &w.items[1]).unwrap();
    drop(s);
    sync_merge(&w.a, PW, &w.sk, &opts()).unwrap();
    // B made an unrelated edit meanwhile (diverged), then syncs.
    let s = open(&b, &w.sk);
    let mut one = s.read_item(&w.vid, &w.items[0]).unwrap().1;
    one.notes = Some("from B".into());
    s.update_item(&w.vid, &w.items[0], &one).unwrap();
    drop(s);
    let r = sync_merge(&b, PW, &w.sk, &opts()).unwrap();
    assert!(r.conflicts.is_empty(), "{:?}", r.conflicts);
    assert!(r.reapplied.is_empty(), "a remotely deleted item is not re-applied: {:?}", r.reapplied);
    let s = Session::reopen_verify(&b, PW, &w.sk, false).unwrap();
    assert!(s.read_record(&w.vid, &w.items[1]).is_err(), "remote delete must propagate (old MVP resurrected it)");
    assert_eq!(s.read_item(&w.vid, &w.items[0]).unwrap().1.notes.as_deref(), Some("from B"));
    drop(s);
    assert_clean_and_synced(&b, &w.bare);
    // A pulls B's edit; the delete stays.
    sync_merge(&w.a, PW, &w.sk, &opts()).unwrap();
    let s = Session::reopen_verify(&w.a, PW, &w.sk, false).unwrap();
    assert!(s.read_record(&w.vid, &w.items[1]).is_err());
    assert_eq!(s.read_item(&w.vid, &w.items[0]).unwrap().1.notes.as_deref(), Some("from B"));
    drop(s);
    assert_clean_and_synced(&w.a, &w.bare);
}

#[test]
fn delete_vs_concurrent_edit_keeps_the_edit_and_reports_it() {
    let w = world();
    let b = clone(&w, "b");
    let s = open(&w.a, &w.sk);
    s.remove_item(&w.vid, &w.items[0]).unwrap();
    drop(s);
    sync_merge(&w.a, PW, &w.sk, &opts()).unwrap();
    let s = open(&b, &w.sk);
    let mut one = s.read_item(&w.vid, &w.items[0]).unwrap().1;
    one.password = Some("edited on B".into());
    s.update_item(&w.vid, &w.items[0], &one).unwrap();
    drop(s);
    let r = sync_merge(&b, PW, &w.sk, &opts()).unwrap();
    assert_eq!(r.conflicts.len(), 1, "{:?}", r.conflicts);
    assert!(r.conflicts[0].contains("remote deleted, local edited"), "{:?}", r.conflicts);
    assert!(r.log.contains("conflict:"));
    let s = Session::reopen_verify(&b, PW, &w.sk, false).unwrap();
    assert_eq!(s.read_item(&w.vid, &w.items[0]).unwrap().1.password.as_deref(), Some("edited on B"));
}

#[test]
fn local_only_vault_and_local_delete_are_pushed() {
    let w = world();
    let b = clone(&w, "b");
    let s = open(&b, &w.sk);
    let v2 = s.create_vault("Work").unwrap();
    let wi = s.add_item(&v2, &item("Work item")).unwrap();
    s.remove_item(&w.vid, &w.items[0]).unwrap();
    drop(s);
    // A diverges too, so this is a real merge (the old MVP aborted on a local-only vault).
    let s = open(&w.a, &w.sk);
    s.add_item(&w.vid, &item("A new")).unwrap();
    drop(s);
    sync_merge(&w.a, PW, &w.sk, &opts()).unwrap();
    let r = sync_merge(&b, PW, &w.sk, &opts()).unwrap();
    assert!(r.reapplied.iter().any(|(v, i)| v == &v2 && i == &wi), "{:?}", r.reapplied);
    sync_merge(&w.a, PW, &w.sk, &opts()).unwrap();
    let s = Session::reopen_verify(&w.a, PW, &w.sk, false).unwrap();
    assert_eq!(s.read_item(&v2, &wi).unwrap().1.title, "Work item");
    assert!(s.read_record(&w.vid, &w.items[0]).is_err(), "local delete must reach the other device (old MVP lost it)");
    assert_eq!(s.list_items(&w.vid).unwrap().len(), 2); // Two + "A new"
    drop(s);
    assert_clean_and_synced(&w.a, &w.bare);
    assert_eq!(local_vault_files(&w.a).unwrap(), local_vault_files(&b).unwrap());
}

/// Wraps the git transport and lets another device push right before our ref update.
struct Racy<'a> {
    inner: GitCliRemote,
    root: PathBuf,
    before_push: Option<Box<dyn FnOnce() + 'a>>,
    pushes: u32,
}
impl RemoteRepo for Racy<'_> {
    fn head(&self) -> anyhow::Result<Option<String>> { self.inner.head() }
    fn list_tree(&self, c: &str) -> anyhow::Result<Vec<TreeEntry>> { self.inner.list_tree(c) }
    fn get_blob(&self, s: &str) -> anyhow::Result<Vec<u8>> { self.inner.get_blob(s) }
    fn create_commit(&mut self, p: Option<&str>, t: &[(String, Vec<u8>)], m: &str) -> anyhow::Result<String> { self.inner.create_commit(p, t, m) }
    fn update_ref(&mut self, n: &str, o: Option<&str>) -> anyhow::Result<()> {
        self.pushes += 1;
        if let Some(f) = self.before_push.take() {
            f();
            // Bypass the transport's early head check to exercise the server-side rejection.
            let refspec = format!("{n}:refs/heads/main");
            let out = Command::new("git").arg("-C").arg(&self.root).args(["push", "-q", "origin", &refspec]).output().unwrap();
            assert!(!out.status.success(), "non-fast-forward push must be rejected");
            return Err(gitpass_core::sync_engine::RefUpdateConflict.into());
        }
        self.inner.update_ref(n, o)
    }
    fn merge_base(&self, h: &str) -> anyhow::Result<Option<String>> { self.inner.merge_base(h) }
    fn finish(&mut self, h: &str) -> anyhow::Result<()> { self.inner.finish(h) }
}

#[test]
fn concurrent_push_is_rejected_then_refetched_and_merged() {
    let w = world();
    let b = clone(&w, "b");
    let s = open(&b, &w.sk);
    let bi = s.add_item(&w.vid, &item("From B")).unwrap();
    drop(s);
    let s = open(&w.a, &w.sk);
    let ai = s.add_item(&w.vid, &item("From A")).unwrap();
    drop(s);
    let s = Session::unlock_with(&b, PW, &w.sk, &opts()).unwrap();
    let inner = GitCliRemote::open(&b).unwrap();
    let id = inner.remote_id();
    let a = w.a.clone();
    let sk = w.sk.clone();
    let mut repo = Racy { inner, root: b.clone(), before_push: Some(Box::new(move || { sync_merge(&a, PW, &sk, &opts()).unwrap(); })), pushes: 0 };
    let out = sync_with_remote(&s, &mut repo, &id, &SyncOptions::default()).unwrap();
    assert_eq!(out.attempts, 2);
    assert_eq!(repo.pushes, 2);
    drop(s);
    let s = Session::reopen_verify(&b, PW, &w.sk, false).unwrap();
    assert!(s.read_item(&w.vid, &ai).is_ok() && s.read_item(&w.vid, &bi).is_ok());
    drop(s);
    assert_clean_and_synced(&b, &w.bare);
}

#[test]
fn rewound_remote_is_refused_as_rollback() {
    let w = world();
    let first = git_in(&w.bare, &["rev-parse", "refs/heads/main"]);
    let s = open(&w.a, &w.sk);
    s.add_item(&w.vid, &item("Three")).unwrap();
    drop(s);
    sync_merge(&w.a, PW, &w.sk, &opts()).unwrap();
    let before = local_vault_files(&w.a).unwrap();
    git_in(&w.bare, &["update-ref", "refs/heads/main", &first]);
    let e = sync_merge(&w.a, PW, &w.sk, &opts()).unwrap_err();
    assert!(format!("{e:#}").contains("rollback"), "{e:#}");
    assert_eq!(local_vault_files(&w.a).unwrap(), before);
}

#[test]
fn remote_with_a_non_vault_file_is_refused() {
    let w = world();
    let c = clone(&w, "c");
    fs::write(c.join("notes.txt"), "plaintext").unwrap();
    git_in(&c, &["add", "notes.txt"]);
    git_in(&c, &["commit", "-q", "-m", "oops"]);
    git_in(&c, &["push", "-q", "origin", "HEAD"]);
    let e = sync_merge(&w.a, PW, &w.sk, &opts()).unwrap_err();
    assert!(format!("{e:#}").contains("allowlist"), "{e:#}");
    assert!(!w.a.join("notes.txt").exists());
}

#[test]
fn no_origin_commits_locally_and_skips() {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PW, fast()).unwrap();
    git_in(d.path(), &["init", "-q", "-b", "main"]);
    identity(d.path());
    let r = sync_merge(d.path(), PW, &sk, &opts()).unwrap();
    assert!(r.log.contains("no 'origin' remote configured"), "{}", r.log);
    assert!(!git_in(d.path(), &["rev-parse", "HEAD"]).is_empty());
}

// ---------- mobile-style API device against the same bare repo ----------

/// Fake GitHub Git Data / Contents API backed by a local bare repository
/// (git plumbing), with GitHub's semantics: 409 for object creation in an
/// empty repo, non-forced PATCH = fast-forward only (422 otherwise).
struct BareGitHub {
    bare: PathBuf,
}

const ZERO: &str = "0000000000000000000000000000000000000000";

impl BareGitHub {
    fn git(&self, args: &[&str], stdin: Option<&[u8]>, index: Option<&Path>) -> (bool, Vec<u8>) {
        let mut c = Command::new("git");
        c.arg("-C").arg(&self.bare).args(args).stdout(Stdio::piped()).stderr(Stdio::piped())
            .env("GIT_AUTHOR_NAME", "api").env("GIT_AUTHOR_EMAIL", "api@test").env("GIT_COMMITTER_NAME", "api").env("GIT_COMMITTER_EMAIL", "api@test");
        if let Some(i) = index {
            c.env("GIT_INDEX_FILE", i);
        }
        c.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
        let mut ch = c.spawn().unwrap();
        if let Some(s) = stdin {
            ch.stdin.take().unwrap().write_all(s).unwrap();
        }
        let o = ch.wait_with_output().unwrap();
        (o.status.success(), o.stdout)
    }
    fn text(&self, args: &[&str]) -> Option<String> {
        let (ok, out) = self.git(args, None, None);
        ok.then(|| String::from_utf8_lossy(&out).trim().to_string())
    }
    fn head(&self) -> Option<String> {
        self.text(&["rev-parse", "--verify", "-q", "refs/heads/main"])
    }
    fn write_tree(&self, entries: &[(String, String)]) -> String {
        let d = tempfile::tempdir().unwrap();
        let idx = d.path().join("index");
        let info: String = entries.iter().map(|(p, s)| format!("100644 {s}\t{p}\n")).collect();
        assert!(self.git(&["update-index", "--add", "--index-info"], Some(info.as_bytes()), Some(&idx)).0);
        let (ok, out) = self.git(&["write-tree"], None, Some(&idx));
        assert!(ok);
        String::from_utf8_lossy(&out).trim().to_string()
    }
}

fn ok_json(status: u16, v: Value) -> Result<HttpResponse, HttpError> {
    Ok(HttpResponse { status, body: serde_json::to_vec(&v).unwrap(), ..Default::default() })
}
fn st(code: u16) -> Result<HttpResponse, HttpError> {
    ok_json(code, json!({"message": "x"}))
}

impl HttpClient for BareGitHub {
    fn send(&self, req: &HttpRequest<'_>) -> Result<HttpResponse, HttpError> {
        let body: Value = req.body.map(|b| serde_json::from_slice(b).unwrap()).unwrap_or(Value::Null);
        let p = req.path.strip_prefix("/repos/o/vault").expect("fixed test repo");
        let empty = self.head().is_none();
        match (req.method, p) {
            (HttpMethod::Get, "") => ok_json(200, json!({"full_name": "o/vault"})),
            (HttpMethod::Get, "/git/ref/heads/main") => match self.head() {
                Some(h) => ok_json(200, json!({"ref": "refs/heads/main", "object": {"type": "commit", "sha": h}})),
                None => st(409),
            },
            (HttpMethod::Get, x) if x.starts_with("/git/commits/") => {
                let sha = &x["/git/commits/".len()..];
                let Some(raw) = self.text(&["cat-file", "-p", sha]) else { return st(404) };
                let tree = raw.lines().find_map(|l| l.strip_prefix("tree ")).unwrap().to_string();
                let parents: Vec<Value> = raw.lines().filter_map(|l| l.strip_prefix("parent ")).map(|p| json!({"sha": p})).collect();
                ok_json(200, json!({"sha": sha, "tree": {"sha": tree}, "parents": parents}))
            }
            (HttpMethod::Get, x) if x.starts_with("/git/trees/") => {
                let sha = x["/git/trees/".len()..].strip_suffix("?recursive=1").unwrap();
                let (ok, out) = self.git(&["ls-tree", "-r", "-t", "-l", "-z", sha], None, None);
                if !ok {
                    return st(404);
                }
                let mut tree = vec![];
                for rec in out.split(|b| *b == 0).filter(|r| !r.is_empty()) {
                    let rec = String::from_utf8_lossy(rec);
                    let (meta, path) = rec.split_once('\t').unwrap();
                    let f: Vec<&str> = meta.split_whitespace().collect();
                    let mode = if f[1] == "tree" { "040000" } else { f[0] };
                    let mut e = json!({"path": path, "mode": mode, "type": f[1], "sha": f[2]});
                    if f[1] == "blob" {
                        e["size"] = json!(f[3].parse::<u64>().unwrap());
                    }
                    tree.push(e);
                }
                ok_json(200, json!({"sha": sha, "truncated": false, "tree": tree}))
            }
            (HttpMethod::Get, x) if x.starts_with("/git/blobs/") => {
                let sha = &x["/git/blobs/".len()..];
                let (ok, bytes) = self.git(&["cat-file", "blob", sha], None, None);
                if !ok {
                    return st(404);
                }
                let enc = B64.encode(&bytes);
                let wrapped: Vec<String> = enc.as_bytes().chunks(60).map(|c| String::from_utf8(c.to_vec()).unwrap()).collect();
                ok_json(200, json!({"sha": sha, "encoding": "base64", "size": bytes.len(), "content": wrapped.join("\n")}))
            }
            (HttpMethod::Post, "/git/blobs") => {
                if empty {
                    return st(409);
                }
                let bytes = B64.decode(body["content"].as_str().unwrap()).unwrap();
                let (_, out) = self.git(&["hash-object", "-w", "--stdin"], Some(&bytes), None);
                ok_json(201, json!({"sha": String::from_utf8_lossy(&out).trim()}))
            }
            (HttpMethod::Post, "/git/trees") => {
                if empty {
                    return st(409);
                }
                let entries: Vec<(String, String)> = body["tree"].as_array().unwrap().iter()
                    .map(|e| (e["path"].as_str().unwrap().to_string(), e["sha"].as_str().unwrap().to_string())).collect();
                ok_json(201, json!({"sha": self.write_tree(&entries)}))
            }
            (HttpMethod::Post, "/git/commits") => {
                if empty {
                    return st(409);
                }
                let tree = body["tree"].as_str().unwrap();
                let mut args = vec!["commit-tree", tree];
                let parents: Vec<String> = body["parents"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect();
                for p in &parents {
                    args.extend(["-p", p.as_str()]);
                }
                args.extend(["-m", body["message"].as_str().unwrap()]);
                let sha = self.text(&args).unwrap();
                ok_json(201, json!({"sha": sha, "tree": {"sha": tree}, "parents": parents.iter().map(|p| json!({"sha": p})).collect::<Vec<_>>()}))
            }
            (HttpMethod::Put, "/contents/format.json") => {
                if !empty {
                    return st(422);
                }
                let bytes = B64.decode(body["content"].as_str().unwrap()).unwrap();
                let (_, out) = self.git(&["hash-object", "-w", "--stdin"], Some(&bytes), None);
                let blob = String::from_utf8_lossy(&out).trim().to_string();
                let tree = self.write_tree(&[("format.json".into(), blob.clone())]);
                let c = self.text(&["commit-tree", &tree, "-m", "init"]).unwrap();
                assert!(self.git(&["update-ref", "refs/heads/main", &c, ZERO], None, None).0);
                ok_json(201, json!({"content": {"sha": blob}, "commit": {"sha": c}}))
            }
            (HttpMethod::Post, "/git/refs") => {
                let sha = body["sha"].as_str().unwrap();
                if !self.git(&["update-ref", "refs/heads/main", sha, ZERO], None, None).0 {
                    return st(422);
                }
                ok_json(201, json!({"object": {"sha": sha}}))
            }
            (HttpMethod::Patch, "/git/refs/heads/main") => {
                assert_eq!(body["force"], json!(false));
                let new = body["sha"].as_str().unwrap();
                let cur = self.head().unwrap();
                if !self.git(&["merge-base", "--is-ancestor", &cur, new], None, None).0
                    || !self.git(&["update-ref", "refs/heads/main", new, &cur], None, None).0
                {
                    return st(422);
                }
                ok_json(200, json!({"object": {"sha": new}}))
            }
            (m, x) => panic!("unexpected {m:?} {x}"),
        }
    }
}

fn api_sync(root: &Path, sk: &SecretKey, bare: &Path) -> gitpass_core::sync_remote::SyncOutcome {
    let s = open(root, sk);
    let mut api = GitHubApi::new(
        BareGitHub { bare: bare.to_path_buf() },
        "github_pat_FAKE_TEST_0123456789".into(),
        GitHubRepoRef::new("o", "vault", "main").unwrap(),
        Limits::default(),
        Default::default(),
    )
    .unwrap();
    let id = api.remote_id();
    sync_with_remote(&s, &mut api, &id, &SyncOptions::default()).unwrap()
}

fn copy_vault_files(src: &Path, dst: &Path) {
    // A mobile first pull: just the committed vault files, no .git.
    for (p, b) in local_vault_files(src).unwrap() {
        let t = dst.join(&p);
        fs::create_dir_all(t.parent().unwrap()).unwrap();
        fs::write(t, b).unwrap();
    }
}

#[test]
fn desktop_git_cli_and_mobile_api_sync_through_one_bare_repo() {
    let w = world();
    // Mobile device M pulled the vault earlier (files only, no git).
    let m = w._d.path().join("mobile");
    copy_vault_files(&w.a, &m);
    let out = api_sync(&m, &w.sk, &w.bare);
    assert!(!out.pushed && !out.local_changed, "first API sync of identical content is a no-op: {out:?}");

    // M adds an item and deletes item 1; pushes through the API.
    let s = open(&m, &w.sk);
    let mi = s.add_item(&w.vid, &item("From mobile")).unwrap();
    s.remove_item(&w.vid, &w.items[0]).unwrap();
    drop(s);
    assert!(api_sync(&m, &w.sk, &w.bare).pushed);

    // Desktop edits item 2 concurrently and merges via git.
    let s = open(&w.a, &w.sk);
    let mut two = s.read_item(&w.vid, &w.items[1]).unwrap().1;
    two.password = Some("desktop-edit".into());
    s.update_item(&w.vid, &w.items[1], &two).unwrap();
    drop(s);
    let r = sync_merge(&w.a, PW, &w.sk, &opts()).unwrap();
    assert!(r.conflicts.is_empty(), "{:?}", r.conflicts);
    let s = Session::reopen_verify(&w.a, PW, &w.sk, false).unwrap();
    assert_eq!(s.read_item(&w.vid, &mi).unwrap().1.title, "From mobile");
    assert!(s.read_record(&w.vid, &w.items[0]).is_err(), "mobile delete propagates to desktop");
    drop(s);
    assert_clean_and_synced(&w.a, &w.bare);

    // Mobile pulls the desktop edit (fast-forward).
    let out = api_sync(&m, &w.sk, &w.bare);
    assert!(out.local_changed && !out.pushed, "{out:?}");
    let s = open(&m, &w.sk);
    assert_eq!(s.read_item(&w.vid, &w.items[1]).unwrap().1.password.as_deref(), Some("desktop-edit"));
    drop(s);

    // Both transports converge on byte-identical trees: desktop working tree,
    // mobile files and the bare repo's head tree.
    assert_eq!(local_vault_files(&w.a).unwrap(), local_vault_files(&m).unwrap());
    let head_tree = git_in(&w.bare, &["rev-parse", "refs/heads/main^{tree}"]);
    assert_eq!(git_in(&w.a, &["rev-parse", "HEAD^{tree}"]), head_tree);
    // Plain (rebase) sync still works afterwards and finds nothing to do.
    let log = gitpass_core::sync::sync(&w.a).unwrap();
    assert!(log.contains("nothing to commit"), "{log}");
    git_in(&w.bare, &["fsck", "--no-progress"]);
}

#[test]
fn mobile_api_device_bootstraps_an_empty_bare_repo_then_desktop_merges() {
    let d = tempfile::tempdir().unwrap();
    let bare = d.path().join("remote.git");
    fs::create_dir_all(&bare).unwrap();
    git_in(&bare, &["init", "--bare", "-q", "-b", "main"]);
    let m = d.path().join("m");
    fs::create_dir_all(&m).unwrap();
    let sk = store::init(&m, PW, fast()).unwrap();
    gitpass_core::remote::ensure_gitignore(&m).unwrap();
    gitpass_core::remote::ensure_gitattributes(&m).unwrap();
    let s = open(&m, &sk);
    let vid = s.create_vault("Phone").unwrap();
    let i1 = s.add_item(&vid, &item("Phone item")).unwrap();
    drop(s);
    assert!(api_sync(&m, &sk, &bare).pushed);
    // Desktop clones and merges with its own edit.
    let a = d.path().join("a");
    let o = Command::new("git").args(["clone", "-q", bare.to_str().unwrap(), a.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success());
    identity(&a);
    let s = open(&a, &sk);
    let i2 = s.add_item(&vid, &item("Desk item")).unwrap();
    drop(s);
    sync_merge(&a, PW, &sk, &opts()).unwrap();
    api_sync(&m, &sk, &bare);
    let s = open(&m, &sk);
    assert!(s.read_item(&vid, &i1).is_ok() && s.read_item(&vid, &i2).is_ok());
    drop(s);
    assert_eq!(local_vault_files(&a).unwrap(), local_vault_files(&m).unwrap());
    assert_clean_and_synced(&a, &bare);
}
