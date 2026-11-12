//! Two-way sync for the mobile apps on the shared core engine (#455).
//!
//! The merge, the safety checks and the GitHub Git Data API protocol all run
//! in `gitpass-core` (`sync_remote::sync_with_remote` over
//! `github_api::GitHubApi`), the same engine desktop and CLI use. The only
//! thing the app supplies is the HTTPS round trip ([`HttpTransport`]), so the
//! platform network stack (proxy, TLS trust store) is used and the native
//! library carries no TLS code. The app never parses GitHub JSON or vault
//! data for sync.
//!
//! Secrets: the token is passed in per call (core validates it is a
//! fine-grained PAT) and never stored, logged or returned. The app's
//! transport adds the `Authorization` header itself, so the token never
//! crosses the bridge inside a request record. Errors are fixed text: no token, URL, file path,
//! response body or item data.
use crate::{GitpassError, Vault};
use gitpass_core::github_api::{
    GitHubApi, GitHubApiError, GitHubRepoRef, HttpClient, HttpError, HttpMethod, HttpRequest, HttpResponse, Limits,
};
use gitpass_core::sync_remote::{sync_with_remote, SyncOptions};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// One header the transport must send.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

/// One HTTPS request to `https://api.github.com` + `path`.
///
/// Transport contract (same as core's `HttpRequest`): send
/// `Authorization: Bearer <token>` with the token the app passed to
/// [`Vault::sync_github`], in that header only (the token is deliberately
/// not a field here, so a logged request cannot leak it); add `headers`; never
/// follow redirects (return the 3xx status); stop reading after
/// `max_response_bytes` with [`HttpTransportError::TooLarge`]; honour
/// `timeout_ms`; never cache; never log the request.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HttpTransportRequest {
    /// "GET", "POST", "PATCH" or "PUT".
    pub method: String,
    /// Starts with `/repos/`; never contains credentials.
    pub path: String,
    pub headers: Vec<HttpHeader>,
    /// JSON body for POST/PATCH/PUT.
    pub body: Option<Vec<u8>>,
    pub max_response_bytes: u64,
    pub timeout_ms: u64,
}

/// Status, bounded body and the rate-limit headers the engine reads.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HttpTransportResponse {
    pub status: u16,
    pub body: Vec<u8>,
    /// `retry-after`, seconds.
    pub retry_after: Option<u64>,
    /// `x-ratelimit-remaining`.
    pub rate_limit_remaining: Option<u64>,
    /// `x-ratelimit-reset`, unix seconds.
    pub rate_limit_reset: Option<u64>,
}

/// Transport failure. Carries no text from lower layers on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum HttpTransportError {
    #[error("timeout")]
    Timeout,
    #[error("cancelled")]
    Cancelled,
    #[error("response too large")]
    TooLarge,
    #[error("redirect")]
    Redirect,
    #[error("transport failure")]
    Transport,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for HttpTransportError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        HttpTransportError::Transport
    }
}

/// The HTTPS round trip, implemented by the app (Kotlin/Swift) or by tests.
#[uniffi::export(with_foreign)]
pub trait HttpTransport: Send + Sync {
    fn send(&self, request: HttpTransportRequest) -> Result<HttpTransportResponse, HttpTransportError>;
}

/// Cancels a running [`Vault::sync_github`]: checked before every request
/// and between steps. Cancel it when the vault locks.
#[derive(Debug, Default, uniffi::Object)]
pub struct SyncCancellation {
    flag: Arc<AtomicBool>,
}

#[uniffi::export]
impl SyncCancellation {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// The GitHub repository and branch holding the vault. Not secret.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GitHubSyncTarget {
    pub owner: String,
    pub repo: String,
    pub branch: String,
}

/// What a sync did. Ids and field names only — never item data.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SyncReport {
    /// Remote head after the sync.
    pub head: String,
    /// A commit was pushed.
    pub pushed: bool,
    /// The local vault changed (remote edits/deletes applied).
    pub local_changed: bool,
    /// Conflicts, already resolved fail-safe; one line each
    /// (`vault/item: kind`).
    pub conflicts: Vec<String>,
    /// Items field-merged with the remote.
    pub merged_items: u32,
    /// Local-only items that were pushed.
    pub local_only_items: u32,
    /// Attempts used (> 1 when the remote moved concurrently).
    pub attempts: u32,
}

/// Adapts the app transport to core's [`HttpClient`].
struct ForeignHttp(Arc<dyn HttpTransport>);

