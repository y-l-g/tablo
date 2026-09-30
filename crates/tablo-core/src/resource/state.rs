//! Cursor/URL state: [`TablePage`], [`Sort`], [`TableState`], and the URL codec.
//!
//! Both entry points share one parse contract and the `filters`
//! transport is bounded where it is parsed.

use std::collections::HashMap;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use topcoat::{
    context::Cx,
    runtime::{Signal, signal},
};

use crate::query_term::clamp_query_term;

/// The live table's browser state.
///
/// The page owns these signals and hands their handles to the `table_search`
/// shard through `Table::render_live_normalized`; each tracked read inside
/// the shard becomes a `dep` marker the browser watches, so writing any signal
/// re-renders the table in place — no navigation, no scroll jump. Sort links,
/// the pager, the filter transport, the bulk selection, and the clear links
/// rendered by the table write them.
///
/// The two carriers meet in exactly two methods:
/// [`TableState::to_signals`] seeds these handles from a parsed state, and
/// [`TableSignals::to_state`] rebuilds the state from their current values.
/// A new interaction dimension is a field here plus one arm in each, instead
/// of a hand-written conversion at every seam.
///
/// `q`/`filters`/`sort`/`dir`/`group_by` reset the cursor when they change;
/// [`Self::cursor`] pages within the current result set. All values are
/// untrusted by the time the shard reads them back (the client owns the
/// signal).
#[derive(Clone)]
pub struct TableSignals {
    /// `?q=` — the search term (escaped substring match).
    pub q: Signal<String>,
    /// `?filters=` — the composed `key:value,key2:value2` transport.
    pub filters: Signal<String>,
    /// `?sort=` — the active sort column name (`""` = the table default).
    pub sort: Signal<String>,
    /// `?dir=` — `asc`/`desc` for [`Self::sort`].
    pub dir: Signal<String>,
    /// The live cursor, as one signal: `""` (no cursor),
    /// `after:<token>` or `before:<token>`. One signal makes the
    /// `after`+`before` pair Toasty rejects unrepresentable in the browser — no
    /// cross-write interleaving can produce it — and lets every result-set
    /// transition clear pagination with a single write. Written through
    /// `cursor_after` / `cursor_before` / `cursor_none`, read through
    /// `split_cursor`.
    pub cursor: Signal<String>,
    /// `?group_by=` — the active grouping (`""` = ungrouped).
    /// Seeded from the page-load state and changed via navigation
    /// (`?group_by=` links); no live control writes it yet, so it persists
    /// across in-place reruns. A future control writing it must clear the
    /// cursor like the other result-set dimensions.
    pub group_by: Signal<String>,
    /// The bulk selection: comma-separated record keys, `""` when nothing is
    /// selected. Row checkboxes carry no `checked` attribute —
    /// `bulk.js` sets `checked` from the transport after every swap and change
    /// — and the script writes the transport, whose bound `change` handler
    /// writes this signal, so a live rerun re-renders the boxes from the
    /// selection instead of dropping it. The shard carries the handle without
    /// reading it: the table needs it to bind the boxes, but a checkbox click
    /// must not reload rows.
    pub bulk: Signal<String>,
}

/// The live cursor wire format: `after:<token>` / `before:<token>`,
/// with the empty string meaning "no cursor". One signal carries it, so the
/// browser can never hold both cursors at once.
const CURSOR_AFTER: &str = "after:";
const CURSOR_BEFORE: &str = "before:";

/// Wire value for a forward cursor (`?after=<token>`).
pub(crate) fn cursor_after(token: &str) -> String {
    format!("{CURSOR_AFTER}{token}")
}

/// Wire value for a backward cursor (`?before=<token>`).
pub(crate) fn cursor_before(token: &str) -> String {
    format!("{CURSOR_BEFORE}{token}")
}

/// Wire value for no cursor — what every result-set transition writes, and
/// what a fresh page seeds.
pub(crate) fn cursor_none() -> String {
    String::new()
}

