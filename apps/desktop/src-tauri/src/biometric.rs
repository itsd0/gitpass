//! macOS Touch ID unlock: the master password + Secret Key are kept in the user's login Keychain
//! (never on disk in the vault, never logged) and are only released after biometry.
//!
//! ## Why the legacy (file-based) login keychain, not `.biometryCurrentSet`
//! A `SecAccessControl` with `.biometryCurrentSet` lives in the data-protection keychain, which
//! requires a `keychain-access-groups` / `application-identifier` entitlement backed by an Apple
//! provisioning profile. gitpass is signed with a free self-signed identity ("gitpass Local
//! Signing"), so `SecItemAdd` fails with errSecMissingEntitlement (-34018). We therefore:
//!
//! 1. store the item in the legacy login keychain with the default ACL, which trusts only the
//!    creating app's code signature (other apps get a Keychain password prompt);
//! 2. gate every read with LocalAuthentication `evaluatePolicy(.deviceOwnerAuthenticationWithBiometrics)`;
//!    the item is not read unless Touch ID succeeds (this is the one prompt per unlock — the
//!    keychain read itself does not prompt for the trusted app);
//! 3. record `evaluatedPolicyDomainState` at enrolment inside the item and refuse to release the
//!    secret if it changed — fingerprints added/removed (the property `.biometryCurrentSet` gives).
//!    The secret is not returned in that case and the item is deleted, so the user re-enables
//!    Touch ID with the master password. Note: Apple warns the state can also change across major
//!    macOS upgrades; the effect is the same harmless re-enrolment.
//!
//! ## Honest tradeoff
//! This is weaker than SEP-enforced access control. The Touch ID check is enforced by gitpass's
//! own code, not by the Secure Enclave/keychain: any process running as the user that is signed
//! with the same "gitpass Local Signing" identity (i.e. anyone holding that private key, which sits
//! in the same login keychain), or a modified gitpass binary re-signed with it, could read the item
//! without Touch ID. Malware already running as the user can also prompt for / sniff the master
//! password, so the practical delta is small, but it is real. A build with a real Apple team ID
//! could use `.biometryCurrentSet` instead; that path is not implemented because gitpass ships
//! self-signed only.
//!
//! Saving always deletes the old item and adds a new one, so disable/re-enable and re-enrolment
//! leave no stale item or stale domain state. Fallback to the master password is at the UI
//! level: biometric failure never locks the user out.
//! Linux: unsupported. Windows: STUB (NoBiometrics).
use anyhow::{anyhow, bail, Result};
use gitpass_gui_core::api::BiometricStore;
use zeroize::Zeroizing;

/// Item payload: `gp-bio-v2\n<hex domain state>\n<password>\n<secret key>`.
/// The secret key never contains '\n'; the password is split off by the last newline.
const BLOB_V2: &str = "gp-bio-v2";

fn encode_blob(state: &[u8], password: &str, secret_key: &str) -> Zeroizing<String> {
    let hex: String = state.iter().map(|b| format!("{b:02x}")).collect();
    Zeroizing::new(format!("{BLOB_V2}\n{hex}\n{password}\n{secret_key}"))
}

