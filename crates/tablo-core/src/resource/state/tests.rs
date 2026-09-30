use std::collections::HashMap;

use topcoat::context::{Cx, CxTestBuilder};

use super::*;
use crate::query_term::MAX_QUERY_TERM;

#[test]
fn from_live_args_builds_state() {
    let state = TableState::from_live_args(
        "  Ada ",
        "status:published, featured:true",
        "name",
        "desc",
        "",
    );
    assert_eq!(state.search.as_deref(), Some("Ada"));
    assert_eq!(
        state.filters.get("status").map(String::as_str),
        Some("published")
    );
    assert_eq!(
        state.filters.get("featured").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        state.sort,
        Some(Sort {
            column: "name".to_string(),
            descending: true,
        })
    );
    assert!(state.after.is_none() && state.before.is_none());
    // Blank inputs → neutral state.
    assert_eq!(
        TableState::from_live_args("", "", "", "", ""),
        TableState::default()
    );
}

#[test]
fn live_and_url_constructors_share_one_contract() {
    // `from_live_args` is the public seam a page owning its own
    // signals is documented to call, so it must apply the GET path's
    // bounds — `q` trimmed and clamped to `MAX_QUERY_TERM`, `dir` trimmed
    // before comparing — instead of being the looser of the two.
    let long = "x".repeat(MAX_QUERY_TERM + 10);
    let live = TableState::from_live_args(&long, "", "name", " desc ", "");
    let url = TableState::from_cx(&cx_with_query(&format!("q={long}&sort=name&dir=+desc+")));
    assert_eq!(live, url, "the two entry points must agree field for field");
    assert_eq!(
        live.search.as_deref().map(str::len),
        Some(MAX_QUERY_TERM),
        "the live constructor must clamp `q` like the GET path"
    );
    assert_eq!(
        live.sort,
        Some(Sort {
            column: "name".to_string(),
            descending: true,
        }),
        "both paths trim `dir` before comparing"
    );
    // Blank inputs agree on neutral state too.
    assert_eq!(
        TableState::from_live_args("", "", "", "", ""),
        TableState::default()
    );
    assert_eq!(
        TableState::from_cx(&cx_with_query("")),
        TableState::default()
    );
}

#[test]
fn oversized_filters_transport_is_refused_whole_and_flagged() {
    // the live shard hands `parse_filters_param` a client-owned
    // signal the router will buffer megabytes of. The transport is bounded
    // where it is parsed, and an oversized one is refused *whole* — never
    // partially applied — through the GH #148 malformed channel, so the
    // list warns and the export 400s instead of running unfiltered.
    let huge = format!("status:published,{}", "k:v,".repeat(2 * 1024 * 1024));
    assert!(huge.len() > MAX_FILTERS_PARAM);
    let state = TableState::from_live_args("", &huge, "", "", "");
    assert!(
        state.filters.is_empty(),
        "an oversized transport must not be partially applied"
    );
    assert_eq!(
        state.malformed_filters,
        vec![FILTERS_OVERFLOW_SEGMENT.to_string()]
    );
    // The refusal is bounded and survives into every rebuilt link.
    let param = state.filters_param().expect("the refusal must project");
    assert_eq!(param, FILTERS_OVERFLOW_SEGMENT);
    assert!(
        param.len() < MAX_FILTERS_PARAM,
        "the projected transport stays bounded, got {} bytes",
        param.len()
    );

    // A segment flood under the byte cap is refused the same way: 32
    // one-character pairs are small but would each cost a map entry.
    let flood = vec!["k:v"; MAX_FILTER_SEGMENTS + 1].join(",");
    assert!(flood.len() <= MAX_FILTERS_PARAM);
    let state = TableState::from_cx(&cx_with_query(&format!("filters={flood}")));
    assert!(state.filters.is_empty());
    assert_eq!(
        state.malformed_filters,
        vec![FILTERS_OVERFLOW_SEGMENT.to_string()]
    );

    // A transport inside both bounds still applies, unchanged.
    let ok = vec!["k:v"; MAX_FILTER_SEGMENTS].join(",");
    let state = TableState::from_live_args("", &ok, "", "", "");
    assert!(state.malformed_filters.is_empty());
    assert_eq!(state.filters.get("k").map(String::as_str), Some("v"));
}