/// Split the live cursor wire value into the `(after, before)` pair the loader
/// consumes. At most one side is ever `Some`: a value naming neither direction
/// (a tampered signal, or one the client never sent) degrades to "no cursor" —
/// the drop-pagination retry contract of GH #110 — rather than the pair error
/// of GH #155 that a URL carrying both cursors reaches at load time.
pub(crate) fn split_cursor(wire: &str) -> (Option<String>, Option<String>) {
    let wire = wire.trim();
    for (prefix, forward) in [(CURSOR_AFTER, true), (CURSOR_BEFORE, false)] {
        if let Some(token) = wire.strip_prefix(prefix) {
            let token = token.trim();
            if token.is_empty() {
                break;
            }
            return if forward {
                (Some(token.to_string()), None)
            } else {
                (None, Some(token.to_string()))
            };
        }
    }
    (None, None)
}

/// Whether `key` is selected in the live bulk wire.
///
/// The wire is comma-delimited on both ends — `,a,b,`, empty when nothing is
/// selected — so the client-side `checked` binding tests membership with a
/// plain `contains(",<key>,")` instead of a substring test that would confuse
/// `b` with `ab`. [`parse_bulk_ids`](crate::panel) already ignores the empty
/// segments the delimiters produce, so the same wire is the form transport.
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

/// Request-scoped table state, parsed from the current URL query.
///
/// The single parse point shared by loaders (the search term, ordering via
/// `Table::order_bys_for`) and render (active sort, toolbar values,
/// pagination links), so the URL is the one truth for list state. The fixed parameter
/// names assume one table per page — per-table prefixes are deferred until a
/// real page needs two tables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableState {
    /// `?q=` — trimmed and clamped to `MAX_QUERY_TERM` chars; `None` when
    /// absent or blank.
    pub search: Option<String>,
    /// `?sort=` + `?dir=` — `None` when absent or blank.
    pub sort: Option<Sort>,
    /// `?after=` — encoded forward cursor.
    pub after: Option<String>,
    /// `?before=` — encoded backward cursor.
    pub before: Option<String>,
    /// `?filters=` — `key:value,key2:value2` (comma-separated, colon-delimited),
    /// bounded at parse time by `MAX_FILTERS_PARAM`/`MAX_FILTER_SEGMENTS`.
    pub filters: HashMap<String, String>,
    /// `?filters=` segments that carry no `key:value` pair: kept so
    /// `Table::unapplied_filters` can flag them (list banner, export 400)
    /// instead of silently dropping them, and so [`Self::filters_param`]
    /// round-trips them — a link built from this state keeps the warning
    /// until a valid `?filters=` replaces it.
    pub malformed_filters: Vec<String>,
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
    /// `?filters=` transport.
    filters: Option<&'a str>,
    /// `?group_by=` column.
    group_by: Option<&'a str>,
    /// `?after=` forward cursor.
    after: Option<&'a str>,
    /// `?before=` backward cursor.
    before: Option<&'a str>,
    /// `?delete=` row key for the confirmation dialog.
    delete: Option<&'a str>,
}

/// Longest `?filters=` transport parsed: the live shard hands this
/// the client-owned `filters` signal, and the router buffers shard bodies up
/// to megabytes — so the same bounded-echoed-state posture as
/// [`MAX_QUERY_TERM`](crate::query_term::MAX_QUERY_TERM) has to hold here,
/// where the transport is parsed, rather than at the shard that happens to read
/// it.
pub(crate) const MAX_FILTERS_PARAM: usize = 1024;

/// Most segments one `?filters=` transport may carry: the byte cap
/// alone still admits a thousand one-character segments, and each surviving
/// segment becomes a map entry every rebuilt URL echoes.
pub(crate) const MAX_FILTER_SEGMENTS: usize = 32;

/// The one segment an over-long or over-full `?filters=` collapses to.
/// It carries no `key:value` pair, so it rides the GH #148 malformed channel:
/// [`Table::unapplied_filters`](crate::resource::Table::unapplied_filters)
/// flags it (the list warns, the export refuses with 400 instead of exporting
/// an over-broad CSV) and [`TableState::filters_param`] re-emits it, so the
/// warning survives pagination and sort links.
pub(crate) const FILTERS_OVERFLOW_SEGMENT: &str = "filters=overflow";

