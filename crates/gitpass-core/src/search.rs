//! Ranked fuzzy matching over item title / username / url (and id),
//! plus structured search query operator parsing (tag:, vault:, type:, is:, has:, url:, negation).
use crate::item::Category;
use crate::merge::ItemRecord;
use crate::store::Item;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterOp {
    Tag(String),
    Vault(String),
    Type(Category),
    IsFavorite,
    IsArchived,
    HasOtp,
    HasPasskey,
    HasAttachment,
    HasUrl,
    Url(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub negated: bool,
    pub op: FilterOp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Query {
    pub terms: String,
    pub filters: Vec<Filter>,
}

/// Tokenizes query string respecting single and double quotes.
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let n = chars.len();
    let mut i = 0;

    while i < n {
        while i < n && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= n {
            break;
        }

        let mut token = String::new();
        let mut in_quote: Option<char> = None;

        while i < n {
            let c = chars[i];
            if let Some(q) = in_quote {
                if c == q {
                    in_quote = None;
                } else {
                    token.push(c);
                }
                i += 1;
            } else if c == '"' || c == '\'' {
                in_quote = Some(c);
                i += 1;
            } else if c.is_whitespace() {
                break;
            } else {
                token.push(c);
                i += 1;
            }
        }
        if !token.is_empty() {
            tokens.push(token);
        }
    }
    tokens
}

/// Parse query string into free-text search terms and structured filters.
/// Unknown operators are preserved as free-text terms and never cause an error.
pub fn parse_query(q: &str) -> Query {
    let tokens = tokenize(q);
    let mut terms = Vec::new();
    let mut filters = Vec::new();

    for token in tokens {
        let (negated, body) = if let Some(stripped) = token.strip_prefix('-') {
            (true, stripped)
        } else {
            (false, token.as_str())
        };

        if let Some((op_name, op_val)) = body.split_once(':') {
            let val = op_val.trim();
            let parsed_op = match op_name.to_ascii_lowercase().as_str() {
                "tag" if !val.is_empty() => Some(FilterOp::Tag(val.to_string())),
                "vault" if !val.is_empty() => Some(FilterOp::Vault(val.to_string())),
                "type" if !val.is_empty() => val.parse::<Category>().ok().map(FilterOp::Type),
                "is" => match val.to_ascii_lowercase().as_str() {
                    "favorite" | "fav" => Some(FilterOp::IsFavorite),
                    "archived" => Some(FilterOp::IsArchived),
                    _ => None,
                },
                "has" => match val.to_ascii_lowercase().as_str() {
                    "otp" | "totp" => Some(FilterOp::HasOtp),
                    "passkey" => Some(FilterOp::HasPasskey),
                    "attachment" | "attachments" | "file" | "files" => Some(FilterOp::HasAttachment),
                    "url" | "website" => Some(FilterOp::HasUrl),
                    _ => None,
                },
                "url" if !val.is_empty() => Some(FilterOp::Url(val.to_string())),
                _ => None,
            };

            if let Some(op) = parsed_op {
                filters.push(Filter { negated, op });
            } else {
                terms.push(token);
            }
        } else {
            terms.push(token);
        }
    }

    Query {
        terms: terms.join(" "),
        filters,
    }
}

/// Lightweight snapshot of an item's non-secret attributes for candidate filtering.
#[derive(Clone, Debug)]
pub struct CandidateInfo<'a> {
    pub item: &'a Item,
    pub category: Category,
    pub tags: &'a [String],
    pub favorite: bool,
    pub archived: bool,
    pub has_otp: bool,
    pub has_attachment: bool,
    pub urls: Vec<&'a str>,
    pub vault_name: Option<String>,
}

impl<'a> CandidateInfo<'a> {
    pub fn from_item(item: &'a Item) -> Self {
        Self {
            item,
            category: Category::Login,
            tags: &[],
            favorite: false,
            archived: false,
            has_otp: item.totp.is_some(),
            has_attachment: false,
            urls: item.url.as_deref().into_iter().collect(),
            vault_name: None,
        }
    }

