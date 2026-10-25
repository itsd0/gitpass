//! Ranked fuzzy matching over item title / username / url (and id).
use crate::store::Item;

/// Score `q` against `hay` (case-insensitive). None = no match. Higher is better.
/// exact > prefix > word-prefix > substring > in-order subsequence (penalised by gaps).
pub fn score_str(q: &str, hay: &str) -> Option<i64> {
    let q = q.to_lowercase(); let h = hay.to_lowercase();
    if q.is_empty() { return Some(0); }
    if h.is_empty() { return None; }
    if h == q { return Some(1000); }
    if h.starts_with(&q) { return Some(800 - (h.len() as i64 - q.len() as i64).min(100)); }
    if let Some(pos) = h.find(&q) {
        let word = h[..pos].ends_with(|c: char| !c.is_alphanumeric());
        return Some(if word { 600 } else { 500 } - (pos as i64).min(100));
    }
    // subsequence
    let hc: Vec<char> = h.chars().collect();
    let (mut i, mut first, mut last, mut gaps) = (0usize, None, 0usize, 0i64);
    for c in q.chars() {
        let start = i;
        while i < hc.len() && hc[i] != c { i += 1; }
        if i == hc.len() { return None; }
        if first.is_none() { first = Some(i); } else { gaps += (i - start) as i64; }
        last = i; i += 1;
    }
    let _ = last;
    Some((300 - gaps * 10 - first.unwrap_or(0) as i64).max(1))
}

/// Best score of an item across fields; title weighted highest. Id prefix match is top priority.
pub fn score_item(q: &str, id: &str, it: &Item) -> Option<i64> {
    if id == q { return Some(10_000); }
    if !q.is_empty() && id.starts_with(q) { return Some(5_000); }
    let t = score_str(q, &it.title).map(|s| s + 50);
    let u = it.username.as_deref().and_then(|u| score_str(q, u));
    let w = it.url.as_deref().and_then(|u| score_str(q, u)).map(|s| s - 20);
    [t, u, w].into_iter().flatten().max()
}

/// Filter and rank (descending score, then title). Returns (score, id, item).
pub fn rank(q: &str, items: Vec<(String, Item)>) -> Vec<(i64, String, Item)> {
    let mut v: Vec<_> = items.into_iter().filter_map(|(id, it)| score_item(q, &id, &it).map(|s| (s, id, it))).collect();
    v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.title.to_lowercase().cmp(&b.2.title.to_lowercase())));
    v
}

/// A single clear winner: exactly one match, or top score is exact/id-level, or beats #2 by a wide margin.
pub fn clear_winner<T>(ranked: &[(i64, String, T)]) -> Option<usize> {
    match ranked {
        [] => None,
        [_] => Some(0),
        [a, b, ..] => if a.0 >= 1000 && b.0 < a.0 || a.0 - b.0 >= 200 { Some(0) } else { None },
    }
}
