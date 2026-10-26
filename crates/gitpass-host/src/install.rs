//! Native messaging manifest generation + per-OS install locations.
//!
//! | Browser  | Linux (per user)                                         | macOS (per user)                                                  | Windows |
//! |----------|----------------------------------------------------------|-------------------------------------------------------------------|---------|
//! | chrome   | ~/.config/google-chrome/NativeMessagingHosts/            | ~/Library/Application Support/Google/Chrome/NativeMessagingHosts/ | HKCU\Software\Google\Chrome\NativeMessagingHosts\com.gitpass.host |
//! | chromium | ~/.config/chromium/NativeMessagingHosts/                 | ~/Library/Application Support/Chromium/NativeMessagingHosts/      | HKCU\Software\Chromium\NativeMessagingHosts\com.gitpass.host |
//! | brave    | ~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/ | ~/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts/ | HKCU\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.gitpass.host |
//! | firefox  | ~/.mozilla/native-messaging-hosts/                       | ~/Library/Application Support/Mozilla/NativeMessagingHosts/       | HKCU\Software\Mozilla\NativeMessagingHosts\com.gitpass.host |
//!
//! On Windows the JSON is written to `%LOCALAPPDATA%\gitpass\<browser>\com.gitpass.host.json` and the
//! registry key's default value is set to that path (via `reg add`).
use crate::HOST_NAME;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Browser { Chrome, Chromium, Brave, Firefox }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os { Linux, MacOs, Windows }

impl Browser {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "chrome" => Browser::Chrome, "chromium" => Browser::Chromium,
            "brave" => Browser::Brave, "firefox" => Browser::Firefox,
            o => bail!("unknown browser '{o}' (chrome|chromium|brave|firefox)"),
        })
    }
    pub fn is_firefox(self) -> bool { self == Browser::Firefox }
    pub fn name(self) -> &'static str {
        match self { Browser::Chrome => "chrome", Browser::Chromium => "chromium", Browser::Brave => "brave", Browser::Firefox => "firefox" }
    }
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") { Os::MacOs } else if cfg!(windows) { Os::Windows } else { Os::Linux }
    }
}

/// Chromium extension ids are 32 chars a–p; Firefox ids are `name@domain` or `{uuid}`.
pub fn validate_extension_id(b: Browser, id: &str) -> Result<()> {
    if b.is_firefox() {
        let ok = (id.contains('@') || (id.starts_with('{') && id.ends_with('}'))) && !id.contains(['"', '\\', '/']);
        if !ok { bail!("invalid Firefox extension id '{id}'"); }
    } else if id.len() != 32 || !id.bytes().all(|c| (b'a'..=b'p').contains(&c)) {
        bail!("invalid Chromium extension id '{id}' (32 chars a-p; see chrome://extensions)");
    }
    Ok(())
}

/// The string the browser passes as the caller identity (argv[1] on Chromium; extension id argv[2] on Firefox).
pub fn caller_origin(b: Browser, id: &str) -> String {
    if b.is_firefox() { id.to_string() } else { format!("chrome-extension://{id}/") }
}

pub fn manifest_json(b: Browser, exe: &Path, ext_id: &str) -> Value {
    let mut m = json!({
        "name": HOST_NAME,
        "description": "gitpass native messaging host",
        "path": exe.to_string_lossy(),
        "type": "stdio",
    });
    if b.is_firefox() { m["allowed_extensions"] = json!([ext_id]); }
    else { m["allowed_origins"] = json!([caller_origin(b, ext_id)]); }
    m
}

/// Directory the manifest JSON goes into.
pub fn manifest_dir(b: Browser, os: Os, home: &Path, local_app_data: Option<&Path>) -> PathBuf {
    match os {
        Os::Linux => home.join(match b {
            Browser::Chrome => ".config/google-chrome/NativeMessagingHosts",
            Browser::Chromium => ".config/chromium/NativeMessagingHosts",
            Browser::Brave => ".config/BraveSoftware/Brave-Browser/NativeMessagingHosts",
            Browser::Firefox => ".mozilla/native-messaging-hosts",
        }),
        Os::MacOs => home.join("Library/Application Support").join(match b {
            Browser::Chrome => "Google/Chrome/NativeMessagingHosts",
            Browser::Chromium => "Chromium/NativeMessagingHosts",
            Browser::Brave => "BraveSoftware/Brave-Browser/NativeMessagingHosts",
            Browser::Firefox => "Mozilla/NativeMessagingHosts",
        }),
        Os::Windows => local_app_data.map(Path::to_path_buf).unwrap_or_else(|| home.join("AppData").join("Local"))
            .join("gitpass").join(b.name()),
    }
}

/// Windows registry key (HKCU) whose default value must point at the manifest file.
pub fn windows_registry_key(b: Browser) -> String {
    let base = match b {
        Browser::Chrome => r"Software\Google\Chrome",
        Browser::Chromium => r"Software\Chromium",
        Browser::Brave => r"Software\BraveSoftware\Brave-Browser",
        Browser::Firefox => r"Software\Mozilla",
    };
    format!(r"HKCU\{base}\NativeMessagingHosts\{HOST_NAME}")
}

/// Write the manifest (and on Windows, register it). Returns the manifest path.
pub fn install(b: Browser, os: Os, home: &Path, exe: &Path, ext_id: &str) -> Result<PathBuf> {
    validate_extension_id(b, ext_id)?;
    let dir = manifest_dir(b, os, home, dirs::data_local_dir().as_deref());
    std::fs::create_dir_all(&dir)?;
    let p = dir.join(format!("{HOST_NAME}.json"));
    std::fs::write(&p, serde_json::to_string_pretty(&manifest_json(b, exe, ext_id))?)?;
    if os == Os::Windows && cfg!(windows) {
        let st = std::process::Command::new("reg")
            .args(["add", &windows_registry_key(b), "/ve", "/t", "REG_SZ", "/d"]).arg(&p).arg("/f").status()?;
        if !st.success() { bail!("reg add failed"); }
    }
    Ok(p)
}
