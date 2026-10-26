#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod commands;
use commands as c;
use std::sync::Arc;

fn main() {
    let app = gitpass_gui_core::api::App::new(gitpass_gui_core::default_vault_dir(), Arc::new(c::SysClipboard));
    tauri::Builder::default()
        .manage(app)
        .setup(|h| { c::spawn_idle_watcher(h.handle().clone()); Ok(()) })
        .invoke_handler(tauri::generate_handler![
            c::status, c::show_quick_access, c::unlock, c::lock, c::list_vaults, c::create_vault, c::list_items, c::get_item, c::reveal_field,
            c::copy_field, c::totp_now, c::create_item, c::update_item, c::set_favorite, c::archive_item, c::delete_item,
            c::restore_item, c::purge_item, c::move_item, c::duplicate_item, c::item_version, c::add_attachment,
            c::save_attachment, c::list_tags, c::generate_password, c::password_strength, c::watchtower, c::import_file,
            c::identity, c::fingerprint_of, c::list_members, c::share_vault, c::unshare_vault, c::accept_share, c::list_devices,
            c::revoke_device, c::get_settings, c::set_settings, c::change_password, c::rotate_secret_key, c::sync_status,
            c::sync, c::onboarding, c::set_onboarding, c::emergency_kit, c::favicon_url, c::touch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running gitpass desktop");
}
