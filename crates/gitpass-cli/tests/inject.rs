//! gp:// references, env files, templates, output masking, and `gitpass run`/`inject` end to end
//! against a throwaway vault. All data is FAKE.
use gitpass_cli::inject::*;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use zeroize::Zeroizing;

struct Map(HashMap<(String, String, String), String>);
impl Resolver for Map {
    fn resolve(&self, r: &SecretRef) -> anyhow::Result<Zeroizing<String>> {
        self.0.get(&(r.vault.clone(), r.item.clone(), r.field.clone())).map(|v| Zeroizing::new(v.clone()))
            .ok_or_else(|| anyhow::anyhow!("missing"))
    }
}
fn map() -> Map {
    let mut m = HashMap::new();
    m.insert(("Dev".into(), "Postgres".into(), "password".into()), "pg-FAKE-s3cret".into());
    m.insert(("Dev".into(), "Postgres".into(), "username".into()), "app".into());
    m.insert(("My Vault".into(), "a/b".into(), "token".into()), "tok-FAKE-123".into());
    Map(m)
}

#[test]
fn parse_refs() {
    assert_eq!(SecretRef::parse("gp://Dev/Postgres/password").unwrap(), SecretRef { vault: "Dev".into(), item: "Postgres".into(), field: "password".into() });
    assert_eq!(SecretRef::parse("gp://My%20Vault/a%2Fb/token").unwrap().item, "a/b");
    for bad in ["xx://a/b/c", "gp://a/b", "gp://a//c", "gp://a/b/c/d", "gp://a/b/%zz"] { assert!(SecretRef::parse(bad).is_err(), "{bad}"); }
}

#[test]
fn env_file_and_template() {
    let m = map();
    let mut r = Resolved::new(&m);
    let env = parse_env_file("# c\nexport DB_PASS=gp://Dev/Postgres/password\nURL=\"postgres://{{ gp://Dev/Postgres/username }}:x@h\"\nPLAIN='hi'\n\nT=gp://My%20Vault/a%2Fb/token\n", &mut r).unwrap();
    let e: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(e, vec![("DB_PASS", "pg-FAKE-s3cret"), ("URL", "postgres://app:x@h"), ("PLAIN", "hi"), ("T", "tok-FAKE-123")]);
    assert_eq!(r.secrets.len(), 3);
    assert!(parse_env_file("1BAD=x", &mut r).is_err());
    assert!(parse_env_file("NOEQ", &mut r).is_err());
    assert!(parse_env_file("U=pre-gp://Dev/Postgres/username", &mut r).is_err(), "unwrapped embedded ref rejected");
    assert!(parse_env_file("X=gp://Dev/Nope/password", &mut r).is_err(), "unresolvable reference is an error, not empty");
    let mut r = Resolved::new(&m);
    let t = render_template("user={{gp://Dev/Postgres/username}}\npass={{ gp://Dev/Postgres/password }}\nkeep={{ other }}\n", &mut r).unwrap();
    assert_eq!(t.as_str(), "user=app\npass=pg-FAKE-s3cret\nkeep={{ other }}\n");
}

#[test]
fn masker_handles_split_writes() {
    let secrets = vec![Zeroizing::new("s3cret".to_string()), Zeroizing::new("ab".to_string())];
    let mut m = Masker::new(Vec::new(), &secrets);
    for chunk in ["x s3", "cr", "et y a", "b z s3cre"] { m.feed(chunk.as_bytes()).unwrap(); }
    m.finish().unwrap();
    assert_eq!(String::from_utf8(m.get_ref().clone()).unwrap(), format!("x {MASK} y {MASK} z s3cre"));
}

fn bin() -> &'static str { env!("CARGO_BIN_EXE_gitpass") }
fn gp(root: &Path, args: &[&str], item_pw: Option<&str>) -> std::process::Output {
    let mut c = Command::new(bin());
    c.args(args).env("GITPASS_PATH", root).env("GITPASS_PASSWORD", "fake-master-pw-123").env("GITPASS_DEVICE_NAME", "test");
    if let Some(p) = item_pw { c.env("GITPASS_ITEM_PASSWORD", p); }
    c.output().unwrap()
}

