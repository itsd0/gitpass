#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod agent_ui;
mod biometric;
mod commands;
mod hotkey;
mod system_events;
mod tray;
#[cfg(test)]
mod updater_tests;
use commands as c;
use std::sync::Arc;
use tauri::Manager;

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
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let _ = c::toggle_quick_access(app);
                    }
                })
                .build(),
        )
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let handle = window.app_handle();
                    let show_tray = handle
                        .try_state::<Arc<gitpass_gui_core::api::App>>()
                        .map(|s| s.get_settings().show_menu_bar_icon)
                        .unwrap_or(true);
                    if show_tray {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            }
        })
        .manage(app)
        .setup(|h| {
            c::spawn_idle_watcher(h.handle().clone());
            system_events::start(h.handle().clone());
            agent_ui::start(h.handle());
            hotkey::register_startup_hotkey(h.handle());
            c::setup_menu(h)?;
            tray::setup(h.handle())?;
            // Windows are created content-protected by tauri.conf.json; re-apply the saved
            // setting at startup so a user who turned it off is not protected by default.
            {
                let handle = h.handle().clone();
                let on = handle.state::<c::App>().get_settings().hide_from_screen_recording;
                c::apply_content_protection(&handle, on);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            c::status, c::show_quick_access, c::unlock, c::lock, c::list_vaults, c::create_vault, c::update_vault, c::delete_vault, c::list_items, c::get_item, c::reveal_field,
            c::copy_field, c::totp_now, c::create_item, c::update_item, c::set_favorite, c::archive_item, c::delete_item,
            c::restore_item, c::purge_item, c::ignore_finding, c::unignore_finding, c::empty_trash, c::bulk_items, c::move_item, c::duplicate_item, c::item_version, c::restore_item_version, c::add_attachment,
            c::save_attachment, c::remove_attachment, c::list_tags, c::rename_tag, c::delete_tag, c::generate_password, c::copy_password, c::password_strength, c::watchtower, c::import_file, c::export_items,
            c::identity, c::fingerprint_of, c::list_members, c::share_vault, c::unshare_vault, c::accept_share, c::list_devices,
            c::revoke_device, c::get_settings, c::set_settings, c::change_password, c::rotate_secret_key, c::sync_status,
            c::sync, c::remote_connect, c::remote_disconnect, c::open_new_repo_page, c::open_change_password, c::onboarding, c::set_onboarding, c::emergency_kit, c::emergency_kit_data, c::favicon_url, c::touch, c::create_account, c::unlock_biometric, c::ssh_generate, c::ssh_import, c::ssh_agent_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running gitpass desktop");
}
