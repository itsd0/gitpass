//! Integration tests for `gitpass add --category` (login, api-credential, secure-note, credit-card,
//! bank-account, server, identity, wifi, ssh-key).
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

#[test]
fn add_bank_account_prompts_stdin_and_list_category_filter() {
    let v = setup_vault();
    const ACCT_NUM: &str = "987654321098";
    const PIN: &str = "4321";

    // 1. Add bank-account via env prompts
    let out_add = gp_env(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "bank-account",
            "--title",
            "Chase Checking",
            "--bank",
            "JPMorgan Chase",
            "--holder",
            "Alice Example",
            "--account-type",
            "checking",
            "--routing",
            "021000021",
            "--branch-phone",
            "+1 800 935 9935",
            "--branch-address",
            "123 Main St, New York, NY",
            "--notes",
            "Primary checking account",
        ],
        None,
        &[("GITPASS_BANK_ACCOUNT_NUMBER", ACCT_NUM), ("GITPASS_BANK_PIN", PIN)],
    );
    assert!(out_add.status.success(), "add failed: {}", String::from_utf8_lossy(&out_add.stderr));

    // 2. Add another item of a different category
    let out_add2 = gp(
        &v.root,
        &["add", "Personal", "--category", "secure-note", "--title", "My Note", "--notes", "Hello world"],
        None,
    );
    assert!(out_add2.status.success());

    // 3. list --category filters by category
    let out_list_bank = gp(&v.root, &["list", "Personal", "--category", "bank-account", "--json"], None);
    assert!(out_list_bank.status.success());
    let bank_items: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_list_bank.stdout)).unwrap();
    assert_eq!(bank_items.as_array().unwrap().len(), 1);
    assert_eq!(bank_items[0]["title"], "Chase Checking");
    assert_eq!(bank_items[0]["category"], "bank_account");

    let out_list_note = gp(&v.root, &["list", "Personal", "--category", "secure-note", "--json"], None);
    assert!(out_list_note.status.success());
    let note_items: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_list_note.stdout)).unwrap();
    assert_eq!(note_items.as_array().unwrap().len(), 1);
    assert_eq!(note_items[0]["title"], "My Note");

    // 4. get without --reveal masks account number and PIN
    let out_get = gp(&v.root, &["get", "Personal", "Chase Checking"], None);
    assert!(out_get.status.success());
    let text = String::from_utf8_lossy(&out_get.stdout);
    assert!(text.contains("category: bank_account"));
    assert!(text.contains("bank name: JPMorgan Chase"));
    assert!(text.contains("account holder: Alice Example"));
    assert!(text.contains("account type: checking"));
    assert!(text.contains("account number: ••••••••"));
    assert!(text.contains("routing / SWIFT / IBAN: 021000021"));
    assert!(text.contains("PIN: ••••••••"));
    assert!(!text.contains(ACCT_NUM));
    assert!(!text.contains(PIN));

    // 5. get with --reveal reveals both
    let out_rev = gp(&v.root, &["get", "Personal", "Chase Checking", "--reveal"], None);
    assert!(out_rev.status.success());
    let text_rev = String::from_utf8_lossy(&out_rev.stdout);
    assert!(text_rev.contains(&format!("account number: {ACCT_NUM}")));
    assert!(text_rev.contains(&format!("PIN: {PIN}")));

    // 6. get --json masks unless --reveal
    let out_json = gp(&v.root, &["get", "Personal", "Chase Checking", "--json"], None);
    assert!(out_json.status.success());
    let val: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_json.stdout)).unwrap();
    assert_eq!(val["category"], "bank_account");
    let fields = val["fields"].as_array().unwrap();
    let acct_f = fields.iter().find(|f| f["label"] == "account number").unwrap();
    assert_eq!(acct_f["kind"], "concealed");
    assert_eq!(acct_f["value"], serde_json::Value::Null);

    // 7. Test stdin flag for account number
    let out_stdin = gp_env(
        &v.root,
        &["add", "Personal", "--category", "bank_account", "--title", "Savings Account", "--account-number-stdin"],
        Some("555544443333\n"),
        &[("GITPASS_BANK_PIN", "9999")],
    );
    assert!(out_stdin.status.success(), "stdin add failed: {}", String::from_utf8_lossy(&out_stdin.stderr));
    let out_stdin_get = gp(&v.root, &["get", "Personal", "Savings Account", "--reveal"], None);
    assert!(out_stdin_get.status.success());
    assert!(String::from_utf8_lossy(&out_stdin_get.stdout).contains("account number: 555544443333"));
}

