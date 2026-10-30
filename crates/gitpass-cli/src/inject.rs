//! `gitpass run` / `gitpass inject`: resolve `gp://<vault>/<item>/<field>` secret references.
//!
//! * `run` resolves references in an env file and passes them ONLY through the child's environment
//!   (nothing is written to disk); the child's stdout/stderr are piped through [`Masker`] so resolved
//!   values are replaced with `<concealed by gitpass>`.
//! * `inject` renders a template, replacing `{{ gp://v/i/f }}` placeholders.
//!
//! Segments are percent-decoded (`%2F` = `/`, `%20` = space, `%25` = `%`), so names containing `/` work.
use anyhow::{anyhow, bail, Context, Result};
use std::io::{Read, Write};
use zeroize::Zeroizing;

pub const MASK: &str = "<concealed by gitpass>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef { pub vault: String, pub item: String, pub field: String }

fn pct_decode(s: &str) -> Result<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = s.get(i + 1..i + 3).ok_or_else(|| anyhow!("bad percent-escape in '{s}'"))?;
            out.push(u8::from_str_radix(h, 16).map_err(|_| anyhow!("bad percent-escape in '{s}'"))?);
            i += 3;
        } else { out.push(b[i]); i += 1; }
    }
    Ok(String::from_utf8(out)?)
}

impl SecretRef {
    pub fn parse(s: &str) -> Result<Self> {
        let rest = s.trim().strip_prefix("gp://").ok_or_else(|| anyhow!("secret reference must start with gp://: '{s}'"))?;
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
            bail!("secret reference must be gp://<vault>/<item>/<field> (got '{s}'; percent-encode '/' as %2F)");
        }
        Ok(SecretRef { vault: pct_decode(parts[0])?, item: pct_decode(parts[1])?, field: pct_decode(parts[2])? })
    }
}

/// Standard item fields that `gp://` references resolve by name before looking at custom fields.
const STANDARD_FIELDS: [&str; 7] = ["password", "username", "url", "notes", "title", "totp", "otp"];

/// Percent-encode one reference segment so [`SecretRef::parse`] reads it back unchanged. Letters, digits
/// (any script) and `-_.~` stay as written; everything else, including `/`, `%`, spaces and braces, becomes `%XX`.
pub fn encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() { out.push_str(&format!("%{b:02X}")); }
        }
    }
    out
}

impl std::fmt::Display for SecretRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "gp://{}/{}/{}", encode_segment(&self.vault), encode_segment(&self.item), encode_segment(&self.field))
    }
}

/// The field segment that resolves to the field the user asked for (`wanted` is a standard name, a field
/// label or a field id). `fields` is every live `(id, label)` of the item. A standard name stays as is; a
/// custom field uses its label when that label is unique and cannot be mistaken for a standard field, and
/// its id otherwise.
pub fn field_segment(wanted: &str, fields: &[(String, String)]) -> Result<String> {
    let lower = wanted.to_ascii_lowercase();
    if STANDARD_FIELDS.contains(&lower.as_str()) { return Ok(lower); }
    let hit = fields.iter().find(|(id, _)| id == wanted)
        .or_else(|| fields.iter().find(|(_, label)| label.eq_ignore_ascii_case(wanted)));
    let Some((id, label)) = hit else {
        let names: Vec<&str> = fields.iter().map(|(_, l)| l.as_str()).collect();
        bail!("no field '{wanted}' in this item (custom fields: {})", if names.is_empty() { "none".to_string() } else { names.join(", ") });
    };
    let same_label = fields.iter().filter(|(_, l)| l.eq_ignore_ascii_case(label)).count();
    let shadowed = STANDARD_FIELDS.contains(&label.to_ascii_lowercase().as_str());
    Ok(if same_label == 1 && !shadowed && !label.is_empty() { label.clone() } else { id.clone() })
}

/// Looks up a single reference (the binary implements this on an unlocked Session; tests use a map).
pub trait Resolver { fn resolve(&self, r: &SecretRef) -> Result<Zeroizing<String>>; }

