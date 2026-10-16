//! List state: [`TableState`], [`Cursor`], [`Sort`], and the URL codec.
//!
//! The URL query spells list state; [`TableState::from_query`] parses it and one
//! encoder projects every link.

use std::{borrow::Cow, collections::BTreeMap};

use topcoat::context::Cx;

use super::filter::FilterValue;
use crate::topcoat_compat::href;

/// The selection token for `key` on the bulk wire: the key percent-encoded between the commas that
/// delimit it, so a key holding the wire's separator stays one token.
pub(crate) fn bulk_token(key: &str) -> String {
    format!(",{},", href::encode_query_value(key))
}

/// The key a [`bulk_token`]'s body spells: the inverse of the encoding it applies.
///
/// Each valid escape decodes, and a `%` that starts no valid escape stays as written. A body
/// whose decoded bytes are not UTF-8 reads as written, whole, so a posted id holding no escape
/// keeps its literal text.
pub(crate) fn bulk_key(body: &str) -> String {
    href::decode_query_value(body)
}

/// A column and a direction to sort a table by: parsed from `?sort=<column>&dir=asc|desc`, or
/// declared as the table's [default](super::Table::default_sort).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Sort {
    /// The app-level field name of the column (matches `TextColumn::name`).
    pub column: String,
    /// `true` for `dir=desc`.
    pub descending: bool,
}

impl Sort {
    /// Sort by `column`, smallest first.
    pub fn asc(column: impl Into<String>) -> Self {
        Self {
            column: column.into(),
            descending: false,
        }
    }

    /// Sort by `column`, largest first.
    pub fn desc(column: impl Into<String>) -> Self {
        Self {
            column: column.into(),
            descending: true,
        }
    }
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
    /// `?after=` or `?before=`, which a [`Pagination::Cursor`](super::Pagination::Cursor) table
    /// reads.
    pub cursor: Option<Cursor>,
    /// `?page=`, from 1, which a numbered or simple table reads; `None` when absent or not a
    /// positive number.
    pub page: Option<u32>,
    /// `?per_page=`, honored when the table offers it; `None` when absent or not a positive
    /// number.
    pub per_page: Option<u32>,
    /// The active filters, by name: `?f.<name>=<value>`, repeated for each
    /// value, and `?f.<name>.<part>=<value>` for each part. The parse keeps
    /// no filter holding no value and no part.
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
    /// `?page=` page number.
    page: Option<u32>,
    /// `?per_page=` page size.
    per_page: Option<u32>,
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