impl TableState {
    /// Parse the state from the request in `cx`.
    ///
    /// A blank or unknown query parses as neutral state rather than failing
    /// the request. Duplicate keys (`?filters=a&filters=b`) resolve to the
    /// first occurrence, so a repeated filter never vanishes: rejecting the
    /// duplicate would fail the whole decode, and answering empty state instead
    /// would drop every filter — including export's fail-closed guard.
    /// Cursor errors still surface later, at decode time, where they are
    /// precise — including the conflicting `after` + `before` pair, which fails
    /// at load time. Renders without a request context (e.g. unit
    /// tests) get neutral state.
    pub fn from_cx(cx: &Cx) -> Self {
        let Some(parts) = topcoat::context::try_request_context::<http::request::Parts>(cx) else {
            return Self::default();
        };
        let params = first_wins_query_params(parts.uri.query().unwrap_or(""));
        Self::from_parts(|key| params.get(key).map(String::as_str))
    }

    /// One shared constructor behind [`Self::from_cx`] and
    /// [`Self::from_live_args`]: every query-state parse
    /// funnels through one contract, so the public live-args entry point — the
    /// documented seam for a page owning its own signals — cannot be the
    /// looser one. `q` is trimmed and clamped to
    /// [`MAX_QUERY_TERM`](crate::query_term::MAX_QUERY_TERM), `dir` is
    /// trimmed before comparing, and the `filters` transport is bounded at
    /// [`MAX_FILTERS_PARAM`]/[`MAX_FILTER_SEGMENTS`].
    ///
    /// The live args arrive named at the single `match` below, where a
    /// transposed positional pair would not compile silently.
    fn from_parts<'a>(get: impl Fn(&str) -> Option<&'a str>) -> Self {
        let non_empty = |v: Option<&str>| {
            v.map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        };
        let (filters, malformed_filters) = parse_filters_param(get("filters").unwrap_or_default());
        Self {
            search: get("q").map(clamp_query_term).filter(|t| !t.is_empty()),
            sort: non_empty(get("sort")).map(|column| Sort {
                column,
                descending: get("dir").map(str::trim) == Some("desc"),
            }),
            after: non_empty(get("after")),
            before: non_empty(get("before")),
            filters,
            malformed_filters,
            group_by: non_empty(get("group_by")),
            // The delete dialog is opt-in through `?delete=`; `?open=false`
            // is the dismissal mirror `dialog.js` writes. Any other
            // `open` value stays neutral (open).
            delete: non_empty(get("delete")),
            open: match get("open") {
                Some("false") => Some(false),
                Some("true") => Some(true),
                _ => None,
            },
        }
    }

    /// Serialized `filters` for URL (`key:value,key2:value2`), or `None` when empty.
    ///
    /// Keys/values escape `%`, `:`, `,` (`%25`/`%3A`/`%2C`) so a
    /// free-text value like `a,b` round-trips instead of splitting.
    ///
    /// This is the expensive half of a URL projection (a `format!` per pair,
    /// a sort, a join, and a percent-encode per byte), so a table render must
    /// encode it a bounded number of times — never once per row.
    /// `TableState::row_url_base` exists to make that structural.
    pub fn filters_param(&self) -> Option<String> {
        if self.filters.is_empty() && self.malformed_filters.is_empty() {
            return None;
        }
        let mut pairs: Vec<String> = self
            .filters
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}:{}",
                    encode_filter_component(k),
                    encode_filter_component(v)
                )
            })
            .collect();
        pairs.sort();
        // Malformed segments ride along verbatim: they have no
        // colon to protect and re-enter `parse_filters_param` as malformed on
        // the next request, keeping the banner (and export's fail-closed 400)
        // alive across pagination.
        pairs.extend(self.malformed_filters.iter().cloned());
        Some(pairs.join(","))
    }

    /// URL projection: `TableState` owns the table's URL vocabulary.
    /// Callers ask for a user intent, never a parameter list, so adding a
    /// parameter cannot silently drop it from half the links.
    ///
    /// One private encoder ([`Self::project_url`]) holds the vocabulary in
    /// canonical order `q, sort, dir, filters, group_by, after, before`
    /// (`delete` appended by its intent). The parser is first-wins with unique
    /// keys, so order is semantically irrelevant.
    ///
    /// Expects `group_by` pre-normalized: render seams normalize through
    /// [`Table::normalize_state`], so the projection echoes `state.group_by`
    /// as-is. `open` is never emitted by any link; `delete` only by
    /// [`Self::row_url_base`]'s dialog intent.
    ///
    /// Full state, including cursors; never `delete`/`open`. The streamed
    /// retry link for failures that keep their evidence.
    pub(crate) fn list_url(&self, path: &str) -> String {
        let filters = self.filters_param();
        self.project_url(
            path,
            UrlProjection {
                search: self.search.as_deref(),
                sort: self.sort_pair(),
                filters: filters.as_deref(),
                group_by: self.group_by.as_deref(),
                after: self.after.as_deref(),
                before: self.before.as_deref(),
                ..Default::default()
            },
        )
    }

    /// Drops `q` (and the cursors + dialog of its result set); keeps the
    /// `filters` transport including malformed segments.
    pub(crate) fn without_search(&self, path: &str) -> String {
        let filters = self.filters_param();
        self.project_url(
            path,
            UrlProjection {
                sort: self.sort_pair(),
                filters: filters.as_deref(),
                group_by: self.group_by.as_deref(),
                ..Default::default()
            },
        )
    }

    /// Drops `filters` and malformed segments (and the cursors + dialog of
    /// their result set); keeps the search term.
    pub(crate) fn without_filters(&self, path: &str) -> String {
        self.project_url(
            path,
            UrlProjection {
                search: self.search.as_deref(),
                sort: self.sort_pair(),
                group_by: self.group_by.as_deref(),
                ..Default::default()
            },
        )
    }

    /// Drops `after` and `before`; keeps everything else. Back-to-first-page
    /// and the cursor-failure retry link.
    pub(crate) fn without_cursor(&self, path: &str) -> String {
        let filters = self.filters_param();
        self.project_url(
            path,
            UrlProjection {
                search: self.search.as_deref(),
                sort: self.sort_pair(),
                filters: filters.as_deref(),
                group_by: self.group_by.as_deref(),
                ..Default::default()
            },
        )
    }

    /// Full state + `after`, drops `before` and the dialog.
    pub(crate) fn with_after(&self, path: &str, token: &str) -> String {
        let filters = self.filters_param();
        self.project_url(
            path,
            UrlProjection {
                search: self.search.as_deref(),
                sort: self.sort_pair(),
                filters: filters.as_deref(),
                group_by: self.group_by.as_deref(),
                after: Some(token),
                ..Default::default()
            },
        )
    }

    /// Full state + `before`, drops `after` and the dialog.
    pub(crate) fn with_before(&self, path: &str, token: &str) -> String {
        let filters = self.filters_param();
        self.project_url(
            path,
            UrlProjection {
                search: self.search.as_deref(),
                sort: self.sort_pair(),
                filters: filters.as_deref(),
                group_by: self.group_by.as_deref(),
                before: Some(token),
                ..Default::default()
            },
        )
    }

    /// Replaces `sort`/`dir`, drops cursors and the dialog: a new ordering is
    /// a new result set.
    pub(crate) fn sorted_by(&self, path: &str, column: &str, descending: bool) -> String {
        let filters = self.filters_param();
        self.project_url(
            path,
            UrlProjection {
                search: self.search.as_deref(),
                sort: Some((column, if descending { "desc" } else { "asc" })),
                filters: filters.as_deref(),
                group_by: self.group_by.as_deref(),
                ..Default::default()
            },
        )
    }

    /// The shared parameters of every row-action URL on one page, encoded
    /// once.
    ///
    /// A row's action URL is this base plus the row's record key, so a table
    /// render pays for the filter transport — the expensive half of the
    /// projection — once, however many rows the page holds. Build it before
    /// the row loop and call [`RowUrlBase::delete_dialog`] per row; that pair
    /// is the full-state-plus-`delete` projection, which keeps the
    /// cursors and never emits `open`.
    pub(crate) fn row_url_base(&self, path: &str) -> RowUrlBase {
        RowUrlBase(self.list_url(path))
    }

    /// `?sort=` column + `?dir=` value for the projection.
    fn sort_pair(&self) -> Option<(&str, &str)> {
        self.sort
            .as_ref()
            .map(|s| (s.column.as_str(), if s.descending { "desc" } else { "asc" }))
    }

    /// The one encoder: every table link's parameter vocabulary lives here.
    ///
    /// The exhaustive destructure fails compilation when a field is added to
    /// `TableState`, forcing the author to decide where it projects.
    fn project_url(&self, path: &str, projection: UrlProjection<'_>) -> String {
        let TableState {
            search: _,
            sort: _,
            after: _,
            before: _,
            filters: _,
            malformed_filters: _,
            group_by: _,
            delete: _,
            open: _,
        } = self;
        build_url(
            path,
            &[
                ("q", projection.search),
                ("sort", projection.sort.map(|(column, _)| column)),
                ("dir", projection.sort.map(|(_, dir)| dir)),
                ("filters", projection.filters),
                ("group_by", projection.group_by),
                ("after", projection.after),
                ("before", projection.before),
                ("delete", projection.delete),
            ],
        )
    }

    /// Rebuild list state from live-search shard args.
    ///
    /// Shard requests hit the `table_search` shard's own endpoint, so
    /// [`Self::from_cx`] would see the endpoint URI — not the list page's
    /// query. The page hands over the current values of its interaction
    /// signals instead: the search term, the filter transport, the sort column
    /// and its direction, and the page's grouping. Live search resets
    /// pagination (`after`/`before` are always `None` — a new search is a new
    /// result set, same as the GET toolbar) and keeps the page's `group_by`.
    ///
    /// Every argument is client-owned by the time the shard reads it back, so
    /// this applies [`Self::from_cx`]'s bounds through the shared
    /// `Self::from_parts` — the public constructor is not the
    /// looser one.
    ///
    /// [`TableSignals::to_state`] is the shard's call site for this:
    /// it reads the signals and passes their values here, so the shard never
    /// rebuilds state by hand.
    pub fn from_live_args(
        q: &str,
        filters_param: &str,
        sort: &str,
        dir: &str,
        group_by: &str,
    ) -> Self {
        // `after`/`before`/`delete`/`open` stay `None`: live search resets
        // pagination and the delete dialog with every keystroke (a new search
        // is a new result set, same as the GET toolbar), and the panel
        // renders the dialog outside the shard region for live tables.
        // Missing keys read as absent through `from_parts`.
        Self::from_parts(|key| match key {
            "q" => Some(q),
            "filters" => Some(filters_param),
            "sort" => Some(sort),
            "dir" => Some(dir),
            "group_by" => Some(group_by),
            _ => None,
        })
    }

    /// Seed the page's interaction signals from this state.
    ///
    /// The one state→signal conversion: the page owns the handles
    /// ([`TableSignals`]) and every control it renders writes them, so the
    /// values the page loaded with are what the signals start from. The
    /// cursor is one signal seeded from the `after`/`before` pair
    /// in this state; the bulk selection starts empty (the page never loads a
    /// selection); `group_by` seeds from the page-load value and persists
    /// across in-place reruns.
    ///
    /// Call it with the *parsed* state, before normalizing: an unknown
    /// `?group_by=` seeds the signal as written, and the shard's
    /// [`TableSignals::to_state`] + `Table::normalize_state` drop it on the
    /// way back in, exactly as the GET path does.
    ///
    /// Creates the signals, so it carries [`topcoat::runtime::signal`]'s
    /// contract: call it while a view is collecting signal declarations — the
    /// panel calls it from the live page's render, and the declarations ride
    /// that page's hoisted parts.
    pub fn to_signals(&self, cx: &Cx) -> TableSignals {
        TableSignals {
            q: signal(cx, || self.search.clone().unwrap_or_default()),
            filters: signal(cx, || self.filters_param().unwrap_or_default()),
            // The projection's own spelling of `?sort=`/`?dir=`, so
            // the signal and every link agree on the direction word.
            sort: signal(cx, || {
                self.sort_pair()
                    .map(|(column, _)| column.to_string())
                    .unwrap_or_default()
            }),
            dir: signal(cx, || {
                self.sort_pair()
                    .map(|(_, dir)| dir.to_string())
                    .unwrap_or_else(|| "asc".to_string())
            }),
            cursor: signal(cx, || match (&self.after, &self.before) {
                (Some(token), _) => cursor_after(token),
                (None, Some(token)) => cursor_before(token),
                (None, None) => cursor_none(),
            }),
            group_by: signal(cx, || self.group_by.clone().unwrap_or_default()),
            bulk: signal(cx, String::new),
        }
    }
}

