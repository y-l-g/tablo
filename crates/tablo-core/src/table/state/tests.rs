use std::collections::BTreeMap;

use topcoat::context::{Cx, CxTestBuilder};

use super::*;
use crate::{query_term::MAX_QUERY_TERM, topcoat_compat::href::encode_path_segment};

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
    assert_eq!(
        TableState::from_query("q=").search,
        None,
        "a blank term is no search"
    );
    assert_eq!(TableState::from_query("q=%20%20").search, None);
}

#[test]
fn table_state_duplicate_params_keep_first_and_never_fail_open() {
    // A duplicate param keeps its first value.
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

/// A value the parser keeps as written: no surrounding whitespace, no control character.
const KEPT: &str = "[\\PC&&\\S]([\\PC]{0,20}[\\PC&&\\S])?";

/// The states a parse can produce, every part optional.
fn parsed_state() -> impl proptest::strategy::Strategy<Value = TableState> {
    use proptest::{collection, option, prelude::*};

    (
        option::of(KEPT),
        option::of(("[a-z_]{1,12}", any::<bool>())),
        option::of((KEPT, any::<bool>())),
        collection::btree_map("[a-z_]{1,12}", KEPT, 0..6),
        option::of("[a-z_]{1,12}"),
    )
        .prop_map(|(search, sort, cursor, filters, group_by)| TableState {
            search,
            sort: sort.map(|(column, descending)| Sort { column, descending }),
            cursor: cursor.map(|(token, after)| {
                if after {
                    Cursor::After(token)
                } else {
                    Cursor::Before(token)
                }
            }),
            filters,
            group_by,
            ..TableState::default()
        })
}

proptest::proptest! {
    /// Every link a table writes parses back to the state it was written from, whatever its
    /// search, filter values and cursor spell.
    #[test]
    fn any_state_round_trips_through_its_query(state in parsed_state()) {
        proptest::prop_assert_eq!(TableState::from_query(&state.query()), state);
    }
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
    assert!(TableState::from_query("filters=status:draft").filters_dropped);
    assert!(!TableState::from_query("filters=").filters_dropped);
}

#[test]
fn unknown_keys_are_skipped_without_being_remembered() {
    let query = (0..20_000)
        .map(|i| format!("x{i}=v"))
        .chain(["q=Ada".to_string(), "q=Grace".to_string()])
        .collect::<Vec<_>>()
        .join("&");
    let state = TableState::from_query(&query);
    assert_eq!(state.search.as_deref(), Some("Ada"));
}

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

/// Fully populated projection source.
fn populated_state() -> TableState {
    TableState {
        prefix: None,
        search: Some("Ada".to_string()),
        sort: Some(Sort {
            column: "name".to_string(),
            descending: true,
        }),
        cursor: Some(Cursor::After("after-cur".to_string())),
        filters: BTreeMap::from([("status".to_string(), "published".to_string())]),
        filters_dropped: false,
        group_by: Some("status".to_string()),
    }
}

/// Project through an intent and re-parse the URL with the real parser.
fn reparse(url: &str) -> TableState {
    TableState::from_query(query_of(url))
}

/// Each projection URL changes only its own part of the state; every one but the plain list URL
/// and a new cursor returns to the first page.
#[test]
fn each_projection_changes_only_its_own_part_of_the_state() {
    let source = populated_state();
    let expect = |change: &dyn Fn(&mut TableState)| {
        let mut expected = source.clone();
        change(&mut expected);
        expected
    };
    let path = "/admin/users";
    assert_eq!(reparse(&source.list_url(path)), source);
    assert_eq!(TableState::from_query(&source.query()), source);
    assert_eq!(
        reparse(&source.without_search(path)),
        expect(&|state| {
            state.search = None;
            state.cursor = None;
        })
    );
    assert_eq!(
        reparse(&source.without_filters(path)),
        expect(&|state| {
            state.filters = BTreeMap::new();
            state.cursor = None;
        })
    );
    assert_eq!(
        reparse(&source.without_cursor(path)),
        expect(&|state| state.cursor = None)
    );
    assert_eq!(
        reparse(&source.sorted_by(path, "title", false)),
        expect(&|state| {
            state.sort = Some(Sort {
                column: "title".to_string(),
                descending: false,
            });
            state.cursor = None;
        })
    );
    for cursor in [
        Cursor::After("tok2".to_string()),
        Cursor::Before("tok2".to_string()),
    ] {
        assert_eq!(
            reparse(&source.with_cursor(path, &cursor)),
            expect(&|state| state.cursor = Some(cursor.clone()))
        );
    }
}

#[test]
fn prefixed_states_share_one_query_without_colliding() {
    let query = "q=list&comments.q=ada&comments.sort=body&comments.dir=desc\
                 &comments.f.status=open&tags.q=rust&comments.after=tok";
    let comments = TableState::from_query_prefixed(query, "comments");
    assert_eq!(comments.prefix.as_deref(), Some("comments"));
    assert_eq!(comments.search.as_deref(), Some("ada"));
    assert_eq!(
        comments.sort,
        Some(Sort {
            column: "body".to_string(),
            descending: true
        })
    );
    assert_eq!(
        comments.filters.get("status").map(String::as_str),
        Some("open")
    );
    assert_eq!(comments.cursor, Some(Cursor::After("tok".to_string())));
    assert_eq!(
        TableState::from_query_prefixed(query, "tags")
            .search
            .as_deref(),
        Some("rust")
    );
    // The bare list state ignores every keyed parameter.
    assert_eq!(
        TableState::from_query(query).search.as_deref(),
        Some("list")
    );

    let url = comments.list_url("/admin/posts/1");
    assert_eq!(
        url,
        "/admin/posts/1?comments.q=ada&comments.sort=body&comments.dir=desc\
         &comments.f.status=open&comments.after=tok"
    );
    assert_eq!(
        TableState::from_query_prefixed(query_of(&url), "comments"),
        comments,
        "a prefixed link round-trips"
    );
}
