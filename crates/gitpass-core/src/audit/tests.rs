use super::*;
use crate::item::{Category, Field, FieldKind, Section};
use crate::store::Item;

fn card_item(title: &str, expiry: &str) -> ItemRecord {
    let mut it = Item::default();
    it.title = title.into();
    let mut rec = ItemRecord::new(it);
    rec.category = Category::CreditCard;
    rec.sections = vec![Section {
        id: "main".into(),
        title: None,
        fields: vec![Field {
            id: "expiry".into(),
            label: "expiry date".into(),
            kind: FieldKind::MonthYear,
            value: Some(expiry.into()),
            removed: false,
        }],
    }];
    rec
}

fn api_cred_item(title: &str, expires: &str) -> ItemRecord {
    let mut it = Item::default();
    it.title = title.into();
    let mut rec = ItemRecord::new(it);
    rec.category = Category::ApiCredential;
    rec.sections = vec![Section {
        id: "main".into(),
        title: None,
        fields: vec![Field {
            id: "expires".into(),
            label: "expires".into(),
            kind: FieldKind::Date,
            value: Some(expires.into()),
            removed: false,
        }],
    }];
    rec
}

#[test]
fn test_unused_login_threshold_and_fallbacks() {
    use std::collections::BTreeMap;
    // 2027-01-15T00:00:00Z
    let now_secs: u64 = 1_800_000_000;
    let day = 86400u64;
    let login = |title: &str| {
        let mut it = Item::default();
        it.title = title.into();
        let mut rec = ItemRecord::new(it);
        rec.category = Category::Login;
        rec
    };
    let mut rows: Vec<(String, String, ItemRecord)> = vec![];
    let mut push = |id: &str, rec: ItemRecord| rows.push(("v".into(), id.into(), rec));

    // Recently edited (fallback when never used): not flagged.
    let mut recent = login("Recent");
    recent.item.updated_at = (now_secs - 10 * day).to_string();
    push("recent", recent);

    // Never used, no edit time, recently created: not flagged.
    let mut created = login("New");
    created.created_at = Some(now_secs - 10 * day);
    push("created", created);

    // No timestamp at all: old age is unknown, so not flagged.
    push("unknown", login("Unknown"));

    // Old edit time and no use recorded: flagged with the days since that edit.
    let mut old = login("Old");
    old.item.updated_at = (now_secs - 400 * day).to_string();
    push("old", old);

    // ISO 8601 `updated_at` (older items) is parsed too.
    let mut iso = login("Old ISO");
    iso.item.updated_at = crate::device::iso8601((now_secs - 400 * day) as i64);
    push("iso", iso);

    // Used recently even though edited long ago: not flagged.
    let mut used = login("Used");
    used.item.updated_at = (now_secs - 400 * day).to_string();
    push("used", used);

    // Last use is old even though the item was edited recently: flagged.
    let mut stale = login("Stale");
    stale.item.updated_at = (now_secs - 5 * day).to_string();
    push("stale", stale);

    // Exactly at the threshold is kept; one day past it is flagged.
    let mut boundary = login("Boundary");
    boundary.item.updated_at = (now_secs - 365 * day).to_string();
    push("boundary", boundary);
    let mut beyond = login("Beyond");
    beyond.item.updated_at = (now_secs - 366 * day).to_string();
    push("beyond", beyond);

    // Archived, deleted and non-login items are never flagged.
    let mut archived = login("Archived");
    archived.item.updated_at = (now_secs - 400 * day).to_string();
    archived.archived = true;
    push("archived", archived);
    let mut deleted = login("Deleted");
    deleted.item.updated_at = (now_secs - 400 * day).to_string();
    deleted.deleted_at = Some(now_secs - 400 * day);
    push("deleted", deleted);
    let mut card = card_item("Old card", "12/30");
    card.item.updated_at = (now_secs - 400 * day).to_string();
    push("card", card);

    let mut last_used = BTreeMap::new();
    last_used.insert("used".to_string(), now_secs - 5 * day);
    last_used.insert("stale".to_string(), now_secs - 400 * day);
    let opts = AuditOpts { now_secs, unused_months: Some(12), last_used, ..Default::default() };
    let report = audit(&rows, &opts, None).unwrap();

    let find = |id: &str| report.per_item.iter().find(|i| i.id == id).map(|i| i.findings.clone()).unwrap_or_default();
    assert!(find("recent").is_empty());
    assert!(find("created").is_empty());
    assert!(find("unknown").is_empty());
    assert_eq!(find("old"), vec![Finding::Unused { days: 400 }]);
    assert_eq!(find("iso"), vec![Finding::Unused { days: 400 }]);
    assert!(find("used").is_empty());
    assert_eq!(find("stale"), vec![Finding::Unused { days: 400 }]);
    assert!(find("boundary").is_empty());
    assert_eq!(find("beyond"), vec![Finding::Unused { days: 366 }]);
    assert!(find("archived").is_empty() && find("deleted").is_empty() && find("card").is_empty());
    assert_eq!(report.unused, 4);

    // 24 months is looser, 6 months is stricter.
    let looser = AuditOpts { now_secs, unused_months: Some(24), ..Default::default() };
    assert_eq!(audit(&rows, &looser, None).unwrap().unused, 0);
    let stricter = AuditOpts { now_secs, unused_months: Some(6), ..Default::default() };
    // 6 months = 182 days also catches the item exactly 365 days old, plus the four 12-month ones.
    assert_eq!(audit(&rows, &stricter, None).unwrap().unused, 5);
    assert_eq!(dates_unused_threshold_days(6), 182);
    assert_eq!(dates_unused_threshold_days(24), 730);

    // Off (None or 0 months) disables the check.
    for off in [None, Some(0)] {
        let opts = AuditOpts { now_secs, unused_months: off, ..Default::default() };
        let r = audit(&rows, &opts, None).unwrap();
        assert_eq!(r.unused, 0);
        assert!(!r.per_item.iter().flat_map(|i| &i.findings).any(|f| matches!(f, Finding::Unused { .. })));
    }
}

