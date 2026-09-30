//! List state: [`TableState`], [`Cursor`], [`Sort`], the URL codec, and the
//! live table's signals.
//!
//! The URL query is the one spelling of list state: the GET page and the live
//! shard parse it with [`TableState::from_query`], and every link projects it
//! back through one encoder.

use std::collections::BTreeMap;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use topcoat::{
    context::Cx,
    runtime::{Signal, signal},
};

use crate::query_term::clamp_query_term;

/// The live table's browser state: the list's query string and the bulk
/// selection.
///
/// The page owns these signals and hands their handles to the `table_search`
/// shard; each tracked read inside the shard becomes a `dep` marker the
/// browser watches, so writing either re-renders the table in place, with no
/// navigation and no scroll jump.
///
/// `query` is the URL query every control already spells in its `href`: a sort
/// link, a pager link or a clear link writes its own `href` query, and the
/// search and filter scripts edit their own keys of the current value. The
/// shard parses it with [`TableState::from_query`], the GET path's parser, so
/// the live table and the page cannot disagree about what a query means.
/// `bulk` is the selection, which is not URL state and survives a rerun.
///
/// Both values are untrusted by the time the shard reads them back: the client
/// owns the signals.
#[derive(Clone)]
pub(crate) struct TableSignals {
    /// The list's URL query, without the leading `?`.
    pub(crate) query: Signal<String>,
    /// The bulk selection: comma-delimited record keys (`,a,b,`), empty when
    /// nothing is selected. Row checkboxes carry no `checked` attribute:
    /// `bulk.js` sets `checked` from the transport after every swap and
    /// change, and the script writes the transport, whose bound `change`
    /// handler writes this signal, so a rerun re-renders the boxes from the
    /// selection instead of dropping it. The shard carries the handle without
    /// reading it: a checkbox click must not reload rows.
    pub(crate) bulk: Signal<String>,
}

/// Test helper: whether the comma-delimited selection wire names `key`.
///
/// The wire is `,a,b,`-delimited on both ends so membership is an exact
/// segment match, not a substring test that would confuse `b` with `ab`.
/// [`parse_bulk_ids`](crate::panel) already ignores the empty segments the
/// delimiters produce, so the same wire is the form transport.
#[cfg(test)]
pub(crate) fn bulk_wire_contains(wire: &str, key: &str) -> bool {
    wire.split(',').any(|segment| segment == key)
}

/// Which column the table is currently sorted by, parsed from
/// `?sort=<column>&dir=asc|desc`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Sort {
    /// The app-level field name of the column (matches `TextColumn::name`).
    pub column: String,
    /// `true` for `dir=desc`.
    pub descending: bool,
}

/// A pagination cursor: the page after, or the page before, an encoded row.
///
/// Toasty pages from one cursor at a time, so one value holds it: a URL that
/// names both `?after=` and `?before=` parses as no cursor, the first page,
/// which is where the cursor retry lands anyway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cursor {
    /// `?after=` — the page after the encoded row.
    After(String),
    /// `?before=` — the page before the encoded row.
    Before(String),
}

impl Cursor {
    /// The encoded token.
    pub fn token(&self) -> &str {
        match self {
            Self::After(token) | Self::Before(token) => token,
        }
    }
}

/// Request-scoped table state, parsed from the list's URL query.
///
/// The single parse point shared by loaders (the search term, ordering via
/// `Table::order_bys_for`) and render (active sort, toolbar values,
/// pagination links), so the URL is the one truth for list state. The fixed
/// parameter names assume one table per page — per-table prefixes are deferred
/// until a real page needs two tables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableState {
    /// `?q=` — trimmed and clamped to `MAX_QUERY_TERM` chars; `None` when
    /// absent or blank.
    pub search: Option<String>,
    /// `?sort=` + `?dir=` — `None` when absent or blank.
    pub sort: Option<Sort>,
    /// `?after=` or `?before=`.
    pub cursor: Option<Cursor>,
    /// `?f.<name>=<value>`, one parameter per active filter, by name. A blank
    /// value is no filter.
    pub filters: BTreeMap<String, String>,
    /// More than `MAX_FILTERS` filter parameters arrived and the rest were
    /// dropped: `Table::unapplied_filters` reports it, so the list warns and
    /// the export refuses instead of exporting an over-broad CSV.
    pub filters_overflow: bool,
    /// `?group_by=` — field name to group by (in-memory, `count` summarizer).
    pub group_by: Option<String>,
    /// `?delete=` — the row key whose delete confirmation dialog opens on the
    /// list page. The dialog's confirmed POST re-enters the delete
    /// route; the parameter itself is never a write.
    pub delete: Option<String>,
    /// `?open=false` — set by `dialog.js` when Escape/backdrop dismisses the
    /// delete dialog, so the next render stays closed. Absent (or `true`)
    /// renders it open.
    pub open: Option<bool>,
}

