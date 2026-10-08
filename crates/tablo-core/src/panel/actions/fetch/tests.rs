use toasty::Db;
use topcoat::router::Body;

use super::*;
use crate::{
    Ability, ResourceDef, lens,
    panel::test_support::{Subscriber, mount, panel_for},
};

#[tokio::test]
async fn find_by_key_loads_one_row_scoped_and_404s_malformed() {
    struct SubscriberResource;
    impl Resource for SubscriberResource {
        type Model = Subscriber;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Subscriber.email),
            )))
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
    let cx = crate::test_support::panel_cx::<SubscriberResource>(&db);
    let mut ex = crate::db::db(&cx);

    // Existing id → exactly that row (typed PK filter, not a full scan).
    let got = find_by_key(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &a.id.to_string(),
        &mut ex,
    )
    .await
    .unwrap();
    assert_eq!(got.id, a.id);

    // Well-formed but unknown id → 404.
    let missing = uuid::Uuid::new_v4().to_string();
    assert!(
        find_by_key(
            &cx,
            &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
            &missing,
            &mut ex
        )
        .await
        .is_err(),
        "unknown id must not resolve"
    );

    // Malformed id (not a Uuid) → 404, not a query error.
    assert!(
        find_by_key(
            &cx,
            &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
            "not-a-uuid",
            &mut ex
        )
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

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("pairs")
                .policy(|_cx: &Cx, ability: Ability<'_, Pair>| {
                    matches!(
                        ability,
                        Ability::ViewAny | Ability::View(_) | Ability::Update(_)
                    )
                })
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Pair.name),
                )))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Pair)]
    struct PairForm {
        name: String,
    }
    let db = Db::builder()
        .models(toasty::models!(Pair))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router = mount(db, panel_for::<PairResource>()).expect("panel builds");
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

/// The record pages split on the query they load: the edit page, delete and
/// the write re-loads read the record's own columns through `find_by_key`,
/// while the detail page loads the relations its columns declare, which here
/// include `parent`. The edit load must not pay for the detail page's
/// relations.
#[tokio::test]
async fn record_loads_skip_the_detail_pages_includes() {
    #[derive(Debug, toasty::Model, Clone)]
    struct Parent {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }

    #[derive(Debug, toasty::Model, Clone)]
    struct Child {
        #[key]
        #[auto]
        id: uuid::Uuid,
        #[index]
        parent_id: uuid::Uuid,
        #[belongs_to(key = parent_id, references = id)]
        parent: toasty::Deferred<Parent>,
    }

    struct ChildResource;
    impl Resource for ChildResource {
        type Model = Child;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .table(crate::table::Table::new(crate::table::ComputedColumn::new(
                    "Id",
                    |c: &Child| c.id.to_string(),
                )))
                .view(crate::Detail::new(
                    crate::table::ComputedColumn::new("Parent", |c: &Child| {
                        c.parent.get().name.clone()
                    })
                    .include(Child::fields().parent()),
                ))
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Parent, Child))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let parent = toasty::create!(Parent {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let child = toasty::create!(Child {
        parent_id: parent.id,
    })
    .exec(&mut db)
    .await
    .unwrap();
    let cx = crate::test_support::panel_cx::<ChildResource>(&db);
    let mut ex = crate::db::db(&cx);
    let id = child.id.to_string();

    let resource = crate::resource::require_mounted::<ChildResource>(&cx).unwrap();
    let record = find_by_key(&cx, &resource, &id, &mut ex).await.unwrap();
    assert!(
        record.parent.is_unloaded(),
        "the edit and write loads read only the record's own columns"
    );
    let detail = find_detail(&cx, &resource, &id, &mut ex).await.unwrap();
    assert!(
        !detail.parent.is_unloaded(),
        "the detail load carries its columns' includes"
    );
}
