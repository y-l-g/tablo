//! List state: [`TableState`], [`Cursor`], [`Sort`], the URL codec, and the
//! table's signals.
//!
//! The URL query spells list state; [`TableState::from_query`] parses it and one
//! encoder projects every link.

use std::{borrow::Cow, collections::BTreeMap};

use topcoat::{
    context::Cx,
    runtime::{Signal, signal},
};

use crate::{query_term::clamp_query_term, topcoat_compat::href};

/// A table's browser state: the list's query string, the bulk selection, and
/// the write its confirmation dialog asks about.
///
/// The page reads `query` on the server, so writing it in the browser reruns
/// the page with the new list state.
#[derive(Clone)]
pub(crate) struct TableSignals {
    /// The list's URL query, without the leading `?`.
    pub(crate) query: Signal<String>,
    /// The bulk selection as `,key,` tokens; empty selects none.
    pub(crate) bulk: Signal<String>,
    pub(crate) confirm: ConfirmSignals,
}

/// The write the confirmation dialog asks about.
#[derive(Clone)]
pub(crate) struct ConfirmSignals {
    /// The write's POST target; empty while the dialog is closed.
    pub(crate) action: Signal<String>,
    pub(crate) title: Signal<String>,
    /// The confirming button's label.
    pub(crate) label: Signal<String>,
    /// Whether the write takes the bulk selection.
    pub(crate) bulk: Signal<bool>,
}

impl TableSignals {
    /// The signals of the table whose parameters carry `prefix`, `query`
    /// seeded with the request's query as written.
    ///
    /// Keyed by the page's path too: runtime navigation keeps the values of
    /// signals two pages share, so another page's table starts from its own
    /// URL. Creates the signals, so it carries [`topcoat::runtime::signal`]'s
    /// contract: call it while a page body runs.
    pub(crate) fn new(cx: &Cx, prefix: Option<&str>) -> Self {
        let path = topcoat::context::try_request_context::<http::request::Parts>(cx)
            .map(|parts| parts.uri.path().to_string())
            .unwrap_or_default();
        let cx = cx.keyed(("tablo-table", path, prefix.unwrap_or_default()));
        let query = request_query(&cx);
        Self {
            query: signal(&cx, move || query),
            bulk: signal(&cx, String::new),
            confirm: ConfirmSignals {
                action: signal(&cx, String::new),
                title: signal(&cx, String::new),
                label: signal(&cx, String::new),
                bulk: signal(&cx, || false),
            },
        }
    }

    /// The list state `query` spells, read so a change reruns the page.
    pub(crate) fn state(&self, prefix: Option<&str>) -> TableState {
        let query = self.query.get();
        match prefix {
            Some(prefix) => TableState::from_query_prefixed(&query, prefix),
            None => TableState::from_query(&query),
        }
    }
}

/// The selection token for `key` on the bulk wire.
pub(crate) fn bulk_token(key: &str) -> String {
    format!(",{key},")
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
/// Conflicting cursors parse as none and render the first page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cursor {
    /// `?after=` — the page after the encoded row.
    After(String),
    /// `?before=` — the page before the encoded row.
    Before(String),
}

/// Request-scoped table state, parsed from the list's URL query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableState {
    /// The prefix every parameter of this table carries, or `None` for a page's
    /// own list.
    pub prefix: Option<String>,
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
    /// Filter parameters the parse dropped.
    pub filters_dropped: bool,
    /// `?group_by=` — field name to group by (in-memory, `count` summarizer).
    pub group_by: Option<String>,
}

/// The URL parameters one table link projects.
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
}

/// Caps applied filters per query.
pub(crate) const MAX_FILTERS: usize = 32;

/// Caps filter name and value length in bytes.
pub(crate) const MAX_FILTER_LEN: usize = 256;

/// The prefix that names a filter parameter: `?f.status=published`.
const FILTER_PREFIX: &str = "f.";

