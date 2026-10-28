//! System event observers for auto-lock on Mac sleep, screen lock, and user switch.
use crate::commands::App;
use gitpass_gui_core::api::LockTrigger;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Dispatches a system lock event. Returns true if the vault locked as a result.
pub fn on_system_lock_event<R: Runtime>(app: &AppHandle<R>, trigger: LockTrigger) -> bool {
    let st = app.state::<App>();
    let reason = match trigger {
        LockTrigger::Sleep => "sleep",
        LockTrigger::ScreenLock => "screen",
        LockTrigger::UserSwitch => "user_switch",
    };
    if st.handle_system_lock_event(trigger) {
        let _ = app.emit("locked", reason);
        true
    } else {
        false
    }
}

#[cfg(target_os = "macos")]
pub fn start<R: Runtime>(app: AppHandle<R>) {
    macos::register_observers(app);
}

#[cfg(not(target_os = "macos"))]
pub fn start<R: Runtime>(_app: AppHandle<R>) {}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::NSString;
    use std::sync::Mutex;

    static OBSERVERS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

    pub fn register_observers<R: Runtime>(app: AppHandle<R>) {
        unsafe {
            // 1. NSWorkspace notifications: willSleep & sessionDidResignActive
            if let Some(cls_ws) = AnyClass::get(c"NSWorkspace") {
                let ws: *mut AnyObject = msg_send![cls_ws, sharedWorkspace];
                if !ws.is_null() {
                    let nc: *mut AnyObject = msg_send![ws, notificationCenter];
                    if !nc.is_null() {
                        // Will Sleep
                        let app_sleep = app.clone();
                        let sleep_name = NSString::from_str("NSWorkspaceWillSleepNotification");
                        let sleep_block = RcBlock::new(move |_note: *mut AnyObject| {
                            on_system_lock_event(&app_sleep, LockTrigger::Sleep);
                        });
                        let token: *mut AnyObject = msg_send![
                            nc,
                            addObserverForName: &*sleep_name,
                            object: std::ptr::null::<AnyObject>(),
                            queue: std::ptr::null::<AnyObject>(),
                            usingBlock: &*sleep_block
                        ];
                        if !token.is_null() {
                            OBSERVERS.lock().unwrap().push(token as usize);
                        }

                        // Fast User Switching: sessionDidResignActive
                        let app_switch = app.clone();
                        let switch_name = NSString::from_str("NSWorkspaceSessionDidResignActiveNotification");
                        let switch_block = RcBlock::new(move |_note: *mut AnyObject| {
                            on_system_lock_event(&app_switch, LockTrigger::UserSwitch);
                        });
                        let token: *mut AnyObject = msg_send![
                            nc,
                            addObserverForName: &*switch_name,
                            object: std::ptr::null::<AnyObject>(),
                            queue: std::ptr::null::<AnyObject>(),
                            usingBlock: &*switch_block
                        ];
                        if !token.is_null() {
                            OBSERVERS.lock().unwrap().push(token as usize);
                        }
                    }
                }
            }

            // 2. Distributed notification: com.apple.screenIsLocked
            if let Some(cls_dnc) = AnyClass::get(c"NSDistributedNotificationCenter") {
                let dnc: *mut AnyObject = msg_send![cls_dnc, defaultCenter];
                if !dnc.is_null() {
                    let app_screen = app.clone();
                    let screen_name = NSString::from_str("com.apple.screenIsLocked");
                    let screen_block = RcBlock::new(move |_note: *mut AnyObject| {
                        on_system_lock_event(&app_screen, LockTrigger::ScreenLock);
                    });
                    let token: *mut AnyObject = msg_send![
                        dnc,
                        addObserverForName: &*screen_name,
                        object: std::ptr::null::<AnyObject>(),
                        queue: std::ptr::null::<AnyObject>(),
                        usingBlock: &*screen_block
                    ];
                    if !token.is_null() {
                        OBSERVERS.lock().unwrap().push(token as usize);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    fn temp_test_dir() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("gitpass-sys-test-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn test_lock_dispatch_with_mock_os_notifications() {
        let root = temp_test_dir();
        let app_core = Arc::new(gitpass_gui_core::api::App::new(root.clone(), Arc::new(gitpass_gui_core::api::MemClipboard::default())));
        let res = app_core.create_account(&root.join("v").display().to_string(), "correct horse", false).unwrap();
        assert!(app_core.status().unwrap().unlocked);

        let app = mock_builder()
            .manage(app_core.clone())
            .build(mock_context(noop_assets()))
            .unwrap();

        let handle = app.handle();

        // 1. Mock sleep notification when enabled
        assert!(on_system_lock_event(handle, LockTrigger::Sleep));
        assert!(!app_core.status().unwrap().unlocked);

        // Re-unlock
        app_core.unlock(&root.join("v").display().to_string(), "correct horse", Some(&res.secret_key)).unwrap();
        assert!(app_core.status().unwrap().unlocked);

        // Disable sleep lock in settings
        let mut s = app_core.get_settings();
        s.lock_on_sleep = false;
        app_core.set_settings(s).unwrap();

        // Mock sleep notification when disabled -> skipped!
        assert!(!on_system_lock_event(handle, LockTrigger::Sleep));
        assert!(app_core.status().unwrap().unlocked);

        // 2. Mock screen lock notification
        assert!(on_system_lock_event(handle, LockTrigger::ScreenLock));
        assert!(!app_core.status().unwrap().unlocked);

        app_core.unlock(&root.join("v").display().to_string(), "correct horse", Some(&res.secret_key)).unwrap();
        let mut s = app_core.get_settings();
        s.lock_on_screen_lock = false;
        app_core.set_settings(s).unwrap();
        assert!(!on_system_lock_event(handle, LockTrigger::ScreenLock));
        assert!(app_core.status().unwrap().unlocked);

        // 3. Mock user switch notification
        assert!(on_system_lock_event(handle, LockTrigger::UserSwitch));
        assert!(!app_core.status().unwrap().unlocked);

        app_core.unlock(&root.join("v").display().to_string(), "correct horse", Some(&res.secret_key)).unwrap();
        let mut s = app_core.get_settings();
        s.lock_on_user_switch = false;
        app_core.set_settings(s).unwrap();
        assert!(!on_system_lock_event(handle, LockTrigger::UserSwitch));
        assert!(app_core.status().unwrap().unlocked);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_macos_observers_registration() {
        let root = temp_test_dir();
        let app_core = Arc::new(gitpass_gui_core::api::App::new(root.clone(), Arc::new(gitpass_gui_core::api::MemClipboard::default())));
        let app = mock_builder()
            .manage(app_core)
            .build(mock_context(noop_assets()))
            .unwrap();
        start(app.handle().clone());
        let _ = std::fs::remove_dir_all(&root);
    }
}