#[test]
#[cfg(unix)]
fn run_and_inject_end_to_end() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Dev"], None).status.success());
    assert!(gp(&root, &["add", "Dev", "--title", "Postgres", "--username", "app"], Some("pg-FAKE-s3cret-9")).status.success());
    let envf = d.path().join(".env.gitpass");
    std::fs::write(&envf, "DB_PASSWORD=gp://Dev/Postgres/password\nDB_USER=gp://Dev/Postgres/username\n").unwrap();
    let before: Vec<_> = walk(d.path());

    // child sees the value in its env; its stdout/stderr have it masked
    let o = gp(&root, &["run", "--env-file", envf.to_str().unwrap(), "--", "sh", "-c",
        "test \"$DB_PASSWORD\" = pg-FAKE-s3cret-9 && echo \"pw=$DB_PASSWORD user=$DB_USER\" && echo \"err=$DB_PASSWORD\" >&2; exit 7"], None);
    let out = String::from_utf8_lossy(&o.stdout); let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(7), "exit code propagated; stderr={err}");
    assert!(out.contains(&format!("pw={MASK} user={MASK}")), "stdout={out}");
    assert!(err.contains(&format!("err={MASK}")), "stderr={err}");
    assert!(!out.contains("pg-FAKE-s3cret-9") && !err.contains("pg-FAKE-s3cret-9"));
    // --no-masking passes raw output through
    let o = gp(&root, &["run", "--no-masking", "--env-file", envf.to_str().unwrap(), "--", "sh", "-c", "echo $DB_PASSWORD"], None);
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "pg-FAKE-s3cret-9");
    // nothing new on disk contains the secret
    for p in walk(d.path()) {
        if before.contains(&p) || p.is_dir() { continue; }
        let b = std::fs::read(&p).unwrap_or_default();
        assert!(!b.windows(16).any(|w| w == b"pg-FAKE-s3cret-9"), "secret written to {}", p.display());
    }
    for p in walk(d.path()) { if p.is_file() { let b = std::fs::read(&p).unwrap(); assert!(!b.windows(16).any(|w| w == b"pg-FAKE-s3cret-9"), "plaintext in {}", p.display()); } }
    // bad reference fails before the child runs
    std::fs::write(&envf, "X=gp://Dev/Nope/password\n").unwrap();
    let o = gp(&root, &["run", "--env-file", envf.to_str().unwrap(), "--", "sh", "-c", "echo RAN"], None);
    assert!(!o.status.success()); assert!(!String::from_utf8_lossy(&o.stdout).contains("RAN"));

    // inject
    let tpl = d.path().join("cfg.tpl");
    std::fs::write(&tpl, "user: {{ gp://Dev/Postgres/username }}\npass: {{ gp://Dev/Postgres/password }}\n").unwrap();
    let o = gp(&root, &["inject", "-i", tpl.to_str().unwrap()], None);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "user: app\npass: pg-FAKE-s3cret-9\n");
    let outp = d.path().join("cfg.out");
    assert!(gp(&root, &["inject", "-i", tpl.to_str().unwrap(), "-o", outp.to_str().unwrap()], None).status.success());
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&outp).unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
#[cfg(unix)]
fn run_and_inject_missing_secret_errors() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Dev"], None).status.success());
    let out_add1 = gp(&root, &["add", "Dev", "--title", "Postgres", "--username", "app"], Some("pg-FAKE-s3cret-9"));
    assert!(out_add1.status.success());
    let out_str = String::from_utf8_lossy(&out_add1.stdout);
    let item1_id = out_str.trim().strip_prefix("added ").expect("item id").to_string();

    // Add item with no password (empty field)
    assert!(gp(&root, &["add", "Dev", "--title", "KeylessItem", "--username", "guest"], Some("")).status.success());
    // Add a second Postgres item to trigger ambiguous title error
    assert!(gp(&root, &["add", "Dev", "--title", "Postgres", "--username", "app2"], Some("pg-FAKE-s3cret-2")).status.success());

    let canary = d.path().join("canary.txt");
    let envf = d.path().join("test.env");

    // 1. Missing vault fails before child runs
    std::fs::write(&envf, "V=gp://NoSuchVault/Postgres/password\n").unwrap();
    let o = gp(&root, &["run", "--env-file", envf.to_str().unwrap(), "--", "sh", "-c", &format!("touch {}", canary.display())], None);
    assert!(!o.status.success(), "missing vault must fail");
    assert!(!canary.exists(), "child must not run on missing vault");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("vault"), "stderr={err}");
    assert!(!err.contains("pg-FAKE-s3cret"), "no secrets in stderr: {err}");

    // 2. Missing item in existing vault fails before child runs
    std::fs::write(&envf, "I=gp://Dev/NoSuchItem/password\n").unwrap();
    let o = gp(&root, &["run", "--env-file", envf.to_str().unwrap(), "--", "sh", "-c", &format!("touch {}", canary.display())], None);
    assert!(!o.status.success(), "missing item must fail");
    assert!(!canary.exists(), "child must not run on missing item");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("item"), "stderr={err}");
    assert!(!err.contains("pg-FAKE-s3cret"), "no secrets in stderr: {err}");

    // 3. Missing field on existing item fails before child runs
    let env_fld = format!("F=gp://Dev/{item1_id}/nonexistent_field\n");
    std::fs::write(&envf, &env_fld).unwrap();
    let o = gp(&root, &["run", "--env-file", envf.to_str().unwrap(), "--", "sh", "-c", &format!("touch {}", canary.display())], None);
    assert!(!o.status.success(), "missing field must fail");
    assert!(!canary.exists(), "child must not run on missing field");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("no such field"), "stderr={err}");
    assert!(!err.contains("pg-FAKE-s3cret"), "no secrets in stderr: {err}");

    // 4. Empty/absent password field fails before child runs
    std::fs::write(&envf, "E=gp://Dev/KeylessItem/password\n").unwrap();
    let o = gp(&root, &["run", "--env-file", envf.to_str().unwrap(), "--", "sh", "-c", &format!("touch {}", canary.display())], None);
    assert!(!o.status.success(), "empty field must fail");
    assert!(!canary.exists(), "child must not run on empty field");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("has no such field"), "stderr={err}");

    // 5. Ambiguous item title fails, but item ID succeeds
    std::fs::write(&envf, "A=gp://Dev/Postgres/password\n").unwrap();
    let o = gp(&root, &["run", "--env-file", envf.to_str().unwrap(), "--", "sh", "-c", &format!("touch {}", canary.display())], None);
    assert!(!o.status.success(), "ambiguous title must fail");
    assert!(!canary.exists(), "child must not run on ambiguous title");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("items have that title; use the item id"), "stderr={err}");

    // Exact item ID reference succeeds even when title is ambiguous
    std::fs::write(&envf, format!("VALID=gp://Dev/{item1_id}/password\n")).unwrap();
    let o = gp(&root, &["run", "--env-file", envf.to_str().unwrap(), "--", "sh", "-c",
        "test \"$VALID\" = pg-FAKE-s3cret-9 && echo OK"], None);
    assert!(o.status.success(), "stderr={}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stdout).contains("OK"));

    // 6. Non-existent env file fails before child runs
    let o = gp(&root, &["run", "--env-file", d.path().join("does_not_exist.env").to_str().unwrap(), "--", "sh", "-c", &format!("touch {}", canary.display())], None);
    assert!(!o.status.success());
    assert!(!canary.exists());

    // 7. Inject fails on missing secret and does NOT create output file
    let bad_tpl = d.path().join("missing.tpl");
    let bad_out = d.path().join("missing.out");
    std::fs::write(&bad_tpl, "pass: {{ gp://Dev/NoSuchItem/password }}\n").unwrap();
    let o = gp(&root, &["inject", "-i", bad_tpl.to_str().unwrap(), "-o", bad_out.to_str().unwrap()], None);
    assert!(!o.status.success(), "inject with missing secret must fail");
    assert!(!bad_out.exists(), "output file must not be created on inject failure");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!err.contains("pg-FAKE-s3cret"), "no secrets in stderr: {err}");
}