impl HttpClient for ForeignHttp {
    fn send(&self, req: &HttpRequest<'_>) -> Result<HttpResponse, HttpError> {
        if req.cancel.load(Ordering::SeqCst) {
            return Err(HttpError::Cancelled);
        }
        let request = HttpTransportRequest {
            method: match req.method {
                HttpMethod::Get => "GET",
                HttpMethod::Post => "POST",
                HttpMethod::Patch => "PATCH",
                HttpMethod::Put => "PUT",
            }
            .to_string(),
            path: req.path.to_string(),
            headers: req.headers.iter().map(|(n, v)| HttpHeader { name: (*n).into(), value: (*v).into() }).collect(),
            body: req.body.map(|b| b.to_vec()),
            max_response_bytes: req.max_response_bytes as u64,
            timeout_ms: req.timeout.as_millis().min(u64::MAX as u128) as u64,
        };
        let resp = self.0.send(request).map_err(|e| match e {
            HttpTransportError::Timeout => HttpError::Timeout,
            HttpTransportError::Cancelled => HttpError::Cancelled,
            HttpTransportError::TooLarge => HttpError::TooLarge,
            HttpTransportError::Redirect => HttpError::Redirect,
            HttpTransportError::Transport => HttpError::Transport,
        })?;
        if req.cancel.load(Ordering::SeqCst) {
            return Err(HttpError::Cancelled);
        }
        // Enforce the bound even if a transport ignores it.
        if resp.body.len() > req.max_response_bytes {
            return Err(HttpError::TooLarge);
        }
        Ok(HttpResponse {
            status: resp.status,
            body: resp.body,
            retry_after: resp.retry_after,
            rate_limit_remaining: resp.rate_limit_remaining,
            rate_limit_reset: resp.rate_limit_reset,
        })
    }
}

const GENERIC_FAILURE: &str = "Sync failed; check the repository settings and the connection, then try again";

/// Fixed-text error for the UI. GitHub transport errors and the engine's own
/// refusals are fixed strings without paths or data; everything else (I/O,
/// decode, lower layers) collapses to one generic message.
fn sync_error(e: &anyhow::Error) -> GitpassError {
    if let Some(g) = e.chain().find_map(|c| c.downcast_ref::<GitHubApiError>()) {
        return GitpassError::Generic(g.to_string());
    }
    let top = e.to_string();
    let engine_refusal = (top.contains("refusing") || top.contains("try again later") || top.contains("nothing was changed"))
        && !top.contains('/')
        && !top.contains('\\')
        && top.len() <= 240;
    GitpassError::Generic(if engine_refusal { top } else { GENERIC_FAILURE.to_string() })
}

#[uniffi::export]
impl Vault {
    /// Pull, field-merge and push this vault with a GitHub repository on the
    /// shared core engine. Deletes propagate both ways; conflicts are resolved
    /// fail-safe and listed in the report; a rolled-back remote, foreign files
    /// on the remote and a remote password/KDF change are refused. An empty
    /// repository is initialized with this vault. The token must be a
    /// fine-grained PAT (`github_pat_…`) with Contents read/write on the repo.
    pub fn sync_github(
        &self,
        target: GitHubSyncTarget,
        token: String,
        transport: Arc<dyn HttpTransport>,
        cancellation: Arc<SyncCancellation>,
    ) -> Result<SyncReport, GitpassError> {
        let token = zeroize::Zeroizing::new(token);
        let repo_ref = GitHubRepoRef::new(&target.owner, &target.repo, &target.branch)
            .map_err(|_| GitpassError::Generic(GitHubApiError::InvalidConfig.to_string()))?;
        let session = self.session.lock().map_err(|_| GitpassError::Generic("vault lock poisoned".into()))?;
        let mut api = GitHubApi::new(ForeignHttp(transport), token.to_string(), repo_ref, Limits::default(), cancellation.flag.clone())
            .map_err(|e| sync_error(&e))?;
        let remote_id = api.remote_id();
        let opts = SyncOptions { message: "gitpass mobile sync".into(), ..SyncOptions::default() };
        let out = sync_with_remote(&session, &mut api, &remote_id, &opts).map_err(|e| {
            if cancellation.is_cancelled() {
                GitpassError::Generic(GitHubApiError::Cancelled.to_string())
            } else {
                sync_error(&e)
            }
        })?;
        Ok(SyncReport {
            head: out.head,
            pushed: out.pushed,
            local_changed: out.local_changed,
            conflicts: out.conflicts.iter().map(|c| c.describe()).collect(),
            merged_items: out.merged.len().min(u32::MAX as usize) as u32,
            local_only_items: out.local_only.len().min(u32::MAX as usize) as u32,
            attempts: out.attempts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_fixed_text() {
        let e = anyhow::Error::new(GitHubApiError::Auth).context("while syncing /tmp/secret/path");
        let GitpassError::Generic(m) = sync_error(&e);
        assert_eq!(m, GitHubApiError::Auth.to_string());

        let io = anyhow::anyhow!("No such file: /data/user/0/app/vault/format.json");
        let GitpassError::Generic(m) = sync_error(&io);
        assert_eq!(m, GENERIC_FAILURE);

        let refusal = anyhow::anyhow!("remote tree contains a file outside the vault allowlist; refusing to sync");
        let GitpassError::Generic(m) = sync_error(&refusal);
        assert!(m.contains("refusing"));

        let refusal_with_path = anyhow::anyhow!("refusing to store non-vault path secrets/x.txt");
        let GitpassError::Generic(m) = sync_error(&refusal_with_path);
        assert_eq!(m, GENERIC_FAILURE);
    }
}
