use topcoat::{
    context::CxTestBuilder,
    view::{ViewExt, attributes},
};

use super::*;
use crate::{card_header, card_title};

#[tokio::test]
async fn panel_is_border_only_and_forwards_attrs() {
    let cx = CxTestBuilder::new().build();
    let cx_ref = &cx;
    let html = view! {
        cx_ref =>
        panel(
            attrs: attributes! { id="page-panel" class="my-class" },
            card_header(card_title("Upload"))
        )
    }
    .single()
    .await
    .unwrap()
    .render(&cx);

    assert!(html.contains("Upload"), "title missing: {html}");
    assert!(html.contains("id=\"page-panel\""), "attrs missing: {html}");
    assert!(html.contains("my-class"), "caller class missing: {html}");
    assert!(
        !html.contains("bg-card"),
        "page panel draws only its border: {html}"
    );
    assert!(
        !html.contains("shadow-sm"),
        "page panel carries no shadow: {html}"
    );
}