#[test]
#[cfg(unix)]
fn run_and_inject_env_file_output_and_features() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Dev"], None).status.success());
    assert!(gp(&root, &["add", "Dev", "--title", "Postgres", "--username", "app"], Some("pg-FAKE-s3cret-9")).status.success());
    assert!(gp(&root, &["add", "Dev", "--title", "Svc", "--username", "svc", "--totp", "JBSWY3DPEHPK3PXP"], Some("svc-FAKE-pass-1")).status.success());

    // Import record with custom sections and custom fields
    let import_file = d.path().join("custom.json");
    std::fs::write(&import_file, r#"{
        "gitpass_export": 1,
        "items": [
            {
                "title": "Stripe",
                "category": "api_credential",
                 "updated_at": "1700000000",
                "sections": [
                    {
                        "id": "keys",
                        "title": "API Keys",
                        "fields": [
                            { "id": "f_sec", "label": "secret_key", "kind": "concealed", "value": "sk_live_FAKE_SECRET_987" }
                        ]
                    }
                ]
            }
        ]
    }"#).unwrap();
    assert!(gp(&root, &["import", "--from", "gitpass-json", "Dev", import_file.to_str().unwrap()], None).status.success());

    // 1. Multiple --env-file stacking and custom fields + TOTP
    let env1 = d.path().join(".env.base");
    let env2 = d.path().join(".env.secrets");
    std::fs::write(&env1, "# Base configuration\nAPP_ENV=staging\nPORT=8080\n").unwrap();
    std::fs::write(&env2, "DB_PASS=gp://Dev/Postgres/password\nSTRIPE_KEY=gp://Dev/Stripe/secret_key\nOTP=gp://Dev/Svc/totp\n").unwrap();

    let o = gp(&root, &["run", "--env-file", env1.to_str().unwrap(), "--env-file", env2.to_str().unwrap(), "--", "sh", "-c",
        "test \"$APP_ENV\" = staging && test \"$PORT\" = 8080 && test \"$DB_PASS\" = pg-FAKE-s3cret-9 && test \"$STRIPE_KEY\" = sk_live_FAKE_SECRET_987 && test ${#OTP} -eq 6 && echo \"p=$DB_PASS s=$STRIPE_KEY o=$OTP\""], None);
    let out = String::from_utf8_lossy(&o.stdout);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(o.status.success(), "stderr={err}");
    assert!(out.contains(&format!("p={MASK} s={MASK} o={MASK}")), "stdout={out}");
    assert!(!out.contains("pg-FAKE-s3cret-9") && !out.contains("sk_live_FAKE_SECRET_987"));

    // 2. .env file generation via `gitpass inject`
    let env_tpl = d.path().join(".env.template");
    let env_out = d.path().join(".env");
    std::fs::write(&env_tpl, "# Autogenerated .env file\nAPP_ENV=production\nDB_URL=\"postgres://{{ gp://Dev/Postgres/username }}:{{ gp://Dev/Postgres/password }}@db:5432/app\"\nAPI_KEY={{ gp://Dev/Stripe/secret_key }}\nPASSTHROUGH={{ UNTOUCHED_VAR }}\n").unwrap();

    let o = gp(&root, &["inject", "-i", env_tpl.to_str().unwrap(), "-o", env_out.to_str().unwrap()], None);
    assert!(o.status.success(), "stderr={}", String::from_utf8_lossy(&o.stderr));
    assert!(env_out.exists(), ".env must exist");

    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&env_out).unwrap().permissions().mode() & 0o777, 0o600, ".env must be mode 0600");

    let env_content = std::fs::read_to_string(&env_out).unwrap();
    assert!(env_content.contains("APP_ENV=production"));
    assert!(env_content.contains("DB_URL=\"postgres://app:pg-FAKE-s3cret-9@db:5432/app\""));
    assert!(env_content.contains("API_KEY=sk_live_FAKE_SECRET_987"));
    assert!(env_content.contains("PASSTHROUGH={{ UNTOUCHED_VAR }}"));
}

