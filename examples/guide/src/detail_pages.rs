//! The Detail pages chapter's free-function snippets. Method overrides live
//! on the canonical resources.

use tablo::prelude::*;

use crate::models::{Audit, Post};

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