#[test]
fn live_args_filters_round_trip_through_transport() {
    // GH #136 §5: the live `filters` string is the same transport the URL
    // parses — encode/decode round-trips without loss.
    let state =
        TableState::from_live_args("Ada", "status:published,featured:true", "name", "desc", "");
    let param = state.filters_param().expect("live filters serialize");
    let back = TableState::from_live_args("Ada", &param, "name", "desc", "");
    assert_eq!(back.filters, state.filters);
    assert_eq!(back.search, state.search);
    assert_eq!(back.sort, state.sort);
}

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
    assert_eq!(state.after.as_deref(), Some("abc123"));
    assert!(state.before.is_none());

    // Absent / blank / malformed → neutral state
    let cx = cx_with_query("");
    let state = TableState::from_cx(&cx);
    assert_eq!(state, TableState::default());
    let cx = cx_with_query("q=&sort=&dir=weird");
    let state = TableState::from_cx(&cx);
    assert_eq!(state, TableState::default());
}

#[test]
fn table_state_duplicate_params_keep_first_and_never_fail_open() {
    // a duplicate param keeps its first value and never fails
    // open — answering empty state would drop every filter (and export's
    // fail-closed guard along with it).
    let cx = cx_with_query("filters=status:published&filters=status:draft&q=Ada&q=Grace");
    let state = TableState::from_cx(&cx);
    assert_eq!(
        state.filters.get("status").map(String::as_str),
        Some("published"),
        "first occurrence must win, not vanish"
    );
    assert_eq!(state.search.as_deref(), Some("Ada"));
}

#[test]
fn table_state_parses_filters_param() {
    let cx = cx_with_query("filters=status:published,featured:true");
    let state = TableState::from_cx(&cx);
    assert_eq!(
        state.filters.get("status").map(String::as_str),
        Some("published")
    );
    assert_eq!(
        state.filters.get("featured").map(String::as_str),
        Some("true")
    );
    assert!(state.malformed_filters.is_empty());
}

#[test]
fn filters_param_round_trips_reserved_chars() {
    let mut filters = HashMap::new();
    filters.insert("q".to_string(), "a,b".to_string());
    filters.insert("tag".to_string(), "x:y%z".to_string());
    let state = TableState {
        filters,
        ..TableState::default()
    };
    let param = state.filters_param().expect("must serialize");
    assert!(param.contains("%2C") && param.contains("%3A") && param.contains("%25"));
    let (back, malformed) = parse_filters_param(&param);
    assert!(
        malformed.is_empty(),
        "round-trip must not invent malformed segments, got {malformed:?}"
    );
    assert_eq!(back.get("q").map(String::as_str), Some("a,b"));
    assert_eq!(back.get("tag").map(String::as_str), Some("x:y%z"));
    // Duplicate keys keep the first, never silent last-wins.
    let (dup, dup_malformed) = parse_filters_param("k:a,k:b");
    assert_eq!(dup.get("k").map(String::as_str), Some("a"));
    assert!(dup_malformed.is_empty());
    // Legacy plain values still parse.
    let (legacy, legacy_malformed) = parse_filters_param("status:published, featured:true");
    assert_eq!(legacy.get("status").map(String::as_str), Some("published"));
    assert!(legacy_malformed.is_empty());
    // Blank segments stay silent (the boundary between "skipped" and
    // "malformed"); space-padded keys still parse.
    let (blank, blank_bad) = parse_filters_param(",,status:draft");
    assert!(
        blank_bad.is_empty(),
        "blank segments are skipped, got {blank_bad:?}"
    );
    assert_eq!(blank.get("status").map(String::as_str), Some("draft"));
    // Colon-less and empty-value segments are malformed, not dropped.
    let (ok, bad) = parse_filters_param("foobar,:val,key:,status:published");
    assert_eq!(ok.get("status").map(String::as_str), Some("published"));
    assert_eq!(
        bad,
        ["foobar".to_string(), ":val".to_string(), "key:".to_string()]
    );
    // Round-trip keeps them flagged: filters_param re-emits them verbatim
    // (last, after the sorted pairs), so the next parse flags them again.
    let state = TableState {
        filters: ok,
        malformed_filters: bad.clone(),
        ..TableState::default()
    };
    let param = state.filters_param().expect("must serialize");
    let (again_ok, again_bad) = parse_filters_param(&param);
    assert_eq!(again_bad, bad, "malformed segments must round-trip");
    assert_eq!(
        again_ok.get("status").map(String::as_str),
        Some("published")
    );
    // Percent-escape round-trips per component (case-insensitive decode).
    for raw in ["a,b", "x:y%z", "100%", "a:b:c", "%3A%2C%25"] {
        let enc = encode_filter_component(raw);
        assert_eq!(decode_filter_component(&enc), raw, "round-trip {raw:?}");
    }
}

