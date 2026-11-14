//! Desktop host role for pairing v2.
//!
//! The desktop shows `qr` and never learns a code to display. The phone shows
//! the 6-digit code; the person types it into [`App::pairing_host_confirm`].
//! Owner, repository, branch and the fine-grained PAT come from the vault's git
//! `origin` and the git-ignored `.gitpass-sync-token` file, not from UI arguments.
//! `pairing_host_listen` binds a short-lived TCP listener on LAN addresses only
//! (never `0.0.0.0`). The phone connects to an address from the QR. Bytes are
//! the pairing frames; this module does not decrypt them.
use super::super::dto::PairingHostPrepared;
use super::App;
use anyhow::{anyhow, Result};
use gitpass_core::crypto::SecretKey;
use gitpass_core::pairing::{self, HostSession, PairingError, PairingRequest, PairingStep};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use rand::Rng;

pub(crate) struct PairingHold {
    host: Option<HostSession>,
    listeners: Vec<TcpListener>,
    stream: Option<TcpStream>,
    cancel: Arc<AtomicBool>,
    /// Test builds can move the clock forward. Release builds have no override.
    #[cfg(test)]
    test_now: Option<i64>,
}

impl Default for PairingHold {
    fn default() -> Self {
        Self {
            host: None,
            listeners: Vec::new(),
            stream: None,
            cancel: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            test_now: None,
        }
    }
}

fn clock_from(g: &PairingHold) -> i64 {
    #[cfg(test)]
    if let Some(now) = g.test_now {
        return now;
    }
    let _ = g;
    pairing::now_unix()
}

enum Poll {
    Got(TcpStream),
    Wait,
    Fail(&'static str),
    Stop(&'static str),
}

enum ReadWait {
    Bytes(usize),
    Again,
    Eof,
    Expired,
    Fail,
    Closed,
}

/// Bytes to write on the pairing connection, plus a fixed error string when the step failed.
#[derive(Debug)]
pub struct PairingTurn {
    pub outbound: Vec<u8>,
    pub phase: String,
    pub error: Option<String>,
}

pub(crate) fn clear_host_on_lock(slots: &Mutex<PairingHold>, dir: &Path) {
    let mut g = slots.lock().unwrap_or_else(|e| e.into_inner());
    g.cancel.store(true, Ordering::SeqCst);
    g.listeners.clear();
    g.stream.take();
    let Some(host) = g.host.take() else { return };
    let sid = host.session_id().to_string();
    drop(host);
    if let Ok(mut consumed) = pairing::load_consumed(dir) {
        consumed.cancel_accept(&sid);
        let _ = pairing::save_consumed(dir, &consumed);
    }
}

const SYNC_TOKEN_FILE: &str = ".gitpass-sync-token";

/// Owner, repository and branch from the vault's `origin`.
/// The fine-grained PAT comes from `.gitpass-sync-token` when that file exists.
/// Otherwise `typed` is used and is not written to disk. Errors do not include
/// the token or the URL.
fn load_github_sync(root: &Path, typed: Option<&str>) -> Result<(String, String, String, String)> {
    let url = super::sync::git(root, &["remote", "get-url", "origin"])
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("sync is not configured"))?;
    let branch = super::sync::git(root, &["symbolic-ref", "--short", "HEAD"])
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("sync is not configured"))?;
    let (owner, repo) = github_owner_repo(&url).ok_or_else(|| anyhow!("sync is not configured"))?;
    if root.join(SYNC_TOKEN_FILE).is_file() {
        gitpass_core::device::ensure_ignored(root, SYNC_TOKEN_FILE)
            .map_err(|_| anyhow!("sync token is not configured"))?;
        let stored = stored_token(root).ok_or_else(|| anyhow!("sync token is not configured"))?;
        return Ok((owner, repo, branch, stored));
    }
    let typed = typed
        .map(str::trim)
        .filter(|t| !t.is_empty() && !t.contains('\n') && !t.contains('\0'));
    let Some(token) = typed else {
        return Err(anyhow!("sync token is not configured"));
    };
    Ok((owner, repo, branch, token.to_string()))
}

fn stored_token(root: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(root.join(SYNC_TOKEN_FILE)).ok()?;
    let token = raw.trim();
    if token.is_empty() || token.contains('\n') || token.contains('\0') {
        return None;
    }
    Some(token.to_string())
}

