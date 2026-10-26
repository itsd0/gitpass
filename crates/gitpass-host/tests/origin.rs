use gitpass_host::origin::*;

#[test]
fn same_site_matches() {
    assert!(origin_matches("https://paypal.com", "https://paypal.com"));
    assert!(origin_matches("paypal.com", "https://www.paypal.com"));
    assert!(origin_matches("https://login.example.co.uk/path?q=1", "https://www.example.co.uk"));
    assert!(origin_matches("https://GitHub.com/login", "https://github.com:443"));
    assert!(origin_matches("http://localhost:3000", "http://localhost:3000"));
    assert!(origin_matches("https://foo.github.io", "https://foo.github.io"));
}

#[test]
fn phishing_lookalikes_do_not_match() {
    for evil in [
        "https://paypal.com.evil.io",
        "https://paypal.com-login.io",
        "https://paypa1.com",
        "https://evilpaypal.com",
        "https://paypal.co",
        "https://xn--pypal-4ve.com", // pаypal with Cyrillic а
        "https://pаypal.com",
        "https://paypal.com@evil.io",
    ] {
        assert!(!origin_matches("https://paypal.com", evil), "{evil} must not match");
    }
}

#[test]
fn public_suffix_hosts_are_isolated() {
    // github.io is a public suffix: different user pages are different sites
    assert!(!origin_matches("https://alice.github.io", "https://bob.github.io"));
    assert!(!origin_matches("https://a.example.co.uk", "https://b.co.uk"));
    assert!(!origin_matches("https://co.uk", "https://example.co.uk"));
}

#[test]
fn scheme_and_transport_rules() {
    assert!(!origin_matches("https://example.com", "http://example.com"), "no http");
    assert!(!origin_matches("http://example.com", "http://example.com"), "plain http refused except localhost");
    assert!(!origin_matches("http://localhost:3000", "https://localhost:3000"));
    assert!(!origin_matches("http://localhost:3000", "http://localhost:4000"), "loopback port must match");
    assert!(!origin_matches("https://example.com", "file:///etc/passwd"));
    assert!(!origin_matches("https://example.com", "chrome-extension://abc/"));
    assert!(!origin_matches("https://example.com", "example.com"), "request origin needs explicit scheme");
    assert!(!origin_matches("", "https://example.com"));
    assert!(!origin_matches("javascript:alert(1)", "https://example.com"));
}

#[test]
fn ip_hosts_exact() {
    assert!(origin_matches("https://192.168.1.10", "https://192.168.1.10"));
    assert!(!origin_matches("https://192.168.1.10", "https://192.168.1.11"));
    assert!(origin_matches("http://127.0.0.1:8080", "http://127.0.0.1:8080"));
    assert!(parse_request_origin("http://10.0.0.1").is_none());
}

#[test]
fn registrable_domain_examples() {
    assert_eq!(registrable_domain("paypal.com.evil.io").as_deref(), Some("evil.io"));
    assert_eq!(registrable_domain("a.b.example.co.uk").as_deref(), Some("example.co.uk"));
    assert_eq!(registrable_domain("github.io"), None);
    assert_eq!(registrable_domain("localhost"), None);
}
