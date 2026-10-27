use std::collections::BTreeMap;

use topcoat::context::Cx;

use super::*;
use crate::{schema::relationship::MAX_QUERY_TERM, topcoat_compat::href::encode_path_segment};

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
    Cx::default().with(parts)
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
fn a_duplicate_param_keeps_its_first_value() {
    let state = TableState::from_query("q=Ada&q=Grace&sort=name&sort=email");
    assert_eq!(state.search.as_deref(), Some("Ada"));
    assert_eq!(state.sort.map(|sort| sort.column).as_deref(), Some("name"));
}

#[test]
fn a_repeated_filter_holds_each_value_once_in_order() {
    let state = TableState::from_query(
        "f.status=published&f.featured=true&f.status=draft&f.status=published&f.status=%20",
    );
    assert_eq!(
        state.filters,
        BTreeMap::from([
            ("featured".to_string(), "true".into()),
            (
                "status".to_string(),
                FilterValue::from("published").with("draft")
            ),
        ]),
    );
    assert!(!state.filters_dropped);
}

#[test]
fn a_filter_part_follows_the_name_and_keeps_its_first_value() {
    let state = TableState::from_query(
        "f.created.from=2024-01-01&f.created.to=2024-01-31&f.created.from=2023-01-01",
    );
    assert_eq!(
        state.filters,
        BTreeMap::from([(
            "created".to_string(),
            FilterValue::new()
                .with_part("from", "2024-01-01")
                .with_part("to", "2024-01-31"),
        )]),
    );
}

#[test]
fn a_blank_name_part_or_value_is_no_filter() {
    let state = TableState::from_query("f.empty=&f.=x&f..from=x&f.created.=x&f.created.to=");
    assert_eq!(state.filters, BTreeMap::new());
    assert!(!state.filters_dropped);
}

#[test]
fn a_filter_spells_its_parameters_encoded() {
    let state = TableState::default();
    let value = FilterValue::from("draft")
        .with("review")
        .with_part("from", "2024-01-01");
    assert_eq!(
        state.spell_filter("status", &value),
        "f.status=draft&f.status=review&f.status.from=2024-01-01",
        "one parameter per value then per part, in the order the filter holds them"
    );
    assert_eq!(
        state.spell_filter("status", &FilterValue::from("R&D")),
        "f.status=R%26D",
        "a value holding the separator stays one parameter, as the link spells it"
    );
}

/// A value the parser keeps as written: no surrounding whitespace, no control character.
const KEPT: &str = "[\\PC&&\\S]([\\PC]{0,20}[\\PC&&\\S])?";

/// A value the parser keeps, or one past the length it keeps: a filter value the codec cannot
/// carry must not reach the query.
fn filter_text() -> impl proptest::strategy::Strategy<Value = String> {
    use proptest::prelude::*;

    prop_oneof![
        7 => KEPT,
        1 => "[\\PC&&\\S]{260,300}",
    ]
}

/// The filter values a parse can produce: at least one value or part.
fn filter_value() -> impl proptest::strategy::Strategy<Value = FilterValue> {
    use proptest::{collection, prelude::*};

    // Every eighth case spells more values and parts than the codec carries, in kept spellings
    // that survive the length cap, so the count cap stays exercised too.
    let values = prop_oneof![
        7 => collection::vec(filter_text(), 0..4),
        1 => collection::vec(KEPT, MAX_FILTER_VALUES + 1..MAX_FILTER_VALUES + 8),
    ];
    let parts = prop_oneof![
        7 => collection::btree_map("[a-z_]{1,8}", filter_text(), 0..3),
        1 => collection::btree_map("[a-z_]{1,8}", KEPT, MAX_FILTER_VALUES + 1..MAX_FILTER_VALUES + 8),
    ];
    (values, parts)
        .prop_map(|(values, parts)| {
            let value = values
                .into_iter()
                .fold(FilterValue::new(), FilterValue::with);
            parts
                .into_iter()
                .fold(value, |value, (part, v)| value.with_part(part, v))
        })
        .prop_filter("an active filter holds a value", |value| !value.is_empty())
}

