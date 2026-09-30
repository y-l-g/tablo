use std::collections::BTreeMap;

use topcoat::context::{Cx, CxTestBuilder};

use super::*;
use crate::query_term::MAX_QUERY_TERM;

fn cx_with_query(query: &str) -> Cx {
    let uri = if query.is_empty() {
        "/admin".to_string()
    } else {
        format!("/admin?{query}")
    };
    let (parts, ()) = http::Request::builder()
        .uri(uri)
        .body(())
        .unwrap()
        .into_parts();
    CxTestBuilder::new().request_context(parts).build()
}

#[test]
fn table_state_parses_query_params() {
    let cx = cx_with_query("q=Ada+Lovelace&sort=name&dir=desc&after=abc123");
    let state = TableState::from_cx(&cx);
    assert_eq!(
        state.search.as_deref(),
        Some("Ada Lovelace"),
        "plus must decode to space"
    );
    assert_eq!(
        state.sort,
        Some(Sort {
            column: "name".to_string(),
            descending: true,
        })
    );
    assert_eq!(state.cursor, Some(Cursor::After("abc123".to_string())));

    // Absent / blank / malformed → neutral state
    assert_eq!(TableState::from_query(""), TableState::default());
    assert_eq!(
        TableState::from_query("q=&sort=&dir=weird"),
        TableState::default()
    );
}

/// The GET page and the live shard parse one query the same way: `from_cx`
/// is `from_query` over the request URI.
#[test]
fn from_cx_is_from_query_over_the_request() {
    let query = "q=Ada&sort=name&dir=desc&f.status=published&group_by=status&before=tok";
    assert_eq!(
        TableState::from_cx(&cx_with_query(query)),
        TableState::from_query(query)
    );
}

#[test]
fn the_search_term_is_clamped() {
    let long = "a".repeat(MAX_QUERY_TERM + 50);
    let state = TableState::from_query(&format!("q={long}"));
    assert_eq!(
        state.search.as_deref().map(|s| s.chars().count()),
        Some(MAX_QUERY_TERM)
    );
}

#[test]
fn table_state_duplicate_params_keep_first_and_never_fail_open() {
    // A duplicate param keeps its first value and never fails open: answering
    // empty state would drop every filter (and export's fail-closed guard
    // along with it).
    let state = TableState::from_query("f.status=published&f.status=draft&q=Ada&q=Grace");
    assert_eq!(
        state.filters.get("status").map(String::as_str),
        Some("published"),
        "first occurrence must win, not vanish"
    );
    assert_eq!(state.search.as_deref(), Some("Ada"));
}

#[test]
fn filters_are_one_parameter_each() {
    let state = TableState::from_query("f.status=published&f.featured=true&f.empty=&f.=x");
    assert_eq!(
        state.filters,
        BTreeMap::from([
            ("featured".to_string(), "true".to_string()),
            ("status".to_string(), "published".to_string()),
        ]),
        "a blank value or a blank name is no filter"
    );
    assert!(!state.filters_dropped);
}

#[test]
fn a_filter_value_with_separators_round_trips() {
    let mut state = TableState::default();
    state
        .filters
        .insert("author".to_string(), "Smith, John: 50% & co".to_string());
    assert_eq!(TableState::from_query(&state.query()), state);
}

#[test]
fn filters_past_the_cap_are_dropped_and_flagged() {
    let query = (0..MAX_FILTERS + 5)
        .map(|i| format!("f.k{i}=v"))
        .collect::<Vec<_>>()
        .join("&");
    let state = TableState::from_query(&query);
    assert_eq!(state.filters.len(), MAX_FILTERS);
    assert!(
        state.filters_dropped,
        "the dropped filters must be reported, so the export refuses"
    );
}

#[test]
fn oversized_filters_are_dropped_and_flagged() {
    // Every link echoes every applied filter, so a filter is bounded where the
    // query is parsed, like the count.
    let long = "a".repeat(MAX_FILTER_LEN + 1);
    for query in [format!("f.status={long}"), format!("f.{long}=v")] {
        let state = TableState::from_query(&format!("{query}&f.featured=true"));
        assert_eq!(
            state.filters,
            BTreeMap::from([("featured".to_string(), "true".to_string())])
        );
        assert!(state.filters_dropped);
    }
}

