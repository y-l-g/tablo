use topcoat::context::CxTestBuilder;

use super::*;
use crate::test_support::{User, memory_db};

async fn seeded_db() -> Db {
    let mut db = memory_db(toasty::models!(User)).await;

    toasty::create!(User { name: "Ada" })
        .exec(&mut db)
        .await
        .expect("seed user");
    db
}

#[tokio::test]
async fn db_returns_the_app_context_db() {
    let seeded = seeded_db().await;
    let cx = CxTestBuilder::new().app_context(seeded.clone()).build();

    let mut from_helper = db(&cx);

    // The helper must return the same pooled `Db`, so a query through it
    // sees the rows seeded through the original handle.
    let users: Vec<User> = User::all()
        .exec(&mut from_helper)
        .await
        .expect("query users");
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "Ada");
}
