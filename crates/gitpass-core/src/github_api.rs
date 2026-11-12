//! GitHub Git Data API transport (issue #453): a [`RemoteRepo`] over
//! `https://api.github.com` for hosts that cannot run `git` (mobile) and for
//! token-based desktop sync.
//!
//! # Design decision: Rust owns the protocol, the host may own HTTPS
//!
//! All protocol logic — URL building, JSON parsing, limits, object-id
//! verification, allowed-path enforcement, error mapping — lives here in
//! core. The only thing abstracted is one HTTPS round trip, the
//! [`HttpClient`] trait, so:
//! * desktop/CLI can use the bundled `ureq` client
//!   ([`UreqHttpClient`], feature `github-ureq`, off by default so core stays
//!   network-free unless a host opts in);
//! * mobile hosts can later supply their platform HTTPS stack through a
//!   UniFFI callback implementing [`HttpClient`] without re-implementing any
//!   protocol or crypto in Swift/Kotlin;
//! * tests drive the whole transport through an in-process fake GitHub — no
//!   network, never a real repository.
//!
//! # Security properties
//! * Origin is fixed: requests carry only a path below `/repos/{owner}/{repo}/`
//!   built from validated owner/repo/branch names and 40-hex object ids;
//!   clients must send them to `https://api.github.com` and never follow
//!   redirects (a 3xx is an error here as well).
//! * Fine-grained personal access tokens only (`github_pat_…`, needs
//!   "Contents: read and write" on the one vault repository). The token
//!   lives in zeroizing memory, goes only into the `Authorization` header,
//!   never into a URL, error, log or `Debug` output.
//! * Errors never echo response bodies or headers ([`GitHubApiError`]).
//! * Limits: per-response bytes, tree entry count, per-blob and total bytes,
//!   request count, an overall deadline and a cancellation flag.
//! * Every listed tree is re-hashed from its entries and must equal the tree
//!   id GitHub reported (and is not truncated); every blob is re-hashed and
//!   must equal its id; trees/blobs GitHub returns on push are checked
//!   against locally computed ids. Remote paths must pass
//!   [`is_allowed_path`]; only regular `100644` blobs are accepted (no
//!   symlinks, executables or submodules).
//! * The ref update is `PATCH …/git/refs/heads/{branch}` with `force:false`
//!   on a commit whose only parent is the expected head: GitHub rejects it
//!   (422) unless it is a fast-forward of the current head, i.e. unless the
//!   head is still `expected_old` — a compare-and-swap. Rejection surfaces as
//!   [`RefUpdateConflict`] so the driver re-fetches and re-plans.
//! * Rate limits (403/429 with `retry-after` or `x-ratelimit-remaining: 0`)
//!   surface as [`GitHubApiError::RateLimited`] with the wait in seconds; no
//!   automatic sleeping inside the transport.
//!
//! Empty repositories: GitHub's Git Data API cannot create objects in a
//! repository with no commits (409). For a root commit on an empty
//! repository the transport first creates a bootstrap commit holding only
//! the public `format.json` through the Contents API (which fails if the
//! branch appeared meanwhile, surfacing as [`RefUpdateConflict`]), then
//! pushes the real tree as its child and fast-forwards the ref onto it. The
//! history gains one extra commit; no secret is ever in it.

use crate::sync_engine::{git_blob_sha, git_tree_id, is_allowed_path, RefUpdateConflict, RemoteRepo, TreeEntry};
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// The only origin requests may go to.
pub const GITHUB_API_ORIGIN: &str = "https://api.github.com";

/// HTTP method used by the transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Patch,
    Put,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Put => "PUT",
        }
    }
}

/// One request. `path` starts with `/repos/` and is appended to
/// [`GITHUB_API_ORIGIN`]; it never contains credentials. Clients must send
/// `authorization` as the `Authorization` header and nowhere else, add
/// `headers`, refuse redirects, stop reading after `max_response_bytes`
/// (returning [`HttpError::TooLarge`]), honour `timeout`, and abort when
/// `cancel` becomes true.
pub struct HttpRequest<'a> {
    pub method: HttpMethod,
    pub path: &'a str,
    pub authorization: &'a str,
    pub headers: &'a [(&'static str, &'static str)],
    /// JSON body for POST/PATCH.
    pub body: Option<&'a [u8]>,
    pub max_response_bytes: usize,
    pub timeout: Duration,
    pub cancel: &'a AtomicBool,
}

