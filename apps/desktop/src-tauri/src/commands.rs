//! Thin Tauri 2 wrappers over `gitpass_gui_core::api::App`, one per contract `client.ts` command.
//! Command names are snake_case; Tauri 2 maps the frontend's camelCase args to these snake_case params.
//! Errors are returned to the frontend as strings.
use gitpass_gui_core::api as a;
/// Shared with the extension agent server (`agent_ui`), hence the Arc.
pub type App = std::sync::Arc<a::App>;
use tauri::{AppHandle, Emitter, Manager, State};
use zeroize::Zeroizing;

type R<T> = Result<T, String>;
fn e(x: anyhow::Error) -> String { format!("{x:#}") }

/// System clipboard via arboard (a fresh handle per call; arboard handles are not Sync on all platforms).
pub struct SysClipboard;
impl a::Clipboard for SysClipboard {
    fn set_text(&self, s: &str) -> anyhow::Result<()> {
        arboard::Clipboard::new().and_then(|mut c| c.set_text(s.to_string())).map_err(|x| anyhow::anyhow!("clipboard: {x}"))
    }
    fn get_text(&self) -> Option<String> { arboard::Clipboard::new().ok()?.get_text().ok() }
    #[cfg(target_os = "macos")]
    fn set_marked(&self, s: &str, markers: gitpass_pasteboard::PasteboardMarkers) -> anyhow::Result<()> {
        gitpass_pasteboard::write(s, markers)
    }
}

