//! The Data access chapter's snippets.

use tablo::prelude::*;
use toasty::Db;
use topcoat::context::Cx;

use crate::{
    models::{Post, PostStatus, User},
    resources::{AuthorResource, CommentResource, PostResource},
};

pub async fn load_users(cx: &Cx) -> topcoat::Result<Vec<User>> {
    // ANCHOR: data-access-db
    let mut db = tablo::db::db(cx);
    let users = User::all().exec(&mut db).await?;
    // ANCHOR_END: data-access-db
    Ok(users)
}

pub fn user_filters(prefix: &str) {
    // ANCHOR: data-access-filters
    User::filter(User::fields().email().eq("ada@example.com"));
    User::filter(User::fields().name().starts_with(prefix)).order_by(User::fields().name().asc());
    // ANCHOR_END: data-access-filters
}

pub async fn load_published(mut db: Db) -> topcoat::Result<Vec<Post>> {
    // ANCHOR: data-access-published
    let posts = Post::filter(Post::fields().status().eq(PostStatus::Published))
        .include(Post::fields().author())
        .exec(&mut db)
        .await?;
    // ANCHOR_END: data-access-published
    Ok(posts)
}

pub async fn load_author_names(mut db: Db) -> topcoat::Result<()> {
    // ANCHOR: data-access-relations
    let posts = Post::all()
        .include(Post::fields().author())
        .exec(&mut db)
        .await?;
    for post in &posts {
        let _name = &post.author.get().name; // no extra query
    }
    // ANCHOR_END: data-access-relations
    Ok(())
}

pub async fn featured_table(cx: &Cx) -> topcoat::Result<()> {
    // ANCHOR: data-access-wired-table
    let table = tablo::panel::wired_table::<PostResource>(cx)?;
    let query = scoped_query::<PostResource>(cx)?.filter(Post::fields().featured().eq(true));
    let body = table.render(cx, query).await?;
    // ANCHOR_END: data-access-wired-table
    let _ = body;
    Ok(())
}

// ANCHOR: data-access-panel
pub fn admin_panel() -> Panel {
    Panel::new("admin")
        .resource::<PostResource>()
        .resource::<CommentResource>()
        .resource::<AuthorResource>()
}
// ANCHOR_END: data-access-panel

pub async fn count_drafts(db: &Db, tenant: uuid::Uuid) -> topcoat::Result<usize> {
    // ANCHOR: data-access-job
    let cx = admin_panel().context(db)?.with(tablo::Tenant(tenant));
    let mut ex = tablo::db::db(&cx);
    let drafts = scoped_query::<PostResource>(&cx)?
        .filter(Post::fields().status().eq(PostStatus::Draft))
        .exec(&mut ex)
        .await?;
    // ANCHOR_END: data-access-job
    Ok(drafts.len())
}