fn github_owner_repo(url: &str) -> Option<(String, String)> {
    let path = url
        .trim()
        .strip_prefix("https://github.com/")
        .or_else(|| url.trim().strip_prefix("ssh://git@github.com/"))
        .or_else(|| url.trim().strip_prefix("git@github.com:"))?;
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = path.split('/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty())?;
    if parts.next().is_some() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

fn turn(step: PairingStep, phase: &str) -> PairingTurn {
    let failed = step.error.is_some();
    PairingTurn {
        outbound: step.outbound,
        phase: if failed { "failed".into() } else { phase.into() },
        error: step.error.map(|e| e.to_string()),
    }
}

impl App {
    /// Open a host session for `addresses` and `port`. Returns the QR only.
    pub fn pairing_host_begin(&self, addresses: &[String], port: u16) -> Result<PairingHostPrepared> {
        self.pairing_host_begin_with(addresses, port, None)
    }

    pub(crate) fn pairing_host_begin_with(
        &self,
        addresses: &[String],
        port: u16,
        typed_token: Option<&str>,
    ) -> Result<PairingHostPrepared> {
        let (account_id, secret, owner, repo, branch, token) = self.with(|u| {
            let (owner, repo, branch, token) = load_github_sync(&u.session.root, typed_token)?;
            Ok((
                u.session.fmt.account_id.clone(),
                SecretKey(u.secret.0),
                owner,
                repo,
                branch,
                token,
            ))
        })?;
        let request = {
            let token = zeroize::Zeroizing::new(token);
            PairingRequest::new(&account_id, &owner, &repo, &branch, secret, &token)?
        };
        self.with(|_| {
            let previous = {
                let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
                g.host.take()
            };
            let mut consumed = pairing::load_consumed(&self.settings_dir)?;
            if let Some(prev) = previous.as_ref() {
                consumed.cancel_accept(prev.session_id());
            }
            let now = pairing::now_unix();
            let (host, qr) = match HostSession::begin(addresses, port, request, now, &mut consumed) {
                Ok(v) => v,
                Err(e) => {
                    if let Some(prev) = previous {
                        self.pairing.lock().map_err(|_| anyhow!("poisoned"))?.host = Some(prev);
                    }
                    return Err(e.into());
                }
            };
            if let Err(e) = pairing::save_consumed(&self.settings_dir, &consumed) {
                if let Some(prev) = previous {
                    self.pairing.lock().map_err(|_| anyhow!("poisoned"))?.host = Some(prev);
                }
                return Err(e.into());
            }
            let prepared = PairingHostPrepared {
                qr,
                expires_at: host.expires_at(),
                port: host.port(),
                addresses: host.addresses().to_vec(),
            };
            self.pairing.lock().map_err(|_| anyhow!("poisoned"))?.host = Some(host);
            Ok(prepared)
        })
    }

    /// Feed bytes from the phone. The returned bytes are the desktop's reply.
    pub fn pairing_host_push(&self, bytes: &[u8]) -> Result<PairingTurn> {
        self.with(|_| {
            let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
            let now = clock_from(&g);
            let host = g.host.as_mut().ok_or(PairingError::NotReady)?;
            let step = host.push(bytes, now);
            let phase = host.phase().to_string();
            Ok(turn(step, &phase))
        })
    }

    /// Release the grant when `code` is the code the phone displayed.
    /// The third wrong code ends the session and marks it used.
    /// The socket write happens after the vault lock is released, with a timeout,
    /// so a peer that stops reading cannot stall the rest of the app.
    pub fn pairing_host_confirm(&self, code: &str) -> Result<PairingTurn> {
        self.with(|_| Ok(()))?;
        let (out, sid, exp, now, succeeded, ended, mut stream) = {
            let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
            let now = clock_from(&g);
            let host = g.host.as_mut().ok_or(PairingError::NotReady)?;
            let sid = host.session_id().to_string();
            let exp = host.expires_at();
            let step = host.confirm(code, now);
            let phase = host.phase().to_string();
            let ended = matches!(
                step.error,
                Some(PairingError::Exhausted | PairingError::Expired)
            );
            let succeeded = step.error.is_none() && phase == "done";
            let out = turn(step, &phase);
            let stream = if out.outbound.is_empty() { None } else { g.stream.take() };
            (out, sid, exp, now, succeeded, ended, stream)
        };
        if let Some(mut stream) = stream.take() {
            let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
            if stream.write_all(&out.outbound).and_then(|_| stream.flush()).is_err() {
                self.close_link(true);
                return Err(anyhow!("pairing connection closed"));
            }
            if succeeded || ended {
                let mut consumed = pairing::load_consumed(&self.settings_dir)?;
                consumed.mark_released(&sid, exp, now)?;
                pairing::save_consumed(&self.settings_dir, &consumed)?;
                let _ = stream.shutdown(Shutdown::Both);
                self.close_link(true);
            } else {
                let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
                g.stream = Some(stream);
            }
            return Ok(out);
        }
        if succeeded || ended {
            let mut consumed = pairing::load_consumed(&self.settings_dir)?;
            consumed.mark_released(&sid, exp, now)?;
            pairing::save_consumed(&self.settings_dir, &consumed)?;
            self.close_link(true);
        }
        Ok(out)
    }

    /// Bind on this machine's LAN addresses and publish them in the QR.
    /// There is no code in the result. An empty LAN returns a fixed error.
    /// `typed_token` is used only when `.gitpass-sync-token` is missing, and it is never written.
    pub fn pairing_host_listen_with(&self, typed_token: Option<&str>) -> Result<PairingHostPrepared> {
        let ips = lan_ips();
        if ips.is_empty() {
            return Err(anyhow!("pairing needs a local network address"));
        }
        self.listen_on(&ips, typed_token)
    }

    pub fn pairing_host_listen(&self) -> Result<PairingHostPrepared> {
        self.pairing_host_listen_with(None)
    }

    /// Bind `ips` on one shared port. Tests pass the loopback address.
    /// Production uses [`App::pairing_host_listen`], which skips loopback.
    #[cfg(test)]
    pub fn pairing_host_listen_on(&self, ips: &[IpAddr]) -> Result<PairingHostPrepared> {
        self.listen_on(ips, None)
    }

    fn listen_on(&self, ips: &[IpAddr], typed_token: Option<&str>) -> Result<PairingHostPrepared> {
        self.with(|_| Ok(()))?;
        let (listeners, port, bound) = bind_shared_port(ips)?;
        let prepared = self.pairing_host_begin_with(&bound, port, typed_token)?;
        let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
        g.cancel.store(false, Ordering::SeqCst);
        g.listeners = listeners;
        g.stream.take();
        Ok(prepared)
    }

    /// Accept one phone and run the exchange until the code can be typed.
    /// The listening port closes as soon as that connection is accepted.
    pub fn pairing_host_accept(&self, timeout: Duration) -> Result<()> {
        self.with(|_| Ok(()))?;
        let deadline = Instant::now() + timeout;
        let stream = self.accept_one(deadline)?;
        {
            let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
            g.listeners.clear();
            let _ = stream.set_nodelay(true);
            let _ = stream.set_nonblocking(false);
            g.stream = Some(stream);
        }
        self.pump_until_code(deadline)
    }

    /// Test-only: treat the session as expired without waiting two minutes.
    #[cfg(test)]
    pub fn pairing_host_expire_for_test(&self) {
        if let Ok(mut g) = self.pairing.lock() {
            if let Some(host) = g.host.as_ref() {
                g.test_now = Some(host.expires_at());
            }
        }
    }

    fn accept_one(&self, deadline: Instant) -> Result<TcpStream> {
        loop {
            if Instant::now() >= deadline {
                return Err(anyhow!("pairing timed out"));
            }
            let poll = {
                let g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
                if g.cancel.load(Ordering::SeqCst) || g.listeners.is_empty() || g.host.as_ref().is_none() {
                    Poll::Fail("pairing failed")
                } else if clock_from(&g) >= g.host.as_ref().unwrap().expires_at() {
                    Poll::Stop("pairing session expired")
                } else {
                    let mut found = None;
                    let mut failed = false;
                    for listener in &g.listeners {
                        let _ = listener.set_nonblocking(true);
                        match listener.accept() {
                            Ok((stream, _)) => {
                                found = Some(stream);
                                break;
                            }
                            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                            Err(_) => {
                                failed = true;
                                break;
                            }
                        }
                    }
                    if failed {
                        Poll::Stop("pairing failed")
                    } else if let Some(stream) = found {
                        Poll::Got(stream)
                    } else {
                        Poll::Wait
                    }
                }
            };
            match poll {
                Poll::Got(stream) => return Ok(stream),
                Poll::Wait => thread::sleep(Duration::from_millis(20)),
                Poll::Fail(msg) => return Err(anyhow!(msg)),
                Poll::Stop(msg) => {
                    self.close_link(true);
                    return Err(anyhow!(msg));
                }
            }
        }
    }

    fn pump_until_code(&self, deadline: Instant) -> Result<()> {
        let mut buf = [0u8; 8192];
        loop {
            if Instant::now() >= deadline {
                self.close_link(true);
                return Err(anyhow!("pairing timed out"));
            }
            let read = {
                let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
                if g.cancel.load(Ordering::SeqCst) || g.host.as_ref().is_none() {
                    ReadWait::Fail
                } else if clock_from(&g) >= g.host.as_ref().unwrap().expires_at() {
                    ReadWait::Expired
                } else if let Some(stream) = g.stream.as_mut() {
                    let left = deadline.saturating_duration_since(Instant::now());
                    let wait = left.min(Duration::from_millis(200));
                    let wait = if wait.is_zero() { Duration::from_millis(1) } else { wait };
                    if stream.set_read_timeout(Some(wait)).is_err() {
                        ReadWait::Closed
                    } else {
                        match stream.read(&mut buf) {
                            Ok(0) => ReadWait::Eof,
                            Ok(n) => ReadWait::Bytes(n),
                            Err(e) if e.kind() == ErrorKind::TimedOut || e.kind() == ErrorKind::WouldBlock => {
                                ReadWait::Again
                            }
                            Err(_) => ReadWait::Closed,
                        }
                    }
                } else {
                    ReadWait::Closed
                }
            };
            let n = match read {
                ReadWait::Bytes(n) => n,
                ReadWait::Again => continue,
                ReadWait::Eof => {
                    let turn = self.finish_link()?;
                    self.close_link(true);
                    return Err(anyhow!(turn.error.unwrap_or_else(|| "pairing connection closed".into())));
                }
                ReadWait::Expired => {
                    self.close_link(true);
                    return Err(anyhow!("pairing session expired"));
                }
                ReadWait::Fail => {
                    self.close_link(true);
                    return Err(anyhow!("pairing failed"));
                }
                ReadWait::Closed => {
                    self.close_link(true);
                    return Err(anyhow!("pairing connection closed"));
                }
            };
            let turn = match self.pairing_host_push(&buf[..n]) {
                Ok(turn) => turn,
                Err(e) => {
                    self.close_link(true);
                    return Err(e);
                }
            };
            if !turn.outbound.is_empty() && self.write_link(&turn.outbound).is_err() {
                self.close_link(true);
                return Err(anyhow!("pairing connection closed"));
            }
            if let Some(err) = turn.error {
                self.close_link(true);
                return Err(anyhow!(err));
            }
            if turn.phase == "code" {
                return Ok(());
            }
            if turn.phase == "done" || turn.phase == "failed" {
                self.close_link(true);
                return Err(anyhow!("pairing failed"));
            }
        }
    }

    fn finish_link(&self) -> Result<PairingTurn> {
        self.with(|_| {
            let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
            let host = g.host.as_mut().ok_or(PairingError::NotReady)?;
            match host.finish() {
                Ok(()) => Ok(PairingTurn { outbound: Vec::new(), phase: host.phase().to_string(), error: None }),
                Err(e) => Ok(turn(
                    PairingStep { outbound: Vec::new(), error: Some(e) },
                    "failed",
                )),
            }
        })
    }

    fn write_link(&self, bytes: &[u8]) -> Result<()> {
        let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
        let stream = g.stream.as_mut().ok_or_else(|| anyhow!("pairing connection closed"))?;
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        stream.write_all(bytes).map_err(|_| anyhow!("pairing connection closed"))?;
        let _ = stream.flush();
        Ok(())
    }

    fn close_link(&self, drop_host: bool) {
        if let Ok(mut g) = self.pairing.lock() {
            g.listeners.clear();
            if let Some(stream) = g.stream.take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
            if drop_host {
                g.host.take();
            }
        }
    }

    /// Drop the host session and allow a new QR until the old one would have expired.
    pub fn pairing_host_cancel(&self) -> Result<()> {
        let previous = {
            let mut g = self.pairing.lock().map_err(|_| anyhow!("poisoned"))?;
            g.cancel.store(true, Ordering::SeqCst);
            g.listeners.clear();
            if let Some(stream) = g.stream.take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
            g.host.take()
        };
        if let Some(prev) = previous {
            let mut consumed = pairing::load_consumed(&self.settings_dir)?;
            consumed.cancel_accept(prev.session_id());
            pairing::save_consumed(&self.settings_dir, &consumed)?;
        }
        Ok(())
    }
}

/// Bind every address on one port. A single address may use an ephemeral port.
/// Errors never include the address or the OS message.
fn bind_shared_port(ips: &[IpAddr]) -> Result<(Vec<TcpListener>, u16, Vec<String>)> {
    if ips.is_empty() {
        return Err(anyhow!("pairing needs a local network address"));
    }
    if ips.len() == 1 {
        let listener = TcpListener::bind((ips[0], 0)).map_err(|_| anyhow!("pairing listener could not start"))?;
        let addr = listener.local_addr().map_err(|_| anyhow!("pairing listener could not start"))?;
        return Ok((vec![listener], addr.port(), vec![addr.ip().to_string()]));
    }
    for _ in 0..32 {
        let port = rand::thread_rng().gen_range(49152..=65535);
        let mut listeners = Vec::new();
        let mut bound = Vec::new();
        let mut ok = true;
        for ip in ips {
            match TcpListener::bind((*ip, port)) {
                Ok(listener) => match listener.local_addr() {
                    Ok(addr) => {
                        bound.push(addr.ip().to_string());
                        listeners.push(listener);
                    }
                    Err(_) => {
                        ok = false;
                        break;
                    }
                },
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && listeners.len() == ips.len() {
            return Ok((listeners, port, bound));
        }
    }
    Err(anyhow!("pairing listener could not start"))
}

/// LAN addresses suitable for the QR. Loopback, unspecified, multicast and
/// IPv6 link-local addresses are left out. A failure returns an empty list.
fn lan_ips() -> Vec<IpAddr> {
    #[cfg(unix)]
    {
        unix_lan_ips()
    }
    #[cfg(not(unix))]
    {
        Vec::new()
    }
}

#[cfg(unix)]
fn unix_lan_ips() -> Vec<IpAddr> {
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs writes a list we free with freeifaddrs.
    let rc = unsafe { libc::getifaddrs(&mut head) };
    if rc != 0 {
        return Vec::new();
    }
    let _guard = IfaddrList(head);
    let mut out = Vec::new();
    let mut cur = head;
    while !cur.is_null() {
        // SAFETY: cur is a node in the list getifaddrs returned, not yet freed.
        let (flags, addr, next) = unsafe {
            let ifa = &*cur;
            (ifa.ifa_flags, ifa.ifa_addr, ifa.ifa_next)
        };
        cur = next;
        let up = flags & (libc::IFF_UP as u32) != 0;
        let loopback = flags & (libc::IFF_LOOPBACK as u32) != 0;
        if !up || loopback || addr.is_null() {
            continue;
        }
        // SAFETY: ifa_addr is non-null and points at a sockaddr for this family.
        let family = unsafe { (*addr).sa_family } as i32;
        if family == libc::AF_INET {
            let ip = unsafe {
                let sin = &*(addr as *const libc::sockaddr_in);
                Ipv4Addr::from(sin.sin_addr.s_addr.to_ne_bytes())
            };
            if ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast() || ip.is_loopback() {
                continue;
            }
            out.push(IpAddr::V4(ip));
        } else if family == libc::AF_INET6 {
            let ip = unsafe {
                let sin = &*(addr as *const libc::sockaddr_in6);
                Ipv6Addr::from(sin.sin6_addr.s6_addr)
            };
            if ip.is_unspecified() || ip.is_multicast() || ip.is_loopback() || ip.is_unicast_link_local() {
                continue;
            }
            if ip.to_ipv4_mapped().is_some() {
                continue;
            }
            out.push(IpAddr::V6(ip));
        }
    }
    out.sort();
    out.dedup();
    out.truncate(8);
    out
}

#[cfg(unix)]
struct IfaddrList(*mut libc::ifaddrs);

#[cfg(unix)]
impl Drop for IfaddrList {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the pointer came from getifaddrs and is freed once.
            unsafe { libc::freeifaddrs(self.0) }
        }
    }
}

#[cfg(test)]
mod lan_tests {
    use super::lan_ips;

    #[test]
    fn discovered_addresses_are_not_loopback() {
        for ip in lan_ips() {
            assert!(!ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast());
        }
    }
}
