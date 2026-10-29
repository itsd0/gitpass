use std::sync::Arc;
use tauri::Manager;

pub const TRAY_ID: &str = "main-tray";

pub const TRAY_LOCKED_BYTES: &[u8] = include_bytes!("../icons/tray-locked.png");
pub const TRAY_UNLOCKED_BYTES: &[u8] = include_bytes!("../icons/tray-unlocked.png");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayLabels {
    pub open_gitpass: &'static str,
    pub quick_access: &'static str,
    pub generate_password: &'static str,
    pub lock_now: &'static str,
    pub unlock: &'static str,
    pub settings: &'static str,
    pub quit: &'static str,
}

pub fn labels_for_lang(lang: &gitpass_gui_core::api::Language) -> TrayLabels {
    match lang {
        gitpass_gui_core::api::Language::En => TrayLabels {
            open_gitpass: "Open gitpass",
            quick_access: "Quick Access…",
            generate_password: "Generate password…",
            lock_now: "Lock now",
            unlock: "Unlock…",
            settings: "Settings…",
            quit: "Quit gitpass",
        },
        gitpass_gui_core::api::Language::My => TrayLabels {
            open_gitpass: "gitpass ကို ဖွင့်ရန်",
            quick_access: "အမြန်သုံးရန်…",
            generate_password: "စကားဝှက် ထုတ်ယူရန်…",
            lock_now: "ယခု သော့ခတ်ရန်",
            unlock: "သော့ဖွင့်ရန်…",
            settings: "ဆက်တင်များ…",
            quit: "gitpass မှ ထွက်ရန်",
        },
        gitpass_gui_core::api::Language::Th => TrayLabels {
            open_gitpass: "เปิด gitpass",
            quick_access: "การเข้าถึงด่วน…",
            generate_password: "สร้างรหัสผ่าน…",
            lock_now: "ล็อกเดี๋ยวนี้",
            unlock: "ปลดล็อก…",
            settings: "การตั้งค่า…",
            quit: "ออกจาก gitpass",
        },
    }
}

/// Convert a hotkey string (e.g. "CommandOrControl+Shift+Space") to symbols for menu item display.
pub fn format_hotkey_display(hotkey: &str) -> String {
    let s = hotkey.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("disabled") {
        return String::new();
    }
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    let mut out = String::new();
    for p in parts {
        match p.to_lowercase().as_str() {
            "commandorcontrol" | "cmdorctrl" | "command" | "cmd" | "super" => out.push('⌘'),
            "shift" => out.push('⇧'),
            "alt" | "option" => out.push('⌥'),
            "control" | "ctrl" => out.push('⌃'),
            other => {
                let key_name = match other {
                    "space" => "Space".to_string(),
                    k if k.starts_with("key") && k.len() > 3 => k[3..].to_ascii_uppercase(),
                    k if k.len() == 1 => k.to_ascii_uppercase(),
                    k => k.to_string(),
                };
                out.push_str(&key_name);
            }
        }
    }
    out
}

pub fn get_tray_icon_image(locked: bool) -> tauri::image::Image<'static> {
    let bytes = if locked { TRAY_LOCKED_BYTES } else { TRAY_UNLOCKED_BYTES };
    tauri::image::Image::from_bytes(bytes).expect("valid tray icon png bytes")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuState {
    pub locked: bool,
    pub open_label: &'static str,
    pub open_enabled: bool,
    pub quick_access_title: String,
    pub quick_access_enabled: bool,
    pub generate_label: &'static str,
    pub generate_enabled: bool,
    pub lock_now_label: &'static str,
    pub lock_now_enabled: bool,
    pub unlock_label: &'static str,
    pub unlock_enabled: bool,
    pub settings_label: &'static str,
    pub settings_enabled: bool,
    pub quit_label: &'static str,
    pub quit_enabled: bool,
}

