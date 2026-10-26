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
    for bad in ["op://a/b/c", "gp://a/b", "gp://a//c", "gp://a/b/c/d", "gp://a/b/%zz"] { assert!(SecretRef::parse(bad).is_err(), "{bad}"); }
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

fn walk(p: &Path) -> Vec<std::path::PathBuf> {
    let mut v = vec![];
    if let Ok(rd) = std::fs::read_dir(p) { for e in rd.flatten() { let q = e.path(); if q.is_dir() { v.extend(walk(&q)); } v.push(q); } }
    v
}
