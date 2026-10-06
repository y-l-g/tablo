use topcoat::context::CxTestBuilder;

use super::{super::core::tests::User, *};
use crate::{Table, TablePage, TextColumn, lens};

#[tokio::test]
async fn bulk_checkboxes_render_with_keys_and_select_all() {
    let cx = CxTestBuilder::new().build();
    let bulk_table = Table::<User>::new(TextColumn::new(lens!(User.name)))
        .wired()
        .with_delete("/admin/users".to_string())
        .with_bulk_delete(true);
    let rows = vec![
        User {
            id: uuid::Uuid::new_v4(),
            name: "Ada".to_string(),
        },
        User {
            id: uuid::Uuid::new_v4(),
            name: "Bob".to_string(),
        },
    ];
    let page: TablePage<User> = rows.clone().into();
    let html = bulk_table
        .render_loaded(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    // Each row checkbox and the select-all read the bulk signal.
    assert_eq!(
        html.matches("aria-label=\"Select row\"").count(),
        rows.len(),
        "one checkbox per row in {html}"
    );
    assert!(
        html.contains("aria-label=\"Select all rows\"")
            && html.contains("data-topcoat-bind:checked")
            && html.contains("data-topcoat-bind:indeterminate"),
        "missing the bound select-all in {html}"
    );
    // The write form carries the selection and the confirmation marker the handler requires.
    let form_at = html.find("id=\"table-writes\"").expect("the write form");
    assert!(
        html[form_at..].contains("name=\"ids\"")
            && html[form_at..].contains("name=\"confirm\" value=\"1\""),
        "the write form must carry the selection and the confirm marker in {html}"
    );
    // The dialog renders closed and asks for an answer; the bulk delete opens it, disabled
    // while nothing is selected.
    let dialog = &html[html[..form_at].rfind("<dialog").expect("the dialog")..form_at];
    assert!(
        dialog.contains("role=\"alertdialog\"") && !dialog.contains(" open=\"\""),
        "the confirm dialog must render closed as an alert dialog, got {dialog}"
    );
    assert!(
        html.contains("Delete the selected records?") && html.contains("Delete selected"),
        "the bulk delete must ask first in {html}"
    );
    assert!(
        html.contains("data-topcoat-bind:disabled"),
        "the bulk delete must follow the selection in {html}"
    );

    // Without bulk: no checkboxes, no bulk form.
    let plain = Table::<User>::new(TextColumn::new(lens!(User.name)));
    let page: TablePage<User> = rows.into();
    let html = plain
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        !html.contains("Select row") && !html.contains("table-writes"),
        "plain table must not render bulk chrome in {html}"
    );
}