    pub fn from_record(r: &'a ItemRecord, vault_name: Option<&str>) -> Self {
        let mut urls = Vec::new();
        if let Some(u) = r.item.url.as_deref().filter(|u| !u.trim().is_empty()) {
            urls.push(u);
        }
        for s in &r.sections {
            for f in &s.fields {
                if !f.removed && f.kind == crate::item::FieldKind::Url {
                    if let Some(v) = f.value.as_deref().filter(|v| !v.trim().is_empty()) {
                        urls.push(v);
                    }
                }
            }
        }
        let has_otp = r.item.totp.is_some()
            || r.sections.iter().flat_map(|s| &s.fields).any(|f| {
                !f.removed
                    && f.kind == crate::item::FieldKind::Totp
                    && f.value.as_deref().is_some_and(|v| !v.trim().is_empty())
            });

        Self {
            item: &r.item,
            category: r.category,
            tags: &r.tags,
            favorite: r.favorite,
            archived: r.archived,
            has_otp,
            has_attachment: !r.attachments.is_empty(),
            urls,
            vault_name: vault_name.map(str::to_string),
        }
    }
}

pub trait SearchCandidate {
    fn to_candidate_info(&self) -> CandidateInfo<'_>;
}

impl SearchCandidate for Item {
    fn to_candidate_info(&self) -> CandidateInfo<'_> {
        CandidateInfo::from_item(self)
    }
}

impl SearchCandidate for &Item {
    fn to_candidate_info(&self) -> CandidateInfo<'_> {
        CandidateInfo::from_item(self)
    }
}

impl SearchCandidate for ItemRecord {
    fn to_candidate_info(&self) -> CandidateInfo<'_> {
        CandidateInfo::from_record(self, None)
    }
}

impl SearchCandidate for &ItemRecord {
    fn to_candidate_info(&self) -> CandidateInfo<'_> {
        CandidateInfo::from_record(self, None)
    }
}

impl<'a> SearchCandidate for CandidateInfo<'a> {
    fn to_candidate_info(&self) -> CandidateInfo<'_> {
        self.clone()
    }
}

impl Filter {
    pub fn matches(&self, candidate: &impl SearchCandidate) -> bool {
        let info = candidate.to_candidate_info();
        let matched = match &self.op {
            FilterOp::Tag(t) => info.tags.iter().any(|tag| tag.eq_ignore_ascii_case(t)),
            FilterOp::Vault(v) => info
                .vault_name
                .as_deref()
                .is_some_and(|vn| vn.to_lowercase().starts_with(&v.to_lowercase())),
            FilterOp::Type(c) => info.category == *c,
            FilterOp::IsFavorite => info.favorite,
            FilterOp::IsArchived => info.archived,
            FilterOp::HasOtp => info.has_otp,
            FilterOp::HasPasskey => info.category == Category::Passkey,
            FilterOp::HasAttachment => info.has_attachment,
            FilterOp::HasUrl => !info.urls.is_empty(),
            FilterOp::Url(target) => {
                let target_host = crate::audit::host_of(target)
                    .unwrap_or_else(|| target.trim().to_ascii_lowercase());
                info.urls.iter().any(|u| {
                    if let Some(h) = crate::audit::host_of(u) {
                        h == target_host
                            || h.strip_suffix(&target_host)
                                .is_some_and(|p| p.ends_with('.'))
                    } else {
                        false
                    }
                })
            }
        };
        if self.negated {
            !matched
        } else {
            matched
        }
    }
}

impl Filter {
    /// Whether this filter can be evaluated from a bare `Item` (no category, tags, vault or flags).
    pub fn evaluable_on_item(&self) -> bool {
        matches!(self.op, FilterOp::HasOtp | FilterOp::HasUrl | FilterOp::Url(_))
    }
}

impl Query {
    pub fn matches(&self, candidate: &impl SearchCandidate) -> bool {
        self.filters.iter().all(|f| f.matches(candidate))
    }

    /// Like `matches`, for bare `Item` candidates: operators that need record data
    /// (`type:`, `tag:`, `vault:`, `is:`, `has:passkey`, `has:attachment`) are ignored
    /// instead of being evaluated against hard-coded defaults.
    pub fn matches_item(&self, item: &Item) -> bool {
        self.filters.iter().filter(|f| f.evaluable_on_item()).all(|f| f.matches(item))
    }
}

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
    let parsed = parse_query(q);
    let mut v: Vec<_> = items
        .into_iter()
        .filter(|(_, t)| parsed.matches_item(item(t)))
        .filter_map(|(id, it)| score_item(&parsed.terms, &id, item(&it)).map(|s| (s, id, it)))
        .collect();
    v.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| item(&a.2).title.to_lowercase().cmp(&item(&b.2).title.to_lowercase()))
    });
    v
}

