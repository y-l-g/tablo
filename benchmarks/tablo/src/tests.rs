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
        "postgresql://localhost/app",
        "postgresql://localhost/",
        "not-a-url",
    ] {
        let refused = std::panic::catch_unwind(|| super::assert_bench_database(bad));
        assert!(refused.is_err(), "{bad:?} must be refused");
    }
}