#[test]
fn test_expiry_audit_cases() {
    // 2026-05-15 12:00:00 UTC
    let now_secs: u64 = 1778846400;
    let opts = AuditOpts { now_secs, expiring_days: 30, ..Default::default() };

    let items = vec![
        // 1. Expired card (past month: 04/26 ended 2026-04-30 23:59:59)
        ("v".into(), "card_exp".into(), card_item("Expired Card", "04/26")),
        // 2. Card expiring this month (05/26 ends 2026-05-31 23:59:59, 16 days left)
        ("v".into(), "card_soon".into(), card_item("Card Expiring Soon", "05/26")),
        // 3. Card expiring in > 30 days (07/26 ends 2026-07-31 23:59:59)
        ("v".into(), "card_far".into(), card_item("Card Far Future", "07/26")),
        // 4. API credential expires in the past (2026-05-10 23:59:59)
        ("v".into(), "api_exp".into(), api_cred_item("Expired API", "2026-05-10")),
        // 5. API credential expires within the window (2026-05-25 23:59:59, 10 days left)
        ("v".into(), "api_soon".into(), api_cred_item("API Expiring Soon", "2026-05-25")),
        // 6. Unparsable value (no finding, no panic)
        ("v".into(), "card_bad".into(), card_item("Garbage Card", "invalid-expiry")),
        ("v".into(), "api_bad".into(), api_cred_item("Garbage API", "not-a-date")),
    ];

    let report = audit(&items, &opts, None).unwrap();
    assert_eq!(report.expired, 2);
    assert_eq!(report.expiring_soon, 2);

    let find = |id: &str| report.per_item.iter().find(|i| i.id == id).map(|i| i.findings.clone()).unwrap_or_default();

    assert_eq!(find("card_exp"), vec![Finding::Expired { on: "04/26".into() }]);
    assert_eq!(find("card_soon"), vec![Finding::ExpiringSoon { on: "05/26".into(), days: 16 }]);
    assert!(find("card_far").is_empty());
    assert_eq!(find("api_exp"), vec![Finding::Expired { on: "2026-05-10".into() }]);
    assert_eq!(find("api_soon"), vec![Finding::ExpiringSoon { on: "2026-05-25".into(), days: 10 }]);
    assert!(find("card_bad").is_empty());
    assert!(find("api_bad").is_empty());
}

#[test]
fn finding_kinds_lists_every_variant() {
    let all = [
        Finding::Weak { reason: String::new() }, Finding::Reused { group: 1, others: 1 }, Finding::ReusedOldPassword { other_title: String::new(), retired_on: String::new(), same_item: false }, Finding::Old { days: 1 },
        Finding::MissingTotp, Finding::TotpAvailable { site: String::new() }, Finding::PasskeyAvailable { site: String::new() },
        Finding::Pwned { count: 1 }, Finding::Expired { on: String::new() }, Finding::ExpiringSoon { on: String::new(), days: 1 },
        Finding::InsecureUrl { url_host: String::new() }, Finding::Duplicate { group: String::new() }, Finding::WeakSshKey { bits: 2048 },
        Finding::Unused { days: 400 }, Finding::RotationDue { every_days: 90, days: 100 }, Finding::RotationSoon { every_days: 90, in_days: 5 },
    ];
    let kinds: Vec<&str> = all.iter().map(Finding::kind).collect();
    assert_eq!(kinds, FINDING_KINDS);
}

#[test]
fn test_audit_includes_change_password_url_per_item() {
    let mut it = Item::default();
    it.title = "Forum".into();
    it.url = Some("http://forum.example.com/login?token=secret#frag".into());
    it.password = Some("SuperSecurePassword123!".into());
    let mut rec = ItemRecord::new(it);
    rec.ignore_finding("missing_totp");
    let items = vec![("v".into(), "forum".into(), rec)];

    let report = audit(&items, &AuditOpts::default(), None).unwrap();
    // insecure_url is active; missing_totp is ignored. Both carry the same derived URL.
    let active = report.per_item.iter().find(|i| i.id == "forum").unwrap();
    assert_eq!(
        active.change_password_url.as_deref(),
        Some("https://forum.example.com/.well-known/change-password")
    );
    assert!(!active.change_password_url.as_deref().unwrap().contains("secret"));
    let ignored = report.ignored_per_item.iter().find(|i| i.id == "forum").unwrap();
    assert_eq!(ignored.change_password_url, active.change_password_url);

    // No URL -> no change-password URL in JSON.
    let mut no_url = Item::default();
    no_url.title = "No URL".into();
    no_url.password = Some("123456".into());
    let report = audit(&[("v".into(), "none".into(), ItemRecord::new(no_url))], &AuditOpts::default(), None).unwrap();
    assert_eq!(report.per_item.iter().find(|i| i.id == "none").unwrap().change_password_url, None);
}

#[test]
fn test_audit_insecure_urls() {
    let mut login1 = Item::default();
    login1.title = "Public Insecure Login".into();
    login1.url = Some("http://user:secret@insecure.example.com:8080/login?token=sensitive".into());
    login1.password = Some("SuperSecurePassword123!".into());

    let mut login2 = Item::default();
    login2.title = "Secure HTTPS Login".into();
    login2.url = Some("https://secure.example.com/login".into());
    login2.password = Some("SuperSecurePassword123!".into());

    let mut login3 = Item::default();
    login3.title = "Local Router".into();
    login3.url = Some("http://192.168.1.1/admin".into());
    login3.password = Some("SuperSecurePassword123!".into());

    let mut login4 = Item::default();
    login4.title = "Localhost Dev".into();
    login4.url = Some("http://localhost:3000".into());
    login4.password = Some("SuperSecurePassword123!".into());

    let mut login5 = Item::default();
    login5.title = "Local NAS".into();
    login5.url = Some("http://nas.local:8080".into());
    login5.password = Some("SuperSecurePassword123!".into());

    let mut login6 = Item::default();
    login6.title = "Private 10.x".into();
    login6.url = Some("http://10.1.2.3/".into());
    login6.password = Some("SuperSecurePassword123!".into());

    let items = vec![
        ("v".into(), "i1".into(), ItemRecord::new(login1)),
        ("v".into(), "i2".into(), ItemRecord::new(login2)),
        ("v".into(), "i3".into(), ItemRecord::new(login3)),
        ("v".into(), "i4".into(), ItemRecord::new(login4)),
        ("v".into(), "i5".into(), ItemRecord::new(login5)),
        ("v".into(), "i6".into(), ItemRecord::new(login6)),
    ];

    let opts = AuditOpts::default();
    let report = audit(&items, &opts, None).unwrap();
    assert_eq!(report.insecure_urls, 1);

    let f1 = report.per_item.iter().find(|i| i.id == "i1").unwrap();
    let insecure_finding = f1.findings.iter().find(|f| matches!(f, Finding::InsecureUrl { .. })).unwrap();
    assert_eq!(insecure_finding, &Finding::InsecureUrl { url_host: "insecure.example.com".into() });

    // Ensure display format contains no secrets (no username, password, port, query)
    let disp = format!("{insecure_finding}");
    assert_eq!(disp, "saved with unencrypted HTTP URL (insecure.example.com)");
    assert!(!disp.contains("secret"));
    assert!(!disp.contains("token"));
    assert!(!disp.contains("8080"));

    for safe_id in ["i2", "i3", "i4", "i5", "i6"] {
        if let Some(entry) = report.per_item.iter().find(|i| i.id == safe_id) {
            assert!(!entry.findings.iter().any(|f| matches!(f, Finding::InsecureUrl { .. })));
        }
    }
}

