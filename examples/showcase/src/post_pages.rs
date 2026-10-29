//! The posts' record pages: a post as its readers see it, before it is published.

use tablo::{RecordPage, RecordPageDef};
use topcoat::{
    Result,
    context::Cx,
    view::{Unescaped, View, view},
};

use crate::{app::PostResource, models::Post};

/// A post as the public blog renders it, at `/admin/posts/{id}/preview`.
pub struct PostPreviewPage;

impl RecordPage<PostResource> for PostPreviewPage {
    fn declare() -> RecordPageDef<Post> {
        RecordPageDef::new()
            .slug("preview")
            .icon(tablo::ui::icons::EYE)
            // A trashed post has no reader to preview it for.
            .access(|_cx, post: &Post| post.deleted_at.is_none())
    }

    async fn render(cx: &Cx, post: Post) -> Result<impl View> {
        let minutes = read_minutes(word_count(&tablo::rich_text::text(&post.body)));
        Ok(view! {
            cx =>
            <article class="flex max-w-3xl flex-col gap-4 leading-7">
                <h2 class="text-2xl font-semibold tracking-tight">(post.title)</h2>
                <p class="text-sm text-muted-foreground">
                    (minutes)
                    " min read"
                </p>
                (Unescaped::new_unchecked(tablo::rich_text::clean(&post.body)))
            </article>
        })
    }
}

/// Counts the whitespace-separated words in `body`.
pub fn word_count(body: &str) -> usize {
    body.split_whitespace().count()
}

/// Counts the minutes `words` take to read at 200 words per minute.
pub fn read_minutes(words: usize) -> i64 {
    if words == 0 {
        0
    } else {
        words.div_ceil(200) as i64
    }
}
