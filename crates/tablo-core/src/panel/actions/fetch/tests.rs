use toasty::Db;
use topcoat::router::Body;

use super::*;
use crate::panel::test_support::panel_for;

#[tokio::test]
async fn find_by_key_loads_one_row_scoped_and_404s_malformed() {
    use topcoat::context::CxTestBuilder;

    #[derive(Debug, toasty::Model, Clone)]
    struct Subscriber {
        #[key]
        #[auto]
        id: uuid::Uuid,
        #[unique]
        email: String,
    }
    struct SubscriberResource;
    impl Resource for SubscriberResource {
        type Model = Subscriber;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &Cx) -> crate::resource::Table<Subscriber> {
            crate::resource::Table::new(
                |r: &Subscriber| r.id.to_string(),
                crate::resource::TextColumn::r#for(
                    Subscriber::fields().email(),
                    |r: &Subscriber| r.email.clone(),
                ),
            )
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Subscriber))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let a = toasty::create!(Subscriber { email: "a@b.c" })
        .exec(&mut db)
        .await
        .unwrap();
    toasty::create!(Subscriber { email: "z@b.c" })
        .exec(&mut db)
        .await
        .unwrap();
    let cx = CxTestBuilder::new().app_context(db).build();
    let mut ex = crate::db::db(&cx);

    // Existing id → exactly that row (typed PK filter, not a full scan).
    let got = find_by_key::<SubscriberResource>(&cx, &a.id.to_string(), &mut ex)
        .await
        .unwrap();
    assert_eq!(got.id, a.id);

    // Well-formed but unknown id → 404.
    let missing = uuid::Uuid::new_v4().to_string();
    assert!(
        find_by_key::<SubscriberResource>(&cx, &missing, &mut ex)
            .await
            .is_err(),
        "unknown id must not resolve"
    );

    // Malformed id (not a Uuid) → 404, not a query error.
    assert!(
        find_by_key::<SubscriberResource>(&cx, "not-a-uuid", &mut ex)
            .await
            .is_err(),
        "malformed id must not resolve"
    );
}

#[tokio::test]
async fn composite_pk_edit_fails_loudly_not_404() {
    // a composite-PK resource is a programming error the URL
    // scheme cannot serve — 500 with a message, never per-id 404s.

    use crate::resource::Resource;

    #[derive(Debug, Clone, toasty::Model)]
    struct Pair {
        #[key]
        a: String,
        #[key]
        b: String,
        name: String,
    }
    struct PairResource;
    impl Resource for PairResource {
        type Model = Pair;
        type Form = PairForm;
        fn form(_cx: &Cx) -> crate::schema::Schema {
            crate::schema::Schema::new(crate::schema::TextInput::r#for(Pair::fields().name()))
        }

        fn slug() -> String {
            "pairs".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &Pair) -> bool {
            true
        }
        fn can_update(_cx: &Cx, _record: &Pair) -> bool {
            true
        }
        fn table(_cx: &Cx) -> crate::resource::Table<Pair> {
            crate::resource::Table::new(
                |p: &Pair| format!("{}-{}", p.a, p.b),
                crate::resource::TextColumn::r#for(Pair::fields().name(), |p: &Pair| {
                    p.name.clone()
                }),
            )
        }
    }
    #[derive(crate::RecordForm)]
    #[record_form(model = Pair)]
    struct PairForm {
        name: String,
    }
    let db = Db::builder()
        .models(toasty::models!(Pair))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router = panel_for::<PairResource>(db).build().expect("panel builds");
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/pairs/whatever/edit")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        resp.status(),
        http::StatusCode::INTERNAL_SERVER_ERROR,
        "composite PK must fail loudly, got {}",
        resp.status()
    );
}