fn walk(p: &Path) -> Vec<std::path::PathBuf> {
    let mut v = vec![];
    if let Ok(rd) = std::fs::read_dir(p) { for e in rd.flatten() { let q = e.path(); if q.is_dir() { v.extend(walk(&q)); } v.push(q); } }
    v
}

#[test]
#[cfg(unix)]
fn run_inherited_env_end_to_end() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "Dev"], None).status.success());
    assert!(gp(&root, &["add", "Dev", "--title", "Postgres", "--username", "app"], Some("pg-FAKE-s3cret-9")).status.success());
    assert!(gp(&root, &["add", "Dev", "--title", "GitHub", "--username", "octocat"], Some("ghp_FAKE_secret_token")).status.success());

    // 1. Inherited references resolved and masked without --env-file
    let mut c = Command::new(bin());
    c.args(["run", "--", "sh", "-c",
        r#"test "$DB_PASSWORD" = pg-FAKE-s3cret-9 && test "$GH_TOKEN" = ghp_FAKE_secret_token && echo "pw=$DB_PASSWORD tok=$GH_TOKEN" && echo "err=$DB_PASSWORD" >&2; exit 5"#])
        .env("GITPASS_PATH", &root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test")
        .env("DB_PASSWORD", "gp://Dev/Postgres/password")
        .env("GH_TOKEN", "gp://Dev/GitHub/password");
    let o = c.output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(5), "exit code propagated; stderr={err}");
    assert!(out.contains(&format!("pw={MASK} tok={MASK}")), "stdout={out}");
    assert!(err.contains(&format!("err={MASK}")), "stderr={err}");
    assert!(!out.contains("pg-FAKE-s3cret-9") && !out.contains("ghp_FAKE_secret_token"));
    assert!(!err.contains("pg-FAKE-s3cret-9") && !err.contains("ghp_FAKE_secret_token"));

    // 2. Inherited with --no-masking passes plain secret through
    let mut c = Command::new(bin());
    c.args(["run", "--no-masking", "--", "sh", "-c", "echo $GH_TOKEN"])
        .env("GITPASS_PATH", &root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test")
        .env("GH_TOKEN", "gp://Dev/GitHub/password");
    let o = c.output().unwrap();
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "ghp_FAKE_secret_token");

    // 3. Env file overrides inherited environment variable
    let envf = d.path().join("override.env");
    std::fs::write(&envf, "GH_TOKEN=overridden-token-value\n").unwrap();
    let mut c = Command::new(bin());
    c.args(["run", "--no-masking", "--env-file", envf.to_str().unwrap(), "--", "sh", "-c", "echo $GH_TOKEN"])
        .env("GITPASS_PATH", &root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test")
        .env("GH_TOKEN", "gp://Dev/GitHub/password");
    let o = c.output().unwrap();
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "overridden-token-value");

    // 4. No env-file and no gp:// in environment fails before child runs
    let mut c = Command::new(bin());
    c.args(["run", "--", "sh", "-c", "echo RAN"])
        .env("GITPASS_PATH", &root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test");
    let o = c.output().unwrap();
    assert!(!o.status.success(), "must fail without env file or gp:// references");
    assert!(!String::from_utf8_lossy(&o.stdout).contains("RAN"));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("no gp:// references found"), "stderr={err}");

    // 5. Unresolvable inherited reference fails before child runs, naming variable with no secret
    let mut c = Command::new(bin());
    c.args(["run", "--", "sh", "-c", "echo RAN"])
        .env("GITPASS_PATH", &root)
        .env("GITPASS_PASSWORD", "fake-master-pw-123")
        .env("GITPASS_DEVICE_NAME", "test")
        .env("BAD_TOKEN_VAR", "gp://Dev/NoSuchItem/password");
    let o = c.output().unwrap();
    assert!(!o.status.success(), "must fail on unresolvable reference");
    assert!(!String::from_utf8_lossy(&o.stdout).contains("RAN"));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("BAD_TOKEN_VAR"), "stderr must name variable: {err}");
    assert!(err.contains("NoSuchItem"), "stderr must contain reason: {err}");
    assert!(!err.contains("FAKE-s3cret") && !err.contains("ghp_FAKE"), "no secrets in stderr: {err}");
}

