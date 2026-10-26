//! `gitpass run` / `gitpass inject`: resolve `gp://<vault>/<item>/<field>` secret references.
//!
//! * `run` resolves references in an env file and passes them ONLY through the child's environment
//!   (nothing is written to disk); the child's stdout/stderr are piped through [`Masker`] so resolved
//!   values are replaced with `<concealed by gitpass>`.
//! * `inject` renders a template, replacing `{{ gp://v/i/f }}` placeholders.
//!
//! Segments are percent-decoded (`%2F` = `/`, `%20` = space, `%25` = `%`), so names containing `/` work.
use anyhow::{anyhow, bail, Result};
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
    let (prog, args) = cmd.split_first().ok_or_else(|| anyhow!("no command given; usage: gitpass run --env-file F -- <cmd> [args]"))?;
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