/// The states a parse can produce, every part optional.
fn parsed_state() -> impl proptest::strategy::Strategy<Value = TableState> {
    use proptest::{collection, option, prelude::*};

    (
        option::of(KEPT),
        option::of(("[a-z_]{1,12}", any::<bool>())),
        option::of((KEPT, any::<bool>())),
        collection::btree_map("[a-z_]{1,12}", filter_value(), 0..6),
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
fn values_past_the_cap_are_dropped_and_flagged() {
    let query = (0..MAX_FILTER_VALUES + 5)
        .map(|i| format!("f.tag=v{i}"))
        .collect::<Vec<_>>()
        .join("&");
    let state = TableState::from_query(&format!("{query}&f.tag.from=x"));
    assert_eq!(state.filters["tag"].values().len(), MAX_FILTER_VALUES);
    assert_eq!(state.filters["tag"].part("from"), None);
    assert!(state.filters_dropped);
}

#[test]
fn oversized_filters_are_dropped_and_flagged() {
    let long = "a".repeat(MAX_FILTER_LEN + 1);
    for query in [
        format!("f.status={long}"),
        format!("f.{long}=v"),
        format!("f.created.{long}=v"),
    ] {
        let state = TableState::from_query(&format!("{query}&f.featured=true"));
        assert_eq!(
            state.filters,
            BTreeMap::from([("featured".to_string(), "true".into())])
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
        row_dom_id("table", "550e8400-e29b-41d4-a716-446655440000"),
        row_dom_id("table", "550e8400-e29b-41d4-a716-446655440000"),
        "ids must be stable per key"
    );
    let uuid_id = row_dom_id("table", "550e8400-e29b-41d4-a716-446655440000");
    assert!(uuid_id.starts_with("row-table-550e8400-e29b-41d4-a716-446655440000-"));
    assert!(
        uuid_id.is_ascii(),
        "id stays an ASCII token, got {uuid_id:?}"
    );
    // Keys that sanitize to the same token must not collide.
    assert_ne!(
        row_dom_id("table", "Ada Lovelace"),
        row_dom_id("table", "Ada-Lovelace")
    );
    assert_ne!(row_dom_id("table", "a/b?c"), row_dom_id("table", "a-b-c"));
    // Two tables on one page can hold one key.
    assert_ne!(row_dom_id("authors", "7"), row_dom_id("editors", "7"));
}

#[test]
fn group_header_dom_ids_are_stable_and_distinct_from_row_ids() {
    assert_eq!(
        group_header_dom_id("table", "draft"),
        group_header_dom_id("table", "draft"),
        "ids must be stable per label"
    );
    let draft = group_header_dom_id("table", "draft");
    assert!(
        draft.starts_with("group-table-draft-"),
        "the id names its group, got {draft:?}"
    );
    assert!(
        group_header_dom_id("table", "New York").starts_with("group-table-New-York-"),
        "labels sanitize to HTML-safe tokens, got {:?}",
        group_header_dom_id("table", "New York")
    );
    // Two labels sanitizing to one token must not collide, and a group id
    // is never a row id even for the same text.
    assert_ne!(
        group_header_dom_id("table", "New York"),
        group_header_dom_id("table", "New-York")
    );
    assert_ne!(
        group_header_dom_id("table", "draft"),
        row_dom_id("table", "draft")
    );
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
        page: Some(3),
        per_page: Some(50),
        filters: BTreeMap::from([
            (
                "created".to_string(),
                FilterValue::new().with_part("from", "2024-01-01"),
            ),
            (
                "status".to_string(),
                FilterValue::from("published").with("draft"),
            ),
        ]),
        filters_dropped: false,
        group_by: Some("status".to_string()),
    }
}

/// Project through an intent and re-parse the URL with the real parser.
fn reparse(url: &str) -> TableState {
    TableState::from_query(query_of(url))
}

/// Each projection URL changes only its own part of the state; every one but the plain list URL,
/// a new cursor and a write's return returns to the first page.
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
            state.page = None;
        })
    );
    assert_eq!(
        reparse(&source.without_filters(path)),
        expect(&|state| {
            state.filters = BTreeMap::new();
            state.cursor = None;
            state.page = None;
        })
    );
    assert_eq!(
        reparse(&source.first_page(path)),
        expect(&|state| {
            state.cursor = None;
            state.page = None;
        })
    );
    assert_eq!(
        reparse(&source.without_cursor(path)),
        expect(&|state| state.cursor = None),
        "a write returns to the page the reader left"
    );
    assert_eq!(
        reparse(&source.sorted_by(path, "title", false)),
        expect(&|state| {
            state.sort = Some(Sort {
                column: "title".to_string(),
                descending: false,
            });
            state.cursor = None;
            state.page = None;
        })
    );
    for cursor in [
        Cursor::After("tok2".to_string()),
        Cursor::Before("tok2".to_string()),
    ] {
        assert_eq!(
            reparse(&source.with_cursor(path, &cursor)),
            expect(&|state| {
                state.cursor = Some(cursor.clone());
                state.page = None;
            })
        );
    }
    assert_eq!(
        reparse(&source.with_page(path, 7)),
        expect(&|state| {
            state.cursor = None;
            state.page = Some(7);
        })
    );
    // The first page spells no number, so its link is the result set's own.
    assert_eq!(source.with_page(path, 1), source.first_page(path));
}

/// `?page=` and `?per_page=` read only a positive number, and the first occurrence of each.
#[test]
fn page_parameters_read_positive_numbers_only() {
    let state = TableState::from_query("page=4&per_page=50&page=9");
    assert_eq!((state.page, state.per_page), (Some(4), Some(50)));
    for query in ["page=0", "page=-2", "page=two", "page=", "page=99999999999"] {
        assert_eq!(TableState::from_query(query).page, None, "{query}");
    }
}

/// A form rerunning the state carries every parameter but the one it sets and the position.
#[test]
fn hidden_pairs_leave_out_the_position_and_the_named_parameter() {
    let state = TableState::from_query("q=ada&page=3&per_page=50&after=tok&f.status=draft");
    let pairs = state.hidden_pairs("per_page");
    let names: Vec<&str> = pairs.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["q", "f.status"]);
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
        comments.filters.get("status").and_then(FilterValue::single),
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

proptest::proptest! {
    /// A row's selection token holds the separator only at its ends, whatever its key spells, so a
    /// key holding a comma selects one row, and the token reads back as that key.
    #[test]
    fn a_bulk_token_is_one_token_for_its_key(key in "\\PC{0,24}") {
        let token = bulk_token(&key);
        let body = &token[1..token.len() - 1];
        proptest::prop_assert!(token.starts_with(',') && token.ends_with(','));
        proptest::prop_assert!(!body.contains(','), "{token}");
        proptest::prop_assert_eq!(bulk_key(body), key);
    }
}