#[test]
fn test_ignored_findings_audit() {
    // Password is long enough and high entropy, but shared -> reused, not weak
    let shared_pw = "Correct-Horse-Battery-Staple-2026!";
    let mut item1 = Item::default();
    item1.title = "Item 1".into();
    item1.password = Some(shared_pw.into()); // reused
    item1.url = Some("http://example.com".into()); // insecure url, missing totp

    let mut item2 = Item::default();
    item2.title = "Item 2".into();
    item2.password = Some(shared_pw.into()); // reused

    let mut item3 = Item::default();
    item3.title = "Item 3 (weak)".into();
    item3.password = Some("123456".into()); // weak password

    let mut rec1 = ItemRecord::new(item1);
    rec1.ignore_finding("reused");
    rec1.ignore_finding("insecure_url");

    let rec2 = ItemRecord::new(item2);

    let mut rec3 = ItemRecord::new(item3);
    rec3.ignore_finding("weak");

    let items = vec![
        ("v".into(), "id1".into(), rec1),
        ("v".into(), "id2".into(), rec2),
        ("v".into(), "id3".into(), rec3),
    ];

    let opts = AuditOpts::default();
    let report = audit(&items, &opts, None).unwrap();

    // item1 had: reused (ignored), insecure_url (ignored), missing_totp (active)
    // item2 had: reused (active)
    // item3 had: weak (ignored)
    // Total ignored: 3 (reused on id1, insecure_url on id1, weak on id3)
    assert_eq!(report.weak, 0);
    assert_eq!(report.insecure_urls, 0);
    assert_eq!(report.ignored, 3);
    assert_eq!(report.reused, 1); // Only item2 has active reused finding

    let active1 = report.per_item.iter().find(|i| i.id == "id1").unwrap();
    assert!(!active1.findings.iter().any(|f| f.kind() == "reused"));
    assert!(!active1.findings.iter().any(|f| f.kind() == "insecure_url"));
    assert!(active1.findings.iter().any(|f| f.kind() == "missing_totp"));

    let ignored1 = report.ignored_per_item.iter().find(|i| i.id == "id1").unwrap();
    assert_eq!(ignored1.findings.len(), 2);
    assert!(ignored1.findings.iter().any(|f| f.kind() == "reused"));
    assert!(ignored1.findings.iter().any(|f| f.kind() == "insecure_url"));

    let ignored3 = report.ignored_per_item.iter().find(|i| i.id == "id3").unwrap();
    assert_eq!(ignored3.findings.len(), 1);
    assert!(ignored3.findings.iter().any(|f| f.kind() == "weak"));
}

#[test]
fn test_ignored_findings_roundtrip_and_backward_compatibility() {
    // 1. Backward compatibility: older JSON without ignored_findings
    let legacy_json = r#"{"title":"Old Item","updated_at":"1600000000"}"#;
    let rec: ItemRecord = serde_json::from_str(legacy_json).unwrap();
    assert!(rec.ignored_findings.is_empty());
    assert!(!rec.is_finding_ignored("weak"));

    // Reserializing empty ignored_findings does not add "ignored_findings" key
    let reserialized = serde_json::to_string(&rec).unwrap();
    assert!(!reserialized.contains("ignored_findings"));

    // 2. Round-trip with ignored findings
    let mut rec_with_ignored = rec.clone();
    rec_with_ignored.ignore_finding("reused");
    rec_with_ignored.ignore_finding("insecure_url");
    assert!(rec_with_ignored.is_finding_ignored("reused"));
    assert!(rec_with_ignored.is_finding_ignored("insecure_url"));
    assert!(!rec_with_ignored.is_finding_ignored("weak"));

    let json = serde_json::to_string(&rec_with_ignored).unwrap();
    assert!(json.contains("ignored_findings"));

    let parsed: ItemRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.ignored_findings, vec!["reused", "insecure_url"]);

    // 3. Unignore
    let mut rec_unignore = parsed;
    assert!(rec_unignore.unignore_finding("reused"));
    assert!(!rec_unignore.is_finding_ignored("reused"));
    assert!(rec_unignore.is_finding_ignored("insecure_url"));
    assert_eq!(rec_unignore.ignored_findings, vec!["insecure_url"]);
}

fn login_rec(title: &str, username: Option<&str>, url: Option<&str>, pw: Option<&str>) -> ItemRecord {
    let mut it = Item::default();
    it.title = title.into();
    it.username = username.map(|u| u.into());
    it.url = url.map(|u| u.into());
    it.password = pw.map(|p| p.into());
    let mut rec = ItemRecord::new(it);
    rec.category = Category::Login;
    rec
}