#[test]
fn add_server_prompts_stdin_generate_and_get() {
    let v = setup_vault();
    const SRV_PASS: &str = "SecretSrvP@ssw0rd999!";

    // 1. Add server via hidden prompt
    let out_add = gp_env(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "server",
            "--title",
            "Production Web Node",
            "--url",
            "192.168.1.50",
            "--username",
            "ubuntu",
            "--admin-url",
            "https://aws.amazon.com/console",
            "--provider",
            "Amazon Web Services",
            "--notes",
            "Nginx proxy host",
        ],
        None,
        &[("GITPASS_ITEM_PASSWORD", SRV_PASS)],
    );
    assert!(out_add.status.success(), "add failed: {}", String::from_utf8_lossy(&out_add.stderr));

    // 2. list --category server returns it
    let out_list = gp(&v.root, &["list", "Personal", "--category", "server", "--json"], None);
    assert!(out_list.status.success());
    let list_json: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_list.stdout)).unwrap();
    let srv = list_json.as_array().unwrap().iter().find(|i| i["title"] == "Production Web Node").unwrap();
    assert_eq!(srv["category"], "server");
    assert_eq!(srv["username"], "ubuntu");
    assert_eq!(srv["url"], "192.168.1.50");

    // 3. get shows it with password masked unless --reveal
    let out_get = gp(&v.root, &["get", "Personal", "Production Web Node"], None);
    assert!(out_get.status.success());
    let text = String::from_utf8_lossy(&out_get.stdout);
    assert!(text.contains("category: server"));
    assert!(text.contains("URL or hostname: 192.168.1.50"));
    assert!(text.contains("username: ubuntu"));
    assert!(text.contains("password: ••••••••"));
    assert!(text.contains("admin console URL: https://aws.amazon.com/console"));
    assert!(text.contains("hosting provider: Amazon Web Services"));
    assert!(!text.contains(SRV_PASS));

    // 4. get --password-only prints the password
    let out_pw_only = gp(&v.root, &["get", "Personal", "Production Web Node", "--password-only"], None);
    assert!(out_pw_only.status.success());
    assert_eq!(String::from_utf8_lossy(&out_pw_only.stdout).trim(), SRV_PASS);

    // 5. Add server with --password-stdin
    let out_stdin = gp(
        &v.root,
        &["add", "Personal", "--category", "server", "--title", "DB Node", "--url", "db.internal", "--password-stdin"],
        Some("StdinDbPass!"),
    );
    assert!(out_stdin.status.success());
    let out_stdin_get = gp(&v.root, &["get", "Personal", "DB Node", "--password-only"], None);
    assert_eq!(String::from_utf8_lossy(&out_stdin_get.stdout).trim(), "StdinDbPass!");

    // 6. Add server with --generate
    let out_gen = gp(
        &v.root,
        &["add", "Personal", "--category", "server", "--title", "Generated Node", "--url", "gen.internal", "--generate", "32"],
        None,
    );
    assert!(out_gen.status.success());
    let out_gen_get = gp(&v.root, &["get", "Personal", "Generated Node", "--password-only"], None);
    assert_eq!(String::from_utf8_lossy(&out_gen_get.stdout).trim().len(), 32);
}

