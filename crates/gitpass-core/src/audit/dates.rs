//! Expiry date and unused activity timestamp parsing.

/// Parse month_year (MM/YY, MM/YYYY, YYYY/MM, YYYY-MM) or date (YYYY-MM-DD, unix timestamp) to
/// unix seconds. For month_year, expiry is 23:59:59 UTC on the last day of that month.
/// For date without time, expiry is 23:59:59 UTC on that day.
/// Returns None on unparsable / out-of-range input instead of panicking.
pub fn parse_expiry(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if s.is_empty() { return None; }
    // 1. Check if it's all digits (unix epoch seconds)
    if s.len() >= 9 && s.len() <= 12 && s.chars().all(|c| c.is_ascii_digit()) {
        return s.parse::<u64>().ok();
    }
    // 2. Check month_year: MM/YY, MM/YYYY, MM-YY, MM-YYYY, YYYY/MM, YYYY-MM
    let parts: Vec<&str> = s.split(['/', '-']).map(str::trim).collect();
    if parts.len() == 2 {
        let (p1, p2) = (parts[0], parts[1]);
        let (y, m) = if let (Ok(m), Ok(mut y)) = (p1.parse::<i64>(), p2.parse::<i64>()) {
            if (1..=12).contains(&m) {
                if p2.len() == 2 && (0..100).contains(&y) { y += 2000; }
                (y, m)
            } else { (-1, -1) }
        } else if let (Ok(y), Ok(m)) = (p1.parse::<i64>(), p2.parse::<i64>()) {
            if p1.len() == 4 && (1..=12).contains(&m) { (y, m) } else { (-1, -1) }
        } else { (-1, -1) };

        if (1970..=9999).contains(&y) && (1..=12).contains(&m) {
            let (next_y, next_m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            let next_start = crate::import::iso_to_epoch(&format!("{next_y:04}-{next_m:02}-01"))?;
            return next_start.checked_sub(1);
        }
    }
    // 3. Check date: YYYY-MM-DD, YYYY/MM/DD, or ISO 8601 with time
    let norm = s.replace('/', "-");
    if norm.len() == 10 {
        let day_start = crate::import::iso_to_epoch(&norm)?;
        return Some(day_start + 86399);
    }
    if norm.len() >= 19 {
        return crate::import::iso_to_epoch(&norm);
    }
    None
}

/// Days in `months` for the unused-item threshold, counting months as twelfths of 365 days
/// (6 → 182, 12 → 365, 24 → 730). The check is strict: an item exactly at the threshold is kept.
pub fn unused_threshold_days(months: u64) -> u64 { months.saturating_mul(365) / 12 }

/// A timestamp stored either as unix seconds (current items) or ISO 8601 (older items).
pub(crate) fn activity_ts(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if s.is_empty() { return None; }
    if let Ok(n) = s.parse::<u64>() { return Some(n); }
    crate::device::parse_iso8601(s).and_then(|t| u64::try_from(t).ok())
}
