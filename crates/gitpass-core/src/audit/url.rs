//! URL parsing and helper utilities for security audits.

/// Lowercased host of a URL (or bare host): scheme, userinfo, port, path and a leading `www.` are
/// dropped. Returns `None` for empty input or IPv6 literals, which the list does not carry.
pub(crate) fn host_of(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() { return None; }
    let rest = s.split_once("://").map_or(s, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("");
    let host = host.trim_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.starts_with('[') { return None; }
    Some(host.strip_prefix("www.").unwrap_or(&host).to_string())
}

/// Extracts the host if `url` uses plain unencrypted HTTP and points to a public host.
/// Returns None for HTTPS, non-HTTP schemes, localhost, 127.0.0.0/8, ::1, *.local, or RFC1918 private IP ranges.
/// Strips userinfo/password, port, path, query, and fragment so no secrets can leak into findings.
pub fn insecure_url_host(url: &str) -> Option<String> {
    let s = url.trim();
    if s.is_empty() { return None; }
    let rest = if s.len() >= 7 && s[..7].eq_ignore_ascii_case("http://") {
        &s[7..]
    } else {
        return None;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    let host = if host_port.starts_with('[') {
        let end = host_port.find(']')?;
        &host_port[1..end]
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    let host = host.trim_matches('.').to_ascii_lowercase();
    if host.is_empty() { return None; }

    if host == "localhost" || host.ends_with(".localhost") || host == "local" || host.ends_with(".local") {
        return None;
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(v4) => {
                if v4.is_loopback() { return None; }
                let octets = v4.octets();
                let is_rfc1918 = octets[0] == 10
                    || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                    || (octets[0] == 192 && octets[1] == 168);
                if is_rfc1918 { return None; }
            }
            std::net::IpAddr::V6(v6) => {
                if v6.is_loopback() { return None; }
            }
        }
    }

    Some(host)
}

/// The site's standard change-password page (W3C "well-known change password URL" / RFC 8615):
/// `https://<host>/.well-known/change-password`.
///
/// Only http(s) URLs with a plain DNS host are accepted. The subdomain and an explicit port are
/// kept; path, query, fragment and any userinfo are dropped; `http://` is upgraded to https; a
/// trailing dot on the host is removed. Returns `None` for empty or non-http(s) input, IP literals
/// (v4/v6), `localhost`/`*.localhost`/`local`/`*.local`, and malformed hosts.
pub fn change_password_url(item_url: &str) -> Option<String> {
    let s = item_url.trim();
    let rest = if s.get(..8).is_some_and(|p| p.eq_ignore_ascii_case("https://")) {
        &s[8..]
    } else if s.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("http://")) {
        &s[7..]
    } else {
        return None;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    if host_port.starts_with('[') { return None; } // IPv6 literal: no standard change-password page
    let (host, port) = host_port.split_once(':').map_or((host_port, None), |(h, p)| (h, Some(p)));
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() { return None; }
    // Browsers also accept shorthand IPv4 (`127.1`, `2130706433`): an all-numeric host is an IP.
    if host.split('.').all(|label| label.bytes().all(|b| b.is_ascii_digit())) { return None; }
    if host == "localhost" || host.ends_with(".localhost") || host == "local" || host.ends_with(".local") { return None; }
    if !valid_dns_name(&host) { return None; }
    let port = match port {
        Some(p) => {
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) { return None; }
            let n: u32 = p.parse().ok()?;
            if !(1..=65535).contains(&n) { return None; }
            format!(":{n}")
        }
        None => String::new(),
    };
    Some(format!("https://{host}{port}/.well-known/change-password"))
}

/// Dot-separated non-empty DNS labels (ASCII letters/digits, `-`, `_`; no leading/trailing `-`).
fn valid_dns_name(host: &str) -> bool {
    host.split('.').all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
}

/// Extracts the registrable site name from a URL or host.
/// E.g. "https://login.github.com" -> Some("github")
///      "https://github.com" -> Some("github")
///      "sub.example.co.uk" -> Some("example")
///      "amazon.co.jp" -> Some("amazon")
///      "münchen.de" -> Some("münchen")
///      "xn--mnchen-3ya.de" -> Some("xn--mnchen-3ya")
/// Returns None if no host is found or if the host is an IP address.
pub fn extract_site_name(url_or_host: &str) -> Option<String> {
    let host = host_of(url_or_host)?;
    if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    let labels: Vec<&str> = host.split('.').filter(|s| !s.is_empty()).collect();
    if labels.is_empty() {
        return None;
    }
    if labels.len() == 1 {
        return Some(labels[0].to_string());
    }
    if labels.len() >= 3 {
        let last = labels[labels.len() - 1];
        let second_last = labels[labels.len() - 2];
        const CC_SLDS: &[&str] = &[
            "co", "com", "org", "net", "edu", "gov", "ac", "ne", "or", "gen", "nom", "ind",
        ];
        const MULTI_PSL: &[&str] = &[
            "github.io", "gitlab.io", "pages.dev", "vercel.app", "herokuapp.com", "azurewebsites.net", "cloudfront.net",
        ];
        let is_cc_sld = last.len() == 2 && last.chars().all(|c| c.is_ascii_alphabetic()) && CC_SLDS.contains(&second_last);
        let pair = format!("{second_last}.{last}");
        if is_cc_sld || MULTI_PSL.contains(&pair.as_str()) {
            return Some(labels[labels.len() - 3].to_string());
        }
    }
    Some(labels[labels.len() - 2].to_string())
}

/// Pure RFC 3492 Punycode decoder for IDN labels without external dependencies.
/// Returns decoded Unicode string if `encoded` is valid punycode, or None.
pub fn decode_punycode(encoded: &str) -> Option<String> {
    let input = encoded.strip_prefix("xn--").unwrap_or(encoded);
    const BASE: u32 = 36;
    const TMIN: u32 = 1;
    const TMAX: u32 = 26;
    const SKEW: u32 = 38;
    const DAMP: u32 = 700;
    const INITIAL_BIAS: u32 = 72;
    const INITIAL_N: u32 = 128;

    fn adapt(mut delta: u32, numpoints: u32, firsttime: bool) -> u32 {
        delta = if firsttime { delta / DAMP } else { delta / 2 };
        delta += delta / numpoints;
        let mut k = 0;
        while delta > ((BASE - TMIN) * TMAX) / 2 {
            delta /= BASE - TMIN;
            k += BASE;
        }
        k + (((BASE - TMIN + 1) * delta) / (delta + SKEW))
    }

    let mut n = INITIAL_N;
    let mut i = 0u32;
    let mut bias = INITIAL_BIAS;
    let mut output = Vec::new();

    let delim = input.rfind('-');
    let (basic, mut pos) = if let Some(d) = delim {
        (&input[..d], d + 1)
    } else {
        ("", 0)
    };
    for c in basic.chars() {
        output.push(c);
    }

    let chars: Vec<char> = input.chars().collect();
    while pos < chars.len() {
        let oldi = i;
        let mut w = 1u32;
        let mut k = BASE;
        loop {
            if pos >= chars.len() { return None; }
            let c = chars[pos];
            pos += 1;
            let digit = match c {
                'a'..='z' => (c as u32) - ('a' as u32),
                'A'..='Z' => (c as u32) - ('A' as u32),
                '0'..='9' => (c as u32) - ('0' as u32) + 26,
                _ => return None,
            };
            i = i.checked_add(digit.checked_mul(w)?)?;
            let t = if k <= bias {
                TMIN
            } else if k >= bias + TMAX {
                TMAX
            } else {
                k - bias
            };
            if digit < t { break; }
            w = w.checked_mul(BASE - t)?;
            k += BASE;
        }
        let out_len = (output.len() + 1) as u32;
        bias = adapt(i - oldi, out_len, oldi == 0);
        n = n.checked_add(i / out_len)?;
        i %= out_len;
        let ch = char::from_u32(n)?;
        output.insert(i as usize, ch);
        i += 1;
    }

    Some(output.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insecure_url_host_matching() {
        // Public HTTP hosts are flagged
        assert_eq!(insecure_url_host("http://example.com"), Some("example.com".into()));
        assert_eq!(insecure_url_host("http://public.example.com/login?foo=bar"), Some("public.example.com".into()));
        assert_eq!(insecure_url_host("http://example.com:8080/path"), Some("example.com".into()));
        assert_eq!(insecure_url_host("http://172.32.0.1"), Some("172.32.0.1".into()));
        assert_eq!(insecure_url_host("http://8.8.8.8:80/dns"), Some("8.8.8.8".into()));

        // User credentials in URL: only host is returned, NO secrets leaked
        assert_eq!(insecure_url_host("http://admin:secret_pass123@public.site.test:8080/dashboard?token=xyz#hash"), Some("public.site.test".into()));

        // HTTPS is not flagged
        assert_eq!(insecure_url_host("https://example.com"), None);
        assert_eq!(insecure_url_host("https://public.example.com/path"), None);
        assert_eq!(insecure_url_host("https://localhost:8080"), None);

        // localhost and *.localhost are not flagged
        assert_eq!(insecure_url_host("http://localhost"), None);
        assert_eq!(insecure_url_host("http://localhost:3000/app"), None);
        assert_eq!(insecure_url_host("http://dev.localhost:8080"), None);

        // Loopback 127.0.0.0/8 and ::1 are not flagged
        assert_eq!(insecure_url_host("http://127.0.0.1"), None);
        assert_eq!(insecure_url_host("http://127.0.0.5:8080/test"), None);
        assert_eq!(insecure_url_host("http://127.255.255.255"), None);
        assert_eq!(insecure_url_host("http://[::1]"), None);
        assert_eq!(insecure_url_host("http://[::1]:8080/"), None);

        // *.local and local are not flagged
        assert_eq!(insecure_url_host("http://local"), None);
        assert_eq!(insecure_url_host("http://my-nas.local"), None);
        assert_eq!(insecure_url_host("http://printer.office.local:9100"), None);

        // RFC1918 private IPv4 ranges are not flagged
        assert_eq!(insecure_url_host("http://10.0.0.1"), None);
        assert_eq!(insecure_url_host("http://10.255.1.2:8080"), None);
        assert_eq!(insecure_url_host("http://172.16.0.1"), None);
        assert_eq!(insecure_url_host("http://172.24.10.20:8000"), None);
        assert_eq!(insecure_url_host("http://172.31.255.255"), None);
        assert_eq!(insecure_url_host("http://192.168.0.1"), None);
        assert_eq!(insecure_url_host("http://192.168.1.254:8443"), None);

        // Empty, non-http, or malformed
        assert_eq!(insecure_url_host(""), None);
        assert_eq!(insecure_url_host("ftp://ftp.example.com"), None);
        assert_eq!(insecure_url_host("not-a-url"), None);
    }

    #[test]
    fn test_change_password_url() {
        let well_known = "/.well-known/change-password";
        // Plain https host, subdomain kept, path/query/fragment dropped.
        assert_eq!(change_password_url("https://example.com"), Some(format!("https://example.com{well_known}")));
        assert_eq!(
            change_password_url("https://app.example.com/login?next=%2F#top"),
            Some(format!("https://app.example.com{well_known}"))
        );
        // Explicit port kept (and normalized).
        assert_eq!(
            change_password_url("https://example.com:8443/login"),
            Some(format!("https://example.com:8443{well_known}"))
        );
        assert_eq!(change_password_url("http://example.com:080"), Some(format!("https://example.com:80{well_known}")));
        // http is upgraded, scheme/host case and a trailing dot are normalized.
        assert_eq!(change_password_url("http://Example.COM"), Some(format!("https://example.com{well_known}")));
        assert_eq!(change_password_url("HTTPS://example.com."), Some(format!("https://example.com{well_known}")));
        // Userinfo never reaches the output.
        assert_eq!(
            change_password_url("https://user:secret@example.com/account"),
            Some(format!("https://example.com{well_known}"))
        );

        // IP literals (v4 and v6), local names, non-http schemes, bare hosts and empties.
        for bad in [
            "", "   ", "example.com", "not-a-url", "file:///etc/passwd", "ftp://example.com",
            "http://127.0.0.1", "http://127.0.0.5:8080", "http://8.8.8.8", "http://127.1", "http://2130706433", "https://[::1]", "https://[2001:db8::1]:8443",
            "http://localhost", "http://localhost:3000", "https://app.localhost", "http://local", "http://nas.local",
            "https://exa mple.com", "https://example.com:0", "https://example.com:", "https://example.com:notaport",
            "https://example.com:70000", "https://-bad.example.com", "https://a..example.com", "https://example.com\\evil",
        ] {
            assert_eq!(change_password_url(bad), None, "expected None for {bad:?}");
        }
    }
}