impl TableState {
    /// Parse the state from the request in `cx`: [`Self::from_query`] over
    /// the request URI's query. Renders without a request context (e.g. unit
    /// tests) get neutral state.
    pub fn from_cx(cx: &Cx) -> Self {
        Self::from_query(&request_query(cx))
    }

    /// [`Self::from_cx`] for the table whose parameters carry `prefix`, on a
    /// page of several.
    pub fn from_cx_prefixed(cx: &Cx, prefix: &str) -> Self {
        Self::from_query_prefixed(&request_query(cx), prefix)
    }

    /// [`Self::from_query`] for the table whose parameters carry `prefix`:
    /// only the parameters spelled `{prefix}.{name}` are read, as `name`, and
    /// the parsed state carries the prefix so its links spell the same names.
    pub fn from_query_prefixed(query: &str, prefix: &str) -> Self {
        let dotted = format!("{prefix}.");
        let own = form_urlencoded::parse(query.as_bytes()).filter_map(|(name, value)| {
            name.strip_prefix(dotted.as_str())
                .map(|name| (Cow::Owned(name.to_string()), value))
        });
        Self {
            prefix: Some(prefix.to_string()),
            ..Self::from_pairs(own)
        }
    }

    /// The URL parameter this table spells `name` as: `name` itself, or
    /// `{prefix}.{name}` for a prefixed table.
    pub(crate) fn param(&self, name: &str) -> String {
        match &self.prefix {
            Some(prefix) => format!("{prefix}.{name}"),
            None => name.to_string(),
        }
    }

    /// The URL parameter the filter `name` travels as: `f.{name}`, prefixed
    /// like every other parameter.
    pub(crate) fn filter_param(&self, name: &str) -> String {
        self.param(&format!("{FILTER_PREFIX}{name}"))
    }

    /// Parse the state from a URL query (without the leading `?`): the one
    /// parser behind the page and its exports.
    ///
    /// A blank or unknown query parses as neutral state rather than failing
    /// the request. A duplicate key keeps its first occurrence, so a repeated
    /// filter never vanishes. A blank or dropped filter occurrence is no
    /// filter, so a later occurrence of the same filter applies (a dropped one
    /// still flags [`Self::filters_dropped`]). `q` is trimmed and clamped to
    /// `MAX_QUERY_TERM`, `dir` is trimmed before comparing, and at most
    /// `MAX_FILTERS` filters of at most `MAX_FILTER_LEN` bytes apply. A cursor
    /// token is checked later, when it decodes.
    ///
    /// A rerun parses a client-owned query, so the parse is linear in its
    /// length: only the known keys are remembered.
    pub fn from_query(query: &str) -> Self {
        Self::from_pairs(form_urlencoded::parse(query.as_bytes()))
    }

