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
    let (mut i, mut first, mut gaps) = (0usize, None, 0i64);
    for c in q.chars() {
        let start = i;
        while i < hc.len() && hc[i] != c { i += 1; }
        if i == hc.len() { return None; }
        if first.is_none() { first = Some(i); } else { gaps += (i - start) as i64; }
        i += 1;
    }
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

/// Filter and rank by an item accessor (descending score, then case-insensitive title).
pub fn rank_by<T>(
    q: &str,
    items: impl IntoIterator<Item = (String, T)>,
    item: impl Fn(&T) -> &Item,
) -> Vec<(i64, String, T)> {
    let mut v: Vec<_> = items
        .into_iter()
        .filter_map(|(id, it)| score_item(q, &id, item(&it)).map(|s| (s, id, it)))
        .collect();
    v.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| item(&a.2).title.to_lowercase().cmp(&item(&b.2).title.to_lowercase()))
    });
    v
}

/// Filter and rank (descending score, then title). Returns (score, id, item).
pub fn rank(q: &str, items: Vec<(String, Item)>) -> Vec<(i64, String, Item)> {
    rank_by(q, items, |it| it)
}

/// A single clear winner: exactly one match, or top score is exact/id-level, or beats #2 by a wide margin.
pub fn clear_winner<T>(ranked: &[(i64, String, T)]) -> Option<usize> {
    match ranked {
        [] => None,
        [_] => Some(0),
        [a, b, ..] => if a.0 >= 1000 && b.0 < a.0 || a.0 - b.0 >= 200 { Some(0) } else { None },
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn test_item(title: &str, username: Option<&str>, url: Option<&str>) -> Item {
        Item {
            title: title.to_string(),
            username: username.map(|s| s.to_string()),
            password: Some("secret".to_string()),
            url: url.map(|s| s.to_string()),
            notes: None,
            totp: None,
            updated_at: String::new(),
        }
    }

    #[test]
    fn test_score_item_id_exact_and_prefix() {
        let it = test_item("GitHub", Some("alice"), Some("https://github.com"));
        // Exact id match gives 10_000
        assert_eq!(score_item("item-123", "item-123", &it), Some(10_000));
        // Prefix id match gives 5_000
        assert_eq!(score_item("item", "item-123", &it), Some(5_000));
        // Empty query does not trigger prefix match
        assert_ne!(score_item("", "item-123", &it), Some(5_000));
    }

    #[test]
    fn test_score_item_field_weighting() {
        // Title has +50 bonus, username has no bonus, url has -20 penalty.
        let it_title = test_item("target", Some("other_user"), Some("https://other.example"));
        let it_user = test_item("other_title", Some("target"), Some("https://other.example"));
        let it_url = test_item("other_title", Some("other_user"), Some("target"));

        let score_t = score_item("target", "id1", &it_title).unwrap();
        let score_u = score_item("target", "id2", &it_user).unwrap();
        let score_w = score_item("target", "id3", &it_url).unwrap();

        // Exact matches: title = 1000 + 50 = 1050, username = 1000, url = 1000 - 20 = 980
        assert_eq!(score_t, 1050);
        assert_eq!(score_u, 1000);
        assert_eq!(score_w, 980);
        assert!(score_t > score_u);
        assert!(score_u > score_w);

        // Also test realistic URL substring match
        let it_real_url = test_item("other_title", Some("other_user"), Some("https://target.com"));
        let score_real_w = score_item("target", "id4", &it_real_url).unwrap();
        assert!(score_real_w > 0);
        assert!(score_t > score_real_w);
    }

    #[test]
    fn test_score_item_no_match() {
        let it = test_item("GitHub", Some("alice"), Some("https://github.com"));
        assert_eq!(score_item("nomatch", "id123", &it), None);
    }

    #[test]
    fn test_tie_breaking_by_case_insensitive_title() {
        // Two items with the same score:
        let it_b = test_item("beta", Some("user"), None);
        let it_a = test_item("Alpha", Some("user"), None);
        let items = vec![
            ("id_b".to_string(), it_b),
            ("id_a".to_string(), it_a),
        ];
        // Query "user" matches username in both items with identical score (1000)
        let ranked = rank("user", items);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].0, ranked[1].0);
        // "Alpha" should come before "beta" due to case-insensitive title sort
        assert_eq!(ranked[0].1, "id_a");
        assert_eq!(ranked[0].2.title, "Alpha");
        assert_eq!(ranked[1].1, "id_b");
        assert_eq!(ranked[1].2.title, "beta");
    }

    #[test]
    fn test_rank_by_custom_type() {
        struct Record {
            item: Item,
        }
        let r1 = Record { item: test_item("Zulu", Some("match"), None) };
        let r2 = Record { item: test_item("alpha", Some("match"), None) };
        let records = vec![("id_z".to_string(), r1), ("id_a".to_string(), r2)];
        let ranked = rank_by("match", records, |r| &r.item);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].1, "id_a");
        assert_eq!(ranked[1].1, "id_z");
    }
}
