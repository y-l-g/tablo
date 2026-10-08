//! Conditional fields over HTTP.
//!
//! A field a condition hides renders in a disabled fieldset, so the browser posts nothing for it.
//! The panel reads a crafted submission the same way: a hidden key is dropped before the parse, so
//! a create takes the field's blank answer and an edit keeps its stored value.

use tablo::{Ability, Auth, Panel, Resource, ResourceDef, Schema, Table, TextColumn, lens};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{TestClient, body_string, mount};

#[derive(Debug, Clone, toasty::Model)]
struct Customer {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    kind: String,
    vat: Option<String>,
}

struct CustomerResource;

impl Resource for CustomerResource {
    type Model = Customer;
    type Form = CustomerForm;

    fn declare() -> ResourceDef<Self> {
        let c = CustomerForm::controls();
        let vat = c.vat.visible_when(&c.kind, ["company"]);
        ResourceDef::new()
            .slug("customers")
            .policy(|_cx: &topcoat::context::Cx, _ability: Ability<'_, Customer>| true)
            .table(Table::new(TextColumn::new(lens!(Customer.name))))
            .form(Schema::new((c.name, c.kind, vat)))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Customer)]
struct CustomerForm {
    name: String,
    kind: String,
    vat: Option<String>,
}

async fn customers() -> (Db, Router) {
    let db = Db::builder()
        .models(toasty::models!(Customer))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    db.push_schema().await.expect("push schema");
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<CustomerResource>(),
    )
    .expect("panel builds");
    (db, router)
}

async fn only_customer(db: &Db) -> Customer {
    let mut db = db.clone();
    let mut all = Customer::all().exec(&mut db).await.unwrap();
    assert_eq!(all.len(), 1, "one customer");
    all.remove(0)
}

/// A create that hides the field drops its posted key: the field takes its blank answer, and a
/// create that shows it writes the posted value.
#[tokio::test]
async fn a_create_drops_the_key_of_a_hidden_field() {
    let (db, router) = customers().await;
    let client = TestClient::new(&router);
    let response = client
        .submit("/admin/customers/create", "name=Ada&kind=person&vat=FR123")
        .await;
    assert!(
        response.status().is_redirection(),
        "the create succeeds, got {} {}",
        response.status(),
        body_string(response).await
    );
    assert_eq!(only_customer(&db).await.vat, None);

    let (db, router) = customers().await;
    let client = TestClient::new(&router);
    let response = client
        .submit(
            "/admin/customers/create",
            "name=Acme&kind=company&vat=FR123",
        )
        .await;
    assert!(response.status().is_redirection());
    assert_eq!(only_customer(&db).await.vat.as_deref(), Some("FR123"));
}

/// An edit that hides the field keeps its stored value, whatever the submission posts for it.
#[tokio::test]
async fn an_edit_keeps_the_stored_value_of_a_hidden_field() {
    let (db, router) = customers().await;
    let client = TestClient::new(&router);
    let response = client
        .submit(
            "/admin/customers/create",
            "name=Acme&kind=company&vat=FR123",
        )
        .await;
    assert!(response.status().is_redirection());
    let id = only_customer(&db).await.id;

    let response = client
        .submit(
            &format!("/admin/customers/{id}/edit"),
            "name=Acme&kind=person&vat=",
        )
        .await;
    assert!(
        response.status().is_redirection(),
        "the edit succeeds, got {} {}",
        response.status(),
        body_string(response).await
    );
    let customer = only_customer(&db).await;
    assert_eq!(customer.kind, "person", "the shown fields are written");
    assert_eq!(
        customer.vat.as_deref(),
        Some("FR123"),
        "the hidden field keeps its stored value"
    );
}
