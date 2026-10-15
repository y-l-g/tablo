use topcoat::context::CxTestBuilder;

use super::super::core::tests::User;
use crate::{
    Table, TablePage, TableState, lens,
    table::{Sort, TextColumn},
    test_support::Html as _,
};

#[tokio::test]
async fn group_by_survives_pager_and_labels_page_local_counts() {
    let cx = CxTestBuilder::new().build();
    let grouped = Table::<User>::new(TextColumn::new(lens!(User.name)).sortable())
        .group_by(lens!(User.name))
        .paginate(1);
    let state = TableState {
        group_by: Some("name".to_string()),
        sort: Some(Sort {
            column: "name".to_string(),
            descending: false,
        }),
        ..TableState::default()
    };
    let rows = vec![User {
        id: uuid::Uuid::nil(),
        name: "Ada".to_string(),
    }];
    let page = TablePage {
        rows,
        next_cursor: Some("abc".to_string()),
        prev_cursor: None,
        position: None,
    };
    let html = grouped
        .render_with_state(&cx, page, &state, "/admin/users")
        .await
        .html(&cx)
        .await;
    assert!(
        html.contains("on this page"),
        "group header must be page-local, got {html}"
    );
    assert!(
        html.contains("group_by") && html.contains("after=abc"),
        "pager must preserve group_by, got {html}"
    );
}

#[test]
fn the_window_keeps_the_ends_and_the_neighbors() {
    use super::page_window;
    let pages = |page, last| {
        page_window(page, last)
            .into_iter()
            .map(|entry| entry.map_or_else(|| "…".to_string(), |n| n.to_string()))
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(pages(1, 1), "1");
    assert_eq!(pages(1, 3), "1 2 3");
    assert_eq!(pages(1, 10), "1 2 … 10");
    // A gap of one page shows the page rather than an ellipsis.
    assert_eq!(pages(4, 10), "1 2 3 4 5 … 10");
    assert_eq!(pages(6, 12), "1 … 5 6 7 … 12");
    assert_eq!(pages(12, 12), "1 … 11 12");
}

/// A numbered page renders its summary and its page links, the current one marked, and a first
/// page renders Previous as no link.
#[tokio::test]
async fn a_numbered_pager_shows_the_summary_and_the_current_page() {
    let cx = CxTestBuilder::new().build();
    let table = Table::<User>::new(TextColumn::new(lens!(User.name)).sortable()).paginate(1);
    let rows = vec![User {
        id: uuid::Uuid::nil(),
        name: "Ada".to_string(),
    }];
    let page = TablePage {
        rows,
        next_cursor: None,
        prev_cursor: None,
        position: Some(crate::table::PagePosition {
            page: 2,
            per_page: 1,
            total: Some(40),
            has_next: true,
        }),
    };
    let state = TableState::from_query("page=2&q=ad");
    let html = table
        .render_with_state(&cx, page, &state, "/admin/users")
        .await
        .html(&cx)
        .await;
    assert!(html.contains("Showing"), "{html}");
    assert!(html.contains("2–2") && html.contains(">40<"), "{html}");
    assert!(html.contains("aria-current=\"page\""), "{html}");
    assert!(
        html.contains("href=\"/admin/users?q=ad&amp;page=3\""),
        "{html}"
    );
    // Page 1 spells no number, and the links keep the search.
    assert!(html.contains("href=\"/admin/users?q=ad\""), "{html}");
    assert!(html.contains("page=40"), "the last page is linked: {html}");
}