/// The URL parameters one table link projects, named so a call site reads
/// which intent drops what.
///
/// `Default` is the projection that drops everything: a caller names only the
/// parameters it keeps.
#[derive(Default)]
struct UrlProjection<'a> {
    /// `?q=` search term.
    search: Option<&'a str>,
    /// `?sort=` column and `?dir=` value.
    sort: Option<(&'a str, &'a str)>,
    /// The `?f.<name>=` filters.
    filters: bool,
    /// `?group_by=` column.
    group_by: Option<&'a str>,
    /// `?after=` or `?before=`.
    cursor: Option<&'a Cursor>,
    /// `?delete=` row key for the confirmation dialog.
    delete: Option<&'a str>,
}

/// Most filters one query applies. Every filter is a map entry that every
/// rebuilt URL echoes, and the live shard parses a client-owned query, so the
/// count is bounded where the query is parsed.
pub(crate) const MAX_FILTERS: usize = 32;

/// The prefix that names a filter parameter: `?f.status=published`.
const FILTER_PREFIX: &str = "f.";

/// The URL parameter a filter named `name` travels as.
pub(crate) fn filter_param(name: &str) -> String {
    format!("{FILTER_PREFIX}{name}")
}

impl TableState {
    /// Parse the state from the request in `cx`: [`Self::from_query`] over
    /// the request URI's query. Renders without a request context (e.g. unit
    /// tests) get neutral state.
    pub fn from_cx(cx: &Cx) -> Self {
        let Some(parts) = topcoat::context::try_request_context::<http::request::Parts>(cx) else {
            return Self::default();
        };
        Self::from_query(parts.uri.query().unwrap_or(""))
    }

    /// Parse the state from a URL query (without the leading `?`): the one
    /// parser behind the GET page and the live shard.
    ///
    /// A blank or unknown query parses as neutral state rather than failing
    /// the request. A duplicate key keeps its first occurrence, so a repeated
    /// filter never vanishes. `q` is trimmed and clamped to
    /// `MAX_QUERY_TERM`, `dir` is trimmed
    /// before comparing, and at most `MAX_FILTERS` filters apply. A cursor
    /// token is checked later, when it decodes.
    pub fn from_query(query: &str) -> Self {
        let mut state = Self::default();
        let mut seen: Vec<String> = Vec::new();
        let (mut sort, mut dir, mut after, mut before) = (None, None, None, None);
        for (key, value) in form_urlencoded::parse(query.as_bytes()) {
            if let Some(name) = key.strip_prefix(FILTER_PREFIX) {
                let value = value.trim();
                if name.is_empty() || value.is_empty() || state.filters.contains_key(name) {
                    continue;
                }
                if state.filters.len() == MAX_FILTERS {
                    state.filters_overflow = true;
                    continue;
                }
                state.filters.insert(name.to_string(), value.to_string());
                continue;
            }
            if seen.iter().any(|k| *k == key) {
                continue;
            }
            seen.push(key.to_string());
            let non_empty = || Some(value.trim().to_string()).filter(|v| !v.is_empty());
            match key.as_ref() {
                "q" => state.search = Some(clamp_query_term(&value)).filter(|t| !t.is_empty()),
                "sort" => sort = non_empty(),
                "dir" => dir = Some(value.trim() == "desc"),
                "after" => after = non_empty(),
                "before" => before = non_empty(),
                "group_by" => state.group_by = non_empty(),
                // The delete dialog is opt-in through `?delete=`;
                // `?open=false` is the dismissal mirror `dialog.js` writes.
                // Any other `open` value stays neutral (open).
                "delete" => state.delete = non_empty(),
                "open" => {
                    state.open = match value.as_ref() {
                        "false" => Some(false),
                        "true" => Some(true),
                        _ => None,
                    }
                }
                _ => {}
            }
        }
        state.sort = sort.map(|column| Sort {
            column,
            descending: dir.unwrap_or(false),
        });
        state.cursor = match (after, before) {
            (Some(token), None) => Some(Cursor::After(token)),
            (None, Some(token)) => Some(Cursor::Before(token)),
            _ => None,
        };
        state
    }

