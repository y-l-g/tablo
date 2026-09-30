use topcoat::context::CxTestBuilder;

use super::*;
use crate::test_support::User;

async fn seeded_db() -> Db {
    let mut db = Db::builder()
        .models(toasty::models!(User))
        .connect("sqlite::memory:")
        .await
        .expect("connect to in-memory sqlite");
    db.push_schema().await.expect("push schema");

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

#[test]
fn unavailable_maps_infra_failures_to_an_opaque_error() {
    // GH #174: driver text is for the logs, never the error page.
    let err = super::unavailable("secret driver gunk: no such table");
    let rendered = err.to_string();
    assert!(
        rendered.contains("database unavailable"),
        "the opaque message must survive, got {rendered}"
    );
    assert!(
        !rendered.contains("gunk"),
        "driver text must not leak, got {rendered}"
    );
}

/// GH #229: a record hook that fails at the driver is an infra failure, so
/// the write surfaces the opaque mapping — the property
/// `unavailable_maps_infra_failures_to_an_opaque_error` pins, reached
/// through the hook seam.
#[test]
fn hook_failure_maps_driver_errors_to_an_opaque_error() {
    let err = super::hook_failure(
        toasty::Error::from_args(format_args!("secret driver gunk: no such table")).into(),
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains("database unavailable"),
        "the opaque message must survive, got {rendered}"
    );
    assert!(
        !rendered.contains("gunk"),
        "driver text must not leak, got {rendered}"
    );
}

/// GH #229 must not undo GH #174: an app-authored hook error is not the
/// driver's, so it keeps its own mapping — a guard's 404 stays a 404
/// rather than becoming the opaque 500.
#[test]
fn hook_failure_keeps_an_app_error_intact() {
    let guard: topcoat::Error = topcoat::router::error::not_found().into();
    assert!(
        super::hook_failure(guard).is::<topcoat::router::error::NotFoundError>(),
        "an app-hook error must keep its own mapping"
    );
}
