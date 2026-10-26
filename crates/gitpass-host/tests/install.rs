use gitpass_host::install::*;
use std::path::Path;

#[test]
fn manifests() {
    let id = "abcdefghijklmnopabcdefghijklmnop";
    let m = manifest_json(Browser::Chrome, Path::new("/usr/bin/gitpass-host"), id);
    assert_eq!(m["name"], "com.gitpass.host");
    assert_eq!(m["type"], "stdio");
    assert_eq!(m["allowed_origins"][0], format!("chrome-extension://{id}/"));
    assert!(m.get("allowed_extensions").is_none());
    let f = manifest_json(Browser::Firefox, Path::new("/x"), "gitpass@gitpass.dev");
    assert_eq!(f["allowed_extensions"][0], "gitpass@gitpass.dev");
    assert!(validate_extension_id(Browser::Chrome, "short").is_err());
    assert!(validate_extension_id(Browser::Chrome, "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_err());
    assert!(validate_extension_id(Browser::Firefox, "gitpass@gitpass.dev").is_ok());
    assert!(validate_extension_id(Browser::Firefox, "nope").is_err());
}

#[test]
fn paths() {
    let h = Path::new("/home/u");
    assert_eq!(manifest_dir(Browser::Chrome, Os::Linux, h, None), Path::new("/home/u/.config/google-chrome/NativeMessagingHosts"));
    assert_eq!(manifest_dir(Browser::Firefox, Os::Linux, h, None), Path::new("/home/u/.mozilla/native-messaging-hosts"));
    assert_eq!(manifest_dir(Browser::Brave, Os::MacOs, h, None), Path::new("/home/u/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts"));
    assert_eq!(windows_registry_key(Browser::Firefox), r"HKCU\Software\Mozilla\NativeMessagingHosts\com.gitpass.host");
}

#[test]
fn install_writes_file_linux() {
    let d = tempfile::tempdir().unwrap();
    let p = install(Browser::Chromium, Os::Linux, d.path(), Path::new("/opt/gitpass-host"), "abcdefghijklmnopabcdefghijklmnop").unwrap();
    assert!(p.ends_with(".config/chromium/NativeMessagingHosts/com.gitpass.host.json"));
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    assert_eq!(v["path"], "/opt/gitpass-host");
}
