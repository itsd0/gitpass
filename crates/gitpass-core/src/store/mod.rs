//! On-disk vault layout (format v2) and envelope hierarchy: AUK -> Account Key -> Vault Key -> Item,
//! plus the authenticated manifest that protects against rollback / file deletion / substitution.
//! Split from the former single-file `store.rs` (see issue #351); this module
//! re-exports the previous public API unchanged.

pub mod format;
pub mod local_state;
pub mod session;
pub mod sharing;

pub use format::*;
pub use local_state::{get_all_last_used, get_item_last_used, last_epoch, now, now_secs, set_last_epoch, touch_item_used};
pub use session::{init, verify, Session, UnlockOpts};
pub use sharing::*;
pub(crate) use format::{read_json, write_json};
