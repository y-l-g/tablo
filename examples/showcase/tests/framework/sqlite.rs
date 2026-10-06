use toasty::Db;

#[derive(Debug, toasty::Model)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,

    name: String,

    #[unique]
    email: String,
}

async fn roundtrip_db() -> Db {
    let db = Db::builder()
        .models(toasty::models!(User))
        .connect("sqlite::memory:")
        .await
        .expect("connect to in-memory sqlite");
    db.push_schema().await.expect("push schema");
    db
}

#[tokio::test]
async fn unique_collation_matches_the_app_side_probe() {
    let mut db = roundtrip_db().await;

    toasty::create!(User {
        name: "Alice",
        email: "alice@example.com",
    })
    .exec(&mut db)
    .await
    .expect("create Alice");

    let exact = User::all()
        .filter(User::fields().email().eq("alice@example.com"))
        .exec(&mut db)
        .await
        .expect("probe by exact value");
    assert_eq!(exact.len(), 1, "the probe must find the stored value");

    toasty::create!(User {
        name: "Alice (cased)",
        email: "Alice@example.com",
    })
    .exec(&mut db)
    .await
    .expect("a case variant is a distinct value under the default collation");

    let cased_probe = User::all()
        .filter(User::fields().email().eq("ALICE@EXAMPLE.COM"))
        .exec(&mut db)
        .await
        .expect("probe by upper-cased value");
    assert!(
        cased_probe.is_empty(),
        "the index and the probe agree that case variants are distinct — \
         a non-BINARY column collation would break this agreement"
    );
}
