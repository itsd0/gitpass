//! macOS Touch ID unlock: the master password + Secret Key are kept in the user's login Keychain
//! (never on disk in the vault, never logged) and are only read after LocalAuthentication confirms
//! biometry (`LAPolicyDeviceOwnerAuthenticationWithBiometrics`). Other platforms: unsupported.
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
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

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

    impl BiometricStore for MacBiometrics {
        fn supported(&self) -> bool { unsafe { LAContext::new().canEvaluatePolicy_error(policy()) }.is_ok() }
        fn store(&self, vault_path: &str, password: &str, secret_key: &str) -> Result<()> {
            evaluate("enable Touch ID unlock for gitpass")?;
            // secret key never contains '\n'; split on the last newline when loading
            let blob = Zeroizing::new(format!("{password}\n{secret_key}"));
            let _ = delete_generic_password(SERVICE, vault_path);
            set_generic_password(SERVICE, vault_path, blob.as_bytes()).map_err(|e| anyhow!("Keychain: {e}"))
        }
        fn load(&self, vault_path: &str, reason: &str) -> Result<(Zeroizing<String>, Zeroizing<String>)> {
            evaluate(reason)?;
            let raw = Zeroizing::new(get_generic_password(SERVICE, vault_path).map_err(|e| anyhow!("Keychain: {e} (re-enable Touch ID in Settings)"))?);
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
