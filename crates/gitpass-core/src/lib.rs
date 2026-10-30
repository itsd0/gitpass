//! gitpass-core: headless library for gitpass — a two-secret, end-to-end encrypted password manager synced via git.
//!
//! Intended to be shared by the `gitpass` CLI and a future GUI (e.g. Tauri). Main entry points:
//! - [`store::Session`] (unlock / items / vaults / devices), [`store::Item`]
//! - [`merge::merge_records`] / [`merge_items`] field-level merge
//! - [`device`] registry, [`totp::code_now`] / [`totp_now`], [`search::rank`] / [`fuzzy_find`]
//! - [`generate::generate`] (random password, passphrase or PIN)
pub mod audit;
pub mod crypto;
pub mod device;
pub mod fsutil;
pub mod gen_history;
pub mod generate;
pub mod keys;
pub mod import;
pub mod item;
pub mod mask;
pub mod merge;
pub mod remote;
pub mod migrate;
pub mod search;
pub mod ssh;
pub mod passkey;
pub mod store;
pub mod sync;
pub mod totp;
pub mod unlock_throttle;

pub use merge::{merge_records as merge_items, ItemRecord, MergeReport};
pub use search::rank as fuzzy_find;
pub use gen_history::{GeneratorHistory, GeneratorHistoryEntry};
pub use item::{Category, FieldKind};
pub use mask::{api_credential_secret, secret_hint};
pub use store::{Item, Session, UnlockOpts, FORMAT_VERSION, MIN_READ_FORMAT};
pub use totp::code_now as totp_now;