    /// Seed the live page's signals from this state: `query` from the state's
    /// full query, the selection empty.
    ///
    /// Call it with the *parsed* state, before normalizing: an unknown
    /// `?group_by=` seeds the query as written, and the shard's normalizer
    /// drops it on the way back in, exactly as the GET path does.
    ///
    /// Creates the signals, so it carries [`topcoat::runtime::signal`]'s
    /// contract: call it while a view is collecting signal declarations — the
    /// panel calls it from the live page's render, and the declarations ride
    /// that page's hoisted parts.
    pub(crate) fn to_signals(&self, cx: &Cx) -> TableSignals {
        TableSignals {
            query: signal(cx, || self.query()),
            bulk: signal(cx, String::new),
        }
    }

    /// The state's full query: every parameter [`Self::list_url`] carries,
    /// without the path or the leading `?`.
    pub(crate) fn query(&self) -> String {
        self.project(self.full())
    }

    /// URL projection: `TableState` owns the table's URL vocabulary.
    /// Callers ask for a user intent, never a parameter list, so adding a
    /// parameter cannot silently drop it from half the links.
    ///
    /// One private encoder ([`Self::project`]) holds the vocabulary in
    /// canonical order `q, sort, dir, f.*, group_by, after|before` (`delete`
    /// appended by its intent). The parser is first-wins with unique keys, so
    /// order is semantically irrelevant.
    ///
    /// Expects `group_by` pre-normalized: render seams normalize through
    /// [`Table::normalize_state`](crate::resource::Table::normalize_state), so
    /// the projection echoes `state.group_by` as-is. `open` is never emitted by
    /// any link; `delete` only by [`Self::row_url_base`]'s dialog intent.
    ///
    /// Full state, including the cursor; never `delete`/`open`. The streamed
    /// retry link for failures that keep their evidence.
    pub(crate) fn list_url(&self, path: &str) -> String {
        with_query(path, &self.query())
    }

    /// Drops `q` (and the cursor + dialog of its result set); keeps the
    /// filters.
    pub(crate) fn without_search(&self, path: &str) -> String {
        let projection = UrlProjection {
            search: None,
            cursor: None,
            ..self.full()
        };
        with_query(path, &self.project(projection))
    }

    /// Drops the filters (and the cursor + dialog of their result set); keeps
    /// the search term.
    pub(crate) fn without_filters(&self, path: &str) -> String {
        let projection = UrlProjection {
            filters: false,
            cursor: None,
            ..self.full()
        };
        with_query(path, &self.project(projection))
    }

    /// Drops the cursor; keeps everything else. Back-to-first-page and the
    /// cursor-failure retry link.
    pub(crate) fn without_cursor(&self, path: &str) -> String {
        let projection = UrlProjection {
            cursor: None,
            ..self.full()
        };
        with_query(path, &self.project(projection))
    }

    /// Full state with `cursor` in place of the current one, and no dialog:
    /// the pager's links.
    pub(crate) fn with_cursor(&self, path: &str, cursor: &Cursor) -> String {
        let projection = UrlProjection {
            cursor: Some(cursor),
            ..self.full()
        };
        with_query(path, &self.project(projection))
    }