pub fn compute_tray_menu_state(locked: bool, labels: &TrayLabels, quick_access_title: &str) -> TrayMenuState {
    TrayMenuState {
        locked,
        open_label: labels.open_gitpass,
        open_enabled: true,
        quick_access_title: quick_access_title.to_string(),
        quick_access_enabled: true,
        generate_label: labels.generate_password,
        generate_enabled: true,
        lock_now_label: labels.lock_now,
        lock_now_enabled: !locked,
        unlock_label: labels.unlock,
        unlock_enabled: locked,
        settings_label: labels.settings,
        settings_enabled: true,
        quit_label: labels.quit,
        quit_enabled: true,
    }
}

pub fn build_tray_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &TrayMenuState,
) -> Result<tauri::menu::Menu<R>, Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    let open_item = MenuItem::with_id(app, "tray_open", state.open_label, state.open_enabled, None::<&str>)?;
    let qa_item = MenuItem::with_id(app, "tray_quick_access", &state.quick_access_title, state.quick_access_enabled, None::<&str>)?;
    let gen_item = MenuItem::with_id(app, "tray_generate", state.generate_label, state.generate_enabled, None::<&str>)?;
    let lock_item = MenuItem::with_id(app, "tray_lock", state.lock_now_label, state.lock_now_enabled, None::<&str>)?;
    let unlock_item = MenuItem::with_id(app, "tray_unlock", state.unlock_label, state.unlock_enabled, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let settings_item = MenuItem::with_id(app, "tray_settings", state.settings_label, state.settings_enabled, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "tray_quit", state.quit_label, state.quit_enabled, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open_item,
            &qa_item,
            &gen_item,
            &lock_item,
            &unlock_item,
            &sep,
            &settings_item,
            &quit_item,
        ],
    )?;
    Ok(menu)
}

/// Handle a click on a tray menu item.
pub fn handle_tray_menu_event<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: &str) {
    use tauri::Emitter;
    match id {
        "tray_open" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }
        "tray_quick_access" => {
            if let Some(w) = app.get_webview_window("quick-access") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "tray_generate" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
                let _ = w.emit("menu-action", "open-generator");
            }
        }
        "tray_lock" => {
            if let Some(state) = app.try_state::<Arc<gitpass_gui_core::api::App>>() {
                state.lock();
                let _ = app.emit("locked", "tray");
                update_tray(app);
            }
        }
        "tray_unlock" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }
        "tray_settings" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
                let _ = w.emit("menu-action", "open-settings");
            }
        }
        "tray_quit" => {
            app.exit(0);
        }
        _ => {}
    }
}

/// Setup the tray icon during app initialization.
pub fn setup<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    update_tray(app);
    Ok(())
}

