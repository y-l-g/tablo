use topcoat::{
    context::CxTestBuilder,
    view::{ViewExt, attributes},
};

use super::*;

#[tokio::test]
async fn error_state_renders_title_detail_and_styled_action() {
    let cx = CxTestBuilder::new().build();
    let cx_ref = &cx;
    let action = view! { cx_ref => <a href="/admin/users">"Retry"</a> }.boxed();
    let html = view! {
        cx_ref =>
        error_state(
            title: "Couldn't load Users",
            detail: "Something went wrong while loading the records.",
            action: Some(action.into())
        )
    }
    .single()
    .await
    .unwrap()
    .render(&cx);

    assert!(
        html.contains("Couldn't load Users"),
        "title missing: {html}"
    );
    assert!(
        html.contains("Something went wrong while loading the records."),
        "detail missing: {html}"
    );
    // The caller's action reaches the output intact (the classes
    // that style it are paint; the href is the caller's own markup).
    assert!(
        html.contains("Retry") && html.contains("href=\"/admin/users\""),
        "caller action missing: {html}"
    );
    // Icon present; its colour is paint.
    assert!(html.contains("<svg"), "icon missing: {html}");
}

#[tokio::test]
async fn error_state_detail_is_optional_and_attrs_survive() {
    let cx = CxTestBuilder::new().build();
    let cx_ref = &cx;
    let html = view! {
        cx_ref =>
        error_state(
            title: "Couldn't load Users",
            attrs: attributes! { id="load-error" class="my-class" }
        )
    }
    .single()
    .await
    .unwrap()
    .render(&cx);

    assert!(
        html.contains("Couldn't load Users"),
        "title missing: {html}"
    );
    // No detail line: the title is the only `<p>` element the component
    // renders when `detail` is empty. Spelled `<p ` / `<p>` so the icon's
    // `<path>` cannot count as one.
    assert_eq!(
        html.matches("<p ").count() + html.matches("<p>").count(),
        1,
        "no detail expected: {html}"
    );
    assert!(html.contains("id=\"load-error\""), "attrs missing: {html}");
    assert!(html.contains("my-class"), "caller class missing: {html}");
}
