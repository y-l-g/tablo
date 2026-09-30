use topcoat::context::CxTestBuilder;

use super::{super::core::tests::User, *};
use crate::{TablePage, TextColumn};

#[test]
fn live_search_debounce_sits_in_the_locked_band() {
    // GH #172 decision 4: ~150-250ms at the `@input` handler. The
    // markup test below pins the rendered value; this pins the range.
    assert!(
        (150..=250).contains(&LIVE_SEARCH_DEBOUNCE_MS),
        "debounce must sit in the 150-250ms band, got {LIVE_SEARCH_DEBOUNCE_MS}"
    );
}

#[tokio::test]
async fn bulk_checkboxes_render_with_keys_and_select_all() {
    let cx = CxTestBuilder::new().build();
    let bulk_table = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    )
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
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    // Per-row checkbox carries the record key; header select-all present.
    for row in &rows {
        assert!(
            html.contains(&format!("value=\"{}\"", row.id)),
            "missing checkbox value for {} in {html}",
            row.id
        );
    }
    assert!(
        html.contains("data-row-select"),
        "missing row checkbox marker in {html}"
    );
    assert!(
        html.contains("data-bulk-select-all"),
        "missing select-all in {html}"
    );
    // Bulk form keeps the hidden `ids` transport and a submit
    // that ships disabled until `bulk.js` sees a checked row.
    assert!(
        html.contains("data-bulk-form"),
        "missing bulk form in {html}"
    );
    assert!(
        html.contains("name=\"ids\"") && !html.contains("ids comma-separated"),
        "missing hidden ids transport in {html}"
    );
    // the destructive write is gated by the confirmation dialog
    // rather than by a disabled control — the trigger opens it, and the
    // dialog's own submit carries `confirm=1` inside the same form.
    assert!(
        html.contains("data-bulk-confirm-trigger"),
        "missing the bulk confirm trigger in {html}"
    );
    assert!(
        html.contains("data-bulk-confirm-dialog"),
        "missing the bulk confirm dialog in {html}"
    );
    assert!(
        html.contains("name=\"confirm\"") && html.contains("value=\"1\""),
        "the dialog must carry the confirm marker in {html}"
    );
    // Rendered closed: it is opened client-side so that opening it is not
    // a result-set change. Matched as `open="` rather than `open`, because
    // the dialog's class carries Tailwind's `open:` state variants.
    let dialog_at = html
        .find("data-bulk-confirm-dialog")
        .expect("the dialog marker");
    let dialog_tag_start = html[..dialog_at].rfind("<dialog").expect("its <dialog>");
    let dialog_tag_end = html[dialog_tag_start..].find('>').expect("the tag's end");
    let dialog_tag = &html[dialog_tag_start..dialog_tag_start + dialog_tag_end];
    assert!(
        !dialog_tag.contains("open=\""),
        "the bulk confirm dialog must render closed, got {dialog_tag}"
    );
    // `dialog.js` refuses to dismiss an alert dialog on a backdrop
    // click, so the role is the contract that keeps the confirm dialog
    // waiting for an answer rather than treating a stray click as one.
    assert!(
        dialog_tag.contains("role=\"alertdialog\""),
        "the bulk confirm dialog must be an alert dialog, got {dialog_tag}"
    );
    // The dialog is the decision, not decoration: it asks, and
    // it offers a way out that is not deleting. Absorbed from the showcase
    // duplicate so the one test that owns bulk chrome owns all
    // of it.
    assert!(
        html.contains("Delete the selected records?"),
        "the dialog must ask before it deletes, got {html}"
    );
    assert!(
        html.contains("data-dialog-close"),
        "the dialog needs a way out that is not deleting, got {html}"
    );
    // The confirm control rides inside the bulk form, so the confirmed
    // submit ships it with the same payload as the selection: `bulk.js`
    // closes over `trigger.closest('form[data-bulk-form]')`, so a dialog
    // outside the form would be decoration a crafted request skips.
    let form_at = html.find("data-bulk-form").expect("the bulk form");
    assert!(
        form_at < dialog_at,
        "the dialog must live inside the bulk form, got {html}"
    );
    assert!(
        html.contains("Delete selected"),
        "missing bulk button in {html}"
    );
    assert!(
        html.contains("data-table-root"),
        "missing table root scope in {html}"
    );

    // Without bulk: no checkboxes, no bulk form.
    let plain = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    );
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
        !html.contains("data-row-select") && !html.contains("data-bulk-form"),
        "plain table must not render bulk chrome in {html}"
    );
}