    /// Replaces `sort`/`dir`, drops the cursor and the dialog: a new ordering
    /// is a new result set.
    pub(crate) fn sorted_by(&self, path: &str, column: &str, descending: bool) -> String {
        let projection = UrlProjection {
            sort: Some((column, if descending { "desc" } else { "asc" })),
            cursor: None,
            ..self.full()
        };
        with_query(path, &self.project(projection))
    }

    /// The shared parameters of every row-action URL on one page, encoded
    /// once.
    ///
    /// A row's action URL is this base plus the row's record key, so a table
    /// render pays for the projection once, however many rows the page holds.
    /// Build it before the row loop and call [`RowUrlBase::delete_dialog`] per
    /// row; that pair is the full-state-plus-`delete` projection, which keeps
    /// the cursor and never emits `open`.
    pub(crate) fn row_url_base(&self, path: &str) -> RowUrlBase {
        RowUrlBase(self.list_url(path))
    }

    /// `?sort=` column + `?dir=` value for the projection.
    fn sort_pair(&self) -> Option<(&str, &str)> {
        self.sort
            .as_ref()
            .map(|s| (s.column.as_str(), if s.descending { "desc" } else { "asc" }))
    }

    /// The projection that keeps every link parameter.
    fn full(&self) -> UrlProjection<'_> {
        UrlProjection {
            search: self.search.as_deref(),
            sort: self.sort_pair(),
            filters: true,
            group_by: self.group_by.as_deref(),
            cursor: self.cursor.as_ref(),
            delete: None,
        }
    }

    /// The one encoder: every table link's parameter vocabulary lives here.
    ///
    /// The exhaustive destructure fails compilation when a field is added to
    /// `TableState`, forcing the author to decide where it projects.
    fn project(&self, projection: UrlProjection<'_>) -> String {
        let TableState {
            search: _,
            sort: _,
            cursor: _,
            filters: _,
            filters_overflow: _,
            group_by: _,
            delete: _,
            open: _,
        } = self;
        let filters = projection
            .filters
            .then_some(&self.filters)
            .into_iter()
            .flatten()
            .map(|(name, value)| (filter_param(name), Some(value.as_str())));
        let cursor = match projection.cursor {
            Some(Cursor::After(token)) => ("after", Some(token.as_str())),
            Some(Cursor::Before(token)) => ("before", Some(token.as_str())),
            None => ("after", None),
        };
        let pairs: Vec<(String, &str)> = [
            ("q".to_string(), projection.search),
            (
                "sort".to_string(),
                projection.sort.map(|(column, _)| column),
            ),
            ("dir".to_string(), projection.sort.map(|(_, dir)| dir)),
        ]
        .into_iter()
        .chain(filters)
        .chain([
            ("group_by".to_string(), projection.group_by),
            (cursor.0.to_string(), cursor.1),
            ("delete".to_string(), projection.delete),
        ])
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect();
        encode_query(&pairs)
    }
}

/// One page's shared row-action URL parameters, encoded once.
///
/// The base is [`TableState::list_url`] — every parameter a row's action URL
/// shares — so the filter transport is encoded once per render, not once per
/// row. Row-specific intents ([`Self::delete_dialog`]) append to it in the
/// projection's own order.
pub(crate) struct RowUrlBase(String);

impl RowUrlBase {
    /// The `?delete=<key>` confirmation-dialog opener for one row.
    ///
    /// `self.0` is [`TableState::list_url`]'s output, which never carries
    /// `delete`, and `delete` is the projection's last parameter — so this is
    /// byte-for-byte what the one-pass projection builds, without re-encoding
    /// the parameters it shares with the rest of the page.
    pub(crate) fn delete_dialog(&self, key: &str) -> String {
        let separator = if self.0.contains('?') { '&' } else { '?' };
        format!("{}{separator}delete={}", self.0, encode_query_value(key))
    }
}

// Action URL shapes.
//
// `TableState` owns every table link's parameter vocabulary; these own the
// *path* shapes, so a route change has one edit site per shape instead of a
// hand-formatted `format!` at each render seam. The segment literals are shared
// with the panel's route table (`Panel::resource`), so the routes the panel
// registers and the links the table emits are spelled once.

/// The record placeholder the route table registers: `{id}`.
///
/// A route *pattern*, not a URL — the link helpers below take the encoded
/// record key instead.
pub(crate) const RECORD_ROUTE_PARAM: &str = "{id}";