#[test]
fn add_identity_all_fields_and_minimal() {
    let v = setup_vault();

    // 1. Add identity with all fields
    let out_add = gp(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "identity",
            "--title",
            "Alice Personal",
            "--first-name",
            "Alice",
            "--last-name",
            "Doe",
            "--email",
            "alice@example.com",
            "--phone",
            "+1-555-0100",
            "--address",
            "123 Main St, Springfield",
            "--notes",
            "Primary personal contact card",
        ],
        None,
    );
    assert!(out_add.status.success(), "add identity failed: {}", String::from_utf8_lossy(&out_add.stderr));
    let add_msg = String::from_utf8_lossy(&out_add.stdout);
    assert!(add_msg.contains("added "), "expected added msg: {add_msg}");

    // 2. list --category identity --json
    let out_list = gp(&v.root, &["list", "Personal", "--category", "identity", "--json"], None);
    assert!(out_list.status.success());
    let list_json: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_list.stdout)).unwrap();
    let id_item = list_json.as_array().unwrap().iter().find(|i| i["title"] == "Alice Personal").expect("identity found");
    assert_eq!(id_item["category"], "identity");

    // 3. get shows sections and fields
    let out_get = gp(&v.root, &["get", "Personal", "Alice Personal"], None);
    assert!(out_get.status.success());
    let text = String::from_utf8_lossy(&out_get.stdout);
    assert!(text.contains("category: identity"), "stdout:\n{text}");
    assert!(text.contains("[Identification]"), "stdout:\n{text}");
    assert!(text.contains("first name: Alice"), "stdout:\n{text}");
    assert!(text.contains("last name: Doe"), "stdout:\n{text}");
    assert!(text.contains("[Address]"), "stdout:\n{text}");
    assert!(text.contains("address: 123 Main St, Springfield"), "stdout:\n{text}");
    assert!(text.contains("[Contact]"), "stdout:\n{text}");
    assert!(text.contains("email: alice@example.com"), "stdout:\n{text}");
    assert!(text.contains("phone: +1-555-0100"), "stdout:\n{text}");
    assert!(text.contains("notes:    Primary personal contact card"), "stdout:\n{text}");

    // 4. get --json verifies fields and types
    let out_get_json = gp(&v.root, &["get", "Personal", "Alice Personal", "--json"], None);
    assert!(out_get_json.status.success());
    let detail_json: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_get_json.stdout)).unwrap();
    assert_eq!(detail_json["category"], "identity");
    let fields = detail_json["fields"].as_array().unwrap();
    assert!(fields.iter().any(|f| f["label"] == "first name" && f["value"] == "Alice" && f["kind"] == "text"));
    assert!(fields.iter().any(|f| f["label"] == "last name" && f["value"] == "Doe" && f["kind"] == "text"));
    assert!(fields.iter().any(|f| f["label"] == "address" && f["value"] == "123 Main St, Springfield" && f["kind"] == "multiline"));
    assert!(fields.iter().any(|f| f["label"] == "email" && f["value"] == "alice@example.com" && f["kind"] == "email"));
    assert!(fields.iter().any(|f| f["label"] == "phone" && f["value"] == "+1-555-0100" && f["kind"] == "phone"));

    // 5. Add minimal identity with title only (all optional fields omitted)
    let out_min = gp(
        &v.root,
        &["add", "Personal", "--category", "identity", "--title", "Minimal ID"],
        None,
    );
    assert!(out_min.status.success(), "min identity failed: {}", String::from_utf8_lossy(&out_min.stderr));
    let out_min_get = gp(&v.root, &["get", "Personal", "Minimal ID"], None);
    assert!(out_min_get.status.success());
    assert!(String::from_utf8_lossy(&out_min_get.stdout).contains("category: identity"));
}

