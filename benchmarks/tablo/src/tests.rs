#[test]
fn bench_database_guard_accepts_bench_names_and_refuses_the_rest() {
    for ok in [
        "postgresql://localhost/tablo_bench",
        "postgresql://u:p@host:5432/my-test-db?sslmode=require",
        "postgresql://localhost/BENCH",
    ] {
        super::assert_bench_database(ok);
    }
    for bad in [
        "postgresql://localhost/production",
        "postgresql://localhost/latest",
        "postgresql://localhost/contest_results",
        "postgresql://localhost/app",
        "postgresql://localhost/",
        "not-a-url",
    ] {
        let refused = std::panic::catch_unwind(|| super::assert_bench_database(bad));
        assert!(refused.is_err(), "{bad:?} must be refused");
    }
}

/// CI builds the harness without running it, so both modes once broke unnoticed: the panel
/// refused to mount, and the bench request answered as no panel. One pass of each catches that.
#[tokio::test]
async fn the_panel_mounts_and_the_bench_request_opens_the_list() {
    use super::*;

    let mut db = Db::builder()
        .models(toasty::models!(Author, Post, Comment))
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    db.push_schema().await.expect("push_schema sqlite");
    let tenant = TenantId::from(uuid::Uuid::new_v4());
    seed_50(&mut db, tenant).await;
    let (_router, panel) = router(db);
    // Each pass asserts the list gate opens and renders the 50 rows.
    assert_eq!(bench_list_path(&panel, tenant, 1).await.len(), 1);
    assert_eq!(bench_query_only(&panel, tenant, 1).await.len(), 1);
}