/// Path segment of the list page's create page.
pub(crate) const CREATE_ROUTE_SEGMENT: &str = "create";

/// Path segment of a row's edit page.
pub(crate) const EDIT_ROUTE_SEGMENT: &str = "edit";

/// Path segment of a row's delete POST.
pub(crate) const DELETE_ROUTE_SEGMENT: &str = "delete";

/// Path segment of the bulk-delete POST.
pub(crate) const BULK_DELETE_ROUTE_SEGMENT: &str = "bulk-delete";

/// The row's `Edit` link: `{prefix}/{key}/edit`.
pub(crate) fn row_edit_url(prefix: &str, key: &str) -> String {
    format!("{prefix}/{}/{EDIT_ROUTE_SEGMENT}", encode_path_segment(key))
}

/// The row's `View` link: `{prefix}/{key}` — the detail page.
pub(crate) fn row_view_url(prefix: &str, key: &str) -> String {
    format!("{prefix}/{}", encode_path_segment(key))
}

/// The row delete form's POST target: `{prefix}/{key}/delete`.
pub(crate) fn delete_action_url(prefix: &str, key: &str) -> String {
    format!(
        "{prefix}/{}/{DELETE_ROUTE_SEGMENT}",
        encode_path_segment(key)
    )
}

/// The list page's create link: `{list_path}/create`.
pub(crate) fn create_page_url(list_path: &str) -> String {
    format!("{list_path}/{CREATE_ROUTE_SEGMENT}")
}

/// The bulk form's POST target: `{list_path}/bulk-delete`.
pub(crate) fn bulk_delete_url(list_path: &str) -> String {
    format!("{list_path}/{BULK_DELETE_ROUTE_SEGMENT}")
}

/// Every byte outside the RFC 3986 `unreserved` set (`A-Z a-z 0-9 - _ . ~`)
/// is percent-encoded in a query value or path segment.
const NON_UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// Percent-encode a query parameter value (`unreserved` RFC 3986 set passes).
fn encode_query_value(value: &str) -> String {
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

/// FNV-1a (32-bit): stable across runs and Rust versions, no dependency.
/// Used only to disambiguate DOM ids, never for anything security-relevant.
fn fnv1a_32(s: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in s.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Stable DOM id for a table row: Topcoat's morph (#392) follows
/// elements by `id` across reruns, so reorderable row content needs one in
/// addition to the keyed-diff `key:`. Derived from the row key (stable for
/// the record, unlike a loop index), sanitized to an HTML-safe token plus a
/// short hash: distinct keys (`Ada Lovelace`, `Ada-Lovelace`) can sanitize to
/// the same token, and duplicate DOM ids would make the morph follow one row.
pub(crate) fn row_dom_id(key: &str) -> String {
    dom_id("row", key)
}

/// Stable DOM id for a page-local group header.
///
/// Same contract as [`row_dom_id`]: the header is injected, removed and moved
/// as the page is re-sorted, so the in-place morph needs an id derived from the
/// group label it belongs to rather than from its position in the page.
pub(crate) fn group_header_dom_id(label: &str) -> String {
    dom_id("group", label)
}

/// The one sanitizer behind both ids: `{prefix}-{token}-{hash}`.
fn dom_id(prefix: &str, key: &str) -> String {
    let mut out = String::with_capacity(prefix.len() + key.len() + 14);
    out.push_str(prefix);
    out.push('-');
    for c in key.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.') {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    out.push_str(&format!("-{:08x}", fnv1a_32(key)));
    out
}

/// Encode ordered `key=value` pairs as a URL query, without the leading `?`.
fn encode_query(pairs: &[(String, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", encode_query_value(key), encode_query_value(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// `path?query`, or `path` alone for an empty query.
fn with_query(path: &str, query: &str) -> String {
    if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    }
}

/// The query part of a URL this module built: what follows the `?`, or empty.
pub(crate) fn query_of(url: &str) -> &str {
    url.split_once('?').map_or("", |(_, query)| query)
}

#[cfg(test)]
mod tests;
