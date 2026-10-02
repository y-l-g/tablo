//! Bounds the search term the `?q=` transport and option search share.

/// Longest accepted search term, in chars.
pub(crate) const MAX_QUERY_TERM: usize = 128;

/// Clamp a search term to [`MAX_QUERY_TERM`] chars (chars, not bytes, so a
/// multibyte term truncates on boundaries).
pub(crate) fn clamp_query_term(term: &str) -> String {
    term.trim().chars().take(MAX_QUERY_TERM).collect()
}
