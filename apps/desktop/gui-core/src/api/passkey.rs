/// Derive `https://<rp_id>` for a passkey item. The stored `rp_id` field wins;
/// the item's saved URL (core stores `https://<rp_id>` there) is the fallback.
/// Only a bare hostname is accepted — any scheme, path, userinfo, port, space
/// or empty value yields `None`, so no webview-supplied URL can reach the OS
/// opener through this path.
pub fn passkey_site_url(rp_id: Option<&str>, item_url: Option<&str>) -> Option<String> {
    let valid_host = |s: &str| {
        !s.is_empty()
            && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')
            && !s.starts_with(['-', '.'])
            && !s.ends_with(['-', '.'])
            && !s.contains("..")
    };
    let host = rp_id
        .map(str::trim)
        .filter(|s| valid_host(s))
        .map(|s| s.to_ascii_lowercase())
        .or_else(|| {
            item_url
                .and_then(|u| u.strip_prefix("https://"))
                .map(|rest| rest.split(['/', '?', '#']).next().unwrap_or(""))
                .filter(|h| valid_host(h))
                .map(|h| h.to_ascii_lowercase())
        });
    host.map(|h| format!("https://{h}"))
}

#[cfg(test)]
mod passkey_site_url_tests {
    use super::passkey_site_url;

    #[test]
    fn rp_id_wins_and_is_lowercased() {
        assert_eq!(
            passkey_site_url(Some("GitHub.com"), Some("https://other.example.com")),
            Some("https://github.com".to_string())
        );
    }

    #[test]
    fn falls_back_to_saved_url_host() {
        assert_eq!(
            passkey_site_url(None, Some("https://example.com/login?x=1")),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn rejects_non_hostnames() {
        assert_eq!(passkey_site_url(Some("https://evil.example.com"), None), None);
        assert_eq!(passkey_site_url(Some("example.com/evil"), None), None);
        assert_eq!(passkey_site_url(Some("user@example.com"), None), None);
        assert_eq!(passkey_site_url(Some("example.com:8443"), None), None);
        assert_eq!(passkey_site_url(Some("exa mple.com"), None), None);
        assert_eq!(passkey_site_url(Some(""), None), None);
        assert_eq!(passkey_site_url(None, Some("http://example.com")), None);
        assert_eq!(passkey_site_url(None, None), None);
    }
}