#[test]
fn reference_encoding_round_trips_through_parse() {
    for (v, i, f) in [("Dev", "Postgres", "password"), ("My Vault", "a/b", "api key"), ("100% {legit}", "Caf\u{e9} \u{1F600} #1", "x%2Fy"), ("v", "t", "a.b-c_d~e")] {
        let r = SecretRef { vault: v.into(), item: i.into(), field: f.into() };
        let text = r.to_string();
        assert!(text.starts_with("gp://") && !text.contains(' ') && !text.contains('{'), "{text}");
        assert_eq!(text.matches('/').count(), 4, "only the separators are literal slashes: {text}");
        assert_eq!(SecretRef::parse(&text).unwrap(), r, "{text}");
    }
    assert_eq!(SecretRef { vault: "My Vault".into(), item: "a/b".into(), field: "token".into() }.to_string(), "gp://My%20Vault/a%2Fb/token");
    // a produced reference resolves through the same resolver `run`/`inject` use
    let m = map();
    let text = SecretRef { vault: "My Vault".into(), item: "a/b".into(), field: "token".into() }.to_string();
    assert_eq!(render_template(&format!("{{{{ {text} }}}}"), &mut Resolved::new(&m)).unwrap().as_str(), "tok-FAKE-123");
}

#[test]
fn field_segment_prefers_standard_names_then_unique_labels_then_ids() {
    let f = |id: &str, label: &str| (id.to_string(), label.to_string());
    let fields = vec![f("username", "username"), f("password", "password"), f("f1", "API Key"), f("f2", "Region"), f("f3", "region"), f("f4", "Password")];
    assert_eq!(field_segment("Password", &fields).unwrap(), "password");
    assert_eq!(field_segment("otp", &fields).unwrap(), "otp");
    assert_eq!(field_segment("api key", &fields).unwrap(), "API Key");
    assert_eq!(field_segment("f1", &fields).unwrap(), "API Key");
    assert_eq!(field_segment("Region", &fields).unwrap(), "f2", "duplicate labels fall back to the field id");
    assert_eq!(field_segment("f4", &fields).unwrap(), "f4", "a custom label that shadows a standard name uses the id");
    let e = field_segment("nope", &fields).unwrap_err().to_string();
    assert!(e.contains("no field 'nope'") && e.contains("API Key"), "{e}");
}