#[test]
fn test_duplicate_logins_detection() {
    let strong_pw = "Kj9#mQ2$vL5!wP8*zR";
    let mut rec_archived = login_rec("Archived Site", Some("archived_user"), Some("https://archive-test.com/"), Some(strong_pw));
    rec_archived.archived = true;

    let mut rec_deleted = login_rec("Deleted Site", Some("deleted_user"), Some("https://delete-test.com/"), Some(strong_pw));
    rec_deleted.deleted_at = Some(1778846400);

    let items = vec![
        // 1. Same host+username in different case / scheme / www. / port / path are grouped:
        ("v".into(), "dup1".into(), login_rec("Alpha 1", Some("Alice"), Some("https://www.EXAMPLE.com:8443/login?redirect=1"), Some(strong_pw))),
        ("v".into(), "dup2".into(), login_rec("Alpha 2", Some(" alice "), Some("http://example.com/dashboard"), Some(strong_pw))),
        // 2. Different username -> not grouped
        ("v".into(), "diff_user".into(), login_rec("Alpha Bob", Some("bob"), Some("https://example.com/"), Some(strong_pw))),
        // 3. Different host -> not grouped
        ("v".into(), "diff_host".into(), login_rec("Other Site", Some("alice"), Some("https://other.com/"), Some(strong_pw))),
        // 4. Empty username -> not grouped
        ("v".into(), "empty_user1".into(), login_rec("Empty 1", Some("   "), Some("https://empty-test.com/"), Some(strong_pw))),
        ("v".into(), "empty_user2".into(), login_rec("Empty 2", Some(""), Some("https://empty-test.com/"), Some(strong_pw))),
        ("v".into(), "none_user".into(), login_rec("None User", None, Some("https://empty-test.com/"), Some(strong_pw))),
        // 5. Archived items are excluded (and do not cause single active item to be flagged)
        ("v".into(), "archive_active".into(), login_rec("Archive Active", Some("archived_user"), Some("https://archive-test.com/"), Some(strong_pw))),
        ("v".into(), "archive_inactive".into(), rec_archived),
        // 6. Deleted items are excluded
        ("v".into(), "delete_active".into(), login_rec("Delete Active", Some("deleted_user"), Some("https://delete-test.com/"), Some(strong_pw))),
        ("v".into(), "delete_inactive".into(), rec_deleted),
        // 7. Same host+user in different vault scopes are not grouped
        ("vault_other".into(), "other_vault_dup".into(), login_rec("Alpha in other vault", Some("alice"), Some("https://example.com/"), Some(strong_pw))),
    ];

    let opts = AuditOpts::default();
    let report = audit(&items, &opts, None).unwrap();
    assert_eq!(report.duplicates, 2);

    let find = |id: &str| report.per_item.iter().find(|i| i.id == id).map(|i| i.findings.clone()).unwrap_or_default();

    let dup1_f = find("dup1");
    let expected_finding = Finding::Duplicate { group: "example.com/alice".into() };
    assert!(dup1_f.contains(&expected_finding));

    let dup2_f = find("dup2");
    assert!(dup2_f.contains(&expected_finding));

    // Finding text contains no password, path, or query
    let text = expected_finding.to_string();
    assert_eq!(text, "duplicate login for example.com/alice");
    assert!(!text.contains(strong_pw));
    assert!(!text.contains("/login"));
    assert!(!text.contains("redirect=1"));
    assert!(!text.contains("8443"));

    // Others are NOT flagged as duplicate
    for id in ["diff_user", "diff_host", "empty_user1", "empty_user2", "none_user", "archive_active", "archive_inactive", "delete_active", "delete_inactive", "other_vault_dup"] {
        assert!(find(id).iter().all(|f| !matches!(f, Finding::Duplicate { .. })), "{id} should not have Duplicate finding");
    }
}

#[test]
fn test_audit_flags_personal_info_weakness() {
    let mut it_user = Item::default();
    it_user.title = "User Password".into();
    it_user.username = Some("alice".into());
    it_user.url = Some("https://gitlab.com".into());
    it_user.password = Some("github-alice-2024!".into());

    let mut it_site = Item::default();
    it_site.title = "Site Password".into();
    it_site.username = Some("charlie".into());
    it_site.url = Some("https://login.github.com".into());
    it_site.password = Some("G1thub#99-Secure!".into());

    let mut it_ok = Item::default();
    it_ok.title = "OK Item".into();
    it_ok.username = Some("bob".into());
    it_ok.url = Some("https://example.com".into());
    it_ok.password = Some("Xq7#vR2!mK9@pL4$wZ".into());

    let items = vec![
        ("v".into(), "user_item".into(), ItemRecord::new(it_user)),
        ("v".into(), "site_item".into(), ItemRecord::new(it_site)),
        ("v".into(), "ok_item".into(), ItemRecord::new(it_ok)),
    ];

    let report = audit(&items, &AuditOpts::default(), None).unwrap();
    assert_eq!(report.weak, 2);

    let find = |id: &str| report.per_item.iter().find(|i| i.id == id).map(|i| i.findings.clone()).unwrap_or_default();
    let u_findings = find("user_item");
    assert!(u_findings.iter().any(|f| matches!(f, Finding::Weak { reason } if reason == "contains the username")));

    let s_findings = find("site_item");
    assert!(s_findings.iter().any(|f| matches!(f, Finding::Weak { reason } if reason == "contains the site name")));

    let ok_findings = find("ok_item");
    assert!(!ok_findings.iter().any(|f| matches!(f, Finding::Weak { .. })));
}

#[test]
fn test_password_changed_at_derivation() {
    use crate::item::{HistoryEntry, Snapshot};

    // 1. New item without history: uses created_at
    let mut it = Item::default();
    it.title = "Login".into();
    it.password = Some("secretA".into());
    it.updated_at = "1000".into();
    let mut rec = ItemRecord::new(it);
    rec.created_at = Some(1000);
    assert_eq!(rec.password_changed_at(), Some(1000));

    // 2. Edit note at t=2000 (password unchanged): still returns 1000
    rec.history.push(HistoryEntry {
        version: 1,
        updated_at: "1000".into(),
        snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
    });
    rec.item.notes = Some("updated note".into());
    rec.item.updated_at = "2000".into();
    assert_eq!(rec.password_changed_at(), Some(1000));

    // 3. Edit password to secretB at t=3000: returns 3000
    rec.history.push(HistoryEntry {
        version: 2,
        updated_at: "2000".into(),
        snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
    });
    rec.item.password = Some("secretB".into());
    rec.item.updated_at = "3000".into();
    assert_eq!(rec.password_changed_at(), Some(3000));

    // 4. Edit tags at t=4000 (password secretB unchanged): still returns 3000
    rec.history.push(HistoryEntry {
        version: 3,
        updated_at: "3000".into(),
        snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
    });
    rec.tags = vec!["tag1".into()];
    rec.item.updated_at = "4000".into();
    assert_eq!(rec.password_changed_at(), Some(3000));

    // 5. Item without password: returns None
    rec.item.password = None;
    assert_eq!(rec.password_changed_at(), None);
}