#[tauri::command] pub fn status(s: State<App>) -> R<a::UnlockState> { s.status().map_err(e) }
#[tauri::command(async)] pub fn unlock(s: State<'_, App>, vault_path: String, password: String, secret_key: Option<String>) -> R<a::UnlockState> {
    let password = Zeroizing::new(password);
    let sk = secret_key.map(Zeroizing::new);
    s.unlock(&vault_path, &password, sk.as_deref().map(|x| x.as_str())).map_err(e)
}
#[tauri::command(async)] pub fn create_account(s: State<'_, App>, vault_path: String, password: String, save_secret_key: bool) -> R<a::CreateAccountResult> {
    let password = Zeroizing::new(password);
    s.create_account(&vault_path, &password, save_secret_key).map_err(e)
}
#[tauri::command(async)] pub fn unlock_biometric(s: State<'_, App>, vault_path: String) -> R<a::UnlockState> { s.unlock_biometric(&vault_path).map_err(e) }
#[tauri::command] pub fn lock(app: tauri::AppHandle, s: State<App>) { s.lock(); crate::tray::update_tray(&app); }
#[tauri::command] pub fn list_vaults(s: State<App>) -> R<Vec<a::VaultInfo>> { s.list_vaults().map_err(e) }
#[tauri::command] pub fn update_vault(s: State<App>, vault_id: String, name: String, description: Option<String>, icon: Option<String>, color: Option<String>) -> R<a::VaultInfo> { s.update_vault(&vault_id, &name, description, icon, color).map_err(e) }
#[tauri::command] pub fn delete_vault(s: State<App>, vault_id: String, confirm_name: String) -> R<()> { s.delete_vault(&vault_id, &confirm_name).map_err(e) }
#[tauri::command] pub fn create_vault(s: State<App>, name: String, description: Option<String>, icon: Option<String>, color: Option<String>) -> R<a::VaultInfo> { s.create_vault(&name, description, icon, color).map_err(e) }
#[tauri::command] pub fn list_items(s: State<App>, filter: a::Filter) -> R<Vec<a::ItemSummary>> { s.list_items(&filter).map_err(e) }
#[tauri::command] pub fn get_item(s: State<App>, vault_id: String, item_id: String) -> R<a::ItemDetail> { s.get_item(&vault_id, &item_id).map_err(e) }
/// The only command that sends a concealed value to the webview (explicit user "reveal").
#[tauri::command] pub fn reveal_field(s: State<App>, vault_id: String, item_id: String, field_id: String, version: Option<u64>) -> R<String> { s.reveal_field_version(&vault_id, &item_id, &field_id, version).map(|z| z.to_string()).map_err(e) }
/// Copies in Rust; cleared after clipboardClearSecs only if the clipboard still holds our value.
#[tauri::command] pub fn copy_field(s: State<App>, vault_id: String, item_id: String, field_id: String) -> R<()> { s.copy_field(&vault_id, &item_id, &field_id).map_err(e) }
#[tauri::command] pub fn totp_now(s: State<App>, vault_id: String, item_id: String) -> R<a::TotpNow> { s.totp_now(&vault_id, &item_id).map_err(e) }
#[tauri::command] pub fn create_item(s: State<App>, input: a::ItemInput) -> R<a::ItemSummary> { s.create_item(&input).map_err(e) }
#[tauri::command] pub fn update_item(s: State<App>, item_id: String, input: a::ItemInput) -> R<a::ItemSummary> { s.update_item(&item_id, &input).map_err(e) }
#[tauri::command] pub fn set_favorite(s: State<App>, vault_id: String, item_id: String, favorite: bool) -> R<()> { s.set_favorite(&vault_id, &item_id, favorite).map_err(e) }
#[tauri::command] pub fn archive_item(s: State<App>, vault_id: String, item_id: String, archived: bool) -> R<()> { s.archive_item(&vault_id, &item_id, archived).map_err(e) }
#[tauri::command] pub fn delete_item(s: State<App>, vault_id: String, item_id: String) -> R<()> { s.delete_item(&vault_id, &item_id).map_err(e) }
#[tauri::command] pub fn restore_item(s: State<App>, vault_id: String, item_id: String) -> R<()> { s.restore_item(&vault_id, &item_id).map_err(e) }
#[tauri::command] pub fn purge_item(s: State<App>, vault_id: String, item_id: String) -> R<()> { s.purge_item(&vault_id, &item_id).map_err(e) }
#[tauri::command] pub fn ignore_finding(s: State<App>, vault_id: String, item_id: String, kind: String) -> R<()> { s.ignore_finding(&vault_id, &item_id, &kind).map_err(e) }
#[tauri::command] pub fn unignore_finding(s: State<App>, vault_id: String, item_id: String, kind: String) -> R<()> { s.unignore_finding(&vault_id, &item_id, &kind).map_err(e) }
#[tauri::command] pub fn empty_trash(s: State<App>) -> R<usize> { s.empty_trash().map_err(e) }
#[tauri::command] pub fn bulk_items(s: State<App>, req: a::BulkRequest) -> R<a::BulkResult> { s.bulk_items(&req).map_err(e) }
#[tauri::command] pub fn move_item(s: State<App>, vault_id: String, item_id: String, to_vault_id: String) -> R<a::ItemSummary> { s.move_item(&vault_id, &item_id, &to_vault_id).map_err(e) }
#[tauri::command] pub fn duplicate_item(s: State<App>, vault_id: String, item_id: String) -> R<a::ItemSummary> { s.duplicate_item(&vault_id, &item_id).map_err(e) }
#[tauri::command] pub fn item_version(s: State<App>, vault_id: String, item_id: String, version: u64) -> R<a::ItemDetail> { s.item_version(&vault_id, &item_id, version).map_err(e) }
#[tauri::command] pub fn restore_item_version(s: State<App>, vault_id: String, item_id: String, version: u64) -> R<a::ItemDetail> { s.restore_item_version(&vault_id, &item_id, version).map_err(e) }
#[tauri::command(async)] pub fn add_attachment(s: State<'_, App>, vault_id: String, item_id: String, path: String) -> R<a::Attachment> { s.add_attachment(&vault_id, &item_id, &path).map_err(e) }
#[tauri::command(async)] pub fn save_attachment(s: State<'_, App>, vault_id: String, item_id: String, attachment_id: String, dest_path: String) -> R<()> { s.save_attachment(&vault_id, &item_id, &attachment_id, &dest_path).map_err(e) }
#[tauri::command(async)] pub fn remove_attachment(s: State<'_, App>, vault_id: String, item_id: String, attachment_id: String) -> R<()> { s.remove_attachment(&vault_id, &item_id, &attachment_id).map_err(e) }
#[tauri::command] pub fn list_tags(s: State<App>) -> R<Vec<String>> { s.list_tags().map_err(e) }
#[tauri::command] pub fn rename_tag(s: State<App>, old: String, new: String) -> R<a::BulkResult> { s.rename_tag(&old, &new).map_err(e) }
#[tauri::command] pub fn delete_tag(s: State<App>, name: String) -> R<a::BulkResult> { s.delete_tag(&name).map_err(e) }
#[allow(clippy::too_many_arguments)] // Tauri command args map 1:1 to the JS invoke payload.
#[tauri::command] pub fn generate_password(s: State<App>, length: usize, symbols: bool, digits: bool, words: Option<bool>, pin: Option<bool>, separator: Option<char>, capitalize: Option<bool>) -> R<String> {
    use gitpass_core::generate::{Mode, Request};
    let mode = if pin.unwrap_or(false) { Mode::Pin } else if words.unwrap_or(false) { Mode::Words } else { Mode::Random };
    let pw = a::generate(&Request { mode, length: Some(length), symbols, digits, separator, capitalize: capitalize.unwrap_or(false) }).map_err(e)?;
    let _ = s.record_generated_password(&pw, "desktop", None);
    Ok(pw)
}
#[tauri::command] pub fn get_generator_history(s: State<App>) -> R<Vec<a::GeneratorHistoryEntryDto>> { s.get_generator_history().map_err(e) }
#[tauri::command] pub fn delete_generator_history_entry(s: State<App>, id: String) -> R<bool> { s.delete_generator_history_entry(&id).map_err(e) }
#[tauri::command] pub fn clear_generator_history(s: State<App>) -> R<()> { s.clear_generator_history().map_err(e) }
#[tauri::command] pub fn record_generated_password(s: State<App>, password: String, source: String, host: Option<String>) -> R<Option<a::GeneratorHistoryEntryDto>> { s.record_generated_password(&password, &source, host).map_err(e) }
#[tauri::command] pub fn copy_password(s: State<App>, password: String) -> R<()> { s.copy_password(&password).map_err(e) }
#[tauri::command] pub fn password_strength(s: State<App>, password: String) -> u8 { s.password_strength(&Zeroizing::new(password)) }
#[tauri::command(async)] pub fn watchtower(s: State<'_, App>, hibp: bool) -> R<a::WatchtowerReport> { s.watchtower(hibp).map_err(e) }
#[tauri::command(async)] pub fn import_file(s: State<'_, App>, vault_id: String, format: String, path: String, dry_run: bool) -> R<a::ImportResult> { s.import_file(&vault_id, &format, &path, dry_run).map_err(e) }
#[tauri::command(async)] pub fn export_items(s: State<'_, App>, req: a::ExportRequest) -> R<a::ExportResult> { s.export_items(&req).map_err(e) }
#[tauri::command] pub fn identity(s: State<App>) -> R<a::Identity> { s.identity().map_err(e) }
/// Fingerprint of a pasted recipient key, shown in the Share dialog for out-of-band verification.
#[tauri::command] pub fn fingerprint_of(s: State<App>, public_key: String) -> R<String> { s.fingerprint_of(&public_key).map_err(e) }
#[tauri::command] pub fn list_members(s: State<App>, vault_id: String) -> R<Vec<a::Member>> { s.list_members(&vault_id).map_err(e) }
#[tauri::command] pub fn share_vault(s: State<App>, vault_id: String, public_key: String, label: Option<String>) -> R<a::Member> { s.share_vault(&vault_id, &public_key, label).map_err(e) }
#[tauri::command] pub fn unshare_vault(s: State<App>, vault_id: String, fingerprint: String) -> R<()> { s.unshare_vault(&vault_id, &fingerprint).map_err(e) }
#[tauri::command] pub fn accept_share(s: State<App>, path: String) -> R<a::VaultInfo> { s.accept_share(&path).map_err(e) }
#[tauri::command] pub fn list_devices(s: State<App>) -> R<Vec<a::DeviceInfo>> { s.list_devices().map_err(e) }
#[tauri::command(async)] pub fn revoke_device(s: State<'_, App>, id: String, rotate_credentials: bool) -> R<()> { s.revoke_device(&id, rotate_credentials).map_err(e) }
#[tauri::command] pub fn get_settings(s: State<App>) -> a::Settings { s.get_settings() }
/// Apply content protection to every open window (main, Quick Access, any secondary window).
/// Best-effort per window: a window that cannot be protected must not fail the settings save.
/// On macOS this sets NSWindow.sharingType = .none, so screenshots and screen shares show a
/// blank window; other platforms report the call as unsupported and are ignored here.
pub fn apply_content_protection(app: &tauri::AppHandle, on: bool) {
    use tauri::Manager;
    for w in app.webview_windows().values() { let _ = w.set_content_protected(on); }
}
#[tauri::command(async)] pub fn set_settings(app: tauri::AppHandle, s: State<'_, App>, settings: a::Settings) -> R<a::Settings> {
    let on = settings.hide_from_screen_recording;
    let saved = s.set_settings(settings).map_err(e)?;
    apply_content_protection(&app, on);
    crate::tray::update_tray(&app);
    Ok(saved)
}
#[tauri::command(async)] pub fn change_password(s: State<'_, App>, current: String, next: String) -> R<()> {
    let (c, n) = (Zeroizing::new(current), Zeroizing::new(next));
    s.change_password(&c, &n).map_err(e)
}
#[tauri::command(async)] pub fn rotate_secret_key(s: State<'_, App>) -> R<a::RotateResult> { s.rotate_secret_key().map_err(e) }
#[tauri::command(async)] pub fn sync_status(s: State<'_, App>) -> R<a::SyncStatus> { s.sync_status().map_err(e) }
#[tauri::command(async)] pub fn remote_connect(s: State<'_, App>, url: String) -> R<a::SyncStatus> { s.remote_connect(&url).map_err(e) }
/// Open an https URL in the default browser. Only HTTPS is allowed; callers pass URLs derived in
/// Rust (never raw webview input) and the argument is never routed through a shell.
fn open_external(url: &str) -> R<()> {
    if !url.starts_with("https://") { return Err("only https URLs can be opened".into()); }
    #[cfg(target_os = "macos")] let r = std::process::Command::new("/usr/bin/open").arg(url).status();
    #[cfg(target_os = "windows")] let r = std::process::Command::new("explorer").arg(url).status();
    #[cfg(all(unix, not(target_os = "macos")))] let r = std::process::Command::new("xdg-open").arg(url).status();
    r.map(|_| ()).map_err(|x| x.to_string())
}
/// Open https://github.com/new (fixed URL; no user input reaches the shell) in the default browser.
#[tauri::command] pub fn open_new_repo_page() -> R<()> { open_external("https://github.com/new") }
/// Open the site's standard change-password page for an item. The URL is derived in Rust from the
/// item's saved URL, so the webview never supplies a URL to the OS.
#[tauri::command] pub fn open_change_password(s: State<App>, vault_id: String, item_id: String) -> R<()> {
    match s.item_change_password_url(&vault_id, &item_id).map_err(e)? {
        Some(url) => open_external(&url),
        None => Err("this item has no change-password URL".into()),
    }
}
/// Open the passkey's site (`https://<rp_id>`, derived in Rust from the item) in the default browser.
#[tauri::command] pub fn open_passkey_site(s: State<App>, vault_id: String, item_id: String) -> R<()> {
    match s.item_passkey_site_url(&vault_id, &item_id).map_err(e)? {
        Some(url) => open_external(&url),
        None => Err("this item has no passkey site".into()),
    }
}
#[tauri::command(async)] pub fn remote_disconnect(s: State<'_, App>) -> R<a::SyncStatus> { s.remote_disconnect().map_err(e) }
#[tauri::command(async)] pub fn sync(s: State<'_, App>, merge: bool) -> R<a::SyncResult> { s.sync(merge).map_err(e) }
#[tauri::command] pub fn onboarding(s: State<App>) -> a::Onboarding { s.onboarding() }
#[tauri::command] pub fn set_onboarding(s: State<App>, patch: a::OnboardingPatch) -> R<a::Onboarding> { s.set_onboarding(patch).map_err(e) }
#[tauri::command] pub fn emergency_kit(s: State<App>) -> R<String> { s.emergency_kit().map_err(e) }
/// Structured kit for the printable page; pass the master password to reveal the Secret Key.
#[tauri::command(async)] pub fn emergency_kit_data(s: State<'_, App>, password: Option<String>) -> R<a::EmergencyKitData> {
    let pw = password.map(Zeroizing::new);
    s.emergency_kit_data(pw.as_deref().map(|x| x.as_str())).map_err(e)
}
/// Favicon URL (null when showWebsiteIcons setting is off or fetch fails).
#[tauri::command(async)] pub fn favicon_url(s: State<'_, App>, url: String) -> Option<String> { s.favicon_url(&url) }
/// Not in the contract: resets the idle timer (harmless extra; the contract UI may ignore it).
#[tauri::command] pub fn touch(s: State<App>) { s.state.touch() }

