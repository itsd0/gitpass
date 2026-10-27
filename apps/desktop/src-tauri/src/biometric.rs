//! macOS Touch ID unlock: the master password + Secret Key are kept in the user's login Keychain
//! (never on disk in the vault, never logged) and are only released after biometry.
//!
//! The Keychain item itself carries a `SecAccessControl` with
//! `.biometryCurrentSet` and `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`:
//! reading it always requires Touch ID for the currently enrolled fingers, the
//! item never syncs to iCloud Keychain, and adding a new fingerprint invalidates
//! the old item (the user must re-enable Touch ID in Settings). This is defense
//! in depth on top of the LocalAuthentication check: even a process that could
//! read the login keychain without a prompt cannot read this item.
//!
//! Saving always deletes the old item and adds a new one: `SecItemUpdate` keeps
//! the old item's access settings, so an in-place update would silently keep a
//! legacy item without biometry protection.
//!
//! On load there is exactly one system prompt: the Keychain prompt for the
//! access-controlled item. The code only checks `canEvaluatePolicy` first (no
//! prompt) so a machine without Touch ID gets a clean error instead of a
//! confusing prompt. The `reason` argument is kept for the trait API; the
//! prompt text shown is the system Keychain prompt, not the custom reason.
//! Fallback to the master password is at the UI level: biometric unlock
//! failing never locks the user out — the password form is always available.
//! Linux: unsupported. Windows: STUB (NoBiometrics); Windows Hello +
//! Credential Manager is not implemented yet.
use anyhow::{anyhow, bail, Result};
use gitpass_gui_core::api::BiometricStore;
use zeroize::Zeroizing;

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};
    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::passwords::{
        delete_generic_password, generic_password, set_generic_password_options, AccessControlOptions,
        PasswordOptions,
    };

    const SERVICE: &str = "app.gitpass.desktop.biometric";

    pub struct MacBiometrics;
    fn policy() -> LAPolicy { LAPolicy::DeviceOwnerAuthenticationWithBiometrics }

    fn evaluate(reason: &str) -> Result<()> {
        let ctx = unsafe { LAContext::new() };
        unsafe { ctx.canEvaluatePolicy_error(policy()) }.map_err(|e| anyhow!("Touch ID unavailable: {}", e.localizedDescription()))?;
        let (tx, rx) = std::sync::mpsc::channel::<std::result::Result<(), String>>();
        let block = RcBlock::new(move |ok: Bool, err: *mut NSError| {
            let r = if ok.as_bool() { Ok(()) } else {
                let msg = unsafe { err.as_ref() }.map(|e| e.localizedDescription().to_string()).unwrap_or_else(|| "authentication failed".into());
                Err(msg)
            };
            let _ = tx.send(r);
        });
        unsafe { ctx.evaluatePolicy_localizedReason_reply(policy(), &NSString::from_str(reason), &block) };
        rx.recv().map_err(|_| anyhow!("Touch ID: no reply"))?.map_err(|m| anyhow!("Touch ID: {m}"))
    }

    /// The access control every gitpass biometric item is created with.
    fn access_control() -> Result<SecAccessControl> {
        SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly),
            AccessControlOptions::BIOMETRY_CURRENT_SET.bits(),
        )
        .map_err(|e| anyhow!("Keychain access control: {e}"))
    }

    impl BiometricStore for MacBiometrics {
        fn supported(&self) -> bool { unsafe { LAContext::new().canEvaluatePolicy_error(policy()) }.is_ok() }
        fn store(&self, vault_path: &str, password: &str, secret_key: &str) -> Result<()> {
            evaluate("enable Touch ID unlock for gitpass")?;
            // secret key never contains '\n'; split on the last newline when loading
            let blob = Zeroizing::new(format!("{password}\n{secret_key}"));
            // Delete-then-add: an update would keep the old item's access
            // settings, so a legacy item without biometry protection would
            // silently survive a re-save.
            match delete_generic_password(SERVICE, vault_path) {
                Ok(()) => {}
                Err(e) if e.code() == -25300 => {} // errSecItemNotFound: nothing to replace
                Err(e) => bail!("Keychain: {e} (disable Touch ID in Settings, then re-enable it)"),
            }
            let mut options = PasswordOptions::new_generic_password(SERVICE, vault_path);
            options.set_access_control(access_control()?);
            set_generic_password_options(blob.as_bytes(), options).map_err(|e| anyhow!("Keychain: {e}"))
        }
        fn load(&self, vault_path: &str, _reason: &str) -> Result<(Zeroizing<String>, Zeroizing<String>)> {
            // Availability check only (no prompt). The Keychain read below
            // prompts once for Touch ID under the item's access control.
            let ctx = unsafe { LAContext::new() };
            unsafe { ctx.canEvaluatePolicy_error(policy()) }.map_err(|e| anyhow!("Touch ID unavailable: {}", e.localizedDescription()))?;
            let options = PasswordOptions::new_generic_password(SERVICE, vault_path);
            let raw = Zeroizing::new(generic_password(options).map_err(|e| anyhow!("Keychain: {e} (Touch ID declined, fingerprints changed, or item missing — use your master password, or re-enable Touch ID in Settings)"))?);
            let s = Zeroizing::new(String::from_utf8(raw.to_vec()).map_err(|_| anyhow!("Keychain item corrupt"))?);
            let i = s.rfind('\n').ok_or_else(|| anyhow!("Keychain item corrupt"))?;
            Ok((Zeroizing::new(s[..i].to_string()), Zeroizing::new(s[i + 1..].to_string())))
        }
        fn delete(&self, vault_path: &str) -> Result<()> {
            match delete_generic_password(SERVICE, vault_path) {
                Ok(()) => Ok(()),
                Err(e) if e.code() == -25300 => Ok(()), // errSecItemNotFound
                Err(e) => bail!("Keychain: {e}"),
            }
        }
    }
}

pub fn platform() -> Box<dyn BiometricStore> {
    #[cfg(target_os = "macos")] { Box::new(mac::MacBiometrics) }
    #[cfg(not(target_os = "macos"))] { Box::new(gitpass_gui_core::api::NoBiometrics) }
}
