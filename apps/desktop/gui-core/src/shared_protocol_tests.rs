//! Shared extension protocol tests: asserts that Host (standalone) and Agent (desktop app)
//! return identical JSON for every shared extension command and request fixture.

use crate::agent::*;
use crate::api::*;
use gitpass_core::crypto::KdfParams;
use gitpass_core::item::{Category, FieldKind};
use gitpass_core::store::{self, UnlockOpts};
use gitpass_host::handler::{Host, HostConfig};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

fn fast() -> KdfParams {
    KdfParams {
        alg: "argon2id".into(),
        m_kib: 8192,
        t: 1,
        p: 1,
    }
}

struct DummyUi;
impl AgentUi for DummyUi {
    fn request_unlock(&self, _reason: &str) {}
    fn open_item(&self, _v: Option<&str>, _i: Option<&str>) {}
    fn open_view(&self, _v: &str, _s: Option<&str>) {}
}

fn fld(id: &str, kind: FieldKind, v: &str) -> Field {
    Field {
        id: id.into(),
        label: id.into(),
        kind,
        value: Some(v.into()),
        masked: None,
        diff: None,
    }
}

#[test]
fn test_host_and_agent_identical_protocol_json() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    std::fs::create_dir_all(&root).unwrap();

    let cfg = tempfile::tempdir().unwrap();
    let sockdir = tempfile::Builder::new().prefix("gpa").tempdir_in("/tmp").unwrap();

    let sk = store::init(&root, "correct horse", fast()).unwrap();
    let skf = d.path().join("secret.key");
    std::fs::write(&skf, format!("{}\n", sk.encode())).unwrap();

    // 1. Set up desktop App
    let app = Arc::new(App::new(cfg.path().to_path_buf(), Arc::new(MemClipboard::default())));
    app.set_unlock_opts(UnlockOpts { accept_rollback: false, target_kdf: fast() });
    app.unlock(&root.display().to_string(), "correct horse", Some(&sk.encode())).unwrap();

    let vid = app.list_vaults().unwrap()[0].id.clone();

    // Add diverse test records (login, credit card, identity, passkey)
    let login1 = app.create_item(&ItemInput {
        vault_id: vid.clone(),
        category: Category::Login,
        title: "GitHub".into(),
        url: Some("https://github.com/login".into()),
        sections: vec![Section {
            id: "login".into(),
            title: None,
            fields: vec![
                fld("username", FieldKind::Text, "octocat"),
                fld("password", FieldKind::Concealed, "FAKE-gh-secret-pw"),
                fld("totp", FieldKind::Totp, "JBSWY3DPEHPK3PXP"),
            ],
        }],
        notes: Some("github dev note".into()),
        tags: Some(vec!["dev".into(), "vcs".into()]),
        favorite: Some(true), related_item_ids: None }).unwrap();

    let _login2 = app.create_item(&ItemInput {
        vault_id: vid.clone(),
        category: Category::Login,
        title: "GitLab".into(),
        url: Some("https://gitlab.example.com/sign_in".into()),
        sections: vec![Section {
            id: "login".into(),
            title: None,
            fields: vec![
                fld("username", FieldKind::Text, "gitlabber"),
                fld("password", FieldKind::Concealed, "FAKE-gl-secret-pw"),
            ],
        }],
        notes: None,
        tags: None,
        favorite: None, related_item_ids: None }).unwrap();

    let card = app.create_item(&ItemInput {
        vault_id: vid.clone(),
        category: Category::CreditCard,
        title: "Visa Card".into(),
        url: None,
        sections: vec![Section {
            id: "card".into(),
            title: Some("Card Details".into()),
            fields: vec![
                fld("number", FieldKind::Concealed, "4111 1111 1111 1234"),
                fld("expiry", FieldKind::MonthYear, "12/30"),
                fld("cvv", FieldKind::Concealed, "123"),
                fld("cardholder", FieldKind::Text, "Test Person"),
            ],
        }],
        notes: None,
        tags: None,
        favorite: None, related_item_ids: None }).unwrap();

    let ident = app.create_item(&ItemInput {
        vault_id: vid.clone(),
        category: Category::Identity,
        title: "Test Person".into(),
        url: None,
        sections: vec![Section {
            id: "ident".into(),
            title: None,
            fields: vec![
                fld("name", FieldKind::Text, "Test Person"),
                fld("email", FieldKind::Email, "test@example.com"),
                fld("phone", FieldKind::Phone, "+1 555 0100"),
            ],
        }],
        notes: None,
        tags: None,
        favorite: None, related_item_ids: None }).unwrap();

    let passkey = app.create_item(&ItemInput {
        vault_id: vid.clone(),
        category: Category::Passkey,
        title: "GitHub Passkey".into(),
        url: None,
        sections: vec![Section {
            id: "passkey".into(),
            title: None,
            fields: vec![
                fld("rp_id", FieldKind::Text, "github.com"),
                fld("credential_id", FieldKind::Text, "FAKE-cred-id-1234"),
                fld("user_handle", FieldKind::Text, "dXNlci0x"),
            ],
        }],
        notes: None,
        tags: None,
        favorite: None, related_item_ids: None }).unwrap();

    // Link related items on GitHub login (one available card, one missing item)
    app.state.with(|u| {
        u.session.add_related_item(&vid, &login1.id, &card.id)?;
        u.session.add_related_item(&vid, &login1.id, "missing-item-id")
    }).unwrap();

    // Touch last_used on GitHub login item
    let _ = app.state.with(|x| Ok(x.session.touch_item_used(&login1.id))).unwrap();

    // 2. Set up Agent
    let ui = Arc::new(DummyUi);
    let agent = Agent::start(app.clone(), ui, &sockdir.path().join("gitpass")).unwrap();

    // 3. Set up Host (standalone fallback on same vault)
    let mut host = Host::new(HostConfig {
        vault_path: root.clone(),
        secret_key_path: skf,
        idle_timeout: Duration::from_secs(300),
        target_kdf: fast(),
    });
    let unlock_resp = host.handle(&json!({"id": 1, "cmd": "unlock", "password": "correct horse"}));
    assert_eq!(unlock_resp["ok"], true);

    let gh_id = format!("{}:{}", vid, login1.id);
    let card_id = format!("{}:{}", vid, card.id);
    let ident_id = format!("{}:{}", vid, ident.id);
    let passkey_id = format!("{}:{}", vid, passkey.id);

    // List of request fixtures to verify identical output between Host and Agent
    let fixtures = vec![
        // 1. list_items without args (must include total, next_offset, and last_used)
        json!({"cmd": "list_items"}),

        // 2. list_items with paging (limit and offset)
        json!({"cmd": "list_items", "offset": 1, "limit": 2}),

        // 3. list_items with category filter: logins
        json!({"cmd": "list_items", "category": "login"}),

        // 4. list_items with category filter: credit cards (checks brand and masked subtitle)
        json!({"cmd": "list_items", "category": "credit_card"}),

        // 5. list_items with query
        json!({"cmd": "list_items", "query": "git"}),

        // 6. match without passkeys: only login items matching URL
        json!({"cmd": "match", "origin": "https://github.com"}),

        // 7. match with include_passkeys: true -> both login and passkey matching origin
        json!({"cmd": "match", "origin": "https://github.com", "include_passkeys": true}),

        // 8. match with include_passkeys: false -> passkey excluded
        json!({"cmd": "match", "origin": "https://github.com", "include_passkeys": false}),

        // 9. match on non-matching domain
        json!({"cmd": "match", "origin": "https://nomatch.example"}),

        // 10. get_item for login (concealed password, revealed website, totp, last_used)
        json!({"cmd": "get_item", "item_id": gh_id}),

        // 11. get_item for credit card (masked card number "•••• •••• •••• 1234")
        json!({"cmd": "get_item", "item_id": card_id}),

        // 12. fill_card with source "popup"
        json!({"cmd": "fill_card", "item_id": card_id, "source": "popup"}),

        // 13. fill_identity with source "popup"
        json!({"cmd": "fill_identity", "item_id": ident_id, "source": "popup"}),

        // 14. item_findings for login
        json!({"cmd": "item_findings", "item_id": gh_id}),

        // 15. get_item for passkey
        json!({"cmd": "get_item", "item_id": passkey_id}),

        // 16. list_archived
        json!({"cmd": "list_archived"}),

        // 17. list_deleted
        json!({"cmd": "list_deleted"}),

        // 18. audit_summary (vault-wide watchtower overview)
        json!({"cmd": "audit_summary"}),
    ];

    for (idx, req) in fixtures.iter().enumerate() {
        let req_id = json!(100 + idx);
        let mut full_req = req.clone();
        full_req["id"] = req_id.clone();

        let host_resp = host.handle(&full_req);
        let agent_resp = agent.respond(req_id.clone(), &full_req);

        assert_eq!(
            host_resp["ok"], true,
            "Host failed on fixture {}: req={:?}, resp={:?}",
            idx, req, host_resp
        );
        assert_eq!(
            agent_resp["ok"], true,
            "Agent failed on fixture {}: req={:?}, resp={:?}",
            idx, req, agent_resp
        );

        assert_eq!(
            host_resp["result"], agent_resp["result"],
            "Mismatch on fixture {}: req={:?}\nHost result: {}\nAgent result: {}",
            idx, req, host_resp["result"], agent_resp["result"]
        );
    }

    // Explicitly verify related items on get_item response
    let gh_detail = agent.respond(json!(999), &json!({"id": 999, "cmd": "get_item", "item_id": gh_id}));
    let rel = gh_detail["result"]["item"]["related_items"].as_array().expect("related_items array");
    assert_eq!(rel.len(), 2);
    assert_eq!(rel[0]["id"], card_id);
    assert_eq!(rel[0]["title"], "Visa Card");
    assert_eq!(rel[0]["category"], "credit_card");
    assert_eq!(rel[0]["available"], true);
    assert_eq!(rel[1]["id"], "missing-item-id");
    assert_eq!(rel[1]["title"], "Item not available");
    assert_eq!(rel[1]["category"], serde_json::Value::Null);
    assert_eq!(rel[1]["available"], false);
}
