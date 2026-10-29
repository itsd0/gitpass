//! Integration tests for `gitpass add --category` (login, api-credential, secure-note, credit-card).
//! Validates hidden prompts, stdin flags, secrecy masks in `get` / `get --json`, `list --json`,
//! and rejection of incompatible flags or invalid dates.
//! All test data is FAKE.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn gp_env(
    root: &Path,
    args: &[&str],
    stdin: Option<&str>,
    extra_env: &[(&str, &str)],
) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_gitpass"));
    c.args(args)
        .env("GITPASS_PATH", root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        c.env(k, v);
    }
    let mut ch = c.spawn().unwrap();
    if let Some(s) = stdin {
        let mut i = ch.stdin.take().unwrap();
        i.write_all(s.as_bytes()).unwrap();
    }
    ch.wait_with_output().unwrap()
}

fn gp(root: &Path, args: &[&str], stdin: Option<&str>) -> std::process::Output {
    gp_env(root, args, stdin, &[])
}

struct TestVault {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
}

fn setup_vault() -> TestVault {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Personal"], None).status.success());
    TestVault { _dir: dir, root }
}

#[test]
fn add_api_credential_hidden_prompt_and_secrecy() {
    let v = setup_vault();
    const SECRET_KEY: &str = "FAKE-api-key-xyz-987654";

    // 1. Add api-credential via hidden prompt / env var (never passed as CLI arg)
    let out_add = gp_env(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "api-credential",
            "--title",
            "OpenAI API",
            "--username",
            "org-acme",
            "--url",
            "https://api.openai.com",
            "--expires",
            "2028-06-30",
            "--notes",
            "Production key for LLM workloads",
        ],
        None,
        &[("GITPASS_ITEM_CREDENTIAL", SECRET_KEY)],
    );
    assert!(out_add.status.success(), "add failed: {}", String::from_utf8_lossy(&out_add.stderr));
    let add_msg = String::from_utf8_lossy(&out_add.stdout);
    assert!(add_msg.contains("added "), "expected added message: {add_msg}");

    // 2. gitpass list --json reports right category and username
    let out_list = gp(&v.root, &["list", "Personal", "--json"], None);
    assert!(out_list.status.success());
    let list_json: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_list.stdout)).unwrap();
    let item = list_json.as_array().unwrap().iter().find(|i| i["title"] == "OpenAI API").expect("item found");
    assert_eq!(item["category"], "api_credential");
    assert_eq!(item["username"], "org-acme");
    assert_eq!(item["url"], "https://api.openai.com");
    assert!(!String::from_utf8_lossy(&out_list.stdout).contains(SECRET_KEY));

    // 3. gitpass get shows it with secret masked unless --reveal
    let out_get_masked = gp(&v.root, &["get", "Personal", "OpenAI API"], None);
    assert!(out_get_masked.status.success());
    let text_masked = String::from_utf8_lossy(&out_get_masked.stdout);
    assert!(text_masked.contains("category: api_credential"), "stdout:\n{text_masked}");
    assert!(text_masked.contains("credential: ••••••••"), "stdout:\n{text_masked}");
    assert!(!text_masked.contains(SECRET_KEY), "secret leaked without --reveal");
    assert!(text_masked.contains("username: org-acme"));
    assert!(text_masked.contains("hostname: https://api.openai.com"));
    assert!(text_masked.contains("expires: 2028-06-30"));

    // 4. gitpass get with --reveal reveals the secret
    let out_get_revealed = gp(&v.root, &["get", "Personal", "OpenAI API", "--reveal"], None);
    assert!(out_get_revealed.status.success());
    let text_revealed = String::from_utf8_lossy(&out_get_revealed.stdout);
    assert!(text_revealed.contains(&format!("credential: {SECRET_KEY}")), "stdout:\n{text_revealed}");

    // 5. gitpass get --json masks secret unless --reveal
    let out_json_masked = gp(&v.root, &["get", "Personal", "OpenAI API", "--json"], None);
    assert!(out_json_masked.status.success());
    let json_masked: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_json_masked.stdout)).unwrap();
    assert_eq!(json_masked["category"], "api_credential");
    let fields = json_masked["fields"].as_array().unwrap();
    let cred_field = fields.iter().find(|f| f["label"] == "credential").expect("credential field");
    assert_eq!(cred_field["kind"], "concealed");
    assert_eq!(cred_field["value"], serde_json::Value::Null);

    let out_json_revealed = gp(&v.root, &["get", "Personal", "OpenAI API", "--json", "--reveal"], None);
    assert!(out_json_revealed.status.success());
    let json_revealed: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_json_revealed.stdout)).unwrap();
    let cred_rev = json_revealed["fields"].as_array().unwrap().iter().find(|f| f["label"] == "credential").unwrap();
    assert_eq!(cred_rev["value"], SECRET_KEY);

    // 6. gitpass get --password-only prints the credential
    let out_pw_only = gp(&v.root, &["get", "Personal", "OpenAI API", "--password-only"], None);
    assert!(out_pw_only.status.success());
    assert_eq!(String::from_utf8_lossy(&out_pw_only.stdout).trim(), SECRET_KEY);
}