impl std::fmt::Debug for HttpRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("authorization", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// A response: status, body (bounded) and the rate-limit headers this
/// transport reads. Other headers are not exposed.
#[derive(Debug, Clone, Default)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    /// `retry-after`, seconds.
    pub retry_after: Option<u64>,
    /// `x-ratelimit-remaining`.
    pub rate_limit_remaining: Option<u64>,
    /// `x-ratelimit-reset`, unix seconds.
    pub rate_limit_reset: Option<u64>,
}

/// Transport-level failure. Deliberately carries no message from lower
/// layers (TLS/HTTP errors can contain URLs or header values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    Timeout,
    Cancelled,
    TooLarge,
    Redirect,
    Transport,
}

/// One HTTPS round trip to [`GITHUB_API_ORIGIN`]. See [`HttpRequest`] for
/// the contract implementations must keep.
pub trait HttpClient {
    fn send(&self, req: &HttpRequest<'_>) -> std::result::Result<HttpResponse, HttpError>;
}

/// Errors from the GitHub transport. Display strings are fixed text: no
/// token, URL, path, response body or header ever appears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHubApiError {
    /// 401: token invalid or expired.
    Auth,
    /// 403 (not rate limiting) / 404: no access to the repository or branch.
    NoAccess,
    RateLimited { retry_after_secs: Option<u64> },
    Redirect,
    /// 409 on object creation: repository has no commits yet.
    EmptyRepository,
    Unavailable,
    InvalidResponse,
    /// Response, tree, blob, total-bytes or request-count limit exceeded.
    Limits,
    Timeout,
    Cancelled,
    Network,
    InvalidConfig,
}

impl std::fmt::Display for GitHubApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auth => f.write_str("GitHub rejected the sync token (invalid or expired)"),
            Self::NoAccess => f.write_str(
                "GitHub repository access failed; verify the repository, branch and that the fine-grained token has Contents: read and write on it",
            ),
            Self::RateLimited { retry_after_secs: Some(s) } => write!(f, "GitHub rate limit reached; retry in {s} s"),
            Self::RateLimited { retry_after_secs: None } => f.write_str("GitHub rate limit reached; retry later"),
            Self::Redirect => f.write_str("GitHub redirect rejected"),
            Self::EmptyRepository => f.write_str(
                "the GitHub repository has no commits; initialize it first (desktop git sync or one initial commit)",
            ),
            Self::Unavailable => f.write_str("GitHub is unavailable; retry later"),
            Self::InvalidResponse => f.write_str("invalid GitHub response; refusing to sync"),
            Self::Limits => f.write_str("GitHub response exceeds sync limits; refusing to sync"),
            Self::Timeout => f.write_str("GitHub sync timed out"),
            Self::Cancelled => f.write_str("GitHub sync cancelled"),
            Self::Network => f.write_str("GitHub network request failed"),
            Self::InvalidConfig => f.write_str("invalid GitHub sync settings (owner, repository, branch or token)"),
        }
    }
}

impl std::error::Error for GitHubApiError {}

/// Limits for one transport instance (one sync).
#[derive(Debug, Clone)]
pub struct Limits {
    pub meta_bytes: usize,
    pub tree_bytes: usize,
    pub max_entries: usize,
    pub max_blob_bytes: usize,
    pub max_total_bytes: u64,
    pub max_requests: u32,
    pub request_timeout: Duration,
    pub overall_timeout: Duration,
}

impl Default for Limits {
    /// Same budget as the Android first-pull client: 10 000 entries, 4 MiB
    /// per file, 64 MiB total, 5 minutes.
    fn default() -> Self {
        Self {
            meta_bytes: 64 * 1024,
            tree_bytes: 4 * 1024 * 1024,
            max_entries: 10_000,
            max_blob_bytes: 4 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_requests: 25_000,
            request_timeout: Duration::from_secs(30),
            overall_timeout: Duration::from_secs(300),
        }
    }
}

