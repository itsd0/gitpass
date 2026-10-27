//! Origin matching between a requesting page origin and an item's stored URL.
//!
//! Rules (see README "Origin matching"):
//! 1. The requesting origin must be `https://…`, or `http://` on a loopback host
//!    (`localhost`, `*.localhost`, `127.0.0.1`, `[::1]`). Anything else is refused.
//! 2. The item URL is parsed with the WHATWG `url` crate (IDNA -> punycode, lowercase).
//!    A bare host like `github.com` is treated as `https://github.com`.
//! 3. Schemes must be equal (an `http` item never fills on `https` and vice versa).
//! 4. Hosts: IP addresses and loopback names must match exactly, including port.
//!    Otherwise the registrable domain (eTLD+1, from the Public Suffix List via the `psl`
//!    crate) of both hosts must be equal. If a host has no registrable domain (it *is* a public
//!    suffix, e.g. `github.io`), exact host equality is required.
//!
//! Consequence: `paypal.com.evil.io` has eTLD+1 `evil.io` and never matches `paypal.com`;
//! `login.example.com` matches `www.example.com` (same site; standard password-manager behaviour).
use url::{Host, Url};

fn parse_loose(s: &str) -> Option<Url> {
    let s = s.trim();
    if s.is_empty() { return None; }
    let u = if s.contains("://") { Url::parse(s).ok()? } else { Url::parse(&format!("https://{s}")).ok()? };
    if !matches!(u.scheme(), "https" | "http") || u.host().is_none() { return None; }
    Some(u)
}

fn is_loopback(u: &Url) -> bool {
    match u.host() {
        Some(Host::Domain(d)) => d == "localhost" || d.ends_with(".localhost"),
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// Parse and validate a requesting page origin (rule 1). Strict: needs an explicit scheme.
pub fn parse_request_origin(origin: &str) -> Option<Url> {
    let u = Url::parse(origin.trim()).ok()?;
    match u.scheme() {
        "https" if u.host().is_some() => Some(u),
        "http" if is_loopback(&u) => Some(u),
        _ => None,
    }
}

/// Registrable domain (eTLD+1) of a domain host, if any.
pub fn registrable_domain(host: &str) -> Option<String> {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    let d = psl::domain_str(&h)?;
    // psl applies the implicit "*" rule to unknown TLDs; a single-label host has no eTLD+1.
    if !d.contains('.') { return None; }
    Some(d.to_string())
}

/// True if an item stored with `item_url` may be filled into a page at `origin`.
pub fn origin_matches(item_url: &str, origin: &str) -> bool {
    let (Some(o), Some(i)) = (parse_request_origin(origin), parse_loose(item_url)) else { return false };
    if o.scheme() != i.scheme() { return false; }
    match (o.host(), i.host()) {
        (Some(Host::Domain(od)), Some(Host::Domain(id))) => {
            if is_loopback(&o) || is_loopback(&i) {
                return od == id && o.port_or_known_default() == i.port_or_known_default();
            }
            let od = od.trim_end_matches('.');
            let id = id.trim_end_matches('.');
            match (registrable_domain(od), registrable_domain(id)) {
                (Some(a), Some(b)) => a == b,
                _ => od == id,
            }
        }
        (Some(a), Some(b)) => a == b && o.port_or_known_default() == i.port_or_known_default(),
        _ => false,
    }
}
