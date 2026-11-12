//! Origin matching lives in `gitpass_core::origin` so the browser extension host and the Android
//! AutofillService (#475) share one implementation of the eTLD+1 rules. Re-exported unchanged.
pub use gitpass_core::origin::*;