    /// The `f.{name}` and `f.{name}.{part}` parameters `value` spells, in the order the filter
    /// holds them, prefixed like every other parameter.
    fn filter_pairs<'a>(
        &'a self,
        name: &'a str,
        value: &'a FilterValue,
    ) -> impl Iterator<Item = (String, &'a str)> + 'a {
        let param = self.filter_param(name);
        let values = value
            .values()
            .iter()
            .map(move |held| (param.clone(), held.as_str()));
        let parts = value
            .parts()
            .map(move |(part, held)| (self.filter_param(&format!("{name}.{part}")), held));
        values.chain(parts)
    }

    /// How a link spells `name`'s value: the parameters it travels as, encoded like every other
    /// parameter.
    pub(crate) fn spell_filter(&self, name: &str, value: &FilterValue) -> String {
        href::encode_query(&self.filter_pairs(name, value).collect::<Vec<_>>())
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
        let mut seen = [false; 8];
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
                "page" => 6,
                "per_page" => 7,
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
            let positive = || value.trim().parse::<u32>().ok().filter(|n| *n > 0);
            match slot {
                0 => state.search = Some(clamp_query_term(&value)).filter(|t| !t.is_empty()),
                1 => sort = non_empty(),
                2 => dir = Some(value.trim() == "desc"),
                3 => after = non_empty(),
                4 => before = non_empty(),
                5 => state.group_by = non_empty(),
                6 => state.page = positive(),
                _ => state.per_page = positive(),
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
    /// canonical order `q, sort, dir, f.*, group_by, per_page, page, after|before`. Only a
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

    /// Drops `q` (and the position in its result set); keeps the filters.
    pub(crate) fn without_search(&self, path: &str) -> String {
        let projection = UrlProjection {
            search: None,
            ..self.first_page_projection()
        };
        href::with_query(path, &self.project(projection))
    }

    /// Drops the filters (and the position in their result set); keeps the
    /// search term.
    pub(crate) fn without_filters(&self, path: &str) -> String {
        let projection = UrlProjection {
            filters: false,
            ..self.first_page_projection()
        };
        href::with_query(path, &self.project(projection))
    }

    /// Drops the cursor and the page number; keeps everything else, the page size included.
    /// Back-to-first-page and the cursor-failure retry link.
    pub(crate) fn first_page(&self, path: &str) -> String {
        href::with_query(path, &self.project(self.first_page_projection()))
    }

    /// Full state but the cursor, the page number kept: where a write returns the reader. A
    /// cursor can name a row the write removed; a page number past the end reads as the last.
    pub(crate) fn without_cursor(&self, path: &str) -> String {
        let projection = UrlProjection {
            cursor: None,
            ..self.full()
        };
        href::with_query(path, &self.project(projection))
    }

    /// Full state with `cursor` in place of the current position: a cursor pager's links.
    pub(crate) fn with_cursor(&self, path: &str, cursor: &Cursor) -> String {
        let projection = UrlProjection {
            cursor: Some(cursor),
            ..self.first_page_projection()
        };
        href::with_query(path, &self.project(projection))
    }

    /// Full state at page `page`: a numbered pager's links. Page 1 spells no parameter.
    pub(crate) fn with_page(&self, path: &str, page: u32) -> String {
        let projection = UrlProjection {
            page: Some(page).filter(|page| *page > 1),
            ..self.first_page_projection()
        };
        href::with_query(path, &self.project(projection))
    }

    /// Replaces `sort`/`dir`, drops the position: a new ordering is a new
    /// result set.
    pub(crate) fn sorted_by(&self, path: &str, column: &str, descending: bool) -> String {
        let projection = UrlProjection {
            sort: Some((column, if descending { "desc" } else { "asc" })),
            ..self.first_page_projection()
        };
        href::with_query(path, &self.project(projection))
    }

    /// The parameters a form rerunning this state at its first page carries, besides its own
    /// fields: every one but `skip`, the page and the cursor.
    pub(crate) fn hidden_pairs(&self, skip: &str) -> Vec<(String, String)> {
        let skip = self.param(skip);
        form_urlencoded::parse(self.project(self.first_page_projection()).as_bytes())
            .filter(|(name, _)| *name != skip)
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect()
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
            page: self.page,
            per_page: self.per_page,
        }
    }

    /// The projection that keeps every parameter but the position: the result set's first page.
    fn first_page_projection(&self) -> UrlProjection<'_> {
        UrlProjection {
            cursor: None,
            page: None,
            ..self.full()
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
            page: _,
            per_page: _,
            filters: _,
            filters_dropped: _,
            group_by: _,
        } = self;
        let filters = projection
            .filters
            .then_some(&self.filters)
            .into_iter()
            .flatten()
            .flat_map(|(name, value)| self.filter_pairs(name, value))
            .map(|(key, value)| (key, Some(value)));
        let cursor = match projection.cursor {
            Some(Cursor::After(token)) => ("after", Some(token.as_str())),
            Some(Cursor::Before(token)) => ("before", Some(token.as_str())),
            None => ("after", None),
        };
        let per_page = projection.per_page.map(|n| n.to_string());
        let page = projection.page.map(|n| n.to_string());
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
            (self.param("per_page"), per_page.as_deref()),
            (self.param("page"), page.as_deref()),
            (self.param(cursor.0), cursor.1),
        ])
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect();
        href::encode_query(&pairs)
    }
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
/// `scope` names the table, distinct per table on the page: two tables can hold one key.
pub(crate) fn row_dom_id(scope: &str, key: &str) -> String {
    dom_id(&format!("row-{scope}"), key)
}

/// Stable DOM id for a page-local group header.
///
/// Same contract as [`row_dom_id`]: the header is injected, removed and moved
/// as the page is re-sorted, so the in-place morph needs an id derived from the
/// group label it belongs to rather than from its position in the page.
pub(crate) fn group_header_dom_id(scope: &str, label: &str) -> String {
    dom_id(&format!("group-{scope}"), label)
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