#[test]
fn test_rotation_reminder_due_soon_not_due_and_unset() {
    let now_secs = 2_000_000_000u64;
    let day = 86400u64;
    let mk = |changed_days_ago: u64, every: Option<u32>| {
        let mut it = Item::default();
        it.title = "Bank".into();
        it.password = Some("FAKE-Rotation-Pass-123!".into());
        let t = now_secs - changed_days_ago * day;
        it.updated_at = t.to_string();
        let mut rec = ItemRecord::new(it);
        rec.created_at = Some(t);
        rec.rotate_every_days = every;
        rec
    };
    let opts = AuditOpts { now_secs, max_age_days: 3650, ..Default::default() };
    let run = |rec: ItemRecord| audit(&[("v".into(), "i".into(), rec)], &opts, None).unwrap();

    // due: 95 days old with a 90-day reminder, and exactly on the deadline
    let r = run(mk(95, Some(90)));
    assert_eq!((r.rotation_due, r.rotation_soon), (1, 0));
    assert!(r.per_item[0].findings.contains(&Finding::RotationDue { every_days: 90, days: 95 }));
    assert_eq!(run(mk(90, Some(90))).rotation_due, 1);

    // soon: 80 days old => 10 days left; the 14-day window edge is included, 15 days is not
    let r = run(mk(80, Some(90)));
    assert_eq!((r.rotation_due, r.rotation_soon), (0, 1));
    assert!(r.per_item[0].findings.contains(&Finding::RotationSoon { every_days: 90, in_days: 10 }));
    assert_eq!(run(mk(76, Some(90))).rotation_soon, 1);
    assert_eq!(run(mk(75, Some(90))).rotation_soon, 0);

    // not due, and unset (the default): no rotation findings
    let r = run(mk(10, Some(365)));
    assert_eq!((r.rotation_due, r.rotation_soon), (0, 0));
    let r = run(mk(500, None));
    assert_eq!((r.rotation_due, r.rotation_soon), (0, 0));

    // archived logins are skipped; ignoring the finding moves it to the ignored list
    let mut archived = mk(95, Some(90));
    archived.archived = true;
    assert_eq!(run(archived).rotation_due, 0);
    let mut ignored = mk(95, Some(90));
    ignored.ignore_finding("rotation_due");
    let r = run(ignored);
    assert_eq!((r.rotation_due, r.ignored), (0, 1));

    // measured from the password change, not the last edit: a note edit today keeps it due
    let mut edited = mk(95, Some(90));
    edited.history.push(crate::item::HistoryEntry {
        version: 1,
        updated_at: edited.item.updated_at.clone(),
        snapshot: crate::item::Snapshot { item: edited.item.clone(), category: edited.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
    });
    edited.item.notes = Some("edited today".into());
    edited.item.updated_at = now_secs.to_string();
    assert_eq!(run(edited).rotation_due, 1);
}

#[test]
fn test_rotation_field_roundtrip_and_backward_compat() {
    let legacy: ItemRecord = serde_json::from_str(r#"{"title":"Old Item","updated_at":"1600000000"}"#).unwrap();
    assert_eq!(legacy.rotate_every_days, None);
    assert!(!serde_json::to_string(&legacy).unwrap().contains("rotate_every_days"));
    let mut rec = legacy.clone();
    rec.rotate_every_days = Some(90);
    let back: ItemRecord = serde_json::from_str(&serde_json::to_string(&rec).unwrap()).unwrap();
    assert_eq!(back.rotate_every_days, Some(90));
}

#[test]
fn test_old_password_audit_uses_password_change_time() {
    use crate::item::{HistoryEntry, Snapshot};

    let now_secs = 2_000_000_000u64;
    let day = 86400u64;
    let created = now_secs - 400 * day; // 400 days old

    let mut it = Item::default();
    it.title = "Old Login".into();
    it.password = Some("SecureLongPassword123!".into());
    it.updated_at = created.to_string();
    let mut rec = ItemRecord::new(it);
    rec.created_at = Some(created);

    let opts = AuditOpts { now_secs, max_age_days: 365, ..Default::default() };

    // Fresh audit: 400 days > 365 => flagged as Old
    let report = audit(&[("v".into(), "item1".into(), rec.clone())], &opts, None).unwrap();
    assert_eq!(report.old, 1);
    let findings = &report.per_item[0].findings;
    assert!(findings.iter().any(|f| matches!(f, Finding::Old { days } if *days >= 400)));

    // Simulate editing a note today (now_secs - day). Password did not change.
    rec.history.push(HistoryEntry {
        version: 1,
        updated_at: created.to_string(),
        snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
    });
    rec.item.notes = Some("new note added today".into());
    rec.item.updated_at = (now_secs - day).to_string();

    // Crucial test: editing notes does NOT reset password age!
    let report2 = audit(&[("v".into(), "item1".into(), rec.clone())], &opts, None).unwrap();
    assert_eq!(report2.old, 1, "editing notes must not reset old password age");

    // Now change password to a new password 10 days ago
    rec.history.push(HistoryEntry {
        version: 2,
        updated_at: rec.item.updated_at.clone(),
        snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
    });
    rec.item.password = Some("BrandNewPassword123!".into());
    rec.item.updated_at = (now_secs - 10 * day).to_string();

    // Changing password resets age: 10 days < 365 => no Old finding
    let report3 = audit(&[("v".into(), "item1".into(), rec)], &opts, None).unwrap();
    assert_eq!(report3.old, 0, "changing password must reset old password age");
}

#[test]
fn test_truncated_history_does_not_report_false_old() {
    use crate::item::{HistoryEntry, HISTORY_LIMIT, Snapshot};

    let now_secs = 2_000_000_000u64;
    let day = 86400u64;
    let created = now_secs - 400 * day; // item created 400 days ago

    let mut it = Item::default();
    it.title = "Busy Login".into();
    it.password = Some("OriginalPassword123!".into());
    it.updated_at = created.to_string();
    let mut rec = ItemRecord::new(it);
    rec.created_at = Some(created);
    let mut version = 1u64;

    // Push the current content to history (like Session::prepare_update), capped at HISTORY_LIMIT.
    let push = |rec: &mut ItemRecord, version: &mut u64| {
        rec.history.push(HistoryEntry {
            version: *version,
            updated_at: rec.item.updated_at.clone(),
            snapshot: Snapshot { item: rec.item.clone(), category: rec.category, sections: rec.sections.clone(), tags: rec.tags.clone(), related_item_ids: vec![] },
        });
        let n = rec.history.len();
        if n > HISTORY_LIMIT { rec.history.drain(..n - HISTORY_LIMIT); }
        *version += 1;
    };

    // Password changed 100 days ago.
    push(&mut rec, &mut version);
    rec.item.password = Some("ChangedPassword456!".into());
    rec.item.updated_at = (now_secs - 100 * day).to_string();

    // Followed by more than HISTORY_LIMIT metadata-only edits (notes), one per day.
    for k in 0..(HISTORY_LIMIT as u64 + 5) {
        push(&mut rec, &mut version);
        rec.item.notes = Some(format!("note {k}"));
        rec.item.updated_at = (now_secs - (90 - k) * day).to_string();
    }

    // The password-change entry was drained: every retained entry has the current password.
    assert_eq!(rec.history.len(), HISTORY_LIMIT);
    assert!(rec.history.first().unwrap().version > 1);
    assert!(rec.history.iter().all(|h| h.snapshot.item.password.as_deref() == Some("ChangedPassword456!")));

    // Truncated history: fall back to the oldest retained entry, not created_at.
    let oldest: u64 = rec.history.first().unwrap().updated_at.parse().unwrap();
    assert_eq!(rec.password_changed_at(), Some(oldest));
    assert!(oldest > created);

    let opts = AuditOpts { now_secs, max_age_days: 365, ..Default::default() };
    let report = audit(&[("v".into(), "item1".into(), rec.clone())], &opts, None).unwrap();
    assert_eq!(report.old, 0, "truncated history must not produce a false Old finding");
    assert!(!report.per_item.iter().flat_map(|i| &i.findings).any(|f| matches!(f, Finding::Old { .. })));

    // Complete history (never truncated, starts at version 1): created_at fallback still applies.
    let mut fresh = rec.clone();
    fresh.history.clear();
    fresh.item.password = Some("ChangedPassword456!".into());
    fresh.history.push(HistoryEntry {
        version: 1,
        updated_at: created.to_string(),
        snapshot: Snapshot { item: fresh.item.clone(), category: fresh.category, sections: vec![], tags: vec![], related_item_ids: vec![] },
    });
    assert_eq!(fresh.password_changed_at(), Some(created));
}

#[test]
fn test_watchtower_server_passwords_and_bank_account_exclusion() {
    let now_secs = 1700000000;
    let day = 86400;

    // 1. Server item with weak password
    let mut srv_weak = ItemRecord::new(Item {
        title: "Weak Srv".into(),
        username: Some("root".into()),
        password: None,
        url: Some("https://srv1.example.invalid".into()),
        notes: None,
        totp: None,
        updated_at: now_secs.to_string(),
    });
    srv_weak.category = Category::Server;
    srv_weak.sections = vec![Section {
        id: "main".into(),
        title: None,
        fields: vec![
            Field { id: "password".into(), label: "password".into(), kind: FieldKind::Concealed, value: Some("123456".into()), removed: false },
        ],
    }];

    // 2. Server item sharing password with Login item
    let shared_pw = "SharedComplexPass123!@#";
    let mut srv_reused = ItemRecord::new(Item {
        title: "Reused Srv".into(),
        username: Some("admin".into()),
        password: None,
        url: Some("https://srv2.example.invalid".into()),
        notes: None,
        totp: None,
        updated_at: now_secs.to_string(),
    });
    srv_reused.category = Category::Server;
    srv_reused.sections = vec![Section {
        id: "main".into(),
        title: None,
        fields: vec![
            Field { id: "password".into(), label: "password".into(), kind: FieldKind::Concealed, value: Some(shared_pw.into()), removed: false },
        ],
    }];

    let login_reused = ItemRecord::new(Item {
        title: "Login Site".into(),
        username: Some("user".into()),
        password: Some(shared_pw.into()),
        url: Some("https://login.example.invalid".into()),
        notes: None,
        totp: Some("JBSWY3DPEHPK3PXP".into()),
        updated_at: now_secs.to_string(),
    });

    // 3. Server item with old password
    let mut srv_old = ItemRecord::new(Item {
        title: "Old Srv".into(),
        username: Some("ubuntu".into()),
        password: None,
        url: Some("https://srv3.example.invalid".into()),
        notes: None,
        totp: None,
        updated_at: (now_secs - 400 * day).to_string(),
    });
    srv_old.category = Category::Server;
    srv_old.created_at = Some(now_secs - 400 * day);
    srv_old.sections = vec![Section {
        id: "main".into(),
        title: None,
        fields: vec![
            Field { id: "password".into(), label: "password".into(), kind: FieldKind::Concealed, value: Some("UniqueOldPass987$%^".into()), removed: false },
        ],
    }];

    // 4. Bank account with account number and PIN (even if PIN is "1234" which would be weak if checked as a password)
    let mut bank_item = ItemRecord::new(Item {
        title: "Bank Account".into(),
        username: None,
        password: None,
        url: None,
        notes: None,
        totp: None,
        updated_at: (now_secs - 400 * day).to_string(),
    });
    bank_item.category = Category::BankAccount;
    bank_item.created_at = Some(now_secs - 400 * day);
    bank_item.sections = vec![Section {
        id: "main".into(),
        title: None,
        fields: vec![
            Field { id: "bank_name".into(), label: "bank name".into(), kind: FieldKind::Text, value: Some("Chase".into()), removed: false },
            Field { id: "account_number".into(), label: "account number".into(), kind: FieldKind::Concealed, value: Some("1234567890".into()), removed: false },
            Field { id: "pin".into(), label: "PIN".into(), kind: FieldKind::Concealed, value: Some("1234".into()), removed: false },
        ],
    }];

    let items = vec![
        ("v".into(), "srv_weak".into(), srv_weak),
        ("v".into(), "srv_reused".into(), srv_reused),
        ("v".into(), "login_reused".into(), login_reused),
        ("v".into(), "srv_old".into(), srv_old),
        ("v".into(), "bank_item".into(), bank_item),
    ];

    let opts = AuditOpts { now_secs, max_age_days: 365, ..Default::default() };
    let report = audit(&items, &opts, None).unwrap();

    // Server weak password detected
    assert_eq!(report.weak, 1);
    let weak_item = report.per_item.iter().find(|i| i.id == "srv_weak").unwrap();
    assert!(weak_item.findings.iter().any(|f| matches!(f, Finding::Weak { .. })));

    // Server and Login password reuse detected
    assert_eq!(report.reused, 2);
    let srv_reused_f = report.per_item.iter().find(|i| i.id == "srv_reused").unwrap();
    assert!(srv_reused_f.findings.iter().any(|f| matches!(f, Finding::Reused { .. })));
    let login_reused_f = report.per_item.iter().find(|i| i.id == "login_reused").unwrap();
    assert!(login_reused_f.findings.iter().any(|f| matches!(f, Finding::Reused { .. })));

    // Server old password detected
    assert_eq!(report.old, 1);
    let srv_old_f = report.per_item.iter().find(|i| i.id == "srv_old").unwrap();
    assert!(srv_old_f.findings.iter().any(|f| matches!(f, Finding::Old { .. })));

    // Bank account has NO findings: PIN and account number are never treated as passwords
    assert!(!report.per_item.iter().any(|i| i.id == "bank_item"), "Bank account number and PIN must never produce findings");
}

#[test]
fn test_weak_ssh_key_audit_and_ignore() {
    use crate::ssh::SshKey;
    use ssh_key::private::{KeypairData, RsaKeypair};
    use ssh_key::LineEnding;

    // 1. 2048-bit RSA key: weak (< 3072)
    let rsa2048 = RsaKeypair::random(&mut rand::rngs::OsRng, 2048).unwrap();
    let k2048 = SshKey::import_openssh(&ssh_key::PrivateKey::new(KeypairData::Rsa(rsa2048), "test2048").unwrap().to_openssh(LineEnding::LF).unwrap(), None).unwrap();
    let rec2048 = k2048.to_record("RSA 2048 Key").unwrap();

    // 2. 3072-bit RSA key: not weak (>= 3072)
    let rsa3072 = RsaKeypair::random(&mut rand::rngs::OsRng, 3072).unwrap();
    let k3072 = SshKey::import_openssh(&ssh_key::PrivateKey::new(KeypairData::Rsa(rsa3072), "test3072").unwrap().to_openssh(LineEnding::LF).unwrap(), None).unwrap();
    let rec3072 = k3072.to_record("RSA 3072 Key").unwrap();

    // 3. Ed25519 key: not weak
    let ed = SshKey::generate_ed25519("ed@test").unwrap();
    let rec_ed = ed.to_record("Ed25519 Key").unwrap();

    let items = vec![
        ("v".into(), "id1".into(), rec2048.clone()),
        ("v".into(), "id2".into(), rec3072),
        ("v".into(), "id3".into(), rec_ed),
    ];

    let rep = audit(&items, &AuditOpts::default(), None).unwrap();
    assert_eq!(rep.weak_ssh_keys, 1);
    let id1_f = rep.per_item.iter().find(|i| i.id == "id1").unwrap();
    assert!(id1_f.findings.contains(&Finding::WeakSshKey { bits: 2048 }));
    assert!(!rep.per_item.iter().any(|i| i.id == "id2"));
    assert!(!rep.per_item.iter().any(|i| i.id == "id3"));

    // Ignored weak_ssh_key
    let mut rec_ignored = rec2048;
    rec_ignored.ignore_finding("weak_ssh_key");
    let rep_ign = audit(&[("v".into(), "id1".into(), rec_ignored)], &AuditOpts::default(), None).unwrap();
    assert_eq!(rep_ign.weak_ssh_keys, 0);
    assert_eq!(rep_ign.ignored, 1);
    assert_eq!(rep_ign.ignored_per_item.len(), 1);
    assert!(rep_ign.ignored_per_item[0].findings.contains(&Finding::WeakSshKey { bits: 2048 }));
}

#[test]
fn test_watchtower_own_password_history_not_flagged_as_reused() {
    let mut it = Item::default();
    it.title = "Single Item".into();
    it.username = Some("alice".into());
    it.password = Some("Complex-P@ssword-1234!".into());
    it.url = Some("https://example.com".into());
    it.updated_at = "1700000000".into();
    let mut login = ItemRecord::new(it);
    login.password_history.push(crate::item::PasswordHistoryEntry {
        value: "Complex-P@ssword-1234!".into(),
        replaced_at: "2024-01-01T00:00:00Z".into(),
    });
    login.password_history.push(crate::item::PasswordHistoryEntry {
        value: "Older-Complex-P@ss-5678!".into(),
        replaced_at: "2023-01-01T00:00:00Z".into(),
    });

    let items = vec![("vault1".into(), "item1".into(), login)];
    let report = audit(&items, &AuditOpts::default(), None).unwrap();
    assert_eq!(report.reused, 0);
    assert_eq!(report.reused_groups, 0);
    let item1 = report.per_item.iter().find(|i| i.id == "item1");
    if let Some(i) = item1 {
        assert!(!i.findings.iter().any(|f| matches!(f, Finding::Reused { .. })));
    }
}

#[test]
fn test_reused_old_password_audit() {
    use crate::item::PasswordHistoryEntry;
    let strong = "Kj9#mQ2$vL5!wP8*zR";
    let retired_pw = "Retired-Old-Pass-987!";
    let hist = |value: &str, at: &str| PasswordHistoryEntry { value: value.into(), replaced_at: at.into() };
    // A: current password equals B's retired password (cross-item).
    let a = login_rec("Site A", Some("alice"), None, Some(retired_pw));
    // B: retired the password, now uses a different strong one.
    let mut b = login_rec("Site B", Some("bob"), None, Some(strong));
    b.password_history = vec![hist(retired_pw, "2025-06-01T00:00:00Z"), hist("   ", "2025-01-01T00:00:00Z"), hist("", "2024-01-01T00:00:00Z")];
    // C: current password equals its own retired password (same-item).
    let mut c = login_rec("Site C", Some("carol"), None, Some("Own-Retired-111!"));
    c.password_history = vec![hist("Own-Retired-111!", "2025-03-01T00:00:00Z")];
    // D: clean login, plus an unmatched retired value that must never leak into the report.
    let mut d = login_rec("Site D", Some("dan"), None, Some(strong));
    d.password_history = vec![hist("Never-Leaked-Retired-555!", "2025-02-01T00:00:00Z")];
    let items = vec![
        ("v".into(), "a".into(), a),
        ("v".into(), "b".into(), b),
        ("v".into(), "c".into(), c),
        ("v".into(), "d".into(), d),
    ];
    let report = audit(&items, &AuditOpts::default(), None).unwrap();
    assert_eq!(report.reused_old_password, 2);
    let find = |id: &str| report.per_item.iter().find(|i| i.id == id).map(|i| i.findings.clone()).unwrap_or_default();
    assert_eq!(find("a").iter().filter(|f| matches!(f, Finding::ReusedOldPassword { .. })).count(), 1);
    assert!(find("a").contains(&Finding::ReusedOldPassword { other_title: "Site B".into(), retired_on: "2025-06-01T00:00:00Z".into(), same_item: false }));
    assert!(find("c").contains(&Finding::ReusedOldPassword { other_title: "Site C".into(), retired_on: "2025-03-01T00:00:00Z".into(), same_item: true }));
    assert!(!find("d").iter().any(|f| matches!(f, Finding::ReusedOldPassword { .. })));
    // The finding carries the source title and retirement date, never the retired value.
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains(retired_pw));
    assert!(!json.contains("Own-Retired-111!"));
    assert!(!json.contains("Never-Leaked-Retired-555!"));
    assert!(json.contains("Site B") && json.contains("2025-06-01T00:00:00Z"));
    // Display text is equally value-free.
    let disp = format!("{}", Finding::ReusedOldPassword { other_title: "Site B".into(), retired_on: "2025-06-01T00:00:00Z".into(), same_item: false });
    assert_eq!(disp, "reuses a retired password previously used by Site B (retired 2025-06-01T00:00:00Z)");
    // Ignoring the kind moves it to the ignored list, like the other kinds.
    let mut a_ignored = login_rec("Site A", Some("alice"), None, Some(retired_pw));
    a_ignored.ignore_finding("reused_old_password");
    let mut b2 = login_rec("Site B", Some("bob"), None, Some(strong));
    b2.password_history = vec![hist(retired_pw, "2025-06-01T00:00:00Z")];
    let r2 = audit(&[("v".into(), "a".into(), a_ignored), ("v".into(), "b".into(), b2)], &AuditOpts::default(), None).unwrap();
    assert_eq!((r2.reused_old_password, r2.ignored), (0, 1));
    assert!(r2.ignored_per_item.iter().any(|i| i.findings.iter().any(|f| matches!(f, Finding::ReusedOldPassword { .. }))));
}

#[test]
fn test_watchtower_score_calculation() {
    // 1. Zero items with password -> score 100
    let rep_empty = AuditReport::default();
    assert_eq!(score(&rep_empty, 0), 100);
    assert_eq!(rep_empty.score(0), 100);

    // 2. All clean items -> score 100
    let rep_clean = AuditReport {
        items_checked: 5,
        ..Default::default()
    };
    assert_eq!(score(&rep_clean, 5), 100);
    assert_eq!(rep_clean.score(5), 100);

    // 3. Mixed vault:
    // 5 items checked, 4 with passwords
    // Item 1: weak password (bad)
    // Item 2: reused password and breached (bad - only penalized once)
    // Item 3: old password (old) and missing totp
    // Item 4: clean login with totp
    // Item 5: secure note without password
    //
    // bad_count = 2 (Item 1, Item 2)
    // old_count = 1 (Item 3)
    // missing_totp_count = 1 (Item 3)
    // items_with_password = 4
    // items_checked = 5
    //
    // penalty = (2 / 4) * 80.0 + (1 / 4) * 10.0 + (1 / 5) * 10.0
    //         = 40.0 + 2.5 + 2.0 = 44.5
    // score = round(100 - 44.5) = round(55.5) = 56
    let rep_mixed = AuditReport {
        items_checked: 5,
        per_item: vec![
            ItemFindings {
                vault: "v1".into(),
                id: "item1".into(),
                title: "Item 1".into(),
                change_password_url: None,
                findings: vec![Finding::Weak { reason: "short".into() }],
            },
            ItemFindings {
                vault: "v1".into(),
                id: "item2".into(),
                title: "Item 2".into(),
                change_password_url: None,
                findings: vec![
                    Finding::Reused { group: 1, others: 1 },
                    Finding::Pwned { count: 42 },
                ],
            },
            ItemFindings {
                vault: "v1".into(),
                id: "item3".into(),
                title: "Item 3".into(),
                change_password_url: None,
                findings: vec![
                    Finding::Old { days: 400 },
                    Finding::MissingTotp,
                ],
            },
        ],
        ..Default::default()
    };
    assert_eq!(score(&rep_mixed, 4), 56);
    assert_eq!(rep_mixed.score(4), 56);

    // 4. Reused old passwords: half the bad-password weight, never double-counted.
    // Item A: weak + reused_old_password (counted once, as bad)
    // Item B: reused_old_password only
    // penalty = (1 / 4) * 80.0 + (1 / 4) * 40.0 = 20.0 + 10.0 = 30.0 -> score 70
    let reused_old = |same_item| Finding::ReusedOldPassword { other_title: "Src".into(), retired_on: "2025-01-01T00:00:00Z".into(), same_item };
    let item = |id: &str, findings| ItemFindings { vault: "v1".into(), id: id.into(), title: id.into(), change_password_url: None, findings };
    let rep_old = AuditReport {
        items_checked: 4,
        per_item: vec![
            item("a", vec![Finding::Weak { reason: "short".into() }, reused_old(false)]),
            item("b", vec![reused_old(true)]),
        ],
        ..Default::default()
    };
    assert_eq!(score(&rep_old, 4), 70);
    assert_eq!(SCORE_WEIGHT_REUSED_OLD_PASSWORD, SCORE_WEIGHT_BAD_PASSWORDS / 2.0);
}

#[test]
fn test_is_score_password_item_predicate() {
    let mut it = Item::default();
    it.password = Some("secret".into());
    let mut rec = ItemRecord::new(it);
    rec.category = Category::Login;
    assert!(is_score_password_item(&rec));

    // Empty password is not counted
    rec.item.password = Some("".into());
    assert!(!is_score_password_item(&rec));
    rec.item.password = None;
    assert!(!is_score_password_item(&rec));

    // Archived item is not counted
    rec.item.password = Some("secret".into());
    rec.archived = true;
    assert!(!is_score_password_item(&rec));
    rec.archived = false;

    // Passkey is not counted
    rec.category = Category::Passkey;
    assert!(!is_score_password_item(&rec));
}
