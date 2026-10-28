//! Importing a CSV file: each row a create submission, every row written or none.

use http::header::LOCATION;
use showcase::models::{Author, DEMO_TENANT, Post, Role, User};
use tablo::{
    TenantId,
    testing::{ImportError, ImportRefusal, import_errors, import_refusal, notification},
};

use crate::common::{
    TestClient, body_string, demo_client, find_href_with, full_db, post_count,
    routers::router_for_tests as router, seeded_db, tenanted_db, user_count,
};

/// Posts `csv` to `uri` as the import page's file input does.
async fn import(
    client: &TestClient<'_>,
    uri: &str,
    csv: &str,
) -> http::Response<topcoat::router::Body> {
    client
        .submit_multipart(uri, &[("file", Some("import.csv"), csv)])
        .await
}

async fn user(db: &toasty::Db, email: &str) -> Option<User> {
    User::filter(User::fields().email().eq(email.to_string()))
        .first()
        .exec(&mut db.clone())
        .await
        .expect("look up the user")
}

fn error(row: usize, key: &str) -> ImportError {
    ImportError {
        row,
        key: Some(key.to_string()),
    }
}

#[tokio::test]
async fn the_list_links_the_import_of_an_importable_resource_only() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let users = body_string(client.get("/admin/users").await).await;
    assert_eq!(
        find_href_with(&users, "/-/import").as_deref(),
        Some("/admin/users/-/import")
    );
    let authors = body_string(client.get("/admin/authors").await).await;
    assert_eq!(find_href_with(&authors, "/-/import"), None);
    assert_eq!(client.get("/admin/authors/-/import").await.status(), 404);
}

/// A header cell names a column by key or by label; a column the header leaves out takes the
/// field's default, as the create form's untouched control posts it.
#[tokio::test]
async fn a_file_creates_one_record_per_row_and_lands_on_the_list() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let before = user_count(&db).await;

    let resp = import(
        &client,
        "/admin/users/-/import",
        "Name,email,role,AGE\r\n\
         Barbara Liskov,barbara@example.com,admin,72\r\n\
         \"Liskov, Jr.\",junior@example.com,,\r\n\
         ,,,\r\n",
    )
    .await;
    assert_eq!(resp.status(), 303, "a valid file redirects");
    assert_eq!(
        resp.headers().get(LOCATION).and_then(|v| v.to_str().ok()),
        Some("/admin/users")
    );
    assert!(
        notification(&resp).is_some(),
        "the import flashes its count"
    );
    assert_eq!(
        user_count(&db).await,
        before + 2,
        "the blank row creates nothing"
    );

    let barbara = user(&db, "barbara@example.com")
        .await
        .expect("Barbara is imported");
    assert_eq!(
        (
            barbara.name.as_str(),
            barbara.role,
            barbara.active,
            barbara.age
        ),
        ("Barbara Liskov", Role::Admin, true, 72)
    );
    let junior = user(&db, "junior@example.com")
        .await
        .expect("the quoted row is imported");
    assert_eq!(
        (junior.name.as_str(), junior.role, junior.age),
        ("Liskov, Jr.", Role::Member, 0)
    );
}

/// Each row runs the create form's checks: the parse, a control's rule and `validate_record`.
#[tokio::test]
async fn a_refused_row_imports_nothing_and_every_refusal_is_listed() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let before = user_count(&db).await;

    let resp = import(
        &client,
        "/admin/users/-/import",
        "name,email,age\n\
         Valid,valid@example.com,1\n\
         Bad Email,not-an-email,1\n\
         Negative,negative@example.com,-1\n\
         ,nameless@example.com,1\n\
         Typed,typed@example.com,twelve\n",
    )
    .await;
    assert_eq!(resp.status(), 200, "a refused file renders the page again");
    let html = body_string(resp).await;
    assert_eq!(
        import_errors(&html),
        vec![
            error(3, "email"),
            error(4, "age"),
            error(5, "name"),
            error(6, "age")
        ]
    );
    assert_eq!(
        user_count(&db).await,
        before,
        "the valid row is not written either"
    );
}

/// The unique probe runs inside the import's transaction, so a value is checked against the
/// stored records and against the rows before it.
#[tokio::test]
async fn a_unique_value_is_refused_against_the_table_and_the_earlier_rows() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let taken = User::all()
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("a seeded user")
        .email;
    let before = user_count(&db).await;

    let html = body_string(
        import(
            &client,
            "/admin/users/-/import",
            &format!(
                "name,email\nFirst,twice@example.com\nSecond,twice@example.com\nThird,{taken}\n"
            ),
        )
        .await,
    )
    .await;
    assert_eq!(
        import_errors(&html),
        vec![error(3, "email"), error(4, "email")]
    );
    assert_eq!(user_count(&db).await, before);
    // A row written into the doomed transaction still holds its value against the rows after it.
    let html = body_string(
        import(
            &client,
            "/admin/users/-/import",
            "name,email\nBad,not-an-email\nFirst,again@example.com\nSecond,again@example.com\n",
        )
        .await,
    )
    .await;
    assert_eq!(
        import_errors(&html),
        vec![error(2, "email"), error(4, "email")]
    );
    assert_eq!(user_count(&db).await, before);
}