#[test]
fn add_api_credential_stdin_and_type_alias() {
    let v = setup_vault();
    const SECRET_KEY: &str = "FAKE-api-token-stdin-4321";

    // Use alias --type and --host instead of --category and --url
    let out = gp(
        &v.root,
        &[
            "add",
            "Personal",
            "--type",
            "api_credential",
            "--title",
            "Cloudflare API",
            "--host",
            "https://api.cloudflare.com",
            "--credential-stdin",
        ],
        Some(SECRET_KEY),
    );
    assert!(out.status.success(), "add failed: {}", String::from_utf8_lossy(&out.stderr));

    let out_get = gp(&v.root, &["get", "Personal", "Cloudflare API", "--json", "--reveal"], None);
    assert!(out_get.status.success());
    let val: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_get.stdout)).unwrap();
    assert_eq!(val["category"], "api_credential");
    let fields = val["fields"].as_array().unwrap();
    let cred = fields.iter().find(|f| f["label"] == "credential").unwrap();
    assert_eq!(cred["value"], SECRET_KEY);
}

#[test]
fn add_secure_note_notes_and_stdin() {
    let v = setup_vault();

    // 1. Single-line note via --notes
    let out1 = gp(
        &v.root,
        &["add", "Personal", "--category", "secure-note", "--title", "WiFi Backup", "--notes", "Secret PSK: FAKE-net-key-999"],
        None,
    );
    assert!(out1.status.success());

    let out1_get = gp(&v.root, &["get", "Personal", "WiFi Backup"], None);
    assert!(out1_get.status.success());
    let text1 = String::from_utf8_lossy(&out1_get.stdout);
    assert!(text1.contains("category: secure_note"), "stdout:\n{text1}");
    assert!(text1.contains("note: Secret PSK: FAKE-net-key-999"), "stdout:\n{text1}");

    // 2. Multi-line note via --notes-stdin
    let multiline = "Line 1: Recovery instructions\nLine 2: Server IP 10.0.0.1\nLine 3: Keep offline";
    let out2 = gp(
        &v.root,
        &["add", "Personal", "--category", "secure_note", "--title", "Recovery Guide", "--notes-stdin"],
        Some(multiline),
    );
    assert!(out2.status.success());

    let out2_json = gp(&v.root, &["get", "Personal", "Recovery Guide", "--json"], None);
    assert!(out2_json.status.success());
    let val2: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out2_json.stdout)).unwrap();
    assert_eq!(val2["category"], "secure_note");
    let note_field = val2["fields"].as_array().unwrap().iter().find(|f| f["label"] == "note").unwrap();
    assert_eq!(note_field["kind"], "multiline");
    assert_eq!(note_field["value"], multiline);
}

#[test]
fn add_credit_card_prompts_and_masks() {
    let v = setup_vault();
    const CARD_NUM: &str = "4111222233334444";
    const CARD_CVV: &str = "789";

    let out_add = gp_env(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "credit-card",
            "--title",
            "Bank Visa",
            "--holder",
            "Bob Example",
            "--expiry",
            "11/30",
            "--notes",
            "Rewards credit card",
        ],
        None,
        &[("GITPASS_CARD_NUMBER", CARD_NUM), ("GITPASS_CARD_CVV", CARD_CVV)],
    );
    assert!(out_add.status.success(), "add failed: {}", String::from_utf8_lossy(&out_add.stderr));

    // 1. list --json reports credit_card
    let out_list = gp(&v.root, &["list", "Personal", "--json"], None);
    assert!(out_list.status.success());
    let list_json: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_list.stdout)).unwrap();
    let card = list_json.as_array().unwrap().iter().find(|i| i["title"] == "Bank Visa").unwrap();
    assert_eq!(card["category"], "credit_card");

    // 2. get without --reveal masks number and CVV
    let out_get = gp(&v.root, &["get", "Personal", "Bank Visa"], None);
    assert!(out_get.status.success());
    let text = String::from_utf8_lossy(&out_get.stdout);
    assert!(text.contains("category: credit_card"));
    assert!(text.contains("cardholder name: Bob Example"));
    assert!(text.contains("type: Visa"));
    assert!(text.contains("number: ••••••••"));
    assert!(text.contains("verification number: ••••••••"));
    assert!(text.contains("expiry date: 11/30"));
    assert!(text.contains("notes:    Rewards credit card"));
    assert!(!text.contains(CARD_NUM));
    assert!(!text.contains(CARD_CVV));

    // 3. get with --reveal reveals number and CVV
    let out_rev = gp(&v.root, &["get", "Personal", "Bank Visa", "--reveal"], None);
    assert!(out_rev.status.success());
    let text_rev = String::from_utf8_lossy(&out_rev.stdout);
    assert!(text_rev.contains(&format!("number: {CARD_NUM}")));
    assert!(text_rev.contains(&format!("verification number: {CARD_CVV}")));

    // 4. get --json masks unless --reveal
    let out_json = gp(&v.root, &["get", "Personal", "Bank Visa", "--json"], None);
    assert!(out_json.status.success());
    let val: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_json.stdout)).unwrap();
    let fields = val["fields"].as_array().unwrap();
    let num_f = fields.iter().find(|f| f["label"] == "number").unwrap();
    assert_eq!(num_f["value"], serde_json::Value::Null);
    let cvv_f = fields.iter().find(|f| f["label"] == "verification number").unwrap();
    assert_eq!(cvv_f["value"], serde_json::Value::Null);
}