const HEADERS: &[(&str, &str)] = &[
    ("Accept", "application/vnd.github+json"),
    ("X-GitHub-Api-Version", "2022-11-28"),
    ("User-Agent", "gitpass-sync"),
];

/// GitHub repository + branch, validated so they can be placed in a path
/// without escaping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepoRef {
    owner: String,
    repo: String,
    branch: String,
}

fn valid_name(s: &str) -> bool {
    (1..=100).contains(&s.len())
        && s != "."
        && s != ".."
        && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

fn valid_branch(s: &str) -> bool {
    (1..=200).contains(&s.len())
        && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'/'))
        && s.split('/').all(|c| !c.is_empty() && !c.starts_with('.') && !c.ends_with(".lock"))
        && !s.contains("..")
}

fn valid_oid(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

impl GitHubRepoRef {
    pub fn new(owner: &str, repo: &str, branch: &str) -> Result<Self> {
        if !valid_name(owner) || !valid_name(repo) || !valid_branch(branch) {
            return Err(GitHubApiError::InvalidConfig.into());
        }
        Ok(Self { owner: owner.into(), repo: repo.into(), branch: branch.into() })
    }
    fn base(&self) -> String {
        format!("/repos/{}/{}", self.owner, self.repo)
    }
}

/// Fine-grained PAT shape: `github_pat_` + `[A-Za-z0-9_]`. Classic tokens
/// (`ghp_…`, account-wide scopes) are refused. Pairing uses the same check.
pub(crate) fn is_fine_grained_pat(t: &str) -> bool {
    t.len() <= 255
        && t.strip_prefix("github_pat_")
            .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
}

fn valid_token(t: &str) -> bool {
    is_fine_grained_pat(t)
}

/// The GitHub Git Data API [`RemoteRepo`]. One instance per sync: limits,
/// the deadline and counters are per instance.
pub struct GitHubApi<H: HttpClient> {
    http: H,
    authorization: Zeroizing<String>,
    repo: GitHubRepoRef,
    limits: Limits,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
    requests: Cell<u32>,
    total_bytes: Cell<u64>,
    /// Last listed tree (commit -> entries), reused by `create_commit` to skip
    /// re-uploading unchanged blobs.
    tree_cache: RefCell<Option<(String, Vec<TreeEntry>)>>,
    /// Bootstrap commit this instance created on an empty repository.
    bootstrap: Option<String>,
}

impl<H: HttpClient> std::fmt::Debug for GitHubApi<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHubApi").field("repo", &self.repo).field("token", &"<redacted>").finish_non_exhaustive()
    }
}

fn err(e: GitHubApiError) -> anyhow::Error {
    e.into()
}

impl<H: HttpClient> GitHubApi<H> {
    /// `token` must be a fine-grained PAT; it is moved into zeroizing memory.
    pub fn new(http: H, token: String, repo: GitHubRepoRef, limits: Limits, cancel: Arc<AtomicBool>) -> Result<Self> {
        let token = Zeroizing::new(token);
        if !valid_token(&token) {
            return Err(err(GitHubApiError::InvalidConfig));
        }
        let authorization = Zeroizing::new(format!("Bearer {}", token.as_str()));
        let deadline = Instant::now() + limits.overall_timeout;
        Ok(Self {
            http,
            authorization,
            repo,
            limits,
            cancel,
            deadline,
            requests: Cell::new(0),
            total_bytes: Cell::new(0),
            tree_cache: RefCell::new(None),
            bootstrap: None,
        })
    }

    /// Stable id for the recorded sync base ([`crate::store::SyncBase`]).
    pub fn remote_id(&self) -> String {
        format!("github:{}/{}#{}", self.repo.owner, self.repo.repo, self.repo.branch)
    }

    fn checkpoint(&self) -> Result<()> {
        if self.cancel.load(Ordering::SeqCst) {
            return Err(err(GitHubApiError::Cancelled));
        }
        if Instant::now() >= self.deadline {
            return Err(err(GitHubApiError::Timeout));
        }
        Ok(())
    }