/// Collects every resolved value so output masking knows what to hide.
pub struct Resolved<'a> { inner: &'a dyn Resolver, pub secrets: Vec<Zeroizing<String>> }
impl<'a> Resolved<'a> {
    pub fn new(inner: &'a dyn Resolver) -> Self { Resolved { inner, secrets: vec![] } }
    fn get(&mut self, r: &SecretRef) -> Result<Zeroizing<String>> {
        let v = self.inner.resolve(r)?;
        if !v.is_empty() && !self.secrets.iter().any(|s| **s == *v) { self.secrets.push(v.clone()); }
        Ok(v)
    }
}

/// Env-file value: a whole-value `gp://...` reference, or text with embedded `{{ gp://... }}` placeholders.
fn resolve_value(v: &str, res: &mut Resolved) -> Result<Zeroizing<String>> {
    if v.starts_with("gp://") { return res.get(&SecretRef::parse(v)?); }
    if v.contains("gp://") && !v.contains("{{") { bail!("embedded gp:// reference must be wrapped as {{{{ gp://... }}}}: '{v}'"); }
    render_template(v, res)
}

/// Parse a dotenv-style file: `KEY=value`, `export KEY=value`, `#` comments, optional single/double
/// quotes (no escape processing). A value is either a whole `gp://` reference or
/// contains `{{ gp://... }}` placeholders (needed when the reference is embedded in other text).
pub fn parse_env_file(text: &str, res: &mut Resolved) -> Result<Vec<(String, Zeroizing<String>)>> {
    let mut out = vec![];
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (k, v) = line.split_once('=').ok_or_else(|| anyhow!("env file line {}: expected KEY=value", n + 1))?;
        let k = k.trim();
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || k.starts_with(|c: char| c.is_ascii_digit()) {
            bail!("env file line {}: invalid variable name '{k}'", n + 1);
        }
        let v = v.trim();
        let v = if v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\''))) { &v[1..v.len() - 1] } else { v };
        out.push((k.to_string(), resolve_value(v, res)?));
    }
    Ok(out)
}