#[test]
fn add_login_with_password_stdin() {
    let v = setup_vault();
    const SECRET_PW: &str = "FAKE-stdin-login-pw-1234!";

    let out = gp(
        &v.root,
        &["add", "Personal", "--title", "Script Login", "--username", "bot", "--password-stdin"],
        Some(SECRET_PW),
    );
    assert!(out.status.success(), "add failed: {}", String::from_utf8_lossy(&out.stderr));

    let out_get = gp(&v.root, &["get", "Personal", "Script Login", "--password-only"], None);
    assert!(out_get.status.success());
    assert_eq!(String::from_utf8_lossy(&out_get.stdout).trim(), SECRET_PW);
}

#[test]
fn invalid_flags_and_categories_rejected() {
    let v = setup_vault();

    // 1. Invalid category
    let out1 = gp(&v.root, &["add", "Personal", "--category", "invalid-cat", "--title", "X"], None);
    assert!(!out1.status.success());
    assert!(String::from_utf8_lossy(&out1.stderr).contains("invalid value 'invalid-cat'"));

    // 2. Incompatible flag: --holder on api-credential
    let out2 = gp(&v.root, &["add", "Personal", "--category", "api-credential", "--title", "X", "--holder", "Alice"], None);
    assert!(!out2.status.success());
    assert!(String::from_utf8_lossy(&out2.stderr).contains("--holder is only valid for credit-card items"));

    // 3. Incompatible flag: --totp on credit-card
    let out3 = gp(&v.root, &["add", "Personal", "--category", "credit-card", "--title", "X", "--totp", "123456"], None);
    assert!(!out3.status.success());
    assert!(String::from_utf8_lossy(&out3.stderr).contains("--totp is not supported for credit-card items"));

    // 4. Incompatible flag: --generate on secure-note
    let out4 = gp(&v.root, &["add", "Personal", "--category", "secure-note", "--title", "X", "--generate"], None);
    assert!(!out4.status.success());
    assert!(String::from_utf8_lossy(&out4.stderr).contains("--generate is not supported for secure-note items"));

    // 5. Incompatible flag: --credential-stdin on login
    let out5 = gp(&v.root, &["add", "Personal", "--title", "X", "--credential-stdin"], None);
    assert!(!out5.status.success());
    assert!(String::from_utf8_lossy(&out5.stderr).contains("--credential-stdin is only valid for api-credential items"));

    // 6. Conflicting flags: --notes and --notes-stdin on secure-note
    let out6 = gp(&v.root, &["add", "Personal", "--category", "secure-note", "--title", "X", "--notes", "a", "--notes-stdin"], None);
    assert!(!out6.status.success());
    assert!(String::from_utf8_lossy(&out6.stderr).contains("cannot combine --notes and --notes-stdin"));

    // 7. Conflicting flags: --generate and --password-stdin on login
    let out7 = gp(&v.root, &["add", "Personal", "--title", "X", "--generate", "--password-stdin"], None);
    assert!(!out7.status.success());
    assert!(String::from_utf8_lossy(&out7.stderr).contains("cannot combine --generate and --password-stdin"));

    // 8. Invalid date format for --expires
    let out8 = gp_env(
        &v.root,
        &["add", "Personal", "--category", "api-credential", "--title", "X", "--expires", "2028/06/30"],
        None,
        &[("GITPASS_ITEM_CREDENTIAL", "secret")],
    );
    assert!(!out8.status.success());
    assert!(String::from_utf8_lossy(&out8.stderr).contains("invalid expires date format"));

    // 9. Invalid date format for --expiry
    let out9 = gp_env(
        &v.root,
        &["add", "Personal", "--category", "credit-card", "--title", "X", "--expiry", "13/28"],
        None,
        &[("GITPASS_CARD_NUMBER", "4111111111111111"), ("GITPASS_CARD_CVV", "123")],
    );
    assert!(!out9.status.success());
    assert!(String::from_utf8_lossy(&out9.stderr).contains("invalid card expiry month"));
}
