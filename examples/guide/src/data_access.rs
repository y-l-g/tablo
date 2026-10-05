//! The Data access chapter's snippets.

use tablo::prelude::*;
use tablo_core::{TablePage, TableState};
use toasty::Db;
use topcoat::context::Cx;

use crate::{
    models::{Post, User},
    resources::PostResource,
};

pub async fn load_users(cx: &Cx) -> topcoat::Result<Vec<User>> {
    // ANCHOR: data-access-db
    let mut db = tablo_core::db::db(cx);
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
    let posts = Post::filter(Post::fields().status().eq("published".to_string()))
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
    let table = tablo_core::panel::wired_table::<PostResource>(cx)?;
    let state = TableState::from_cx(cx);
    let query = scoped_query::<PostResource>(cx)?.filter(Post::fields().featured().eq(true));
    let page = TablePage::load(cx, &table, query, &state).await?;
    let body = table
        .render_with_state(cx, page, &state, "/admin/featured")
        .await?;
    // ANCHOR_END: data-access-wired-table
    let _ = body;
    Ok(())
}