/// Render a template: `{{ gp://v/i/f }}` (whitespace inside braces optional) -> value.
pub fn render_template(text: &str, res: &mut Resolved) -> Result<Zeroizing<String>> {
    let mut out = Zeroizing::new(String::with_capacity(text.len()));
    let mut rest = text;
    while let Some(i) = rest.find("{{") {
        let after = &rest[i + 2..];
        let Some(j) = after.find("}}") else { break };
        let inner = after[..j].trim();
        if inner.starts_with("gp://") {
            out.push_str(&rest[..i]);
            out.push_str(&res.get(&SecretRef::parse(inner)?)?);
        } else {
            out.push_str(&rest[..i + 2 + j + 2]);
        }
        rest = &after[j + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Resolve secrets for `gitpass run` from inherited environment variables and/or env files.
///
/// * Scans `inherited` environment variables: any variable whose value is (or contains) a `gp://`
///   reference is resolved, unless overridden by an env file.
/// * Parses and resolves each env file in `env_file_contents`. Values defined in an env file override
///   inherited variables with the same name.
/// * If no env file was given (`env_file_contents` is empty) and no inherited variable contains a
///   `gp://` reference, returns an error.
/// * Fails before running if any reference cannot be resolved, naming the variable without leaking secrets.
pub fn resolve_run_env<I, K, V>(
    inherited: I,
    env_file_contents: &[&str],
    res: &mut Resolved,
) -> Result<Vec<(String, Zeroizing<String>)>>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn insert_or_update(vec: &mut Vec<(String, Zeroizing<String>)>, key: String, val: Zeroizing<String>) {
        if let Some(existing) = vec.iter_mut().find(|(k, _)| *k == key) {
            existing.1 = val;
        } else {
            vec.push((key, val));
        }
    }

    let mut file_vars: Vec<(String, Zeroizing<String>)> = Vec::new();
    for (n, content) in env_file_contents.iter().enumerate() {
        let parsed = parse_env_file(content, res)
            .with_context(|| format!("parsing env file {}", n + 1))?;
        for (k, v) in parsed {
            insert_or_update(&mut file_vars, k, v);
        }
    }

    let mut inherited_vars: Vec<(String, Zeroizing<String>)> = Vec::new();
    let mut has_inherited_ref = false;

    for (k, v) in inherited {
        let k = k.as_ref();
        let v = v.as_ref();
        if v.contains("gp://") {
            has_inherited_ref = true;
            if !file_vars.iter().any(|(fk, _)| fk == k) {
                let resolved = resolve_value(v, res)
                    .map_err(|e| anyhow!("environment variable '{k}': {e}"))?;
                insert_or_update(&mut inherited_vars, k.to_string(), resolved);
            }
        }
    }

    if env_file_contents.is_empty() && !has_inherited_ref {
        bail!("no gp:// references found: no --env-file was given and no inherited environment variables contain gp:// references (usage: gitpass run [--env-file F] -- <cmd>)");
    }

    let mut out = inherited_vars;
    out.extend(file_vars);
    Ok(out)
}


/// Streaming masker: replaces any occurrence of a secret, even if split across writes, by holding back
/// at most `longest_secret - 1` bytes until more data (or EOF) arrives.
pub struct Masker<W: Write> { out: W, secrets: Vec<Vec<u8>>, buf: Vec<u8>, hold: usize }
impl<W: Write> Masker<W> {
    pub fn new(out: W, secrets: &[Zeroizing<String>]) -> Self {
        let mut s: Vec<Vec<u8>> = secrets.iter().filter(|s| !s.is_empty()).map(|s| s.as_bytes().to_vec()).collect();
        s.sort_by_key(|v| std::cmp::Reverse(v.len()));
        let hold = s.first().map(|v| v.len() - 1).unwrap_or(0);
        Masker { out, secrets: s, buf: vec![], hold }
    }
    fn emit(&mut self, upto: usize) -> std::io::Result<()> {
        // scan buf[..] for matches that START before `upto`; emit text before `upto`
        let mut i = 0; let mut o = Vec::new();
        while i < upto {
            if let Some(s) = self.secrets.iter().find(|s| self.buf[i..].starts_with(s)) {
                o.extend_from_slice(MASK.as_bytes()); i += s.len();
            } else { o.push(self.buf[i]); i += 1; }
        }
        self.buf.drain(..i);
        self.out.write_all(&o)?;
        self.out.flush()
    }
    pub fn feed(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.buf.extend_from_slice(data);
        if self.buf.len() > self.hold { let n = self.buf.len() - self.hold; self.emit(n)?; }
        Ok(())
    }
    pub fn finish(&mut self) -> std::io::Result<()> { let n = self.buf.len(); self.emit(n) }
    pub fn get_ref(&self) -> &W { &self.out }
}
impl<W: Write> Drop for Masker<W> { fn drop(&mut self) { zeroize::Zeroize::zeroize(&mut self.buf); for s in &mut self.secrets { zeroize::Zeroize::zeroize(s); } } }

/// Copy `r` to `w` through a masker until EOF.
pub fn pump<R: Read, W: Write>(mut r: R, w: W, secrets: &[Zeroizing<String>]) -> std::io::Result<()> {
    let mut m = Masker::new(w, secrets);
    let mut chunk = [0u8; 8192];
    loop {
        let n = match r.read(&mut chunk) { Ok(0) => break, Ok(n) => n, Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue, Err(e) => return Err(e) };
        m.feed(&chunk[..n])?;
    }
    m.finish()?;
    Ok(())
}

/// Spawn `cmd` with `env` added to the inherited environment. stdin is inherited; stdout/stderr are
/// masked unless `mask` is false. Returns the child's exit code (128+signal on unix signals).
pub fn run_child(cmd: &[String], env: &[(String, Zeroizing<String>)], secrets: &[Zeroizing<String>], mask: bool) -> Result<i32> {
    use std::process::{Command, Stdio};
    let (prog, args) = cmd.split_first().ok_or_else(|| anyhow!("no command given; usage: gitpass run [--env-file F] -- <cmd> [args]"))?;
    let mut c = Command::new(prog);
    c.args(args).stdin(Stdio::inherit());
    for (k, v) in env { c.env(k, v.as_str()); }
    let masking = mask && !secrets.is_empty();
    if masking { c.stdout(Stdio::piped()).stderr(Stdio::piped()); }
    let mut child = c.spawn().map_err(|e| anyhow!("failed to start '{prog}': {e}"))?;
    let mut threads = vec![];
    if masking {
        let so = child.stdout.take().unwrap();
        let se = child.stderr.take().unwrap();
        let s1: Vec<Zeroizing<String>> = secrets.to_vec();
        let s2 = s1.clone();
        threads.push(std::thread::spawn(move || pump(so, std::io::stdout(), &s1)));
        threads.push(std::thread::spawn(move || pump(se, std::io::stderr(), &s2)));
    }
    let st = child.wait()?;
    for t in threads { let _ = t.join(); }
    #[cfg(unix)] { use std::os::unix::process::ExitStatusExt; if let Some(sig) = st.signal() { return Ok(128 + sig); } }
    Ok(st.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct TestResolver(HashMap<(String, String, String), String>);
    impl Resolver for TestResolver {
        fn resolve(&self, r: &SecretRef) -> Result<Zeroizing<String>> {
            self.0
                .get(&(r.vault.clone(), r.item.clone(), r.field.clone()))
                .map(|v| Zeroizing::new(v.clone()))
                .ok_or_else(|| anyhow!("missing"))
        }
    }

    fn test_resolver() -> TestResolver {
        let mut m = HashMap::new();
        m.insert(("Dev".into(), "Postgres".into(), "password".into()), "pg-FAKE-s3cret".into());
        m.insert(("Dev".into(), "Postgres".into(), "username".into()), "app".into());
        m.insert(("Personal".into(), "GitHub".into(), "token".into()), "ghp_FAKE_tok123".into());
        TestResolver(m)
    }

    #[test]
    fn test_inherited_only_references() {
        let resolver = test_resolver();
        let mut res = Resolved::new(&resolver);
        let inherited = vec![
            ("GITHUB_TOKEN", "gp://Personal/GitHub/token"),
            ("PATH", "/usr/bin:/bin"),
            ("DB_PASS", "gp://Dev/Postgres/password"),
        ];
        let resolved = resolve_run_env(inherited, &[], &mut res).unwrap();
        let map: HashMap<String, String> = resolved.into_iter().map(|(k, v)| (k, v.to_string())).collect();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("GITHUB_TOKEN").unwrap(), "ghp_FAKE_tok123");
        assert_eq!(map.get("DB_PASS").unwrap(), "pg-FAKE-s3cret");
        assert_eq!(res.secrets.len(), 2);
    }

    #[test]
    fn test_env_file_overriding_inherited() {
        let resolver = test_resolver();
        let mut res = Resolved::new(&resolver);
        let inherited = vec![
            ("GITHUB_TOKEN", "gp://Personal/GitHub/token"),
            ("DB_PASS", "gp://Dev/Postgres/password"),
            ("PORT", "3000"),
        ];
        let env_files = ["GITHUB_TOKEN=override-token\nDB_PASS=gp://Dev/Postgres/password\nNEW_VAR=hello"];
        let resolved = resolve_run_env(inherited, &env_files, &mut res).unwrap();
        let map: HashMap<String, String> = resolved.into_iter().map(|(k, v)| (k, v.to_string())).collect();
        assert_eq!(map.get("GITHUB_TOKEN").unwrap(), "override-token");
        assert_eq!(map.get("DB_PASS").unwrap(), "pg-FAKE-s3cret");
        assert_eq!(map.get("NEW_VAR").unwrap(), "hello");
        assert_eq!(map.get("PORT"), None);
    }

    #[test]
    fn test_no_references_at_all_error() {
        let resolver = test_resolver();
        let mut res = Resolved::new(&resolver);
        let inherited = vec![
            ("PATH", "/usr/bin"),
            ("USER", "alice"),
        ];
        let err = resolve_run_env(inherited, &[], &mut res).unwrap_err();
        assert!(err.to_string().contains("no gp:// references found"));
        assert!(err.to_string().contains("no --env-file was given"));
    }

    #[test]
    fn test_unresolvable_inherited_reference_error_no_secret() {
        let resolver = test_resolver();
        let mut res = Resolved::new(&resolver);
        let inherited = vec![("SECRET_VAR", "gp://Dev/NoSuchItem/password")];
        let err = resolve_run_env(inherited, &[], &mut res).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("SECRET_VAR"), "must name variable: {msg}");
        assert!(msg.contains("NoSuchItem") || msg.contains("missing"), "must contain reason: {msg}");
        assert!(!msg.contains("FAKE-s3cret") && !msg.contains("ghp_FAKE"), "no secrets in error: {msg}");
    }
}
