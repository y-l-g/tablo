use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use topcoat::context::Cx;

use super::*;
use crate::test_support::tableless_db;

/// A counter, and work that bumps it.
fn counter() -> (Arc<AtomicUsize>, impl FnOnce() + Send + 'static) {
    let count = Arc::new(AtomicUsize::new(0));
    let bump = Arc::clone(&count);
    (count, move || {
        bump.fetch_add(1, Ordering::SeqCst);
    })
}

/// Work queued on a write runs once it commits, and is dropped with a write that rolls back.
#[tokio::test]
async fn queued_work_runs_on_commit_and_never_on_rollback() {
    let mut db = tableless_db(toasty::models!()).await;
    let cx = Cx::default();

    let (committed, bump) = counter();
    let tx = WriteTx::begin(&cx, &mut db).await.unwrap();
    assert!(after_write_commits(cx.id(), bump).is_ok());
    assert_eq!(committed.load(Ordering::SeqCst), 0, "not before the commit");
    tx.commit().await.unwrap();
    assert_eq!(committed.load(Ordering::SeqCst), 1);

    let (rolled_back, bump) = counter();
    let tx = WriteTx::begin(&cx, &mut db).await.unwrap();
    assert!(after_write_commits(cx.id(), bump).is_ok());
    drop(tx);
    assert_eq!(rolled_back.load(Ordering::SeqCst), 0);

    let (_, bump) = counter();
    assert!(
        after_write_commits(cx.id(), bump).is_err(),
        "with no write open the work comes back"
    );
}

/// Another request's write leaves this one free to take a connection.
#[tokio::test]
async fn the_guard_holds_only_for_the_request_that_opened_the_write() {
    let mut db = tableless_db(toasty::models!()).await;
    let (writing, other) = (Cx::default(), Cx::default());
    let _tx = WriteTx::begin(&writing, &mut db).await.unwrap();

    assert!(no_write_open(other.id(), "db(cx)").is_ok());
    assert!(no_write_open(writing.id(), "db(cx)").is_err());
    let (_, bump) = counter();
    assert!(after_write_commits(other.id(), bump).is_err());
}
