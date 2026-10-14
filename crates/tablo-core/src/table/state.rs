//! List state: [`TableState`], [`Cursor`], [`Sort`], the URL codec, and the
//! table's signals.
//!
//! The URL query spells list state; [`TableState::from_query`] parses it and one
//! encoder projects every link.

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
};

use topcoat::{
    context::Cx,
    runtime::{Signal, signal},
};

use super::{TableTarget, filter::FilterValue};
use crate::topcoat_compat::href;

/// A table's browser state: the list's query string, the bulk selection, and
/// the write its confirmation dialog asks about.
///
/// The page reads `query` on the server, so writing it in the browser reruns
/// the page with the new list state.
#[derive(Clone)]
pub(crate) struct TableSignals {
    /// The list's URL query, without the leading `?`.
    pub(crate) query: Signal<String>,
    /// The bulk selection as `,token,` entries, each token's key percent-encoded ([`bulk_token`]
    /// spells one); empty selects none.
    pub(crate) bulk: Signal<String>,
    pub(crate) confirm: ConfirmSignals,
    /// Where each custom action asking for input posts its dialog, by the action's name.
    pub(crate) inputs: HashMap<&'static str, TableTarget>,
    /// The page's path and the table's prefix, which key the signals.
    key: (String, String),
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
        let key = (path, prefix.unwrap_or_default().to_string());
        let cx = cx.keyed(("tablo-table", key.0.as_str(), key.1.as_str()));
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
            inputs: HashMap::new(),
            key,
        }
    }

    /// These signals, with the target of the input dialog of each action in `names`.
    ///
    /// Creates the signals, so it carries [`TableSignals::new`]'s contract.
    pub(crate) fn with_inputs(
        mut self,
        cx: &Cx,
        names: impl Iterator<Item = &'static str>,
    ) -> Self {
        for name in names {
            let cx = cx.keyed((
                "tablo-table-input",
                self.key.0.as_str(),
                self.key.1.as_str(),
                name,
            ));
            let target = TableTarget {
                action: signal(&cx, String::new),
                bulk: signal(&cx, || false),
            };
            self.inputs.insert(name, target);
        }
        self
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

/// The selection token for `key` on the bulk wire: the key percent-encoded between the commas that
/// delimit it, so a key holding the wire's separator stays one token.
pub(crate) fn bulk_token(key: &str) -> String {
    format!(",{},", href::encode_query_value(key))
}

/// The key a [`bulk_token`]'s body spells: the inverse of the encoding it applies.
///
/// A body the codec cannot read, an invalid escape or bytes that decode to no UTF-8, reads as
/// written, so a posted id holding no escape keeps its literal text.
pub(crate) fn bulk_key(body: &str) -> String {
    href::decode_query_value(body)
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
    /// The active filters, by name: `?f.<name>=<value>`, repeated for each
    /// value, and `?f.<name>.<part>=<value>` for each part. The parse keeps
    /// no filter holding no value.
    pub filters: BTreeMap<String, FilterValue>,
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

/// Longest accepted search term, in chars; the `?q=` transport and option search share it.
pub(crate) const MAX_QUERY_TERM: usize = 128;

/// Clamps a search term to [`MAX_QUERY_TERM`] chars, so a multibyte term truncates on a char
/// boundary.
pub(crate) fn clamp_query_term(term: &str) -> String {
    term.trim().chars().take(MAX_QUERY_TERM).collect()
}

/// Caps applied filters per query.
pub(crate) const MAX_FILTERS: usize = 32;

/// Caps the values and parts one filter holds.
pub(crate) const MAX_FILTER_VALUES: usize = 64;

/// Caps filter name, part and value length in bytes.
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
    /// the request. A duplicate key keeps its first occurrence, except a
    /// filter's value: each `f.<name>` occurrence adds one, in order, and a
    /// repeated value or part keeps its first occurrence. A blank or dropped
    /// occurrence adds nothing (a dropped one flags [`Self::filters_dropped`]).
    /// `q` is trimmed and clamped to `MAX_QUERY_TERM`, `dir` is trimmed before
    /// comparing, and at most `MAX_FILTERS` filters of at most
    /// `MAX_FILTER_VALUES` values and parts, each name, part and value at most
    /// `MAX_FILTER_LEN` bytes, apply. A cursor token is checked later, when it
    /// decodes.
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
            if let Some(filter) = key.strip_prefix(FILTER_PREFIX) {
                state.add_filter(filter, value.trim());
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

    /// Add one `f.{filter}={value}` occurrence, `filter` naming the filter
    /// and, after its first `.`, the part.
    fn add_filter(&mut self, filter: &str, value: &str) {
        let (name, part) = match filter.split_once('.') {
            Some((name, part)) => (name, Some(part)),
            None => (filter, None),
        };
        let held = self.filters.get(name);
        if name.is_empty()
            || part == Some("")
            || value.is_empty()
            || held.is_some_and(|held| held.holds(part, value))
        {
            return;
        }
        let room = match held {
            Some(held) => held.len() < MAX_FILTER_VALUES,
            None => self.filters.len() < MAX_FILTERS,
        };
        if !room
            || [name, part.unwrap_or_default(), value]
                .iter()
                .any(|s| s.len() > MAX_FILTER_LEN)
        {
            self.filters_dropped = true;
            return;
        }
        let held = self.filters.entry(name.to_string()).or_default();
        match part {
            Some(part) => held.insert_part(part, value),
            None => held.push(value),
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
    /// canonical order `q, sort, dir, f.*, group_by, after|before`. Only a
    /// filter's values depend on the order, and each filter writes them in
    /// the order it holds them.
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
            .flat_map(|(name, value)| {
                let param = self.filter_param(name);
                let values = value
                    .values()
                    .iter()
                    .map(move |v| (param.clone(), v.as_str()));
                let parts = value
                    .parts()
                    .map(move |(part, v)| (self.filter_param(&format!("{name}.{part}")), v));
                values.chain(parts)
            })
            .map(|(key, value)| (key, Some(value)));
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

/// Path segment of the list page's create page, under [`DASH_ROUTE_SEGMENT`].
pub(crate) const CREATE_ROUTE_SEGMENT: &str = "create";

/// Path segment of the list's CSV export, under [`DASH_ROUTE_SEGMENT`].
pub(crate) const EXPORT_ROUTE_SEGMENT: &str = "export";

/// Path segment of the record form's option search, under [`DASH_ROUTE_SEGMENT`].
pub(crate) const OPTIONS_ROUTE_SEGMENT: &str = "options";

/// Path segment of a row's edit page.
pub(crate) const EDIT_ROUTE_SEGMENT: &str = "edit";

/// Path segment of a row's delete POST.
pub(crate) const DELETE_ROUTE_SEGMENT: &str = "delete";

/// Path segment of the bulk-delete POST, under [`DASH_ROUTE_SEGMENT`].
pub(crate) const BULK_DELETE_ROUTE_SEGMENT: &str = "bulk-delete";

/// Path segment before a custom action's name, on a row
/// (`{prefix}/{key}/-/actions/{name}`) and on the list
/// (`{list}/-/actions/{name}`).
pub(crate) const ACTIONS_ROUTE_SEGMENT: &str = "actions";

/// Static segment guarding the list's own routes, and a record's action and relation routes, from
/// record keys: `matchit` prefers a static segment over `{id}`, so a record whose key is `create`
/// would otherwise lose its detail page to the create form.
pub(crate) const DASH_ROUTE_SEGMENT: &str = "-";

/// A route of the list itself, `{list}/-/{segment}`: the create form, the export, the option
/// search, the bulk delete.
pub(crate) fn list_route(list_path: &str, segment: &str) -> String {
    format!(
        "{}/{DASH_ROUTE_SEGMENT}/{segment}",
        list_path.trim_end_matches('/')
    )
}

/// The action-name placeholder the route table registers: `{action}`.
pub(crate) const ACTION_ROUTE_PARAM: &str = "{action}";

/// Path segment before a record's relation, whose table posts its attach and detach under
/// `{list}/{key}/-/relations/{relation}`.
pub(crate) const RELATIONS_ROUTE_SEGMENT: &str = "relations";

/// The relation placeholder the route table registers: `{relation}`, the related resource's slug.
pub(crate) const RELATION_ROUTE_PARAM: &str = "{relation}";

/// The related record's placeholder the route table registers: `{related}`.
pub(crate) const RELATED_ROUTE_PARAM: &str = "{related}";

/// A record's relation to the resource at `slug`, which its writes post under:
/// `{list}/{key}/-/relations/{slug}`.
pub(crate) fn relation_url(list: &str, key: &str, slug: &str) -> String {
    format!(
        "{list}/{}/{DASH_ROUTE_SEGMENT}/{RELATIONS_ROUTE_SEGMENT}/{slug}",
        href::encode_path_segment(key)
    )
}

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

/// A bulk or header action's POST target: `{url}/-/actions/{name}` under a list or a page.
pub(crate) fn bulk_action_url(list_path: &str, name: &str) -> String {
    format!("{list_path}/{DASH_ROUTE_SEGMENT}/{ACTIONS_ROUTE_SEGMENT}/{name}")
}

/// The option search of the input of the action `name` under a list or a page:
/// `{url}/-/actions/{name}/options`.
pub(crate) fn action_options_url(url: &str, name: &str) -> String {
    format!(
        "{}/options",
        bulk_action_url(url.trim_end_matches('/'), name)
    )
}

/// The list page's create link: `{list_path}/-/create`.
pub(crate) fn create_page_url(list_path: &str) -> String {
    list_route(list_path, CREATE_ROUTE_SEGMENT)
}

/// The list page's export link: `{list_path}/-/export`.
pub(crate) fn export_url(list_path: &str) -> String {
    list_route(list_path, EXPORT_ROUTE_SEGMENT)
}

/// The bulk form's POST target: `{list_path}/-/bulk-delete`.
pub(crate) fn bulk_delete_url(list_path: &str) -> String {
    list_route(list_path, BULK_DELETE_ROUTE_SEGMENT)
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

/// Stable DOM id for a table row: Topcoat's morph (topcoat#392) follows
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
