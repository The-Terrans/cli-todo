use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};

/// Keeps the items whose key fuzzy-matches every word of `query`, best match first.
/// Matching ignores case. Ties keep input order, so a blank query returns every item unchanged.
pub fn rank<K: AsRef<str>, T>(query: &str, items: impl IntoIterator<Item = (K, T)>) -> Vec<T> {
    let pattern = Pattern::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buffer = Vec::new();
    let mut scored: Vec<_> = items
        .into_iter()
        .filter_map(|(key, item)| {
            let haystack = Utf32Str::new(key.as_ref(), &mut buffer);
            pattern
                .score(haystack, &mut matcher)
                .map(|score| (score, item))
        })
        .collect();
    // Stable sort: equal scores stay in Action, Project, Task order.
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, item)| item).collect()
}
