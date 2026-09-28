//! GH #306: a dropped, undrained streamed list body releases its pooled
//! connection, so the next request serves instead of waiting on the pool.

use tablo_core::{
    Auth,
    auth::{AdminUser, AuthSession},
    resource::{Resource, Table, TextColumn},
};
use topcoat::context::Cx;
use uuid::Uuid;

use crate::common::{body_string, get_with_cookies, memory_db, router_with};

#[derive(Debug, Clone, toasty::Model)]
struct PoolDummy {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

struct PoolResource;

impl Resource for PoolResource {
    type Model = PoolDummy;

    fn slug() -> String {
        "dummies".to_string()
    }

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }

    fn can_view(_cx: &Cx, _record: &PoolDummy) -> bool {
        true
    }

    fn table(cx: &Cx) -> Table<PoolDummy> {
        Table::<PoolDummy>::r#for(cx)
            .key(|d: &PoolDummy| d.id.to_string())
            .columns(TextColumn::r#for(
                PoolDummy::fields().name(),
                |d: &PoolDummy| d.name.clone(),
            ))
            .paginate(25)
    }
}

/// A session cookie for `email`, minted directly into `db`.
async fn mint_session(db: &toasty::Db, email: &str) -> (String, String) {
    use std::fmt::Write as _;

    let mut db = db.clone();
    let user = AdminUser::filter(AdminUser::fields().email().eq(email.to_string()))
        .first()
        .exec(&mut db)
        .await
        .expect("look up the seeded admin")
        .expect("the seed creates the admin");
    let token = topcoat::session::Token::random();
    let mut token_hash = String::with_capacity(64);
    for byte in token.hash().iter() {
        write!(token_hash, "{byte:02x}").expect("writing to a String cannot fail");
    }
    toasty::create!(AuthSession {
        token_hash,
        user_id: user.id.to_string(),
        expires_at: jiff::Timestamp::now() + std::time::Duration::from_secs(3600),
        created_at: jiff::Timestamp::now(),
    })
    .exec(&mut db)
    .await
    .expect("mint the session row");
    (tablo_test::SESSION_COOKIE.to_string(), token.encode())
}

#[tokio::test]
async fn a_dropped_list_body_frees_the_pool_for_the_next_request() {
    let mut db = memory_db(toasty::models!(PoolDummy, AdminUser, AuthSession)).await;
    toasty::create!(PoolDummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed a row");
    toasty::create!(AdminUser {
        email: "ada@example.com".to_string(),
        password_hash: "x".to_string(),
        display_name: "Ada".to_string(),
        active: true,
        tenant_id: None,
        created_at: jiff::Timestamp::now(),
    })
    .exec(&mut db)
    .await
    .expect("seed an admin");
    let session = mint_session(&db, "ada@example.com").await;
    let router = router_with::<PoolResource>(db, Auth::password());

    // Abandon the streamed list body without draining it.
    let first = get_with_cookies(
        &router,
        "/admin/dummies",
        &[(&session.0, session.1.clone())],
    )
    .await;
    assert!(first.status().is_success());
    drop(first.into_body());

    // The next session-resolving request serves instead of waiting on the
    // pool. The timeout bounds the wait: a regression hangs here, and the
    // failure names the pool instead of stalling the suite.
    let second = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        get_with_cookies(&router, "/admin/dummies", &[(&session.0, session.1)]),
    )
    .await
    .expect("the next request must not wait on the pool");
    assert!(second.status().is_success());
    let html = body_string(second).await;
    assert!(
        html.contains("Ada"),
        "the next request must render rows, got {html}"
    );
}