    /// The parse behind [`Self::from_query`] and [`Self::from_query_prefixed`],
    /// over the query's decoded pairs with any table prefix already stripped.
    fn from_pairs<'q>(pairs: impl Iterator<Item = (Cow<'q, str>, Cow<'q, str>)>) -> Self {
        let mut state = Self::default();
        let (mut sort, mut dir, mut after, mut before) = (None, None, None, None);
        let mut seen = [false; 6];
        for (key, value) in pairs {
            if let Some(name) = key.strip_prefix(FILTER_PREFIX) {
                let value = value.trim();
                if name.is_empty() || value.is_empty() || state.filters.contains_key(name) {
                    continue;
                }
                if state.filters.len() == MAX_FILTERS
                    || name.len() > MAX_FILTER_LEN
                    || value.len() > MAX_FILTER_LEN
                {
                    state.filters_dropped = true;
                    continue;
                }
                state.filters.insert(name.to_string(), value.to_string());
                continue;
            }
            let slot = match key.as_ref() {
                "q" => 0,
                "sort" => 1,
                "dir" => 2,
                "after" => 3,
                "before" => 4,
                "group_by" => 5,
                // The retired single-parameter filter spelling: a saved link
                // must warn (and its export refuse), not list everything.
                "filters" => {
                    state.filters_dropped |= !value.trim().is_empty();
                    continue;
                }
                _ => continue,
            };
            if std::mem::replace(&mut seen[slot], true) {
                continue;
            }
            let non_empty = || Some(value.trim().to_string()).filter(|v| !v.is_empty());
            match slot {
                0 => state.search = Some(clamp_query_term(&value)).filter(|t| !t.is_empty()),
                1 => sort = non_empty(),
                2 => dir = Some(value.trim() == "desc"),
                3 => after = non_empty(),
                4 => before = non_empty(),
                _ => state.group_by = non_empty(),
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
    /// canonical order `q, sort, dir, f.*, group_by, after|before`. The parser
    /// is first-wins with unique keys, so order is semantically irrelevant.
    ///
    /// Expects `group_by` pre-normalized: render seams normalize through
    /// [`Table::normalize_state`](crate::table::Table::normalize_state), so
    /// the projection echoes `state.group_by` as-is.
    ///
    /// Full state, including the cursor: the retry link for failures that
    /// keep their evidence.
    pub(crate) fn list_url(&self, path: &str) -> String {
        href::with_query(path, &self.query())
    }

    /// Drops `q` (and the cursor of its result set); keeps the filters.
    pub(crate) fn without_search(&self, path: &str) -> String {
        let projection = UrlProjection {
            search: None,
            cursor: None,
            ..self.full()
        };
        href::with_query(path, &self.project(projection))
    }

    /// Drops the filters (and the cursor of their result set); keeps the
    /// search term.
    pub(crate) fn without_filters(&self, path: &str) -> String {
        let projection = UrlProjection {
            filters: false,
            cursor: None,
            ..self.full()
        };
        href::with_query(path, &self.project(projection))
    }

    /// Drops the cursor; keeps everything else. Back-to-first-page and the
    /// cursor-failure retry link.
    pub(crate) fn without_cursor(&self, path: &str) -> String {
        let projection = UrlProjection {
            cursor: None,
            ..self.full()
        };
        href::with_query(path, &self.project(projection))
    }

    /// Full state with `cursor` in place of the current one: the pager's
    /// links.
    pub(crate) fn with_cursor(&self, path: &str, cursor: &Cursor) -> String {
        let projection = UrlProjection {
            cursor: Some(cursor),
            ..self.full()
        };
        href::with_query(path, &self.project(projection))
    }

    /// Replaces `sort`/`dir`, drops the cursor: a new ordering is a new
    /// result set.
    pub(crate) fn sorted_by(&self, path: &str, column: &str, descending: bool) -> String {
        let projection = UrlProjection {
            sort: Some((column, if descending { "desc" } else { "asc" })),
            cursor: None,
            ..self.full()
        };
        href::with_query(path, &self.project(projection))
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
        }
    }

    /// The one encoder: every table link's parameter vocabulary lives here.
    ///
    /// The exhaustive destructure fails compilation when a field is added to
    /// `TableState`, forcing the author to decide where it projects.
    fn project(&self, projection: UrlProjection<'_>) -> String {
        let TableState {
            prefix: _,
            search: _,
            sort: _,
            cursor: _,
            filters: _,
            filters_dropped: _,
            group_by: _,
        } = self;
        let filters = projection
            .filters
            .then_some(&self.filters)
            .into_iter()
            .flatten()
            .map(|(name, value)| (self.filter_param(name), Some(value.as_str())));
        let cursor = match projection.cursor {
            Some(Cursor::After(token)) => ("after", Some(token.as_str())),
            Some(Cursor::Before(token)) => ("before", Some(token.as_str())),
            None => ("after", None),
        };
        let pairs: Vec<(String, &str)> = [
            (self.param("q"), projection.search),
            (
                self.param("sort"),
                projection.sort.map(|(column, _)| column),
            ),
            (self.param("dir"), projection.sort.map(|(_, dir)| dir)),
        ]
        .into_iter()
        .chain(filters)
        .chain([
            (self.param("group_by"), projection.group_by),
            (self.param(cursor.0), cursor.1),
        ])
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect();
        href::encode_query(&pairs)
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
/// primary key instead.
pub(crate) const RECORD_ROUTE_PARAM: &str = "{id}";

/// Path segment of the list page's create page.
pub(crate) const CREATE_ROUTE_SEGMENT: &str = "create";

/// Path segment of a row's edit page.
pub(crate) const EDIT_ROUTE_SEGMENT: &str = "edit";

/// Path segment of a row's delete POST.
pub(crate) const DELETE_ROUTE_SEGMENT: &str = "delete";

/// Path segment of the bulk-delete POST.
pub(crate) const BULK_DELETE_ROUTE_SEGMENT: &str = "bulk-delete";

/// Path segment before a custom action's name, on a row
/// (`{prefix}/{key}/-/actions/{name}`) and on the list
/// (`{list}/-/actions/{name}`).
pub(crate) const ACTIONS_ROUTE_SEGMENT: &str = "actions";

/// Static segment guarding custom action routes from record keys.
pub(crate) const DASH_ROUTE_SEGMENT: &str = "-";

/// The action-name placeholder the route table registers: `{action}`.
pub(crate) const ACTION_ROUTE_PARAM: &str = "{action}";

/// The row's `Edit` link: `{prefix}/{key}/edit`.
pub(crate) fn row_edit_url(prefix: &str, key: &str) -> String {
    format!(
        "{prefix}/{}/{EDIT_ROUTE_SEGMENT}",
        href::encode_path_segment(key)
    )
}

/// The row's `View` link: `{prefix}/{key}` — the detail page.
pub(crate) fn row_view_url(prefix: &str, key: &str) -> String {
    format!("{prefix}/{}", href::encode_path_segment(key))
}

/// The row delete form's POST target: `{prefix}/{key}/delete`.
pub(crate) fn delete_action_url(prefix: &str, key: &str) -> String {
    format!(
        "{prefix}/{}/{DELETE_ROUTE_SEGMENT}",
        href::encode_path_segment(key)
    )
}

/// A row action's POST target: `{prefix}/{key}/-/actions/{name}`.
pub(crate) fn row_action_url(prefix: &str, key: &str, name: &str) -> String {
    format!(
        "{prefix}/{}/{DASH_ROUTE_SEGMENT}/{ACTIONS_ROUTE_SEGMENT}/{name}",
        href::encode_path_segment(key)
    )
}

/// A bulk action's POST target: `{list_path}/-/actions/{name}`.
pub(crate) fn bulk_action_url(list_path: &str, name: &str) -> String {
    format!("{list_path}/{DASH_ROUTE_SEGMENT}/{ACTIONS_ROUTE_SEGMENT}/{name}")
}

/// The list page's create link: `{list_path}/create`.
pub(crate) fn create_page_url(list_path: &str) -> String {
    format!("{list_path}/{CREATE_ROUTE_SEGMENT}")
}

/// The bulk form's POST target: `{list_path}/bulk-delete`.
pub(crate) fn bulk_delete_url(list_path: &str) -> String {
    format!("{list_path}/{BULK_DELETE_ROUTE_SEGMENT}")
}

/// The query parameter naming the page a write lands on after it commits,
/// in place of the resource's list: `?return=/admin/posts/1`. The panel
/// honours it only for a path under its own prefix.
pub(crate) const RETURN_PARAM: &str = "return";

/// `url`, which carries no query, with `?return={target}`.
pub(crate) fn with_return(url: &str, target: &str) -> String {
    format!("{url}?{RETURN_PARAM}={}", href::encode_query_value(target))
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

/// The request's URL query, without the leading `?`; empty without a
/// request context.
pub(crate) fn request_query(cx: &Cx) -> String {
    topcoat::context::try_request_context::<http::request::Parts>(cx)
        .and_then(|parts| parts.uri.query().map(str::to_string))
        .unwrap_or_default()
}

/// The query part of a URL this module built: what follows the `?`, or empty.
pub(crate) fn query_of(url: &str) -> &str {
    url.split_once('?').map_or("", |(_, query)| query)
}

#[cfg(test)]
mod tests;
