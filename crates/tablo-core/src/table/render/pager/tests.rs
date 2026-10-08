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