#[test]
fn the_retired_filters_parameter_is_flagged_not_ignored() {
    // A saved `?filters=` link must warn, and its export refuse, rather than
    // list the whole table as if it were unfiltered.
    assert!(TableState::from_query("filters=status:draft").filters_dropped);
    assert!(!TableState::from_query("filters=").filters_dropped);
}

#[test]
fn unknown_keys_are_skipped_without_being_remembered() {
    // The parse keeps nothing per unknown key, so a client-owned query of
    // many distinct keys parses in time linear in its length.
    let query = (0..20_000)
        .map(|i| format!("x{i}=v"))
        .chain(["q=Ada".to_string(), "q=Grace".to_string()])
        .collect::<Vec<_>>()
        .join("&");
    let state = TableState::from_query(&query);
    assert_eq!(state.search.as_deref(), Some("Ada"));
}

/// Toasty pages from one cursor: a URL naming both lands on the first page,
/// the recovery the cursor retry gives.
#[test]
fn both_cursors_parse_as_the_first_page() {
    assert_eq!(TableState::from_query("after=a&before=b").cursor, None);
    assert_eq!(
        TableState::from_query("before=b").cursor,
        Some(Cursor::Before("b".to_string()))
    );
}

#[test]
fn path_segment_encoding_keeps_uuids_and_escapes_reserved() {
    let uuid = uuid::Uuid::nil().to_string();
    assert_eq!(encode_path_segment(&uuid), uuid);
    assert_eq!(encode_path_segment("a/b"), "a%2Fb");
    assert_eq!(encode_path_segment("a+b@c.com"), "a%2Bb%40c.com");
    assert_eq!(encode_path_segment("100%"), "100%25");
    assert_eq!(encode_path_segment("a?b#c"), "a%3Fb%23c");
}

#[test]
fn row_dom_ids_are_stable_and_html_safe() {
    // morph follows `id`s across reruns — derived from the row
    // key (record-stable), never a loop index, sanitized to tokens.
    assert_eq!(
        row_dom_id("550e8400-e29b-41d4-a716-446655440000"),
        row_dom_id("550e8400-e29b-41d4-a716-446655440000"),
        "ids must be stable per key"
    );
    let uuid_id = row_dom_id("550e8400-e29b-41d4-a716-446655440000");
    assert!(uuid_id.starts_with("row-550e8400-e29b-41d4-a716-446655440000-"));
    assert!(
        uuid_id.is_ascii(),
        "id stays an ASCII token, got {uuid_id:?}"
    );
    // Keys that sanitize to the same token must not collide.
    assert_ne!(row_dom_id("Ada Lovelace"), row_dom_id("Ada-Lovelace"));
    assert_ne!(row_dom_id("a/b?c"), row_dom_id("a-b-c"));
}

#[test]
fn group_header_dom_ids_are_stable_and_distinct_from_row_ids() {
    // the injected group header is moved and removed as the page
    // is re-sorted, so it needs a stable id of its own — and it must never
    // collide with a row id, or the morph would follow the wrong element.
    assert_eq!(
        group_header_dom_id("draft"),
        group_header_dom_id("draft"),
        "ids must be stable per label"
    );
    let draft = group_header_dom_id("draft");
    assert!(
        draft.starts_with("group-draft-"),
        "the id names its group, got {draft:?}"
    );
    assert!(
        group_header_dom_id("New York").starts_with("group-New-York-"),
        "labels sanitize to HTML-safe tokens, got {:?}",
        group_header_dom_id("New York")
    );
    // Two labels sanitizing to one token must not collide, and a group id
    // is never a row id even for the same text.
    assert_ne!(
        group_header_dom_id("New York"),
        group_header_dom_id("New-York")
    );
    assert_ne!(group_header_dom_id("draft"), row_dom_id("draft"));
}

/// Fully populated projection source: every intent projects
/// from this through the real parser (`from_query`), asserting the typed
/// delta — state, not URL bytes.
fn populated_state() -> TableState {
    TableState {
        search: Some("Ada".to_string()),
        sort: Some(Sort {
            column: "name".to_string(),
            descending: true,
        }),
        cursor: Some(Cursor::After("after-cur".to_string())),
        filters: BTreeMap::from([("status".to_string(), "published".to_string())]),
        filters_dropped: false,
        group_by: Some("status".to_string()),
        delete: Some("row-1".to_string()),
        open: Some(false),
    }
}