/// The live seam's other direction: the signals the page owns,
/// read back into the request state the shard loads and renders with.
impl TableSignals {
    /// Rebuild request state from the live signals.
    ///
    /// The one signal→state conversion: every value is client-owned by the time
    /// the shard reads it back, so it goes through
    /// [`TableState::from_live_args`] — the same `q` clamp and `filters` bound
    /// the GET path applies — and the one cursor
    /// wire is split into the `(after, before)` pair the loader consumes.
    /// Pagination and the delete dialog always reset; a
    /// token that does not decode fails loudly at load time.
    pub fn to_state(&self) -> TableState {
        let mut state = TableState::from_live_args(
            &self.q.get(),
            &self.filters.get(),
            &self.sort.get(),
            &self.dir.get(),
            &self.group_by.get(),
        );
        (state.after, state.before) = split_cursor(&self.cursor.get());
        state
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

/// Query-string pairs with the first occurrence winning.
///
/// A duplicate key keeps its first value rather than failing the decode:
/// rejecting it would fail the whole parse, and answering empty state instead
/// would silently drop filters and export's fail-closed guard. Unknown
/// keys are ignored, like the typed decode.
fn first_wins_query_params(query: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (key, value) in form_urlencoded::parse(query.as_bytes()) {
        if !out.contains_key(key.as_ref()) {
            out.insert(key.into_owned(), value.into_owned());
        }
    }
    out
}

/// Parse `filters` query param: `key:value,key2:value2` (trimmed, blank ignored).
///
/// `,`/`:`/`%` inside keys/values are `%`-escaped by [`TableState::filters_param`]
/// decoding restores them. Duplicate keys keep the first occurrence
/// instead of silent last-wins.
///
/// An over-long or over-full transport is refused *whole* — never partially
/// applied, which would silently drop filters the caller did send — and the
/// refusal rides the GH #148 malformed channel as `FILTERS_OVERFLOW_SEGMENT`
/// so the list warns and the export 400s instead of running
/// unfiltered. The bound lives here, where the value is parsed: the live shard
/// hands this the client-owned `filters` signal, which the router buffers up to
/// megabytes of.
fn parse_filters_param(raw: &str) -> (HashMap<String, String>, Vec<String>) {
    // The length test first: it is O(1) and short-circuits the segment scan
    // for the oversized input this bound exists for.
    if raw.len() > MAX_FILTERS_PARAM || raw.split(',').count() > MAX_FILTER_SEGMENTS {
        return (HashMap::new(), vec![FILTERS_OVERFLOW_SEGMENT.to_string()]);
    }
    let mut map = HashMap::new();
    let mut malformed = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        // Split on the first *unescaped* colon: `%3A` stays inside the key/value,
        // so a plain `split_once(':')` is correct on the encoded form.
        let Some((k_enc, v_enc)) = part.split_once(':') else {
            malformed.push(part.to_string());
            continue;
        };
        let k = decode_filter_component(k_enc.trim());
        let v = decode_filter_component(v_enc.trim());
        if k.is_empty() || v.is_empty() {
            malformed.push(part.to_string());
        } else {
            map.entry(k).or_insert(v);
        }
    }
    (map, malformed)
}

/// Escape `%`, `:`, `,` inside a filter key/value.
fn encode_filter_component(s: &str) -> String {
    s.replace('%', "%25")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

/// Decode [`encode_filter_component`] (case-insensitive hex, single pass).
fn decode_filter_component(s: &str) -> String {
    s.replace("%2C", ",")
        .replace("%2c", ",")
        .replace("%3A", ":")
        .replace("%3a", ":")
        .replace("%25", "%")
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

/// Build `path?k=v&…` from ordered optional parameters, skipping `None`.
pub(crate) fn build_url(path: &str, params: &[(&str, Option<&str>)]) -> String {
    let query = params
        .iter()
        .filter_map(|(k, v)| v.map(|v| format!("{k}={}", encode_query_value(v))))
        .collect::<Vec<_>>()
        .join("&");
    if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    }
}

#[cfg(test)]
mod tests;
