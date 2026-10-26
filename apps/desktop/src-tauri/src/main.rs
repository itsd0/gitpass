#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod agent_ui;
mod biometric;
mod commands;
use commands as c;
use std::sync::Arc;

/// Apps launched from Finder get a minimal PATH; add the usual git / gh / credential-helper locations so
/// `git push` can use the same credentials as the user's Terminal (SSH agent, osxkeychain, `gh auth git-credential`).
#[cfg(unix)]
fn extend_path() {
    let cur = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<String> = cur.split(':').filter(|s| !s.is_empty()).map(String::from).collect();
    for p in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] { if !parts.iter().any(|x| x == p) { parts.push(p.into()); } }
    std::env::set_var("PATH", parts.join(":"));
}

#[cfg(not(unix))]
fn extend_path() {}

fn main() {
    extend_path();
    let app = Arc::new(gitpass_gui_core::api::App::new(gitpass_gui_core::default_vault_dir(), Arc::new(c::SysClipboard)).with_biometric(biometric::platform()));
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(app)
        .setup(|h| { c::spawn_idle_watcher(h.handle().clone()); agent_ui::start(h.handle()); Ok(()) })
        .invoke_handler(tauri::generate_handler![
            c::status, c::show_quick_access, c::unlock, c::lock, c::list_vaults, c::create_vault, c::list_items, c::get_item, c::reveal_field,
            c::copy_field, c::totp_now, c::create_item, c::update_item, c::set_favorite, c::archive_item, c::delete_item,
            c::restore_item, c::purge_item, c::move_item, c::duplicate_item, c::item_version, c::add_attachment,
            c::save_attachment, c::list_tags, c::generate_password, c::password_strength, c::watchtower, c::import_file,
            c::identity, c::fingerprint_of, c::list_members, c::share_vault, c::unshare_vault, c::accept_share, c::list_devices,
            c::revoke_device, c::get_settings, c::set_settings, c::change_password, c::rotate_secret_key, c::sync_status,
            c::sync, c::remote_connect, c::remote_disconnect, c::open_new_repo_page, c::onboarding, c::set_onboarding, c::emergency_kit, c::emergency_kit_data, c::favicon_url, c::touch, c::create_account, c::unlock_biometric, c::ssh_generate, c::ssh_import, c::ssh_agent_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running gitpass desktop");
}