/// Project through an intent and re-parse the URL with the real parser.
fn reparse(url: &str) -> TableState {
    TableState::from_query(query_of(url))
}

/// The state a link keeps: every link drops the dialog.
fn without_dialog(mut state: TableState) -> TableState {
    state.delete = None;
    state.open = None;
    state
}

#[test]
fn projection_list_url_round_trips_full_state() {
    // Full state including the cursor; never `delete`/`open`.
    let source = populated_state();
    assert_eq!(
        reparse(&source.list_url("/admin/users")),
        without_dialog(source.clone())
    );
    assert_eq!(
        TableState::from_query(&source.query()),
        without_dialog(source)
    );
}

#[test]
fn projection_without_search_drops_query() {
    // Drops `q` (and its result set's cursor + dialog); keeps the filters.
    let source = populated_state();
    let mut expected = without_dialog(source.clone());
    expected.search = None;
    expected.cursor = None;
    assert_eq!(reparse(&source.without_search("/admin/users")), expected);
}

#[test]
fn projection_without_filters_drops_filters() {
    // Drops the filters (and their result set's cursor + dialog); keeps the
    // search term.
    let source = populated_state();
    let mut expected = without_dialog(source.clone());
    expected.filters = BTreeMap::new();
    expected.cursor = None;
    assert_eq!(reparse(&source.without_filters("/admin/users")), expected);
}

#[test]
fn projection_without_cursor_drops_pagination() {
    // GH #153: drops the cursor; keeps everything else.
    let source = populated_state();
    let mut expected = without_dialog(source.clone());
    expected.cursor = None;
    assert_eq!(reparse(&source.without_cursor("/admin/users")), expected);
}

#[test]
fn projection_with_cursor_replaces_the_cursor() {
    // Full state with the new cursor, in either direction, and no dialog.
    let source = populated_state();
    for cursor in [
        Cursor::After("tok2".to_string()),
        Cursor::Before("tok2".to_string()),
    ] {
        let mut expected = without_dialog(source.clone());
        expected.cursor = Some(cursor.clone());
        assert_eq!(
            reparse(&source.with_cursor("/admin/users", &cursor)),
            expected
        );
    }
}

#[test]
fn projection_sorted_by_replaces_sort() {
    // Replaces `sort`/`dir`, drops the cursor and the dialog.
    let source = populated_state();
    let mut expected = without_dialog(source.clone());
    expected.sort = Some(Sort {
        column: "title".to_string(),
        descending: false,
    });
    expected.cursor = None;
    assert_eq!(
        reparse(&source.sorted_by("/admin/users", "title", false)),
        expected
    );
}

#[test]
fn projection_row_url_base_adds_the_delete_dialog_key() {
    // GH #153: full state including the cursor + `delete=key`; never `open`.
    let source = populated_state();
    let mut expected = source.clone();
    expected.delete = Some("row-9".to_string());
    expected.open = None;
    // Spelled as the page's shared base plus the row key — exactly what a
    // table render does per row.
    assert_eq!(
        reparse(&source.row_url_base("/admin/users").delete_dialog("row-9")),
        expected
    );
    // A state with nothing else to project still opens the dialog.
    assert_eq!(
        TableState::default()
            .row_url_base("/admin/users")
            .delete_dialog("row-9"),
        "/admin/users?delete=row-9"
    );
}

/// the bulk wire is delimited on both ends so membership is exact
/// (`b` is not selected by `,ab,`), and the delimiters ride through the
/// form transport the bulk handler already parses.
#[test]
fn bulk_wire_membership_is_exact() {
    let wire = ",row-1,row-2,";
    assert!(bulk_wire_contains(wire, "row-1"));
    assert!(bulk_wire_contains(wire, "row-2"));
    assert!(!bulk_wire_contains(wire, "row"));
    assert!(!bulk_wire_contains(wire, "row-1a"));
    assert!(!bulk_wire_contains(",row-12,", "row-1"));
    assert!(!bulk_wire_contains("", "row-1"));
}
