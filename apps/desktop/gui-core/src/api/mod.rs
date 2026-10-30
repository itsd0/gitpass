//! v0.5.0 contract API: one method per command in `src/api/client.ts` (same snake_case names),
//! returning exactly the contract shapes (camelCase JSON). Tauri wrappers live in src-tauri.
//!
//! Security: concealed / TOTP field values are `null` in every list/detail payload; only
//! [`App::reveal_field`] returns a secret. [`App::copy_field`] copies inside Rust and clears the
//! clipboard after `clipboardClearSecs` if it still holds the copied value.
//!
//! This module only re-exports the split implementation modules, so every
//! `crate::api::*` / `gitpass_gui_core::api::*` path compiles unchanged.
pub mod app;
pub mod clipboard;
pub mod display;
pub mod dto;
pub mod generator;
pub mod local;
pub mod passkey;
pub mod ssh;

pub use app::App;
pub use clipboard::{Clipboard, ClipboardImage, MemClipboard, UreqHibp};
pub use dto::*;
pub use display::{card_brand, card_label, card_mask, item_expiry, strength, subtitle};
pub use generator::{generate, WORD_SEPARATORS};
pub use local::{LocalFile, SETTINGS_FILE};
pub use passkey::passkey_site_url;
pub use ssh::{SshAgentInfo, SshKeyCreated};
pub use crate::qr::ScannedTotp;
