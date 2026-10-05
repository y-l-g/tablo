//! What Tablo builds on Topcoat's internals because no public API offers it yet.
//!
//! Each item names the upstream gap it fills; it retires when Topcoat closes that gap.
//!
//! - [`async_page`]: a fallible async page body as a view (upstream #123).
//! - [`href`]: runtime-path URL building through percent-encoding (upstream #399).

use std::future::Future;

use topcoat::view::{BoxView, HoistView, View, internal::ThenView};

/// A fallible async page body as a hoisted view.
pub(crate) fn async_page<'a, F, V>(future: F) -> BoxView<'a>
where
    F: Future<Output = topcoat::Result<V>> + Send + 'a,
    V: View + 'a,
{
    Box::pin(HoistView::new(ThenView::new(future)))
}

/// Runtime-path URL building Topcoat's `HrefTarget` does not cover.
pub(crate) mod href {
    use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

    /// Every byte outside the RFC 3986 `unreserved` set (`A-Z a-z 0-9 - _ . ~`)
    /// is percent-encoded in a query value or path segment.
    const NON_UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');

    /// Percent-encode a query parameter value (`unreserved` RFC 3986 set passes).
    pub(crate) fn encode_query_value(value: &str) -> String {
        utf8_percent_encode(value, NON_UNRESERVED).to_string()
    }

    /// Percent-encode a single path segment.
    ///
    /// Row keys are `String` by contract, so `/`, `?`, `#`, `%`, `+` inside a key
    /// must not rewrite the action URL. Topcoat's `path_param_segment` returns
    /// the percent-decoded segment, so this round-trips; UUID keys pass through
    /// unchanged.
    pub(crate) fn encode_path_segment(value: &str) -> String {
        encode_query_value(value)
    }

    /// Encode ordered `key=value` pairs as a URL query, without the leading `?`.
    pub(crate) fn encode_query(pairs: &[(String, &str)]) -> String {
        pairs
            .iter()
            .map(|(key, value)| {
                format!("{}={}", encode_query_value(key), encode_query_value(value))
            })
            .collect::<Vec<_>>()
            .join("&")
    }

    /// `path?query`, or `path` alone for an empty query.
    pub(crate) fn with_query(path: &str, query: &str) -> String {
        if query.is_empty() {
            path.to_string()
        } else {
            format!("{path}?{query}")
        }
    }
}