#[test]
fn client_transport_parses_to_the_client_value() {
    // The literals are the ones `filters.js` composes: the same fixtures
    // run in `crates/tablo-ui/assets/filters.test.js`. Pinning them
    // here joins the two halves — change `encode_filter_component` and the
    // browser's literal stops decoding to its value, change the browser's
    // encoder and the literal it emits stops matching this test.
    let cases = [
        ("name:Smith%2C John", "name", "Smith, John"),
        ("name:a%3Ab", "name", "a:b"),
        ("name:100%25", "name", "100%"),
        ("name:Ada Lovelace", "name", "Ada Lovelace"),
        ("name:%253A%252C%2525", "name", "%3A%2C%25"),
        // The key is escaped with the value.
        ("a%2Cb%3Ac:x", "a,b:c", "x"),
    ];
    for (transport, key, value) in cases {
        let (filters, malformed) = parse_filters_param(transport);
        assert!(
            malformed.is_empty(),
            "{transport:?} must parse clean, got {malformed:?}"
        );
        assert_eq!(
            filters.get(key).map(String::as_str),
            Some(value),
            "{transport:?} must decode to {value:?}"
        );
    }
    // The empty value clears the filter: `filters.js` emits no segment.
    let (empty, malformed) = parse_filters_param("");
    assert!(empty.is_empty() && malformed.is_empty());
    // Several controls join with `,`, and a value's own comma stays inside
    // its segment.
    let (pair, malformed) = parse_filters_param("status:published,q:a%2Cb");
    assert!(malformed.is_empty(), "got {malformed:?}");
    assert_eq!(pair.get("status").map(String::as_str), Some("published"));
    assert_eq!(pair.get("q").map(String::as_str), Some("a,b"));
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
/// from this through the real parser (`from_cx`), asserting the typed
/// delta — state, not URL bytes.
fn populated_state() -> TableState {
    TableState {
        search: Some("Ada".to_string()),
        sort: Some(Sort {
            column: "name".to_string(),
            descending: true,
        }),
        after: Some("after-cur".to_string()),
        before: Some("before-cur".to_string()),
        filters: HashMap::from([("status".to_string(), "published".to_string())]),
        malformed_filters: vec!["bogus".to_string()],
        group_by: Some("status".to_string()),
        delete: Some("row-1".to_string()),
        open: Some(false),
    }
}

/// Project through an intent and re-parse the URL with the real parser.
fn reparse(url: &str) -> TableState {
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or("");
    TableState::from_cx(&cx_with_query(query))
}

#[test]
fn projection_list_url_round_trips_full_state() {
    // full state including cursors; never `delete`/`open`.
    let source = populated_state();
    let mut expected = source.clone();
    expected.delete = None;
    expected.open = None;
    assert_eq!(reparse(&source.list_url("/admin/users")), expected);
}

#[test]
fn projection_without_search_drops_query() {
    // drops `q` (and its result set's cursors + dialog); keeps
    // the `filters` transport including malformed segments.
    let source = populated_state();
    let mut expected = source.clone();
    expected.search = None;
    expected.after = None;
    expected.before = None;
    expected.delete = None;
    expected.open = None;
    assert_eq!(reparse(&source.without_search("/admin/users")), expected);
}

#[test]
fn projection_without_filters_drops_filters() {
    // drops `filters` and malformed segments (and their result
    // set's cursors + dialog); keeps the search term.
    let source = populated_state();
    let mut expected = source.clone();
    expected.filters = HashMap::new();
    expected.malformed_filters = Vec::new();
    expected.after = None;
    expected.before = None;
    expected.delete = None;
    expected.open = None;
    assert_eq!(reparse(&source.without_filters("/admin/users")), expected);
}

#[test]
fn projection_without_cursor_drops_pagination() {
    // GH #153: drops `after`/`before`; keeps everything else.
    let source = populated_state();
    let mut expected = source.clone();
    expected.after = None;
    expected.before = None;
    expected.delete = None;
    expected.open = None;
    assert_eq!(reparse(&source.without_cursor("/admin/users")), expected);
}

#[test]
fn projection_with_after_sets_forward_cursor() {
    // full state + `after`, drops `before` and the dialog.
    let source = populated_state();
    let mut expected = source.clone();
    expected.after = Some("tok2".to_string());
    expected.before = None;
    expected.delete = None;
    expected.open = None;
    assert_eq!(
        reparse(&source.with_after("/admin/users", "tok2")),
        expected
    );
}

#[test]
fn projection_with_before_sets_backward_cursor() {
    // full state + `before`, drops `after` and the dialog.
    let source = populated_state();
    let mut expected = source.clone();
    expected.after = None;
    expected.before = Some("tok2".to_string());
    expected.delete = None;
    expected.open = None;
    assert_eq!(
        reparse(&source.with_before("/admin/users", "tok2")),
        expected
    );
}

#[test]
fn projection_sorted_by_replaces_sort() {
    // replaces `sort`/`dir`, drops cursors and the dialog.
    let source = populated_state();
    let mut expected = source.clone();
    expected.sort = Some(Sort {
        column: "title".to_string(),
        descending: false,
    });
    expected.after = None;
    expected.before = None;
    expected.delete = None;
    expected.open = None;
    assert_eq!(
        reparse(&source.sorted_by("/admin/users", "title", false)),
        expected
    );
}

#[test]
fn projection_row_url_base_adds_the_delete_dialog_key() {
    // GH #153: full state including cursors + `delete=key`;
    // never `open`.
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
/// the live cursor travels as one wire value, so the browser can
/// never hold `after` and `before` at once — the pair Toasty rejects
/// is unreachable from the live path.
#[test]
fn cursor_wire_carries_at_most_one_direction() {
    assert_eq!(cursor_after("tok"), "after:tok");
    assert_eq!(cursor_before("tok"), "before:tok");
    assert_eq!(cursor_none(), "");
    assert_eq!(
        split_cursor(&cursor_after("tok")),
        (Some("tok".into()), None)
    );
    assert_eq!(
        split_cursor(&cursor_before("tok")),
        (None, Some("tok".into()))
    );
    assert_eq!(split_cursor(&cursor_none()), (None, None));
    // Whitespace from the signal is tolerated, like the GET path's trims.
    assert_eq!(
        split_cursor("  after: tok  "),
        (Some("tok".to_string()), None)
    );
    // A tampered or half-written value degrades to no cursor (GH #110's
    // drop-pagination retry contract) instead of erroring the table.
    assert_eq!(split_cursor("tok"), (None, None));
    assert_eq!(split_cursor("after:"), (None, None));
    assert_eq!(split_cursor("before:"), (None, None));
    // Only the prefix is a direction; a token may contain colons.
    assert_eq!(split_cursor("after:a:b"), (Some("a:b".to_string()), None));
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