#[test]
fn add_wifi_hidden_prompt_stdin_and_secrecy() {
    let v = setup_vault();
    const WIFI_PASS: &str = "FAKE-wifi-secret-password-xyz!";

    // 1. Add wifi via hidden prompt / env var (never in argv)
    let out_add = gp_env(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "wifi",
            "--title",
            "Office Wi-Fi",
            "--ssid",
            "Corp-Guest",
            "--security",
            "WPA3",
            "--notes",
            "Guest office access",
        ],
        None,
        &[("GITPASS_WIFI_PASSWORD", WIFI_PASS)],
    );
    assert!(out_add.status.success(), "add wifi failed: {}", String::from_utf8_lossy(&out_add.stderr));
    let add_msg = String::from_utf8_lossy(&out_add.stdout);
    assert!(add_msg.contains("added "), "expected added msg: {add_msg}");
    assert!(!add_msg.contains(WIFI_PASS), "secret leaked in add stdout");
    assert!(!String::from_utf8_lossy(&out_add.stderr).contains(WIFI_PASS), "secret leaked in add stderr");

    // 2. list --category wifi --json
    let out_list = gp(&v.root, &["list", "Personal", "--category", "wifi", "--json"], None);
    assert!(out_list.status.success());
    let list_json: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_list.stdout)).unwrap();
    let wifi_item = list_json.as_array().unwrap().iter().find(|i| i["title"] == "Office Wi-Fi").expect("wifi found");
    assert_eq!(wifi_item["category"], "wifi");
    assert!(!String::from_utf8_lossy(&out_list.stdout).contains(WIFI_PASS), "secret leaked in list --json");

    // 3. get masks wireless password unless --reveal
    let out_get = gp(&v.root, &["get", "Personal", "Office Wi-Fi"], None);
    assert!(out_get.status.success());
    let text = String::from_utf8_lossy(&out_get.stdout);
    assert!(text.contains("category: wifi"));
    assert!(text.contains("network name: Corp-Guest"));
    assert!(text.contains("wireless password: ••••••••"));
    assert!(text.contains("security: WPA3"));
    assert!(text.contains("notes:    Guest office access"));
    assert!(!text.contains(WIFI_PASS), "secret leaked without --reveal");

    // 4. get --reveal reveals wireless password
    let out_reveal = gp(&v.root, &["get", "Personal", "Office Wi-Fi", "--reveal"], None);
    assert!(out_reveal.status.success());
    let text_reveal = String::from_utf8_lossy(&out_reveal.stdout);
    assert!(text_reveal.contains(&format!("wireless password: {WIFI_PASS}")));

    // 5. get --password-only prints only wireless password
    let out_pw_only = gp(&v.root, &["get", "Personal", "Office Wi-Fi", "--password-only"], None);
    assert!(out_pw_only.status.success());
    assert_eq!(String::from_utf8_lossy(&out_pw_only.stdout).trim(), WIFI_PASS);

    // 6. Add wifi with --password-stdin
    let out_stdin = gp(
        &v.root,
        &["add", "Personal", "--category", "wifi", "--title", "Lab Wi-Fi", "--ssid", "LabNet", "--password-stdin"],
        Some("LabSecret123"),
    );
    assert!(out_stdin.status.success());
    let out_stdin_get = gp(&v.root, &["get", "Personal", "Lab Wi-Fi", "--password-only"], None);
    assert_eq!(String::from_utf8_lossy(&out_stdin_get.stdout).trim(), "LabSecret123");

    // 7. Error on missing required field --ssid
    let out_no_ssid = gp(
        &v.root,
        &["add", "Personal", "--category", "wifi", "--title", "No SSID"],
        None,
    );
    assert!(!out_no_ssid.status.success());
    assert!(String::from_utf8_lossy(&out_no_ssid.stderr).contains("--ssid is required for wifi items"));

    // 8. Error on invalid security protocol
    let out_bad_sec = gp(
        &v.root,
        &["add", "Personal", "--category", "wifi", "--title", "Bad Sec", "--ssid", "SomeNet", "--security", "INVALID_SEC"],
        None,
    );
    assert!(!out_bad_sec.status.success());
    assert!(String::from_utf8_lossy(&out_bad_sec.stderr).contains("invalid wifi security 'INVALID_SEC'"));
}

