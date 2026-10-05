//! The Detail pages chapter's free-function snippets. Method overrides live
//! on the canonical resources.

use std::collections::HashMap;

use topcoat::{context::Cx, view::*};

use crate::models::{Audit, Post};

// ANCHOR: detail-view-values
pub fn view_values(_cx: &Cx, audit: &Audit) -> HashMap<String, String> {
    HashMap::from([
        ("action".to_string(), audit.action.clone()),
        ("created_at".to_string(), audit.created_at.to_string()),
    ])
}
// ANCHOR_END: detail-view-values

// ANCHOR: detail-record-label
pub fn record_label(_cx: &Cx, post: &Post) -> Option<String> {
    Some(post.title.clone())
}
// ANCHOR_END: detail-record-label

// ANCHOR: detail-view-content
pub fn view_content<'a>(cx: &'a Cx, post: &Post) -> Option<BoxView<'a>> {
    let words = post.body.split_whitespace().count();
    Some(
        view! {
            cx =>
            <p class="text-sm text-muted-foreground">(format!("{words} words"))</p>
        }
            .boxed(),
    )
}
// ANCHOR_END: detail-view-content