    /// One request; returns (status, parsed JSON) for 2xx, maps everything else.
    fn call(&self, method: HttpMethod, path: &str, body: Option<Value>, limit: usize) -> Result<(u16, Value)> {
        self.checkpoint()?;
        let n = self.requests.get() + 1;
        if n > self.limits.max_requests {
            return Err(err(GitHubApiError::Limits));
        }
        self.requests.set(n);
        let body_bytes = body.map(|b| serde_json::to_vec(&b)).transpose()?;
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        let req = HttpRequest {
            method,
            path,
            authorization: &self.authorization,
            headers: HEADERS,
            body: body_bytes.as_deref(),
            max_response_bytes: limit,
            timeout: self.limits.request_timeout.min(remaining),
            cancel: &self.cancel,
        };
        let resp = self.http.send(&req).map_err(|e| {
            err(match e {
                HttpError::Timeout => GitHubApiError::Timeout,
                HttpError::Cancelled => GitHubApiError::Cancelled,
                HttpError::TooLarge => GitHubApiError::Limits,
                HttpError::Redirect => GitHubApiError::Redirect,
                HttpError::Transport => GitHubApiError::Network,
            })
        })?;
        self.checkpoint()?;
        if resp.body.len() > limit {
            return Err(err(GitHubApiError::Limits));
        }
        let total = self.total_bytes.get() + resp.body.len() as u64;
        if total > self.limits.max_total_bytes {
            return Err(err(GitHubApiError::Limits));
        }
        self.total_bytes.set(total);
        match resp.status {
            200..=299 => {
                let v: Value = serde_json::from_slice(&resp.body).map_err(|_| err(GitHubApiError::InvalidResponse))?;
                Ok((resp.status, v))
            }
            300..=399 => Err(err(GitHubApiError::Redirect)),
            401 => Err(err(GitHubApiError::Auth)),
            403 | 429 if resp.status == 429 || resp.retry_after.is_some() || resp.rate_limit_remaining == Some(0) => {
                let wait = resp.retry_after.or_else(|| {
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
                    resp.rate_limit_reset.map(|r| r.saturating_sub(now))
                });
                Err(err(GitHubApiError::RateLimited { retry_after_secs: wait }))
            }
            403 | 404 => Err(err(GitHubApiError::NoAccess)),
            409 => Err(err(GitHubApiError::EmptyRepository)),
            // 422 is mapped by the callers that expect it (ref updates).
            422 => Err(err(GitHubApiError::InvalidResponse)).map_err(|e| e.context(Status422)),
            500..=599 => Err(err(GitHubApiError::Unavailable)),
            _ => Err(err(GitHubApiError::InvalidResponse)),
        }
    }

    fn get(&self, path: &str, limit: usize) -> Result<Value> {
        Ok(self.call(HttpMethod::Get, path, None, limit)?.1)
    }

    fn post(&self, path: &str, body: Value) -> Result<Value> {
        Ok(self.call(HttpMethod::Post, path, Some(body), self.limits.meta_bytes)?.1)
    }