#[test]
fn add_ssh_key_generate_and_secrecy() {
    let v = setup_vault();

    // 1. Add ssh-key with --generate ed25519
    let out_add = gp(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "ssh-key",
            "--title",
            "Work Laptop",
            "--generate",
            "ed25519",
            "--notes",
            "Primary SSH key for git commits",
        ],
        None,
    );
    assert!(out_add.status.success(), "add ssh-key failed: {}", String::from_utf8_lossy(&out_add.stderr));
    let add_stdout = String::from_utf8_lossy(&out_add.stdout);
    let add_stderr = String::from_utf8_lossy(&out_add.stderr);
    assert!(add_stdout.contains("added "), "expected added msg: {add_stdout}");
    // Private key must NEVER appear in stdout or stderr
    assert!(!add_stdout.contains("OPENSSH PRIVATE KEY"), "private key leaked in add stdout");
    assert!(!add_stderr.contains("OPENSSH PRIVATE KEY"), "private key leaked in add stderr");

    // 2. list --category ssh-key --json
    let out_list = gp(&v.root, &["list", "Personal", "--category", "ssh-key", "--json"], None);
    assert!(out_list.status.success());
    let list_json: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out_list.stdout)).unwrap();
    let key_item = list_json.as_array().unwrap().iter().find(|i| i["title"] == "Work Laptop").expect("ssh-key found");
    assert_eq!(key_item["category"], "ssh_key");
    assert!(!String::from_utf8_lossy(&out_list.stdout).contains("OPENSSH PRIVATE KEY"));

    // 3. get masks private key unless --reveal
    let out_get = gp(&v.root, &["get", "Personal", "Work Laptop"], None);
    assert!(out_get.status.success());
    let text = String::from_utf8_lossy(&out_get.stdout);
    assert!(text.contains("category: ssh_key"), "stdout:\n{text}");
    assert!(text.contains("private key: ••••••••"), "stdout:\n{text}");
    assert!(text.contains("public key: ssh-ed25519 "), "stdout:\n{text}");
    assert!(text.contains("fingerprint: SHA256:"), "stdout:\n{text}");
    assert!(text.contains("key type: ssh-ed25519"), "stdout:\n{text}");
    assert!(text.contains("notes:    Primary SSH key for git commits"), "stdout:\n{text}");
    assert!(!text.contains("OPENSSH PRIVATE KEY"), "private key leaked without --reveal");

    // 4. get --password-only prints the private key
    let out_pw_only = gp(&v.root, &["get", "Personal", "Work Laptop", "--password-only"], None);
    assert!(out_pw_only.status.success());
    let priv_key_text = String::from_utf8_lossy(&out_pw_only.stdout);
    assert!(priv_key_text.contains("-----BEGIN OPENSSH PRIVATE KEY-----"));
    assert!(priv_key_text.contains("-----END OPENSSH PRIVATE KEY-----"));
}