#[cfg(target_os = "macos")]
fn is_screen_locked() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGSessionCopyCurrentDictionary() -> *const std::ffi::c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDictionaryGetValue(dict: *const std::ffi::c_void, key: *const std::ffi::c_void) -> *const std::ffi::c_void;
        fn CFStringCreateWithCString(alloc: *const std::ffi::c_void, c_str: *const std::ffi::c_char, encoding: u32) -> *const std::ffi::c_void;
        fn CFBooleanGetValue(boolean: *const std::ffi::c_void) -> bool;
        fn CFRelease(obj: *const std::ffi::c_void);
    }
    unsafe {
        let dict = CGSessionCopyCurrentDictionary();
        if dict.is_null() { return false; }
        let key = CFStringCreateWithCString(std::ptr::null(), c"CGSSessionScreenIsLocked".as_ptr(), 0x08000100 /* kCFStringEncodingUTF8 */);
        let mut locked = false;
        if !key.is_null() {
            let val = CFDictionaryGetValue(dict, key);
            if !val.is_null() {
                locked = CFBooleanGetValue(val);
            }
            CFRelease(key);
        }
        CFRelease(dict);
        locked
    }
}

#[cfg(not(target_os = "macos"))]
fn is_screen_locked() -> bool { false }

/// Background lock + change watcher (300 ms tick): emits `vault-changed` when the vault is written by anyone,
/// `unlocked` on a locked -> unlocked transition, and `locked` (payload = reason) on EVERY unlocked -> locked transition so the
/// UI returns to the unlock screen, whatever caused it: idle auto-lock (this loop or the lazy expiry inside a
/// command), manual lock, screen lock, or the extension/agent `lock` command. It also locks after system sleep: the loop
/// notices a wall-clock gap (macOS `Instant` does not advance while asleep, so the idle timer alone would not).
pub fn spawn_idle_watcher(app: AppHandle) {
    const TICK: std::time::Duration = std::time::Duration::from_millis(300);
    const SLEEP_GAP: std::time::Duration = std::time::Duration::from_secs(30);
    std::thread::spawn(move || {
        let mut was_unlocked = app.state::<App>().state.is_unlocked();
        let mut last_wall = std::time::SystemTime::now();
        let mut last_stamp = app.state::<App>().state.change_stamp();
        loop {
            std::thread::sleep(TICK);
            let st = app.state::<App>();
            let now = std::time::SystemTime::now();
            let slept = now.duration_since(last_wall).map(|d| d > SLEEP_GAP).unwrap_or(false);
            last_wall = now;
            let screen_locked = is_screen_locked();
            let mut reason = "lock";
            let mut locked_by_system = false;
            if slept {
                locked_by_system = crate::system_events::on_system_lock_event(&app, gitpass_gui_core::api::LockTrigger::Sleep);
                if locked_by_system { reason = "sleep"; }
            }
            if !locked_by_system && screen_locked {
                locked_by_system = crate::system_events::on_system_lock_event(&app, gitpass_gui_core::api::LockTrigger::ScreenLock);
                if locked_by_system { reason = "screen"; }
            }
            if !locked_by_system && st.state.check_idle() {
                reason = "idle";
            }
            let unlocked = st.state.is_unlocked();
            if was_unlocked && !unlocked && !locked_by_system {
                let _ = app.emit("locked", reason);
                crate::tray::update_tray(&app);
            }
            // Unlocked elsewhere (extension Touch ID / request_unlock): the UI refreshes in place.
            if !was_unlocked && unlocked {
                let _ = app.emit("unlocked", "watcher");
                crate::tray::update_tray(&app);
            }
            // Live sync: any vault write (this window, the extension via the agent, CLI, git sync) changes the
            // sealed manifest; the UI refetches the item list (src/lib/agentEvents.ts `vault-changed`).
            let stamp = st.state.change_stamp();
            if stamp != last_stamp { if stamp.is_some() && last_stamp.is_some() { let _ = app.emit("vault-changed", ()); } last_stamp = stamp; }
            was_unlocked = unlocked;
        }
    });
}