/// Live update for the menu bar icon: updates icon, menu items, and tooltip, or removes if disabled.
pub fn update_tray<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let app_state = match app.try_state::<Arc<gitpass_gui_core::api::App>>() {
        Some(s) => s,
        None => return,
    };
    let settings = app_state.get_settings();
    if !settings.show_menu_bar_icon {
        let _ = app.remove_tray_by_id(&TRAY_ID.to_string());
        return;
    }

    let locked = !app_state.status().map(|st| st.unlocked).unwrap_or(false);
    let labels = labels_for_lang(&settings.language);
    let hotkey_disp = format_hotkey_display(&settings.quick_access_hotkey);
    let qa_title = if hotkey_disp.is_empty() {
        labels.quick_access.to_string()
    } else {
        format!("{} ({})", labels.quick_access, hotkey_disp)
    };

    let menu_state = compute_tray_menu_state(locked, &labels, &qa_title);
    let menu = match build_tray_menu(app, &menu_state) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Warning: failed to build tray menu: {e}");
            return;
        }
    };

    let icon = get_tray_icon_image(locked);
    let tooltip = if locked { "gitpass (locked)" } else { "gitpass" };

    if let Some(tray) = app.tray_by_id(&TRAY_ID.to_string()) {
        let _ = tray.set_icon(Some(icon));
        let _ = tray.set_icon_as_template(true);
        let _ = tray.set_menu(Some(menu));
        let _ = tray.set_tooltip(Some(tooltip));
    } else {
        use tauri::tray::TrayIconBuilder;
        let builder = TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .icon_as_template(true)
            .tooltip(tooltip)
            .show_menu_on_left_click(true)
            .menu(&menu)
            .on_menu_event(|app_handle, event| {
                handle_tray_menu_event(app_handle, event.id.as_ref());
            });

        if let Err(e) = builder.build(app) {
            eprintln!("Warning: failed to build tray icon: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitpass_gui_core::api::Language;

    #[test]
    fn test_labels_for_all_languages() {
        for lang in [Language::En, Language::My, Language::Th] {
            let l = labels_for_lang(&lang);
            assert!(!l.open_gitpass.is_empty());
            assert!(!l.quick_access.is_empty());
            assert!(!l.generate_password.is_empty());
            assert!(!l.lock_now.is_empty());
            assert!(!l.unlock.is_empty());
            assert!(!l.settings.is_empty());
            assert!(!l.quit.is_empty());
        }
    }

    #[test]
    fn test_format_hotkey_display() {
        assert_eq!(format_hotkey_display("CommandOrControl+Shift+Space"), "⌘⇧Space");
        assert_eq!(format_hotkey_display("CmdOrCtrl+Shift+Space"), "⌘⇧Space");
        assert_eq!(format_hotkey_display("Ctrl+Alt+k"), "⌃⌥K");
        assert_eq!(format_hotkey_display("Shift+Alt+KeyQ"), "⇧⌥Q");
        assert_eq!(format_hotkey_display(""), "");
        assert_eq!(format_hotkey_display("none"), "");
        assert_eq!(format_hotkey_display("disabled"), "");
    }

    #[test]
    fn test_tray_icons_valid_png() {
        let locked_img = get_tray_icon_image(true);
        assert_eq!(locked_img.width(), 32);
        assert_eq!(locked_img.height(), 32);

        let unlocked_img = get_tray_icon_image(false);
        assert_eq!(unlocked_img.width(), 32);
        assert_eq!(unlocked_img.height(), 32);

        // Bytes must differ between locked and unlocked
        assert_ne!(TRAY_LOCKED_BYTES, TRAY_UNLOCKED_BYTES);
    }

    #[test]
    fn test_menu_state_logic() {
        let labels = labels_for_lang(&Language::En);

        // When locked: lock_now is disabled, unlock is enabled
        let locked_state = compute_tray_menu_state(true, &labels, "Quick Access… (⌘⇧Space)");
        assert!(locked_state.locked);
        assert!(locked_state.open_enabled);
        assert!(locked_state.quick_access_enabled);
        assert_eq!(locked_state.quick_access_title, "Quick Access… (⌘⇧Space)");
        assert!(locked_state.generate_enabled);
        assert!(!locked_state.lock_now_enabled, "Lock now must be disabled when locked");
        assert!(locked_state.unlock_enabled, "Unlock must be enabled when locked");
        assert!(locked_state.settings_enabled);
        assert!(locked_state.quit_enabled);

        // When unlocked: lock_now is enabled, unlock is disabled
        let unlocked_state = compute_tray_menu_state(false, &labels, "Quick Access… (⌘⇧Space)");
        assert!(!unlocked_state.locked);
        assert!(unlocked_state.open_enabled);
        assert!(unlocked_state.quick_access_enabled);
        assert!(unlocked_state.generate_enabled);
        assert!(unlocked_state.lock_now_enabled, "Lock now must be enabled when unlocked");
        assert!(!unlocked_state.unlock_enabled, "Unlock must be disabled when unlocked");
        assert!(unlocked_state.settings_enabled);
        assert!(unlocked_state.quit_enabled);
    }
}
