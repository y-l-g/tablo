use jiff::Timestamp;

use super::erase_user;
use crate::{
    Notification,
    auth::{AuthFactor, AuthSession, AuthToken, ColumnPreference, FactorKind, TokenPurpose},
    notification::StoredNotification,
    test_support::memory_db,
    toasty_compat::model::AppSchema,
};

/// The shipped models `erase_user` deletes a user's rows from: each has a `user_id` column, and
/// [`seed`] and [`rows`] cover each.
const USER_ROWS: [&str; 5] = [
    "AuthSession",
    "AuthToken",
    "AuthFactor",
    "ColumnPreference",
    "StoredNotification",
];

/// The shipped models that hold no row `erase_user` could name: the user record itself, which the
/// app deletes, and the throttle counts, keyed by a hash that holds no login in the clear.
const NO_USER_ROWS: [&str; 2] = ["AdminUser", "AuthThrottle"];

/// One row for `user` in each table that names a user.
async fn seed(db: &mut toasty::Db, user: &str) {
    let now = Timestamp::now();
    toasty::create!(AuthSession {
        token_hash: format!("session-{user}"),
        user_id: user,
        panel: "/admin",
        tenant: None,
        expires_at: now,
        created_at: now,
        impersonating: None,
        impersonating_until: None,
    })
    .exec(&mut *db)
    .await
    .expect("the session is stored");
    toasty::create!(AuthToken {
        token_hash: format!("token-{user}"),
        purpose: TokenPurpose::PasswordReset,
        user_id: user,
        panel: "/admin",
        email: None,
        expires_at: now,
        created_at: now,
    })
    .exec(&mut *db)
    .await
    .expect("the token is stored");
    toasty::create!(AuthFactor {
        user_id: user,
        panel: "/admin",
        kind: FactorKind::EmailCode,
        secret: "",
        confirmed_at: None,
        last_step: None,
        failures: 0,
        retry_at: None,
        created_at: now,
    })
    .exec(&mut *db)
    .await
    .expect("the factor is stored");
    toasty::create!(ColumnPreference {
        key: format!("columns-{user}"),
        user_id: user,
        panel: "/admin",
        resource: "posts",
        columns: "",
    })
    .exec(&mut *db)
    .await
    .expect("the column choice is stored");
    toasty::create!(StoredNotification {
        user_id: user,
        panel: "/admin",
        tenant: None,
        notification: toasty::Json(Notification::info("Hello")),
        read_at: None,
        created_at: now,
    })
    .exec(&mut *db)
    .await
    .expect("the notification is stored");
}

/// How many rows each table of [`USER_ROWS`] holds for `user`, in its order.
async fn rows(db: &mut toasty::Db, user: &str) -> [usize; 5] {
    let id = || user.to_owned();
    [
        AuthSession::filter(AuthSession::fields().user_id().eq(id()))
            .exec(&mut *db)
            .await
            .expect("the sessions read")
            .len(),
        AuthToken::filter(AuthToken::fields().user_id().eq(id()))
            .exec(&mut *db)
            .await
            .expect("the tokens read")
            .len(),
        AuthFactor::filter(AuthFactor::fields().user_id().eq(id()))
            .exec(&mut *db)
            .await
            .expect("the factors read")
            .len(),
        ColumnPreference::filter(ColumnPreference::fields().user_id().eq(id()))
            .exec(&mut *db)
            .await
            .expect("the column choices read")
            .len(),
        StoredNotification::filter(StoredNotification::fields().user_id().eq(id()))
            .exec(&mut *db)
            .await
            .expect("the notifications read")
            .len(),
    ]
}

#[tokio::test]
async fn erasing_a_user_deletes_their_rows_and_no_one_elses() {
    let mut db = memory_db(crate::models!()).await;
    seed(&mut db, "ada").await;
    seed(&mut db, "bob").await;

    erase_user(&mut db, "ada").await.expect("the erasure runs");

    assert_eq!(rows(&mut db, "ada").await, [0; 5]);
    assert_eq!(rows(&mut db, "bob").await, [1; 5]);
}

/// An app registers only the models its panel reads: erasure skips the rest.
#[tokio::test]
async fn erasing_skips_a_model_the_db_does_not_register() {
    let mut db = memory_db(toasty::models!(AuthSession)).await;

    erase_user(&mut db, "ada").await.expect("the erasure runs");
}

/// A model a later release ships is one `erase_user` deletes from, or one listed as holding no
/// user's rows: it cannot join [`register_models`](crate::register_models) unseen.
#[tokio::test]
async fn every_shipped_model_is_erased_or_named_as_holding_no_user_rows() {
    let db = memory_db(crate::models!()).await;
    let mut shipped = AppSchema::of_db(&db).table_models();
    shipped.sort();
    let mut known: Vec<String> = USER_ROWS
        .iter()
        .chain(&NO_USER_ROWS)
        .map(|name| (*name).to_owned())
        .collect();
    known.sort();

    assert_eq!(shipped, known);
}
