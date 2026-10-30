use crate::item::{Category, Field, FieldKind, Section};
use crate::merge::ItemRecord;
use crate::store::{Item, now};

pub(crate) fn opt(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|v| !v.is_empty()).map(String::from)
}

pub(crate) fn append_note(notes: &mut Option<String>, line: String) {
    match notes { Some(n) => { n.push('\n'); n.push_str(&line); } None => *notes = Some(line) }
}

pub(crate) fn fld(id: &str, label: &str, kind: FieldKind, v: Option<String>) -> Option<Field> {
    v.filter(|x| !x.trim().is_empty()).map(|value| Field { id: id.into(), label: label.into(), kind, value: Some(value), removed: false })
}

pub(crate) fn push_section(rec: &mut ItemRecord, id: &str, title: Option<&str>, fields: Vec<Option<Field>>) {
    let fields: Vec<Field> = fields.into_iter().flatten().collect();
    if fields.is_empty() { return; }
    if let Some(s) = rec.sections.iter_mut().find(|s| s.id == id) { s.fields.extend(fields); return; }
    rec.sections.push(Section { id: id.into(), title: title.map(String::from), fields });
}

pub(crate) fn add_tag(rec: &mut ItemRecord, t: &str) {
    let t = t.trim();
    if !t.is_empty() && !rec.tags.iter().any(|x| x == t) { rec.tags.push(t.to_string()); }
}

pub(crate) fn truthy(v: &str) -> bool { matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "y") }

pub(crate) fn record(title: String, category: Category) -> ItemRecord {
    let mut it = Item::default(); it.title = title; it.updated_at = now();
    let mut r = ItemRecord::new(it);
    r.category = category;
    r
}

/// Unique, stable field id for custom fields ("f1", "f2", ...) within a record.
pub(crate) fn next_id(rec: &ItemRecord, prefix: &str) -> String {
    let n = rec.sections.iter().flat_map(|s| s.fields.iter()).filter(|f| f.id.starts_with(prefix)).count() + 1;
    format!("{prefix}{n}")
}

/// Parse ISO 8601 strings or unix timestamps (seconds or milliseconds).
pub fn parse_time(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Ok(epoch) = s.parse::<u64>() {
        if epoch >= 100_000_000_000 {
            return Some(epoch / 1000);
        }
        if epoch > 0 {
            return Some(epoch);
        }
    }
    iso_to_epoch(s)
}

/// Parse `YYYY-MM-DD[T ]HH:MM:SS[.fff][Z]` (UTC assumed) to unix seconds. Returns None otherwise.
pub fn iso_to_epoch(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.len() < 10 { return None; }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let m: i64 = s.get(5..7)?.parse().ok()?;
    let d: i64 = s.get(8..10)?.parse().ok()?;
    if s.get(4..5)? != "-" || s.get(7..8)? != "-" || !(1..=12).contains(&m) || !(1..=31).contains(&d) { return None; }
    let (hh, mm, ss) = if s.len() >= 19 {
        (s.get(11..13)?.parse::<i64>().ok()?, s.get(14..16)?.parse::<i64>().ok()?, s.get(17..19)?.parse::<i64>().ok()?)
    } else { (0, 0, 0) };
    // days from civil (Howard Hinnant)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let t = days * 86400 + hh * 3600 + mm * 60 + ss;
    (t >= 0).then_some(t as u64)
}

pub(crate) fn month_year(m: Option<String>, y: Option<String>) -> Option<String> {
    let y = y?; let y = if y.len() == 2 { format!("20{y}") } else { y };
    Some(match m { Some(m) => format!("{:0>2}/{y}", m), None => y })
}
