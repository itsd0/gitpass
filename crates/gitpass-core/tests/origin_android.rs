//! Android app identity in item URLs (#475).
use gitpass_core::origin::{android_package_of, app_matches, origin_matches};

#[test]
fn android_package_forms() {
    assert_eq!(android_package_of("android://com.example.app").as_deref(), Some("com.example.app"));
    assert_eq!(android_package_of("androidapp://com.example.app/").as_deref(), Some("com.example.app"));
    assert_eq!(android_package_of("ANDROID://com.example.app").as_deref(), Some("com.example.app"));
    assert_eq!(android_package_of("android://AbC-_hash==@com.example.app/").as_deref(), Some("com.example.app"));
    for bad in ["https://com.example.app", "android://", "android://example", "android://1com.example", "android://com..x", "android://com.ex ample", "android://com.example/app", "com.example.app"] {
        assert_eq!(android_package_of(bad), None, "{bad}");
    }
}

#[test]
fn app_matching_is_exact() {
    assert!(app_matches("android://com.example.app", "com.example.app"));
    assert!(!app_matches("android://com.example.app", "com.example.app.evil"));
    assert!(!app_matches("android://com.example.app.evil", "com.example.app"));
    assert!(!app_matches("android://com.example.app", "com.example"));
    assert!(!app_matches("https://example.com", "com.example"));
    assert!(!app_matches("android://com.example.app", "not a package"));
}

#[test]
fn web_matching_is_the_shared_etld1_rule() {
    assert!(origin_matches("https://example.com/login", "https://accounts.example.com"));
    assert!(!origin_matches("https://paypal.com", "https://paypal.com.evil.io"));
    assert!(!origin_matches("https://example.com", "http://example.com"));
    assert!(!origin_matches("android://com.example.app", "https://example.com"));
}