#[test]
fn add_ssh_key_import_and_errors() {
    let v = setup_vault();
    let td = tempfile::tempdir().unwrap();

    // 1. Generate an unencrypted OpenSSH key to test import
    let imp_key = gitpass_core::ssh::SshKey::generate_ed25519("alice@example.invalid").unwrap();
    let imp_file = td.path().join("id_ed25519");
    std::fs::write(&imp_file, imp_key.private_pem().unwrap().as_str()).unwrap();

    let out_add = gp(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "ssh-key",
            "--title",
            "Imported Deploy Key",
            "--private-key-file",
            imp_file.to_str().unwrap(),
        ],
        None,
    );
    assert!(out_add.status.success(), "import failed: {}", String::from_utf8_lossy(&out_add.stderr));
    let add_stdout = String::from_utf8_lossy(&out_add.stdout);
    let add_stderr = String::from_utf8_lossy(&out_add.stderr);
    assert!(add_stdout.contains("added "));
    assert!(!add_stdout.contains("OPENSSH PRIVATE KEY"));
    assert!(!add_stderr.contains("OPENSSH PRIVATE KEY"));

    // Check public key and fingerprint match the imported key
    let out_get = gp(&v.root, &["get", "Personal", "Imported Deploy Key"], None);
    assert!(out_get.status.success());
    let text = String::from_utf8_lossy(&out_get.stdout);
    assert!(text.contains(&imp_key.info.public_key));
    assert!(text.contains(&imp_key.info.fingerprint));

    // 2. Error when neither --generate nor --private-key-file is provided
    let out_none = gp(
        &v.root,
        &["add", "Personal", "--category", "ssh-key", "--title", "No Key Provided"],
        None,
    );
    assert!(!out_none.status.success());
    assert!(String::from_utf8_lossy(&out_none.stderr).contains("specify either --generate ed25519 or --private-key-file <PATH>"));

    // 3. Error when combining --generate and --private-key-file
    let out_conflict = gp(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "ssh-key",
            "--title",
            "Both Options",
            "--generate",
            "ed25519",
            "--private-key-file",
            imp_file.to_str().unwrap(),
        ],
        None,
    );
    assert!(!out_conflict.status.success());
    assert!(String::from_utf8_lossy(&out_conflict.stderr).contains("cannot combine --generate and --private-key-file"));

    // 4. Error on unsupported key type for --generate
    let out_bad_type = gp(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "ssh-key",
            "--title",
            "Bad Key Type",
            "--generate",
            "rsa",
        ],
        None,
    );
    assert!(!out_bad_type.status.success());
    assert!(String::from_utf8_lossy(&out_bad_type.stderr).contains("unsupported key type 'rsa' for --generate"));

    // 5. Error on nonexistent key file
    let out_missing_file = gp(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "ssh-key",
            "--title",
            "Missing File Key",
            "--private-key-file",
            "/path/to/nonexistent/key/file",
        ],
        None,
    );
    assert!(!out_missing_file.status.success());
    assert!(String::from_utf8_lossy(&out_missing_file.stderr).contains("read key file /path/to/nonexistent/key/file"));

    // 6. Error on invalid/corrupted key file
    let bad_file = td.path().join("bad_key");
    std::fs::write(&bad_file, "this is definitely not a valid openssh key\n").unwrap();
    let out_bad_file = gp(
        &v.root,
        &[
            "add",
            "Personal",
            "--category",
            "ssh-key",
            "--title",
            "Bad File Key",
            "--private-key-file",
            bad_file.to_str().unwrap(),
        ],
        None,
    );
    assert!(!out_bad_file.status.success());
    assert!(String::from_utf8_lossy(&out_bad_file.stderr).contains("not an OpenSSH private key"));
}

#[test]
fn add_flags_incompatible_rejection_new_categories() {
    let v = setup_vault();

    // 1. Identity flags on login
    let out1 = gp(
        &v.root,
        &["add", "Personal", "--category", "login", "--title", "X", "--first-name", "Alice"],
        None,
    );
    assert!(!out1.status.success());
    assert!(String::from_utf8_lossy(&out1.stderr).contains("identity flags are only valid for identity items"));

    // 2. Wifi flags on login
    let out2 = gp(
        &v.root,
        &["add", "Personal", "--category", "login", "--title", "X", "--ssid", "HomeNet"],
        None,
    );
    assert!(!out2.status.success());
    assert!(String::from_utf8_lossy(&out2.stderr).contains("wifi flags are only valid for wifi items"));

    // 3. Ssh flags on login
    let out3 = gp(
        &v.root,
        &["add", "Personal", "--category", "login", "--title", "X", "--private-key-file", "/some/path"],
        None,
    );
    assert!(!out3.status.success());
    assert!(String::from_utf8_lossy(&out3.stderr).contains("--private-key-file is only valid for ssh-key items"));

    // 4. Incompatible flags on identity
    let out4 = gp(
        &v.root,
        &["add", "Personal", "--category", "identity", "--title", "X", "--username", "alice"],
        None,
    );
    assert!(!out4.status.success());
    assert!(String::from_utf8_lossy(&out4.stderr).contains("--username is not supported for identity items"));

    // 5. Incompatible flags on wifi
    let out5 = gp(
        &v.root,
        &["add", "Personal", "--category", "wifi", "--title", "X", "--ssid", "Net", "--username", "alice"],
        None,
    );
    assert!(!out5.status.success());
    assert!(String::from_utf8_lossy(&out5.stderr).contains("--username is not supported for wifi items"));

    // 6. Incompatible flags on ssh-key
    let out6 = gp(
        &v.root,
        &["add", "Personal", "--category", "ssh-key", "--title", "X", "--generate", "ed25519", "--password-stdin"],
        None,
    );
    assert!(!out6.status.success());
    assert!(String::from_utf8_lossy(&out6.stderr).contains("--password-stdin is not supported for ssh-key items"));
}