#[test]
#[cfg(unix)]
fn ref_command_prints_references_that_run_and_inject_resolve() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("vault");
    assert!(gp(&root, &["init", "--save-secret-key"], None).status.success());
    assert!(gp(&root, &["vault", "create", "My Vault"], None).status.success());
    assert!(gp(&root, &["add", "My Vault", "--title", "Prod DB/primary", "--username", "app"], Some("pg-FAKE-ref-1")).status.success());

    let refp = gp(&root, &["ref", "My Vault", "Prod DB"], None);
    assert!(refp.status.success(), "{}", String::from_utf8_lossy(&refp.stderr));
    let pw_ref = String::from_utf8_lossy(&refp.stdout).trim().to_string();
    assert_eq!(pw_ref, "gp://My%20Vault/Prod%20DB%2Fprimary/password");
    let user_ref = String::from_utf8_lossy(&gp(&root, &["ref", "My Vault", "Prod DB", "username"], None).stdout).trim().to_string();
    assert_eq!(user_ref, "gp://My%20Vault/Prod%20DB%2Fprimary/username");

    // the printed references resolve through inject and run
    let tpl = d.path().join("t.tpl");
    std::fs::write(&tpl, format!("{{{{ {user_ref} }}}}:{{{{ {pw_ref} }}}}\n")).unwrap();
    let o = gp(&root, &["inject", "-i", tpl.to_str().unwrap()], None);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "app:pg-FAKE-ref-1\n");
    let envf = d.path().join(".env");
    std::fs::write(&envf, format!("PW={pw_ref}\n")).unwrap();
    let o = gp(&root, &["run", "--no-masking", "--env-file", envf.to_str().unwrap(), "--", "sh", "-c", "echo $PW"], None);
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "pg-FAKE-ref-1");

    // the reference never contains the secret
    assert!(!pw_ref.contains("FAKE"));
    // unknown field and ambiguous / missing items are errors, not references
    let o = gp(&root, &["ref", "My Vault", "Prod DB", "nope"], None);
    assert!(!o.status.success() && o.stdout.is_empty());
    let o = gp(&root, &["ref", "My Vault", "zzz-no-such-item"], None);
    assert!(!o.status.success() && o.stdout.is_empty());
    let o = gp(&root, &["ref", "Nope Vault", "Prod DB"], None);
    assert!(!o.status.success() && o.stdout.is_empty());
}
