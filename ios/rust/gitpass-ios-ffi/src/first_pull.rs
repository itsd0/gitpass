//! Downloaded-input gate only: no changes to the shared vault format or global KDF policy.
use crate::GitpassError;
use gitpass_core::store::{Format, Session};
use std::io::Read;
use std::path::Path;

const MAX_FORMAT_BYTES: u64 = 65_536;

fn budget(format: &Format) -> Result<(), GitpassError> {
    let k = &format.kdf;
    // Ceilings, not derivation targets. Never clamp/rewrite an imported vault's parameters.
    // Accept the normal production default (256 MiB, t=3, p=4); capable devices still
    // need that much memory. More expensive vaults require a different capable client.
    if k.alg != "argon2id"
        || k.m_kib < gitpass_core::crypto::KDF_FLOOR_M_KIB
        || k.m_kib > 262_144
        || !(1..=6).contains(&k.t)
        || !(1..=4).contains(&k.p)
        || k.m_kib < 8 * k.p
    {
        return Err(GitpassError::Generic("Imported vault KDF is unsupported on Android: require Argon2id, memory 8192..262144 KiB, iterations 1..6, lanes 1..4; parameters were not changed".into()));
    }
    Ok(())
}

/// Authenticate a PRIVATE, completed staging root before selecting it as an account.
/// Does not register a device, upgrade KDF parameters, repair rollback, or re-seal data.
/// Core may write local throttle/epoch bookkeeping inside staging; encrypted files are unchanged.
/// Caller owns path allowlisting, cancellation/epoch checks and atomic activation.
#[uniffi::export]
pub fn validate_first_pull_snapshot(
    root: String,
    password: String,
    secret_key: String,
) -> Result<(), GitpassError> {
    let result = (|| -> anyhow::Result<()> {
        let root = Path::new(&root);
        let mut bytes = Vec::new();
        std::fs::File::open(root.join("format.json"))?
            .take(MAX_FORMAT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_FORMAT_BYTES,
            "format exceeds limit"
        );
        let format: Format = serde_json::from_slice(&bytes)?;
        // This gate runs BEFORE parsing/deriving any account keys or allocating Argon2 memory.
        budget(&format)?;
        let secret = gitpass_core::crypto::SecretKey::parse(&secret_key)?;
        let session = Session::reopen_verify(root, &password, &secret, false)?;
        // reopen_verify authenticates manifest + every referenced hash, including attachments,
        // and rejects extra encrypted entries. Also decrypt every item (including Trash).
        for (vault_id, _) in session.list_vaults()? {
            for (item_id, _, _) in session.list_records(&vault_id)? {
                session.read_record(&vault_id, &item_id)?;
            }
        }
        Ok(())
    })();
    // Never return file paths, downloaded JSON or credential-bearing lower-level errors to UI/logs.
    result.map_err(|error| {
        if let Some(GitpassError::Generic(message)) = error.downcast_ref::<GitpassError>() {
            GitpassError::Generic(message.clone())
        } else {
            GitpassError::Generic("First-pull authentication failed; verify credentials and snapshot integrity. Existing account is unchanged".into())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitpass_core::crypto::KdfParams;

    fn format_with(kdf: KdfParams) -> Format {
        let mut value =
            serde_json::json!({"version":5,"account_id":"FAKE", "salt":"FAKE", "kdf":kdf});
        // Keep field parsing identical to the real stored format.
        value["created_at"] = serde_json::json!(0);
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn budget_accepts_production_default_without_lowering_it() {
        let format = format_with(KdfParams::default());
        assert_eq!(format.kdf.m_kib, 262_144);
        assert!(budget(&format).is_ok());
        assert_eq!(
            (format.kdf.m_kib, format.kdf.t, format.kdf.p),
            (262_144, 3, 4)
        );
    }

    #[test]
    fn budget_rejects_excessive_or_invalid_cost_before_derivation() {
        for (m, t, p) in [
            (u32::MAX, 3, 4),
            (262_145, 3, 4),
            (262_144, u32::MAX, 4),
            (262_144, 7, 4),
            (262_144, 3, u32::MAX),
            (262_144, 3, 5),
            (8191, 3, 4),
            (8192, 0, 1),
            (8192, 1, 0),
        ] {
            assert!(budget(&format_with(KdfParams {
                alg: "argon2id".into(),
                m_kib: m,
                t,
                p
            }))
            .is_err());
        }
        let malformed = r#"{"version":5,"account_id":"FAKE","salt":"FAKE","kdf":{"alg":"argon2id","m_kib":4294967296,"t":3,"p":4}}"#;
        assert!(serde_json::from_str::<Format>(malformed).is_err());
    }
}
