//! The Detail pages chapter's free-function snippets. Method overrides live
//! on the canonical resources.

use tablo::prelude::*;
use topcoat::{
    Result,
    context::Cx,
    view::{View, view},
};

use crate::{
    models::{Audit, Post},
    resources::PostResource,
};

// ANCHOR: detail-no-form
pub fn audit_view() -> Detail<Audit> {
    Detail::new((
        TextColumn::new(lens!(Audit.action)),
        TextColumn::new(lens!(Audit.created_at)),
    ))
}
// ANCHOR_END: detail-no-form

// ANCHOR: detail-computed
pub fn reading_time() -> ComputedColumn<Post> {
    ComputedColumn::new("Reading time", |post: &Post| {
        let words = post.body.split_whitespace().count();
        format!("{words} words, {} min", words.div_ceil(200))
    })
}
// ANCHOR_END: detail-computed

// ANCHOR: record-page
/// How a search engine lists the post: `/admin/posts/{id}/search-preview`.
pub struct SearchPreviewPage;

impl RecordPage<PostResource> for SearchPreviewPage {
    fn declare() -> RecordPageDef<Post> {
        RecordPageDef::new().icon(tablo::ui::icons::SEARCH)
    }

    async fn render(cx: &Cx, post: Post) -> Result<impl View> {
        Ok(view! {
            cx =>
            <p class="text-lg text-primary">(post.seo.title)</p>
            <p class="text-sm text-muted-foreground">(post.seo.description)</p>
        })
    }
}
// ANCHOR_END: record-page