/// No cell writes what the create form does not post: a key, a column Toasty or the tenant
/// stamps, a repeater's rows.
#[tokio::test]
async fn a_header_naming_what_the_form_does_not_take_refuses_the_file() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let (users, posts) = (user_count(&db).await, post_count(&db).await);

    for (uri, csv) in [
        (
            "/admin/users/-/import",
            "name,email,id\nAda,ada2@example.com,1\n",
        ),
        (
            "/admin/users/-/import",
            "name,email,created_at\nAda,ada2@example.com,x\n",
        ),
        (
            "/admin/posts/-/import",
            "title,featured,tenant_id\nT,false,x\n",
        ),
        ("/admin/posts/-/import", "title,featured,links\nT,false,x\n"),
    ] {
        let html = body_string(import(&client, uri, csv).await).await;
        assert_eq!(
            import_refusal(&html),
            Some(ImportRefusal::UnknownColumn),
            "{uri}: {csv}"
        );
    }
    let html =
        body_string(import(&client, "/admin/users/-/import", "name,Name\nAda,Ada\n").await).await;
    assert_eq!(import_refusal(&html), Some(ImportRefusal::RepeatedColumn));
    assert_eq!(
        (user_count(&db).await, post_count(&db).await),
        (users, posts),
        "a refused file writes nothing"
    );
}

#[tokio::test]
async fn a_file_past_the_row_cap_imports_nothing() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let before = user_count(&db).await;
    let mut csv = "name,email\n".to_string();
    // One row past the default cap, `Limits::import_rows`.
    for row in 0..=1_000 {
        csv.push_str(&format!("User {row},user{row}@example.com\n"));
    }

    let html = body_string(import(&client, "/admin/users/-/import", &csv).await).await;
    assert_eq!(import_refusal(&html), Some(ImportRefusal::TooManyRows));
    assert_eq!(user_count(&db).await, before);
}

async fn author(db: &toasty::Db, tenant: TenantId) -> Author {
    Author::filter(Author::fields().tenant_id().eq(tenant))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("a seeded author")
}

/// The tenant column is stamped on each imported row, and a relationship key is checked against
/// the related resource in the request's tenant, as the create form checks it.
#[tokio::test]
async fn an_import_writes_in_the_requests_tenant_only() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await.tenant(t1);
    let (own, other) = (author(&db, t1).await, author(&db, t2).await);
    let before = post_count(&db).await;

    let html = body_string(
        import(
            &client,
            "/admin/posts/-/import",
            &format!(
                "title,featured,author_id\nOwn,false,{}\nOther,false,{}\n",
                own.id, other.id
            ),
        )
        .await,
    )
    .await;
    assert_eq!(import_errors(&html), vec![error(3, "author_id")]);
    assert_eq!(post_count(&db).await, before);

    let resp = import(
        &client,
        "/admin/posts/-/import",
        &format!("title,featured,author_id\nImported,true,{}\n", own.id),
    )
    .await;
    assert_eq!(resp.status(), 303);
    let imported = Post::filter(Post::fields().title().eq("Imported".to_string()))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the post is imported");
    assert_eq!(imported.tenant_id, t1);
}

/// A multiple choice's control posts its key once per value, so its column repeats.
#[tokio::test]
async fn a_repeated_many_to_many_column_links_each_value() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await.tenant(DEMO_TENANT);
    let mut db_q = db.clone();
    let author = author(&db, DEMO_TENANT).await;
    let categories = showcase::models::Category::filter(
        showcase::models::Category::fields()
            .tenant_id()
            .eq(DEMO_TENANT),
    )
    .exec(&mut db_q)
    .await
    .unwrap();
    let (a, b) = (&categories[0], &categories[1]);

    let resp = import(
        &client,
        "/admin/posts/-/import",
        &format!(
            "title,featured,author_id,categories,categories\nFiled,false,{},{},{}\n",
            author.id, a.id, b.id
        ),
    )
    .await;
    assert_eq!(resp.status(), 303);
    let filed = Post::filter(Post::fields().title().eq("Filed".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the post is imported");
    let mut linked: Vec<uuid::Uuid> = showcase::models::PostCategory::filter(
        showcase::models::PostCategory::fields()
            .post_id()
            .eq(filed.id),
    )
    .exec(&mut db_q)
    .await
    .unwrap()
    .into_iter()
    .map(|link| link.category_id)
    .collect();
    linked.sort();
    let mut expected = vec![a.id, b.id];
    expected.sort();
    assert_eq!(linked, expected);
}
