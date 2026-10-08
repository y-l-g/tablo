//! The Tables chapter's snippets.

use tablo::{
    extend::{Column, Filter, FilterInput},
    prelude::*,
};
use toasty::stmt::Expr;
use topcoat::{Result, context::Cx, view::*};

use crate::{
    models::{Author, Post, PostStatus, User},
    resources::PostResource,
};

// ANCHOR: post-table-post
pub fn post_table() -> Table<Post> {
    Table::new((
        TextColumn::new(lens!(Post.title)),
        TextColumn::new(lens!(Post.status)),
    ))
    // ANCHOR: table-filters
    .filters((
        SelectFilter::of(Post::fields().status()),
        TernaryFilter::new(Post::fields().featured()),
        DateFilter::new(Post::fields().created_at()),
    ))
    // ANCHOR_END: table-filters
    // ANCHOR: table-group-by
    .group_by(lens!(Post.status))
    // ANCHOR_END: table-group-by
}
// ANCHOR_END: post-table-post

// ANCHOR: table-format
pub fn formatted_columns() {
    TextColumn::new(lens!(User.created_at)).format(|at| at.strftime("%Y-%m-%d").to_string());
}
// ANCHOR_END: table-format

// ANCHOR: table-relation-column
pub fn author_column() -> RelationColumn<Post> {
    RelationColumn::new(relation!(Post.author), |a: &Author| a.name.clone())
}

pub fn comment_count_column() -> CountColumn<Post> {
    CountColumn::new(relation!(Post.comments))
}
// ANCHOR_END: table-relation-column

// ANCHOR: table-custom-column
pub struct Initials;

impl Column<User> for Initials {
    fn name(&self) -> &str {
        "initials"
    }
    fn label(&self) -> &str {
        "Initials"
    }

    // The export's cell, and the table's unless `cell` renders a view.
    fn text(&self, u: &User) -> String {
        u.name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .collect()
    }

    fn cell<'a>(&self, cx: &'a Cx, u: &User) -> BoxView<'a> {
        let text = self.text(u);
        view! { cx => <span class="font-mono">(text)</span> }.boxed()
    }
}
// ANCHOR_END: table-custom-column

// ANCHOR: table-promoted-filter
pub fn promoted_filter() -> QueryFilter<Post> {
    QueryFilter::new("promoted", "Promoted")
        .option("Promoted", Post::fields().featured().eq(true))
        .option("Backlog", Post::fields().featured().eq(false))
}
// ANCHOR_END: table-promoted-filter

// ANCHOR: table-adults-filter
pub struct Adults;

impl Filter<User> for Adults {
    fn name(&self) -> &str {
        "adults"
    }
    fn label(&self) -> &str {
        "Adults"
    }
    fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
        (value == "yes").then(|| User::fields().age().ge(18))
    }
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        input.select(cx, vec![("yes".into(), "Adults only".into())])
    }
}
// ANCHOR_END: table-adults-filter

// ANCHOR: table-publish-action
pub(crate) struct Publish;

impl Action<PostResource> for Publish {
    type Input = ();
    const NAME: &'static str = "publish";

    fn can_run(_cx: &Cx, post: &Post) -> bool {
        post.status != PostStatus::Published
    }

    async fn run(_cx: &Cx, posts: &[Post], _: (), ex: &mut dyn toasty::Executor) -> Result<()> {
        for post in posts {
            Post::filter(Post::fields().id().eq(post.id))
                .update()
                .status(PostStatus::Published)
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}
// ANCHOR_END: table-publish-action

// ANCHOR: table-input-action
/// What changing a post's status asks for.
#[derive(ActionInput)]
pub(crate) struct StatusChange {
    #[form(options)]
    pub status: PostStatus,
    #[form(label = "Feature on the home page")]
    pub featured: bool,
}

pub(crate) struct ChangeStatus;

impl Action<PostResource> for ChangeStatus {
    type Input = StatusChange;
    const NAME: &'static str = "change-status";

    fn label(_cx: &Cx) -> String {
        "Change status".to_string()
    }

    async fn run(
        _cx: &Cx,
        posts: &[Post],
        change: StatusChange,
        ex: &mut dyn toasty::Executor,
    ) -> Result<()> {
        for post in posts {
            Post::filter(Post::fields().id().eq(post.id))
                .update()
                .status(change.status)
                .featured(change.featured)
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}
// ANCHOR_END: table-input-action
