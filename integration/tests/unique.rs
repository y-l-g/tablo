//! A `unique()` field through the panel: a value another record holds re-renders the form and
//! writes nothing, the record's own value saves, and the probe reads the declared type.

use tablo::{
    Ability, Resource, ResourceDef, Schema, Table, TextColumn, lens,
    testing::{ErrorKind, field_error},
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{TestClient, body_string, panel_router, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Subscriber {
    #[key]
    #[auto]
    id: Uuid,
    #[unique]
    email: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Subscriber)]
struct SubscriberForm {
    email: String,
}

struct SubscriberResource;

impl Resource for SubscriberResource {
    type Model = Subscriber;
    type Form = SubscriberForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("subscribers")
            .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny | Ability::Create))
            .table(Table::new(TextColumn::new(lens!(Subscriber.email))))
            .form(Schema::new(SubscriberForm::controls().email.unique()))
    }
}

/// An empty value is missing, not a duplicate of the empty value: each empty submit re-renders
/// with the presence error, and none writes.
#[tokio::test]
async fn an_empty_unique_value_is_required_and_writes_nothing() {
    let db = test_db(toasty::models!(Subscriber)).await;
    let router = panel_router::<SubscriberResource>(db.clone());
    for submitted in ["email=+", "email="] {
        let response = TestClient::new(&router)
            .submit("/admin/subscribers/-/create", submitted)
            .await;
        assert_eq!(response.status(), 200, "{submitted}");
        let html = body_string(response).await;
        assert_eq!(
            field_error(&html, "email"),
            Some(ErrorKind::Required),
            "{submitted}: {html}"
        );
    }
    let stored = Subscriber::all().exec(&mut db.clone()).await.unwrap();
    assert!(stored.is_empty(), "{stored:?}");
}

/// A unique column the database types as a `Uuid`: SQLite's INTEGER affinity would coerce `01`
/// to `1`, so a whole number would let a text probe pass.
#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Tagged {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    #[unique]
    token: Uuid,
}

#[derive(tablo::RecordForm)]
#[form(model = Tagged)]
struct TaggedForm {
    name: String,
    token: Uuid,
}

struct TaggedResource;

impl Resource for TaggedResource {
    type Model = Tagged;
    type Form = TaggedForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tagged")
            .policy_fn(|_cx, ability| {
                matches!(
                    ability,
                    Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
                )
            })
            .table(Table::new(TextColumn::new(lens!(Tagged.name))))
            .form(Schema::new(TaggedForm::controls().token.unique()))
    }
}

const MINE: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";
const THEIRS: &str = "3f8fad5b-d9cb-469f-a165-70867728950e";

async fn tagged(tokens: &[(&str, &str)]) -> (Db, Router, Vec<Tagged>) {
    let mut db = test_db(toasty::models!(Tagged)).await;
    let mut rows = Vec::new();
    for (name, token) in tokens {
        rows.push(
            toasty::create!(Tagged {
                name: name.to_string(),
                token: Uuid::parse_str(token).unwrap(),
            })
            .exec(&mut db)
            .await
            .unwrap(),
        );
    }
    let router = panel_router::<TaggedResource>(db.clone());
    (db, router, rows)
}

async fn stored(db: &Db) -> Vec<Tagged> {
    Tagged::all().exec(&mut db.clone()).await.unwrap()
}

/// The probe compares the parsed value, so a re-spelled duplicate is still a duplicate.
#[tokio::test]
async fn a_typed_unique_field_probes_the_parsed_value() {
    let (db, router, _) = tagged(&[("one", MINE)]).await;
    let client = TestClient::new(&router);
    let response = client
        .submit(
            "/admin/tagged/-/create",
            &format!("name=two&token={}", MINE.to_uppercase()),
        )
        .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert_eq!(
        field_error(&html, "token"),
        Some(ErrorKind::Invalid),
        "{html}"
    );
    assert_eq!(stored(&db).await.len(), 1);

    let response = client
        .submit(
            "/admin/tagged/-/create",
            &format!("name=two&token={THEIRS}"),
        )
        .await;
    assert_eq!(response.status(), 303);
    assert_eq!(stored(&db).await.len(), 2);
}

/// An edit re-spelling the record's own value saves it canonically; another record's value
/// refuses the edit.
#[tokio::test]
async fn an_edit_keeps_its_own_unique_value() {
    let (db, router, rows) = tagged(&[("mine", MINE), ("theirs", THEIRS)]).await;
    let client = TestClient::new(&router);
    let url = format!("/admin/tagged/{}/edit", rows[0].id);
    let token = |db_rows: Vec<Tagged>| {
        db_rows
            .into_iter()
            .find(|row| row.id == rows[0].id)
            .unwrap()
            .token
    };

    let response = client
        .submit(&url, &format!("name=mine&token={}", MINE.to_uppercase()))
        .await;
    assert!(response.status().is_redirection(), "{}", response.status());
    assert_eq!(token(stored(&db).await), Uuid::parse_str(MINE).unwrap());

    let response = client
        .submit(&url, &format!("name=mine&token={THEIRS}"))
        .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert_eq!(
        field_error(&html, "token"),
        Some(ErrorKind::Invalid),
        "{html}"
    );
    assert_eq!(token(stored(&db).await), Uuid::parse_str(MINE).unwrap());
}
