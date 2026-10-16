//! Toggleable columns: each signed-in user keeps their own choice, a reset stores nothing, and
//! a panel whose lists toggle columns needs the shipped `ColumnPreference` model.

use tablo::{
    Auth, DeclarationErrorKind, Panel, ReadOnly, Resource, ResourceDef, Table, TextColumn,
    auth::{AdminUser, AuthSession, ColumnPreference, hash_password},
    lens,
};
use toasty::Db;
use uuid::Uuid;

use crate::common::{TestClient, body_string, form_body, memory_db, mount, refusal, rows};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    author: String,
    isbn: String,
}

struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("books")
            .policy(ReadOnly)
            .table(Table::new((
                TextColumn::new(lens!(Book.title)),
                TextColumn::new(lens!(Book.author)).toggleable(),
                TextColumn::new(lens!(Book.isbn)).hidden_by_default(),
            )))
    }
}

/// The columns a client's book list shows.
async fn shown(client: &TestClient<'_>) -> Vec<String> {
    tablo::testing::columns(&body_string(client.get("/admin/books").await).await)
}

/// A database holding one book and the admins `ada` and `grace`.
async fn library() -> (Db, AdminUser, AdminUser) {
    let mut db = memory_db(toasty::models!(
        Book,
        AdminUser,
        AuthSession,
        ColumnPreference
    ))
    .await;
    toasty::create!(Book {
        title: "Dune".to_string(),
        author: "Frank Herbert".to_string(),
        isbn: "978-0441013593".to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed a book");
    let admin = |name: &str| {
        toasty::create!(AdminUser {
            email: format!("{name}@example.com"),
            password_hash: hash_password("opensesame").expect("hash"),
            display_name: name.to_string(),
            active: true,
            created_at: jiff::Timestamp::now(),
        })
    };
    let ada = admin("ada").exec(&mut db).await.expect("seed ada");
    let grace = admin("grace").exec(&mut db).await.expect("seed grace");
    (db, ada, grace)
}

fn panel() -> Panel {
    Panel::new("admin")
        .auth(Auth::password())
        .resource::<BookResource>()
}

#[tokio::test]
async fn each_user_keeps_their_own_column_choice() {
    let (db, ada, grace) = library().await;
    let router = mount(db.clone(), panel()).expect("the panel mounts");
    let ada = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let grace = TestClient::new(&router).sign_in(&db, "admin", &grace).await;
    assert_eq!(shown(&ada).await, ["title", "author"]);

    let resp = ada
        .submit("/admin/books/-/columns", &form_body(&[("column", "isbn")]))
        .await;
    assert_eq!(resp.status(), 303);
    assert_eq!(
        shown(&ada).await,
        ["title", "isbn"],
        "ada hid the author and showed the ISBN"
    );
    let html = body_string(ada.get("/admin/books").await).await;
    assert!(
        rows(&html)[0].cells.contains(&"978-0441013593".to_string()),
        "the shown column renders its cell: {:?}",
        rows(&html)
    );
    assert_eq!(
        shown(&grace).await,
        ["title", "author"],
        "ada's choice leaves grace on the defaults"
    );
}

#[tokio::test]
async fn a_reset_restores_the_defaults_and_stores_nothing() {
    let (db, ada, _) = library().await;
    let router = mount(db.clone(), panel()).expect("the panel mounts");
    let ada = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    ada.submit("/admin/books/-/columns", &form_body(&[("column", "isbn")]))
        .await;

    let resp = ada
        .submit("/admin/books/-/columns", &form_body(&[("reset", "1")]))
        .await;
    assert_eq!(resp.status(), 303);
    assert_eq!(shown(&ada).await, ["title", "author"]);
    let stored = ColumnPreference::all()
        .exec(&mut db.clone())
        .await
        .expect("read the preferences");
    assert!(stored.is_empty(), "the defaults keep no row: {stored:?}");
}

#[tokio::test]
async fn toggleable_columns_need_the_preference_model_only_behind_auth() {
    let db = memory_db(toasty::models!(Book, AdminUser, AuthSession)).await;
    let kinds: Vec<DeclarationErrorKind> = refusal(mount(db.clone(), panel()))
        .into_iter()
        .map(|error| error.kind)
        .collect();
    assert_eq!(
        kinds,
        [DeclarationErrorKind::MissingAuthModels {
            models: vec!["ColumnPreference"],
        }]
    );

    // Without auth there is no user to keep a choice for: the list shows the defaults.
    let open = Panel::new("admin")
        .auth(Auth::disabled())
        .resource::<BookResource>();
    let router = mount(db, open).expect("an open panel needs no preference model");
    let html = body_string(TestClient::new(&router).get("/admin/books").await).await;
    assert_eq!(tablo::testing::columns(&html), ["title", "author"]);
    assert_eq!(tablo::testing::column_picker(&html), None);
}
