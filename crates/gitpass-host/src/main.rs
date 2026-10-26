//! `gitpass-host` — run by the browser (native messaging), or `gitpass-host install ...`.
use anyhow::{bail, Context, Result};
use gitpass_host::framing::{read_message, write_message};
use gitpass_host::handler::{Host, HostConfig};
use gitpass_host::install::{self, Browser, Os};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// `~/.config/gitpass/host.json` (written by `install`).
#[derive(Serialize, Deserialize, Default)]
struct FileConfig {
    vault_path: Option<PathBuf>,
    secret_key_path: Option<PathBuf>,
    idle_secs: Option<u64>,
    /// Caller identities allowed to talk to us (defence in depth; the browser enforces the manifest too).
    #[serde(default)]
    allowed_callers: Vec<String>,
}

fn config_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("GITPASS_HOST_CONFIG") { return Ok(PathBuf::from(p)); }
    Ok(dirs::config_dir().context("no config dir")?.join("gitpass").join("host.json"))
}

fn load_config() -> Result<FileConfig> {
    let p = config_path()?;
    if !p.exists() { return Ok(FileConfig::default()); }
    Ok(serde_json::from_str(&std::fs::read_to_string(&p)?)?)
}

const USAGE: &str = "usage:
  gitpass-host install --browser chrome|chromium|brave|firefox --extension-id <id> [--vault <dir>] [--secret-key <file>] [--idle-secs N] [--exe <path>]
  gitpass-host print-manifest --browser <b> --extension-id <id> [--exe <path>]
  gitpass-host <caller-origin> [...]   (invoked by the browser)";

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("install") | Some("print-manifest") => {
            let b = Browser::parse(&flag(&args, "--browser").context(USAGE)?)?;
            let id = flag(&args, "--extension-id").context(USAGE)?;
            let exe = match flag(&args, "--exe") { Some(p) => PathBuf::from(p), None => std::env::current_exe()?.canonicalize()? };
            if args[0] == "print-manifest" {
                install::validate_extension_id(b, &id)?;
                println!("{}", serde_json::to_string_pretty(&install::manifest_json(b, &exe, &id))?);
                return Ok(());
            }
            let home = dirs::home_dir().context("no home dir")?;
            let p = install::install(b, Os::current(), &home, &exe, &id)?;
            println!("wrote native messaging manifest: {}", p.display());
            if Os::current() == Os::Windows { println!("registered: {}", install::windows_registry_key(b)); }
            // merge host config
            let cp = config_path()?;
            let mut c = load_config()?;
            if let Some(v) = flag(&args, "--vault") { c.vault_path = Some(PathBuf::from(v)); }
            if let Some(v) = flag(&args, "--secret-key") { c.secret_key_path = Some(PathBuf::from(v)); }
            if let Some(v) = flag(&args, "--idle-secs") { c.idle_secs = Some(v.parse()?); }
            let caller = install::caller_origin(b, &id);
            if !c.allowed_callers.contains(&caller) { c.allowed_callers.push(caller); }
            std::fs::create_dir_all(cp.parent().unwrap())?;
            std::fs::write(&cp, serde_json::to_string_pretty(&c)?)?;
            println!("wrote host config: {}", cp.display());
            Ok(())
        }
        Some("-h") | Some("--help") | None => { eprintln!("{USAGE}"); Ok(()) }
        Some(_) => run_host(&args),
    }
}

fn run_host(args: &[String]) -> Result<()> {
    let c = load_config()?;
    // Chromium: argv[1] = "chrome-extension://<id>/". Firefox: argv[1] = manifest path, argv[2] = extension id.
    let caller = if args.len() >= 2 && !args[0].starts_with("chrome-extension://") { args[1].clone() } else { args[0].clone() };
    if c.allowed_callers.is_empty() || !c.allowed_callers.contains(&caller) {
        // Write an error frame so the extension can show it, then exit.
        let _ = write_message(&mut std::io::stdout(), &json!({"id": null, "ok": false, "error": format!("caller {caller} not allowed; run `gitpass-host install`")}));
        bail!("caller {caller} not in allowed_callers");
    }
    let home = dirs::home_dir().context("no home dir")?;
    let vault = c.vault_path.clone().unwrap_or_else(|| home.join(".gitpass"));
    let cfg = HostConfig {
        secret_key_path: c.secret_key_path.clone().unwrap_or_else(|| vault.join("secret.key")),
        vault_path: vault,
        idle_timeout: Duration::from_secs(c.idle_secs.unwrap_or(300)),
        target_kdf: Default::default(),
    };
    let host = Arc::new(Mutex::new(Host::new(cfg)));
    {
        let h = host.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            if let Ok(mut g) = h.lock() { g.expire_if_idle(); }
        });
    }
    let stdin = std::io::stdin();
    let mut rin = stdin.lock();
    let mut out = std::io::stdout();
    while let Some(msg) = read_message(&mut rin)? {
        let resp = host.lock().unwrap().handle(&msg);
        write_message(&mut out, &resp)?;
    }
    Ok(())
}
