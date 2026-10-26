//! Extension agent (v0.6): starts the UNIX-socket server from gitpass-gui-core and provides its UI
//! hooks: bring the window forward, Touch ID (or the password screen) for `request_unlock`, and
//! navigation for `open_in_app`.
use crate::commands::App;
use gitpass_gui_core::agent::{default_dir, Agent, AgentUi};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

struct TauriUi { h: AppHandle }

fn focus_main(h: &AppHandle) -> Option<tauri::WebviewWindow> {
    let w = h.get_webview_window("main")?;
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
    Some(w)
}

impl AgentUi for TauriUi {
    fn request_unlock(&self, reason: &str) {
        let h = self.h.clone();
        let reason = reason.to_string();
        let _ = self.h.run_on_main_thread(move || {
            let _ = focus_main(&h);
            let app = h.state::<App>().inner().clone();
            let bio = app.status().map(|s| s.biometric_available).unwrap_or(false);
            let _ = h.emit("agent-unlock-request", serde_json::json!({"reason": reason, "biometric": bio}));
            if bio {
                // Same Touch ID path as the unlock screen's button; on success reload the UI so it shows the vault.
                let h2 = h.clone();
                std::thread::spawn(move || match app.unlock_biometric("") {
                    Ok(_) => {
                        let _ = h2.emit("unlocked", "agent");
                        if let Some(w) = h2.get_webview_window("main") { let _ = w.eval("window.location.reload()"); }
                    }
                    // Touch ID cancelled/failed: the window is in front showing the password screen.
                    Err(e) => { let _ = h2.emit("agent-unlock-failed", format!("{e:#}")); }
                });
            }
        });
    }
    fn open_item(&self, vault_id: Option<&str>, item_id: Option<&str>) {
        let h = self.h.clone();
        let p = serde_json::json!({"vaultId": vault_id, "itemId": item_id});
        let _ = self.h.run_on_main_thread(move || {
            let w = focus_main(&h);
            if p["itemId"].is_string() {
                let _ = h.emit("open-item", p.clone());
                if let Some(w) = w {
                    let js = format!("window.dispatchEvent(new CustomEvent('gitpass:open-item', {{ detail: {p} }}))");
                    let _ = w.eval(&js);
                }
            }
        });
    }
}

/// Start the agent server; failures are logged, the GUI keeps working without the extension bridge.
pub fn start(h: &AppHandle) {
    let app = h.state::<App>().inner().clone();
    match Agent::start(app, Arc::new(TauriUi { h: h.clone() }), &default_dir()) {
        Ok(a) => { h.manage(a); }
        Err(e) => eprintln!("gitpass: extension agent not started: {e:#}"),
    }
}
