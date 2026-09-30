use topcoat::{
    context::CxTestBuilder,
    view::{ViewExt, attributes},
};

use super::*;

#[tokio::test]
async fn empty_state_renders_title_detail_and_action() {
    let cx = CxTestBuilder::new().build();
    let cx_ref = &cx;
    let action = view! { cx_ref => <a href="/admin/users">"Clear search"</a> }.boxed();
    let html = view! {
        cx_ref =>
        empty_state(
            title: "No records yet",
            detail: "Create the first one.",
            action: Some(action.into()),
            attrs: attributes! { data-empty="" }
        )
    }
    .single()
    .await
    .unwrap()
    .render(&cx);

    assert!(html.contains("No records yet"), "title missing: {html}");
    assert!(
        html.contains("Create the first one."),
        "detail missing: {html}"
    );
    assert!(
        html.contains("href=\"/admin/users\"") && html.contains("Clear search"),
        "caller action missing: {html}"
    );
    assert!(html.contains("data-empty"), "attrs dropped: {html}");
    // The icon is decoration: hidden from assistive tech.
    assert!(
        html.contains("aria-hidden=\"true\"") && html.contains("<svg"),
        "decorative icon missing: {html}"
    );
}

#[tokio::test]
async fn empty_state_detail_and_action_are_optional() {
    let cx = CxTestBuilder::new().build();
    let cx_ref = &cx;
    let html = view! { cx_ref => empty_state(title: "No records yet") }
        .single()
        .await
        .unwrap()
        .render(&cx);
    // Title only: one paragraph, no action wrapper.
    assert_eq!(html.matches("<p class").count(), 1, "got {html}");
    assert!(!html.contains("<a"), "got {html}");
}
