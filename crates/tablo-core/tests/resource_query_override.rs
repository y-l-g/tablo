use tablo_core::Resource;
use toasty::Db;
use topcoat::context::{Cx, CxTestBuilder};

#[derive(Debug, toasty::Model, Clone)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

struct Everyone;

impl Resource for Everyone {
    type Model = User;
    type Form = tablo_core::NoForm<Self::Model>;

    fn table() -> tablo_core::Table<User> {
        tablo_core::Table::new(
            |u: &User| u.id.to_string(),
            tablo_core::TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone()),
        )
    }
}

struct JustAda;

impl Resource for JustAda {
    type Model = User;
    type Form = tablo_core::NoForm<Self::Model>;

    fn table() -> tablo_core::Table<User> {
        tablo_core::Table::new(
            |u: &User| u.id.to_string(),
            tablo_core::TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone()),
        )
    }

    fn query(_cx: &Cx) -> toasty::stmt::Query<toasty::stmt::List<User>> {
        toasty::stmt::Query::<toasty::stmt::List<User>>::all()
            .filter(User::fields().name().eq("Ada"))
    }
}

#[tokio::test]
async fn query_override_scopes_rows() {
    let mut db = Db::builder()
        .models(toasty::models!(User))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    toasty::create!(User {
        name: "Ada".to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(User {
        name: "Bob".to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();

    let cx = CxTestBuilder::new().app_context(db).build();
    let mut db = tablo_core::db::db(&cx);

    let all = Everyone::query(&cx).exec(&mut db).await.unwrap();
    assert_eq!(all.len(), 2);

    let ada_only = JustAda::query(&cx).exec(&mut db).await.unwrap();
    assert_eq!(ada_only.len(), 1);
    assert_eq!(ada_only[0].name, "Ada");
}
