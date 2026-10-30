use super::dto::{Onboarding, Settings};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ------------------------------------------------------------------ local (non-secret) state file
pub const SETTINGS_FILE: &str = ".gitpass-settings.json";

/// Contents of `.gitpass-settings.json` (gitignored; non-secret only).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalFile {
    #[serde(default)] pub settings: Option<Settings>,
    #[serde(default)] pub onboarding: Onboarding,
    /// Rust-only setting: allow fetching favicons. Default off. (MVP: favicon_url still returns null.)
    #[serde(default)] pub allow_favicons: bool,
    /// Local labels for vault member fingerprints.
    #[serde(default)] pub member_labels: BTreeMap<String, String>,
    #[serde(default)] pub last_sync: Option<String>,
    #[serde(default)] pub last_vault_path: Option<String>,
}