/// Show + focus the (pre-declared, hidden) quick-access window.
#[tauri::command] pub fn show_quick_access(app: tauri::AppHandle) -> R<()> {
    use tauri::Manager;
    let w = app.get_webview_window("quick-access").ok_or_else(|| "quick-access window missing".to_string())?;
    w.show().map_err(|x| x.to_string())?; w.set_focus().map_err(|x| x.to_string())
}

/// Toggle visibility and focus of the quick-access window.
/// If currently visible and focused, hides it. Otherwise, shows and focuses it.
pub fn toggle_quick_access(app: &tauri::AppHandle) -> R<()> {
    use tauri::Manager;
    let w = app.get_webview_window("quick-access").ok_or_else(|| "quick-access window missing".to_string())?;
    let visible = w.is_visible().unwrap_or(false);
    let focused = w.is_focused().unwrap_or(false);
    if visible && focused {
        w.hide().map_err(|x| x.to_string())?;
    } else {
        w.show().map_err(|x| x.to_string())?;
        w.set_focus().map_err(|x| x.to_string())?;
    }
    Ok(())
}

/// Build the native application menu including a menu-bar item for Quick Access.
pub fn setup_menu(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
    let handle = app.handle();
    let qa_item = MenuItem::with_id(handle, "quick_access", "Quick Access", true, None::<&str>)?;

    let file_menu = Submenu::with_items(
        handle,
        "File",
        true,
        &[
            &qa_item,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::close_window(handle, None)?,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        handle,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(handle, None)?,
            &PredefinedMenuItem::redo(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::cut(handle, None)?,
            &PredefinedMenuItem::copy(handle, None)?,
            &PredefinedMenuItem::paste(handle, None)?,
            &PredefinedMenuItem::select_all(handle, None)?,
        ],
    )?;

    let window_menu = Submenu::with_items(
        handle,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(handle, None)?,
            &PredefinedMenuItem::maximize(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::close_window(handle, None)?,
        ],
    )?;

    #[cfg(target_os = "macos")]
    let app_menu = Submenu::with_items(
        handle,
        "gitpass",
        true,
        &[
            &PredefinedMenuItem::about(handle, None, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::services(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::hide(handle, None)?,
            &PredefinedMenuItem::hide_others(handle, None)?,
            &PredefinedMenuItem::show_all(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::quit(handle, None)?,
        ],
    )?;

    #[cfg(target_os = "macos")]
    let menu = Menu::with_items(handle, &[&app_menu, &file_menu, &edit_menu, &window_menu])?;
    #[cfg(not(target_os = "macos"))]
    let menu = Menu::with_items(handle, &[&file_menu, &edit_menu, &window_menu])?;

    app.set_menu(menu)?;

    app.on_menu_event(move |app_handle, event| {
        if event.id() == "quick_access" {
            let _ = show_quick_access(app_handle.clone());
        }
    });

    Ok(())
}

// ---- v0.7 SSH keys
#[tauri::command] pub fn ssh_generate(s: State<App>, vault_id: String, title: String, comment: Option<String>) -> R<a::SshKeyCreated> {
    let c = comment.filter(|c| !c.trim().is_empty()).unwrap_or_else(|| title.clone());
    s.ssh_generate(&vault_id, &title, &c).map_err(e)
}
#[tauri::command] pub fn ssh_import(s: State<App>, vault_id: String, title: String, private_key: String, passphrase: Option<String>) -> R<a::SshKeyCreated> {
    let pem = Zeroizing::new(private_key);
    let pp = passphrase.map(Zeroizing::new);
    s.ssh_import(&vault_id, &title, &pem, pp.as_deref().map(|x| x.as_str())).map_err(e)
}
#[tauri::command] pub fn ssh_agent_info(h: AppHandle, s: State<App>) -> a::SshAgentInfo {
    let running = h.try_state::<std::sync::Arc<gitpass_gui_core::ssh_agent::SshAgent>>();
    let path = running.as_ref().map(|a| a.socket_path.clone()).unwrap_or_else(|| gitpass_gui_core::agent::default_dir().join(gitpass_gui_core::ssh_agent::SOCKET_NAME));
    s.ssh_agent_info(&path, running.is_some())
}
#[tauri::command] pub fn ssh_git_signing_config(h: AppHandle, s: State<App>, vault_id: String, item_id: String, email: Option<String>) -> R<gitpass_core::ssh::GitSigningConfig> {
    let running = h.try_state::<std::sync::Arc<gitpass_gui_core::ssh_agent::SshAgent>>();
    let path = running.as_ref().map(|a| a.socket_path.clone()).unwrap_or_else(|| gitpass_gui_core::agent::default_dir().join(gitpass_gui_core::ssh_agent::SOCKET_NAME));
    s.ssh_git_signing_config(&vault_id, &item_id, email.as_deref(), Some(&path.to_string_lossy())).map_err(e)
}
#[tauri::command] pub fn ssh_apply_git_signing(h: AppHandle, s: State<App>, vault_id: String, item_id: String, email: Option<String>, allowed_signers_file: Option<String>, global: Option<bool>) -> R<gitpass_core::ssh::GitSigningConfig> {
    let running = h.try_state::<std::sync::Arc<gitpass_gui_core::ssh_agent::SshAgent>>();
    let path = running.as_ref().map(|a| a.socket_path.clone()).unwrap_or_else(|| gitpass_gui_core::agent::default_dir().join(gitpass_gui_core::ssh_agent::SOCKET_NAME));
    s.ssh_apply_git_signing(&vault_id, &item_id, email.as_deref(), allowed_signers_file.as_deref(), Some(&path.to_string_lossy()), global.unwrap_or(true)).map_err(e)
}