    /// Empty repository: create the bootstrap commit (only `format.json`)
    /// with the Contents API, which also creates the branch.
    fn bootstrap_empty(&mut self, format_json: &[u8], message: &str) -> Result<String> {
        let path = format!("{}/contents/format.json", self.repo.base());
        let body = json!({"message": message, "content": B64.encode(format_json), "branch": self.repo.branch});
        let v = match self.call(HttpMethod::Put, &path, Some(body), self.limits.meta_bytes) {
            Ok((_, v)) => v,
            // 409/422: the branch or file exists now — someone else pushed first.
            Err(e) if is_422(&e) || matches!(e.downcast_ref::<GitHubApiError>(), Some(GitHubApiError::EmptyRepository)) => {
                return Err(RefUpdateConflict.into())
            }
            Err(e) => return Err(e),
        };
        if v["content"]["sha"].as_str() != Some(git_blob_sha(format_json).as_str()) {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        let commit = v["commit"]["sha"].as_str().filter(|s| valid_oid(s)).ok_or_else(|| err(GitHubApiError::InvalidResponse))?;
        self.bootstrap = Some(commit.to_string());
        Ok(commit.to_string())
    }

    fn list_tree_inner(&self, commit: &str) -> Result<Vec<TreeEntry>> {
        if !valid_oid(commit) {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        let base = self.repo.base();
        let c = self.get(&format!("{base}/git/commits/{commit}"), self.limits.meta_bytes)?;
        if c["sha"].as_str() != Some(commit) {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        let tree_sha = c["tree"]["sha"].as_str().filter(|s| valid_oid(s)).ok_or_else(|| err(GitHubApiError::InvalidResponse))?;
        let t = self.get(&format!("{base}/git/trees/{tree_sha}?recursive=1"), self.limits.tree_bytes)?;
        if t["sha"].as_str() != Some(tree_sha) {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        match t["truncated"].as_bool() {
            Some(false) => {}
            Some(true) => return Err(err(GitHubApiError::Limits)),
            None => return Err(err(GitHubApiError::InvalidResponse)),
        }
        let raw = t["tree"].as_array().ok_or_else(|| err(GitHubApiError::InvalidResponse))?;
        if raw.len() > self.limits.max_entries {
            return Err(err(GitHubApiError::Limits));
        }
        let mut entries = vec![];
        let mut total: u64 = 0;
        for e in raw {
            let path = e["path"].as_str().ok_or_else(|| err(GitHubApiError::InvalidResponse))?;
            let sha = e["sha"].as_str().filter(|s| valid_oid(s)).ok_or_else(|| err(GitHubApiError::InvalidResponse))?;
            match (e["type"].as_str(), e["mode"].as_str()) {
                (Some("tree"), Some("040000")) => continue,
                (Some("blob"), Some("100644")) => {}
                // Symlinks, executables, submodules and anything unknown.
                _ => anyhow::bail!("remote tree contains a non-regular file; refusing to sync"),
            }
            if !is_allowed_path(path) {
                anyhow::bail!("remote tree contains a file outside the vault allowlist; refusing to sync");
            }
            let size = e["size"].as_u64().ok_or_else(|| err(GitHubApiError::InvalidResponse))?;
            if size > self.limits.max_blob_bytes as u64 {
                return Err(err(GitHubApiError::Limits));
            }
            total += size;
            if total > self.limits.max_total_bytes {
                return Err(err(GitHubApiError::Limits));
            }
            entries.push(TreeEntry { path: path.to_string(), sha: sha.to_string() });
        }
        // The listing must describe exactly the tree GitHub named.
        let pairs: Vec<(String, String)> = entries.iter().map(|e| (e.path.clone(), e.sha.clone())).collect();
        if git_tree_id(&pairs).ok().as_deref() != Some(tree_sha) {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        Ok(entries)
    }
}

/// Context marker for a 422 response (mapped to [`RefUpdateConflict`] by `update_ref`).
#[derive(Debug)]
struct Status422;
impl std::fmt::Display for Status422 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GitHub rejected the request (422)")
    }
}

fn is_422(e: &anyhow::Error) -> bool {
    e.downcast_ref::<Status422>().is_some()
}

impl<H: HttpClient> RemoteRepo for GitHubApi<H> {
    fn head(&self) -> Result<Option<String>> {
        let base = self.repo.base();
        match self.get(&format!("{base}/git/ref/heads/{}", self.repo.branch), self.limits.meta_bytes) {
            Ok(v) => {
                if v["ref"].as_str() != Some(&format!("refs/heads/{}", self.repo.branch)) || v["object"]["type"].as_str() != Some("commit") {
                    return Err(err(GitHubApiError::InvalidResponse));
                }
                let sha = v["object"]["sha"].as_str().filter(|s| valid_oid(s)).ok_or_else(|| err(GitHubApiError::InvalidResponse))?;
                Ok(Some(sha.to_string()))
            }
            Err(e) if matches!(e.downcast_ref::<GitHubApiError>(), Some(GitHubApiError::NoAccess | GitHubApiError::EmptyRepository)) => {
                // Branch missing, repo empty, or no access: tell them apart
                // with the repository itself (404 there = no access).
                let repo = self.get(&base, self.limits.meta_bytes)?;
                if repo["full_name"].as_str().map(|s| s.to_ascii_lowercase())
                    != Some(format!("{}/{}", self.repo.owner, self.repo.repo).to_ascii_lowercase())
                {
                    return Err(err(GitHubApiError::InvalidResponse));
                }
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    fn list_tree(&self, commit: &str) -> Result<Vec<TreeEntry>> {
        if let Some((c, entries)) = self.tree_cache.borrow().as_ref() {
            if c == commit {
                return Ok(entries.clone());
            }
        }
        let entries = self.list_tree_inner(commit)?;
        *self.tree_cache.borrow_mut() = Some((commit.to_string(), entries.clone()));
        Ok(entries)
    }

    fn get_blob(&self, sha: &str) -> Result<Vec<u8>> {
        if !valid_oid(sha) {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        // base64 is 4/3 of the payload plus GitHub's line feeds and JSON framing.
        let limit = self.limits.max_blob_bytes / 3 * 4 + self.limits.max_blob_bytes / 30 + 4096;
        let v = self.get(&format!("{}/git/blobs/{sha}", self.repo.base()), limit)?;
        if v["sha"].as_str() != Some(sha) || v["encoding"].as_str() != Some("base64") {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        let size = v["size"].as_u64().ok_or_else(|| err(GitHubApiError::InvalidResponse))?;
        if size > self.limits.max_blob_bytes as u64 {
            return Err(err(GitHubApiError::Limits));
        }
        let content = v["content"].as_str().ok_or_else(|| err(GitHubApiError::InvalidResponse))?;
        // GitHub wraps base64 with LF; anything else outside the alphabet is rejected.
        if !content.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=' | b'\n' | b'\r')) {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        let compact: String = content.chars().filter(|c| *c != '\n' && *c != '\r').collect();
        let bytes = B64.decode(compact).map_err(|_| err(GitHubApiError::InvalidResponse))?;
        if bytes.len() as u64 != size || git_blob_sha(&bytes) != sha {
            anyhow::bail!("remote blob failed its identity check; refusing to sync");
        }
        Ok(bytes)
    }

    fn create_commit(&mut self, parent: Option<&str>, tree: &[(String, Vec<u8>)], message: &str) -> Result<String> {
        if parent.is_some_and(|p| !valid_oid(p)) {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        // Root commit: on an empty repository, bootstrap it first and parent
        // the real commit on the bootstrap commit (see module docs).
        let bootstrapped;
        let parent = match parent {
            Some(p) => Some(p),
            None if self.head()?.is_none() => {
                let format = tree
                    .iter()
                    .find(|(p, _)| p == "format.json")
                    .map(|(_, b)| b.clone())
                    .ok_or_else(|| err(GitHubApiError::EmptyRepository))?;
                bootstrapped = self.bootstrap_empty(&format, message)?;
                Some(bootstrapped.as_str())
            }
            None => None,
        };
        let existing: std::collections::BTreeSet<String> = match parent {
            Some(p) => self.list_tree(p)?.into_iter().map(|e| e.sha).collect(),
            None => Default::default(),
        };
        let base = self.repo.base();
        let mut pairs = vec![];
        for (path, bytes) in tree {
            if !is_allowed_path(path) {
                anyhow::bail!("refusing to push a file outside the vault allowlist");
            }
            if bytes.len() > self.limits.max_blob_bytes {
                return Err(err(GitHubApiError::Limits));
            }
            let sha = git_blob_sha(bytes);
            if !existing.contains(&sha) {
                let v = self.post(&format!("{base}/git/blobs"), json!({"content": B64.encode(bytes), "encoding": "base64"}))?;
                if v["sha"].as_str() != Some(sha.as_str()) {
                    return Err(err(GitHubApiError::InvalidResponse));
                }
            }
            pairs.push((path.clone(), sha));
        }
        if pairs.len() > self.limits.max_entries {
            return Err(err(GitHubApiError::Limits));
        }
        let want_tree = git_tree_id(&pairs)?;
        // Full tree (no base_tree): the pushed tree is exactly `tree`.
        let entries: Vec<Value> =
            pairs.iter().map(|(p, s)| json!({"path": p, "mode": "100644", "type": "blob", "sha": s})).collect();
        let t = self.post(&format!("{base}/git/trees"), json!({ "tree": entries }))?;
        if t["sha"].as_str() != Some(want_tree.as_str()) {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        let parents: Vec<&str> = parent.into_iter().collect();
        let c = self.post(&format!("{base}/git/commits"), json!({"message": message, "tree": want_tree, "parents": parents}))?;
        let sha = c["sha"].as_str().filter(|s| valid_oid(s)).ok_or_else(|| err(GitHubApiError::InvalidResponse))?;
        let got_parents: Vec<&str> = c["parents"].as_array().map(|a| a.iter().filter_map(|p| p["sha"].as_str()).collect()).unwrap_or_default();
        if c["tree"]["sha"].as_str() != Some(want_tree.as_str()) || got_parents != parents {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        Ok(sha.to_string())
    }

    fn update_ref(&mut self, new_head: &str, expected_old: Option<&str>) -> Result<()> {
        if !valid_oid(new_head) || expected_old.is_some_and(|o| !valid_oid(o)) {
            return Err(err(GitHubApiError::InvalidResponse));
        }
        // A root push on an empty repository went through a bootstrap commit
        // this instance created: the ref must still be exactly that commit.
        let bootstrap = self.bootstrap.take();
        let expected_old = match (expected_old, bootstrap.as_deref()) {
            (None, Some(b)) => Some(b),
            (e, _) => e,
        };
        let expected_old = expected_old.map(str::to_string);
        let expected_old = expected_old.as_deref();
        // Cheap early check; the server-side fast-forward rule is the real CAS.
        if self.head()?.as_deref() != expected_old {
            return Err(RefUpdateConflict.into());
        }
        let base = self.repo.base();
        let branch = &self.repo.branch;
        let res = match expected_old {
            // Creating the ref fails (422) if it already exists.
            None => self.call(HttpMethod::Post, &format!("{base}/git/refs"), Some(json!({"ref": format!("refs/heads/{branch}"), "sha": new_head})), self.limits.meta_bytes),
            // Not forced: only a fast-forward of the current head is accepted,
            // and new_head's parent is expected_old.
            Some(_) => self.call(HttpMethod::Patch, &format!("{base}/git/refs/heads/{branch}"), Some(json!({"sha": new_head, "force": false})), self.limits.meta_bytes),
        };
        match res {
            Ok((_, v)) => {
                if v["object"]["sha"].as_str() != Some(new_head) {
                    return Err(err(GitHubApiError::InvalidResponse));
                }
                Ok(())
            }
            Err(e) if is_422(&e) => Err(RefUpdateConflict.into()),
            Err(e) => Err(e),
        }
    }
}

/// `ureq`-based [`HttpClient`] for desktop/CLI hosts (feature `github-ureq`).
/// HTTPS to [`GITHUB_API_ORIGIN`] only, no redirects, no proxy-from-env
/// surprises beyond ureq's defaults, bounded reads, cancellation checked
/// between chunks.
#[cfg(feature = "github-ureq")]
pub struct UreqHttpClient {
    agent: ureq::Agent,
}

#[cfg(feature = "github-ureq")]
impl Default for UreqHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "github-ureq")]
impl UreqHttpClient {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .https_only(true)
            .timeout_connect(Duration::from_secs(15))
            .build();
        Self { agent }
    }
}

#[cfg(feature = "github-ureq")]
impl HttpClient for UreqHttpClient {
    fn send(&self, req: &HttpRequest<'_>) -> std::result::Result<HttpResponse, HttpError> {
        use std::io::Read;
        if req.cancel.load(Ordering::SeqCst) {
            return Err(HttpError::Cancelled);
        }
        if !req.path.starts_with("/repos/") {
            return Err(HttpError::Transport);
        }
        let url = format!("{GITHUB_API_ORIGIN}{}", req.path);
        let mut r = self.agent.request(req.method.as_str(), &url).timeout(req.timeout).set("Authorization", req.authorization);
        for (k, v) in req.headers {
            r = r.set(k, v);
        }
        let result = match req.body {
            Some(b) => r.set("Content-Type", "application/json").send_bytes(b),
            None => r.call(),
        };
        let resp = match result {
            Ok(resp) => resp,
            Err(ureq::Error::Status(_, resp)) => resp,
            Err(ureq::Error::Transport(t)) => {
                return Err(match t.kind() {
                    ureq::ErrorKind::Io if t.to_string().contains("timed out") => HttpError::Timeout,
                    _ => HttpError::Transport,
                })
            }
        };
        let status = resp.status();
        if (300..400).contains(&status) {
            return Err(HttpError::Redirect);
        }
        let num = |h: &str| resp.header(h).and_then(|v| v.trim().parse::<u64>().ok());
        let (retry_after, rate_limit_remaining, rate_limit_reset) =
            (num("retry-after"), num("x-ratelimit-remaining"), num("x-ratelimit-reset"));
        if let Some(len) = num("content-length") {
            if len > req.max_response_bytes as u64 {
                return Err(HttpError::TooLarge);
            }
        }
        let mut body = Vec::new();
        let mut reader = resp.into_reader().take(req.max_response_bytes as u64 + 1);
        let mut chunk = [0u8; 16 * 1024];
        loop {
            if req.cancel.load(Ordering::SeqCst) {
                return Err(HttpError::Cancelled);
            }
            let n = reader.read(&mut chunk).map_err(|e| {
                if e.kind() == std::io::ErrorKind::TimedOut { HttpError::Timeout } else { HttpError::Transport }
            })?;
            if n == 0 {
                break;
            }
            if body.len() + n > req.max_response_bytes {
                return Err(HttpError::TooLarge);
            }
            body.extend_from_slice(&chunk[..n]);
        }
        Ok(HttpResponse { status, body, retry_after, rate_limit_remaining, rate_limit_reset })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fine_grained_tokens_are_accepted() {
        assert!(valid_token("github_pat_FAKE_0123456789abcdef"));
        for bad in ["", "ghp_FAKE0123456789", "github_pat_", "github_pat_FAKE token", "github_pat_FAKE\n", "gho_FAKE"] {
            assert!(!valid_token(bad), "{bad:?}");
        }
        assert!(!valid_token(&format!("github_pat_{}", "a".repeat(300))));
    }

    #[test]
    fn repo_and_branch_names_cannot_escape_the_path() {
        assert!(GitHubRepoRef::new("octo-cat", "vault.repo_1", "main").is_ok());
        assert!(GitHubRepoRef::new("o", "r", "feature/sync-2").is_ok());
        for (o, r, b) in [
            ("..", "r", "main"),
            ("o", "r/../x", "main"),
            ("o", "r?x=1", "main"),
            ("o", "r", "../main"),
            ("o", "r", "a//b"),
            ("o", "r", "a..b"),
            ("o", "r", "main.lock"),
            ("o", "r", ".hidden"),
            ("o", "r", "x y"),
            ("o@evil.com", "r", "main"),
            ("o", "r", "main#frag"),
        ] {
            assert!(GitHubRepoRef::new(o, r, b).is_err(), "{o} {r} {b}");
        }
    }

    #[test]
    fn debug_and_errors_never_show_the_token() {
        struct Never;
        impl HttpClient for Never {
            fn send(&self, _: &HttpRequest<'_>) -> std::result::Result<HttpResponse, HttpError> {
                Err(HttpError::Transport)
            }
        }
        let token = "github_pat_FAKE_SECRET_0123456789";
        let api = GitHubApi::new(Never, token.into(), GitHubRepoRef::new("o", "r", "main").unwrap(), Limits::default(), Default::default()).unwrap();
        assert!(!format!("{api:?}").contains("FAKE_SECRET"));
        let e = api.head().unwrap_err();
        assert!(!format!("{e:?} {e:#}").contains("FAKE_SECRET"));
    }
}
