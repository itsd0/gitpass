use std::str::FromStr;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Debug, PartialEq, Eq)]
pub enum HotkeyError {
    Empty,
    NoModifiers,
    ParseError(String),
}

impl std::fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HotkeyError::Empty => write!(f, "Shortcut cannot be empty"),
            HotkeyError::NoModifiers => {
                write!(f, "Shortcut must include at least one modifier (e.g. Command, Shift, Alt, Ctrl)")
            }
            HotkeyError::ParseError(msg) => write!(f, "Parse error: {msg}"),
        }
    }
}

impl std::error::Error for HotkeyError {}

/// Parse and validate a global shortcut string (e.g. "CommandOrControl+Shift+Space").
pub fn parse_and_validate_hotkey(s: &str) -> Result<Shortcut, HotkeyError> {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") || trimmed.eq_ignore_ascii_case("disabled") {
        return Err(HotkeyError::Empty);
    }
    let shortcut = Shortcut::from_str(trimmed).map_err(|e| HotkeyError::ParseError(e.to_string()))?;
    if shortcut.mods.is_empty() {
        return Err(HotkeyError::NoModifiers);
    }
    Ok(shortcut)
}

/// Register the global quick access hotkey from user settings at app startup.
pub fn register_startup_hotkey(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<Arc<gitpass_gui_core::api::App>>() {
        let s = state.get_settings();
        let hotkey = s.quick_access_hotkey.trim();
        if let Ok(shortcut) = parse_and_validate_hotkey(hotkey) {
            if let Err(e) = app.global_shortcut().register(shortcut) {
                eprintln!("Warning: failed to register global hotkey '{hotkey}': {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_validate_valid_hotkeys() {
        assert!(parse_and_validate_hotkey("CommandOrControl+Shift+Space").is_ok());
        assert!(parse_and_validate_hotkey("CmdOrCtrl+Shift+Space").is_ok());
        assert!(parse_and_validate_hotkey("Ctrl+Alt+K").is_ok());
        assert!(parse_and_validate_hotkey("Shift+Alt+KeyQ").is_ok());
        assert!(parse_and_validate_hotkey("Command+Shift+1").is_ok());
    }

    #[test]
    fn test_parse_and_validate_invalid_hotkeys() {
        // Empty or disabled tokens
        assert_eq!(parse_and_validate_hotkey(""), Err(HotkeyError::Empty));
        assert_eq!(parse_and_validate_hotkey("   "), Err(HotkeyError::Empty));
        assert_eq!(parse_and_validate_hotkey("none"), Err(HotkeyError::Empty));
        assert_eq!(parse_and_validate_hotkey("disabled"), Err(HotkeyError::Empty));

        // Missing modifier (Space only)
        assert_eq!(parse_and_validate_hotkey("Space"), Err(HotkeyError::NoModifiers));
        assert_eq!(parse_and_validate_hotkey("KeyA"), Err(HotkeyError::NoModifiers));

        // Missing key
        assert!(matches!(parse_and_validate_hotkey("Ctrl+Shift"), Err(HotkeyError::ParseError(_))));

        // Unknown key token
        assert!(matches!(parse_and_validate_hotkey("Ctrl+NonexistentKey123"), Err(HotkeyError::ParseError(_))));
    }
}
