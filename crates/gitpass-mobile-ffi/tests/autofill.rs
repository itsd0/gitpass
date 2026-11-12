//! #475: autofill matching over the bridge. Temporary vaults, fake secrets only.

use gitpass_core::crypto::KdfParams;
use gitpass_core::store;
use gitpass_mobile_ffi::{
    autofill_app_matches, autofill_origin_matches, open_vault, AutofillQuery, FfiField, FfiSection, ItemInput,
};

const PASSWORD: &str = "FAKE correct horse battery staple";

fn login(title: &str, url: Option<&str>, extra_url: Option<&str>) -> ItemInput {
    ItemInput {
        title: title.into(),
        username: Some(format!("{title}-user")),
        password: Some("pw-FAKE".into()),
        url: url.map(str::to_string),
        notes: None,
        totp: None,
        category: "login".into(),
        tags: vec![],
        favorite: false,
        sections: extra_url
            .map(|u| vec![FfiSection { id: "s1".into(), title: None, fields: vec![FfiField { id: "f1".into(), label: "website".into(), kind: "url".into(), value: Some(u.into()) }] }])
            .unwrap_or_default(),
    }
}

fn web(o: &str) -> AutofillQuery { AutofillQuery { web_origin: Some(o.into()), android_package: None } }
fn app(p: &str) -> AutofillQuery { AutofillQuery { web_origin: None, android_package: Some(p.into()) } }

#[test]
fn matches_by_etld1_and_package_and_never_cross_site() {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), PASSWORD, KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 }).unwrap();
    let v = open_vault(d.path().to_string_lossy().into(), PASSWORD.into(), sk.encode()).unwrap();
    let a = v.create_vault("A".into(), None).unwrap();
    let b = v.create_vault("B".into(), None).unwrap();
    v.create_item(a.clone(), login("Example", Some("https://www.example.com/login"), None)).unwrap();
    v.create_item(b.clone(), login("ExampleApp", None, Some("android://AAAA@com.example.app/"))).unwrap();
    v.create_item(a.clone(), login("Evil", Some("https://example.com.evil.io"), None)).unwrap();
    v.create_item(a.clone(), login("Plain http", Some("http://example.com"), None)).unwrap();
    let trashed = v.create_item(a.clone(), login("Trashed", Some("https://example.com"), None)).unwrap();
    v.delete_item(a.clone(), trashed).unwrap();

    let titles = |q| v.autofill_logins(q).unwrap().into_iter().map(|l| l.title).collect::<Vec<_>>();
    assert_eq!(titles(web("https://accounts.example.com")), vec!["Example"]);
    assert_eq!(titles(web("https://example.com.evil.io")), vec!["Evil"]);
    assert!(titles(web("https://example.org")).is_empty());
    assert!(titles(web("http://example.com")).is_empty(), "plain http pages never match");
    assert_eq!(titles(app("com.example.app")), vec!["ExampleApp"]);
    assert!(titles(app("com.example.app.evil")).is_empty());
    assert!(titles(app("com.example")).is_empty());
    assert!(titles(AutofillQuery::default()).is_empty());
    let hit = v.autofill_logins(web("https://example.com")).unwrap();
    assert_eq!(hit[0].username.as_deref(), Some("Example-user"));
    assert_eq!(hit[0].password.as_deref(), Some("pw-FAKE"));
    assert!(!format!("{:?}", hit[0]).contains("pw-FAKE"), "Debug never prints secrets");
}

#[test]
fn exported_rules() {
    assert!(autofill_origin_matches("example.com".into(), "https://login.example.com".into()));
    assert!(!autofill_origin_matches("https://paypal.com".into(), "https://paypal.com.evil.io".into()));
    assert!(autofill_app_matches("androidapp://com.example.app".into(), "com.example.app".into()));
    assert!(!autofill_app_matches("https://example.com".into(), "com.example".into()));
}
