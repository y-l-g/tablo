use topcoat::context::CxTestBuilder;

use super::{super::core::tests::User, *};
use crate::{
    Table, TablePage, TableState, lens,
    table::{Sort, TextColumn},
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
    };
    let html = grouped
        .render_with_state(&cx, page, &state, "/admin/users")
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("on this page"),
        "group header must be page-local, got {html}"
    );
    assert!(
        html.contains("group_by") && html.contains("after=abc"),
        "pager must preserve group_by, got {html}"
    );
}

#[tokio::test]
async fn void_window_links_back_to_first_page() {
    // a cursor past the last row (rows deleted under pagination)
    // must offer navigation, never a pager-less dead end.
    let cx = CxTestBuilder::new().build();
    let tbl = Table::<User>::new(TextColumn::new(lens!(User.name)).sortable()).paginate(1);
    let void_page = TablePage {
        rows: Vec::new(),
        next_cursor: None,
        prev_cursor: None,
    };
    let state = TableState {
        cursor: Some(crate::table::Cursor::After("abc".to_string())),
        sort: Some(Sort {
            column: "name".to_string(),
            descending: false,
        }),
        ..TableState::default()
    };
    let html = tbl
        .render_with_state(&cx, void_page, &state, "/admin/users")
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("Back to first page"),
        "void window must link home, got {html}"
    );

    // A genuinely empty first page stays pager-less (its empty-state
    // already offers Clear links).
    let empty_first = TablePage {
        rows: Vec::new(),
        next_cursor: None,
        prev_cursor: None,
    };
    let html = tbl
        .render_with_state(&cx, empty_first, &TableState::default(), "/admin/users")
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        !html.contains("Back to first page"),
        "empty first page must stay pager-less, got {html}"
    );
}
