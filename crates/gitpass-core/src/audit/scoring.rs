//! Watchtower security score. Re-exported from the audit module.

use super::{AuditReport, Finding, ItemFindings};
use crate::merge::ItemRecord;

/// Weight constants for Watchtower security score calculation.
///
/// The score is on a 0..=100 scale:
/// - 80% weight: bad passwords (weak, reused, or pwned/breached). An item with
///   multiple bad password findings is only penalized once against the number of items
///   with a password.
/// - 40% weight (half the bad-password weight): a login whose current password matches a
///   retired password-history entry (`reused_old_password`), counted only for items that are
///   not already weak, reused, or breached.
/// - 10% weight: old passwords (> max age, default 365 days) against items with a password.
/// - 10% weight: missing 2FA / TOTP (either unconfigured TOTP or documented 2FA support)
///   against total checked items.
pub const SCORE_WEIGHT_BAD_PASSWORDS: f64 = 80.0;
pub const SCORE_WEIGHT_REUSED_OLD_PASSWORD: f64 = 40.0;
pub const SCORE_WEIGHT_OLD_PASSWORDS: f64 = 10.0;
pub const SCORE_WEIGHT_MISSING_2FA: f64 = 10.0;

/// Returns true if the record is a live, non-archived, non-Passkey item with a non-empty password,
/// suitable for inclusion in the Watchtower score denominator.
pub fn is_score_password_item(rec: &ItemRecord) -> bool {
    rec.deleted_at.is_none()
        && !rec.archived
        && rec.category != crate::item::Category::Passkey
        && rec.item.password.as_deref().is_some_and(|p| !p.is_empty())
}

/// Compute the Watchtower security score (0..=100) from an [`AuditReport`] and
/// the count of live items with a password (`items_with_password`).
///
/// # Weights
/// - **80% Bad passwords**: proportion of items with a password that have a weak,
///   reused, or breached/pwned finding (`bad_items / items_with_password * 80.0`).
///   An item with multiple bad password findings is only penalized once.
/// - **40% Reused old passwords**: proportion of items with a password whose current password
///   matches a retired history entry, counted only when the item is not already a bad password
///   (`reused_old_only / items_with_password * 40.0`).
/// - **10% Old passwords**: proportion of items with a password that have not been
///   changed in over `max_age_days` (`old_items / items_with_password * 10.0`).
/// - **10% Missing 2FA / TOTP**: proportion of all audited items that lack two-factor
///   authentication (`(missing_totp + totp_available) / items_checked.max(1) * 10.0`).
///
/// Returns 100 when `items_with_password == 0`.
pub fn score(report: &AuditReport, items_with_password: usize) -> u8 {
    if items_with_password == 0 {
        return 100;
    }
    let is_bad = |f: &ItemFindings| f.findings.iter().any(|x| matches!(x, Finding::Weak { .. } | Finding::Reused { .. } | Finding::Pwned { .. }));
    let bad_count = report.per_item.iter().filter(|f| is_bad(f)).count();
    let reused_old_only_count = report.per_item.iter().filter(|f| {
        !is_bad(f) && f.findings.iter().any(|x| matches!(x, Finding::ReusedOldPassword { .. }))
    }).count();
    let old_count = report.per_item.iter().filter(|f| {
        f.findings.iter().any(|x| matches!(x, Finding::Old { .. }))
    }).count();
    let missing_totp_count = report.per_item.iter().filter(|f| {
        f.findings.iter().any(|x| matches!(x, Finding::MissingTotp | Finding::TotpAvailable { .. }))
    }).count();

    let penalty = bad_count as f64 / items_with_password as f64 * SCORE_WEIGHT_BAD_PASSWORDS
        + reused_old_only_count as f64 / items_with_password as f64 * SCORE_WEIGHT_REUSED_OLD_PASSWORD
        + old_count as f64 / items_with_password as f64 * SCORE_WEIGHT_OLD_PASSWORDS
        + missing_totp_count as f64 / report.items_checked.max(1) as f64 * SCORE_WEIGHT_MISSING_2FA;

    (100.0 - penalty).round().clamp(0.0, 100.0) as u8
}

impl AuditReport {
    /// Compute the Watchtower security score (0..=100) given `items_with_password`.
    pub fn score(&self, items_with_password: usize) -> u8 { score(self, items_with_password) }
}