/// Filter and rank candidates (supporting category, tags, flags, and vault name).
pub fn rank_by_candidate<T>(
    q: &str,
    items: impl IntoIterator<Item = (String, T)>,
    candidate: impl Fn(&T) -> CandidateInfo<'_>,
) -> Vec<(i64, String, T)> {
    let parsed = parse_query(q);
    let mut v: Vec<_> = items
        .into_iter()
        .filter(|(_, t)| {
            let info = candidate(t);
            parsed.matches(&info)
        })
        .filter_map(|(id, it)| {
            let info = candidate(&it);
            score_item(&parsed.terms, &id, info.item).map(|s| (s, id, it))
        })
        .collect();
    v.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| candidate(&a.2).item.title.to_lowercase().cmp(&candidate(&b.2).item.title.to_lowercase()))
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
    use crate::merge::ItemRecord;

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
        assert_eq!(score_item("item-123", "item-123", &it), Some(10_000));
        assert_eq!(score_item("item", "item-123", &it), Some(5_000));
        assert_ne!(score_item("", "item-123", &it), Some(5_000));
    }

    #[test]
    fn test_score_item_field_weighting() {
        let it_title = test_item("target", Some("other_user"), Some("https://other.example"));
        let it_user = test_item("other_title", Some("target"), Some("https://other.example"));
        let it_url = test_item("other_title", Some("other_user"), Some("target"));

        let score_t = score_item("target", "id1", &it_title).unwrap();
        let score_u = score_item("target", "id2", &it_user).unwrap();
        let score_w = score_item("target", "id3", &it_url).unwrap();

        assert_eq!(score_t, 1050);
        assert_eq!(score_u, 1000);
        assert_eq!(score_w, 980);
        assert!(score_t > score_u);
        assert!(score_u > score_w);

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
        let it_b = test_item("beta", Some("user"), None);
        let it_a = test_item("Alpha", Some("user"), None);
        let items = vec![
            ("id_b".to_string(), it_b),
            ("id_a".to_string(), it_a),
        ];
        let ranked = rank("user", items);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].0, ranked[1].0);
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

    #[test]
    fn test_parse_query_empty_and_plain() {
        let q1 = parse_query("");
        assert_eq!(q1.terms, "");
        assert!(q1.filters.is_empty());

        let q2 = parse_query("   ");
        assert_eq!(q2.terms, "");
        assert!(q2.filters.is_empty());

        let q3 = parse_query("github login");
        assert_eq!(q3.terms, "github login");
        assert!(q3.filters.is_empty());
    }

    #[test]
    fn test_parse_query_operators_and_negation() {
        let q = parse_query("tag:work -tag:personal vault:Home -vault:work type:login -type:card is:favorite -is:archived has:otp -has:passkey has:attachment -has:url url:github.com -url:google.com");
        assert_eq!(q.terms, "");
        assert_eq!(
            q.filters,
            vec![
                Filter { negated: false, op: FilterOp::Tag("work".into()) },
                Filter { negated: true, op: FilterOp::Tag("personal".into()) },
                Filter { negated: false, op: FilterOp::Vault("Home".into()) },
                Filter { negated: true, op: FilterOp::Vault("work".into()) },
                Filter { negated: false, op: FilterOp::Type(Category::Login) },
                Filter { negated: true, op: FilterOp::Type(Category::CreditCard) },
                Filter { negated: false, op: FilterOp::IsFavorite },
                Filter { negated: true, op: FilterOp::IsArchived },
                Filter { negated: false, op: FilterOp::HasOtp },
                Filter { negated: true, op: FilterOp::HasPasskey },
                Filter { negated: false, op: FilterOp::HasAttachment },
                Filter { negated: true, op: FilterOp::HasUrl },
                Filter { negated: false, op: FilterOp::Url("github.com".into()) },
                Filter { negated: true, op: FilterOp::Url("google.com".into()) },
            ]
        );
    }

    #[test]
    fn test_parse_query_quoting() {
        let q = parse_query("tag:\"side project\" -tag:'old project' vault:\"Personal Vault\" free text");
        assert_eq!(q.terms, "free text");
        assert_eq!(
            q.filters,
            vec![
                Filter { negated: false, op: FilterOp::Tag("side project".into()) },
                Filter { negated: true, op: FilterOp::Tag("old project".into()) },
                Filter { negated: false, op: FilterOp::Vault("Personal Vault".into()) },
            ]
        );
    }

    #[test]
    fn test_parse_query_unknown_operators_and_values() {
        let q = parse_query("foo:bar -baz:qux is:unknown has:unknown type:nonexistent normal");
        assert_eq!(q.terms, "foo:bar -baz:qux is:unknown has:unknown type:nonexistent normal");
        assert!(q.filters.is_empty());
    }

    #[test]
    fn test_filters_matching() {
        let mut r = ItemRecord::default();
        r.item.title = "GitHub Work".into();
        r.item.url = Some("https://github.com/myorg".into());
        r.category = Category::Login;
        r.tags = vec!["work".into(), "dev".into()];
        r.favorite = true;
        r.archived = false;
        r.item.totp = Some("otpauth://...".into());

        let cand = CandidateInfo::from_record(&r, Some("Work Vault"));

        assert!(parse_query("tag:work").matches(&cand));
        assert!(parse_query("tag:dev").matches(&cand));
        assert!(!parse_query("tag:personal").matches(&cand));
        assert!(parse_query("-tag:personal").matches(&cand));

        assert!(parse_query("vault:work").matches(&cand));
        assert!(!parse_query("vault:home").matches(&cand));
        assert!(parse_query("-vault:home").matches(&cand));

        assert!(parse_query("type:login").matches(&cand));
        assert!(!parse_query("type:card").matches(&cand));

        assert!(parse_query("is:favorite").matches(&cand));
        assert!(!parse_query("is:archived").matches(&cand));
        assert!(parse_query("-is:archived").matches(&cand));

        assert!(parse_query("has:otp").matches(&cand));
        assert!(!parse_query("has:passkey").matches(&cand));
        assert!(parse_query("-has:passkey").matches(&cand));
        assert!(!parse_query("has:attachment").matches(&cand));
        assert!(parse_query("has:url").matches(&cand));

        assert!(parse_query("url:github.com").matches(&cand));
        assert!(!parse_query("url:google.com").matches(&cand));
    }

    #[test]
    fn test_rank_by_candidate_with_filters() {
        let mut r1 = ItemRecord::default();
        r1.item.title = "GitHub Work".into();
        r1.tags = vec!["work".into()];
        r1.category = Category::Login;

        let mut r2 = ItemRecord::default();
        r2.item.title = "GitHub Personal".into();
        r2.tags = vec!["personal".into()];
        r2.category = Category::Login;

        let items = vec![("id1".into(), r1), ("id2".into(), r2)];

        let res = rank_by_candidate("tag:work", items.clone(), |r| CandidateInfo::from_record(r, Some("Personal")));
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].1, "id1");

        let res_personal = rank_by_candidate("tag:personal", items.clone(), |r| CandidateInfo::from_record(r, Some("Personal")));
        assert_eq!(res_personal.len(), 1);
        assert_eq!(res_personal[0].1, "id2");

        let res_free = rank_by_candidate("github", items, |r| CandidateInfo::from_record(r, Some("Personal")));
        assert_eq!(res_free.len(), 2);
    }

    #[test]
    fn url_operator_matches_exact_host_or_dotted_subdomain_only() {
        let q = parse_query("url:github.com");
        assert!(q.matches(&test_item("a", None, Some("https://github.com/login"))));
        assert!(q.matches(&test_item("b", None, Some("https://gist.github.com"))));
        assert!(!q.matches(&test_item("c", None, Some("https://github.com.evil.io"))));
        assert!(!q.matches(&test_item("d", None, Some("https://notgithub.com"))));
        assert!(!parse_query("url:hub.com").matches(&test_item("e", None, Some("https://notgithub.com.evil.io"))));
    }

    #[test]
    fn item_only_rank_ignores_record_operators() {
        let items = vec![
            ("1".to_string(), test_item("GitHub", Some("me"), Some("https://github.com"))),
            ("2".to_string(), test_item("Gmail", Some("me"), None)),
        ];
        // Record-only operators must neither match everything nor nothing on bare items.
        for q in ["type:login git", "-type:login git", "tag:x git", "vault:x git", "is:favorite git"] {
            let r = rank(q, items.clone());
            assert_eq!(r.iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), vec!["1"], "query {q}");
        }
        // Item-evaluable operators still apply.
        assert_eq!(rank("has:url", items.clone()).iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), vec!["1"]);
        assert_eq!(rank("-has:url g", items.clone()).iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), vec!["2"]);
    }
}