/// Ok(Some(..)) on match; Ok(None) if the recorded biometry state differs; Err if malformed/legacy.
#[allow(clippy::type_complexity)]
fn decode_blob(raw: &str, current_state: &[u8]) -> Result<Option<(Zeroizing<String>, Zeroizing<String>)>> {
    let rest = raw.strip_prefix(BLOB_V2).and_then(|r| r.strip_prefix('\n'))
        .ok_or_else(|| anyhow!("Touch ID item is from an older gitpass build — turn Touch ID off and on again in Settings"))?;
    let (hex, rest) = rest.split_once('\n').ok_or_else(|| anyhow!("Keychain item corrupt"))?;
    let cur: String = current_state.iter().map(|b| format!("{b:02x}")).collect();
    if hex.is_empty() || hex != cur { return Ok(None); }
    let i = rest.rfind('\n').ok_or_else(|| anyhow!("Keychain item corrupt"))?;
    Ok(Some((Zeroizing::new(rest[..i].to_string()), Zeroizing::new(rest[i + 1..].to_string()))))
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

    const SERVICE: &str = "app.gitpass.desktop.biometric";
    const NOT_FOUND: i32 = -25300; // errSecItemNotFound

    pub struct MacBiometrics;
    fn policy() -> LAPolicy { LAPolicy::DeviceOwnerAuthenticationWithBiometrics }

    /// Shows the single Touch ID prompt; on success returns the biometry domain state.
    #[allow(deprecated)] // evaluatedPolicyDomainState: still the only API on macOS < 15
    fn evaluate(reason: &str) -> Result<Vec<u8>> {
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
        rx.recv().map_err(|_| anyhow!("Touch ID: no reply"))?.map_err(|m| anyhow!("Touch ID: {m}"))?;
        let state = unsafe { ctx.evaluatedPolicyDomainState() }.map(|d| d.to_vec()).unwrap_or_default();
        if state.is_empty() { bail!("Touch ID: no biometry state available"); }
        Ok(state)
    }

    fn delete_item(vault_path: &str) -> Result<()> {
        match delete_generic_password(SERVICE, vault_path) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == NOT_FOUND => Ok(()),
            Err(e) => bail!("Keychain: {e}"),
        }
    }

    impl BiometricStore for MacBiometrics {
        fn supported(&self) -> bool { unsafe { LAContext::new().canEvaluatePolicy_error(policy()) }.is_ok() }
        fn store(&self, vault_path: &str, password: &str, secret_key: &str) -> Result<()> {
            let state = evaluate("enable Touch ID unlock for gitpass")?;
            let blob = encode_blob(&state, password, secret_key);
            // Delete-then-add: never update in place (would keep an old ACL / stale state).
            delete_item(vault_path).map_err(|e| anyhow!("{e} (disable Touch ID in Settings, then re-enable it)"))?;
            set_generic_password(SERVICE, vault_path, blob.as_bytes()).map_err(|e| anyhow!("Keychain: {e}"))
        }
        fn load(&self, vault_path: &str, reason: &str) -> Result<(Zeroizing<String>, Zeroizing<String>)> {
            // Touch ID first; the item is not read unless it succeeds.
            let state = evaluate(reason)?;
            let raw = Zeroizing::new(get_generic_password(SERVICE, vault_path)
                .map_err(|e| anyhow!("Keychain: {e} (re-enable Touch ID in Settings)"))?);
            let s = Zeroizing::new(String::from_utf8(raw.to_vec()).map_err(|_| anyhow!("Keychain item corrupt"))?);
            match decode_blob(&s, &state)? {
                Some(v) => Ok(v),
                None => {
                    let _ = delete_item(vault_path);
                    bail!("Touch ID fingerprints changed since Touch ID unlock was enabled — unlock with your master password and re-enable Touch ID in Settings")
                }
            }
        }
        fn delete(&self, vault_path: &str) -> Result<()> { delete_item(vault_path) }
    }
}

pub fn platform() -> Box<dyn BiometricStore> {
    #[cfg(target_os = "macos")] { Box::new(mac::MacBiometrics) }
    #[cfg(not(target_os = "macos"))] { Box::new(gitpass_gui_core::api::NoBiometrics) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blob_roundtrip_and_password_with_newline() {
        let b = encode_blob(&[1, 2, 255], "pa\nss", "A3-SECRET");
        let (p, k) = decode_blob(&b, &[1, 2, 255]).unwrap().unwrap();
        assert_eq!((p.as_str(), k.as_str()), ("pa\nss", "A3-SECRET"));
    }
    #[test]
    fn changed_biometry_state_refuses() {
        let b = encode_blob(&[1, 2, 3], "pw", "sk");
        assert!(decode_blob(&b, &[1, 2, 4]).unwrap().is_none());
        assert!(decode_blob(&b, &[]).unwrap().is_none());
    }
    #[test]
    fn legacy_or_empty_state_item_refuses() {
        assert!(decode_blob("pw\nsk", &[1]).is_err());
        let b = encode_blob(&[], "pw", "sk");
        assert!(decode_blob(&b, &[]).unwrap().is_none());
    }
}
