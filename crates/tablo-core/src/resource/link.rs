//! A record's public page, as its resource declares it.

/// A record's public page, linked from its detail and edit pages.
#[derive(Debug, Clone)]
pub struct PublicLink {
    /// The public page's URL.
    pub url: String,
    /// The link's text.
    pub label: &'static str,
}
