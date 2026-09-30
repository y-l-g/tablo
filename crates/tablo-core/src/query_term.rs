//! The bounded search term the `?q=` transport and the option search share.
//!
//! A leaf: `resource` (the list state's transport parse and the live-search
//! shard) and `schema` (relationship option search) both clamp through here,
//! and neither depends on the other for it.

/// Longest search term accepted (`?q=`): bounded echoed state. Applied by
/// [`TableState::from_query`], so the GET path and the live shard clamp alike,
/// and by the relationship option search so a keystroke burst cannot grow the
/// pattern past the same bound.
///
/// [`TableState::from_query`]: crate::resource::TableState::from_query
pub(crate) const MAX_QUERY_TERM: usize = 128;

/// Clamp a search term to [`MAX_QUERY_TERM`] chars (chars, not bytes, so a
/// multibyte term truncates on boundaries).
pub(crate) fn clamp_query_term(term: &str) -> String {
    term.trim().chars().take(MAX_QUERY_TERM).collect()
}
