//! Conditional fields over HTTP.
//!
//! A field a condition hides, or a control a condition disables, renders in a disabled fieldset,
//! so the browser posts nothing for it. The panel reads a crafted submission the same way: the key
//! is dropped before the parse, so a create takes the field's blank answer, or a disabled field's
//! default, and an edit keeps its stored value.

use tablo::{
    Allow, Auth, Panel, Resource, ResourceDef, Schema, Tab, Table, Tabs, TextColumn, Uploader, lens,
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{TestClient, body_string, form_body, input_value, mount, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Customer {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    kind: String,
    vat: Option<String>,
    logo: String,
}

/// Holds every path it is asked about, so a carried `keep_logo` restores.
struct KeepAll;

impl Uploader for KeepAll {
    async fn keep(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }

    async fn store(&self, filename: &str, _bytes: &[u8]) -> Result<String, String> {
        Ok(format!("/uploads/{filename}"))
    }

    async fn holds(&self, _path: &str) -> bool {
        true
    }
}

struct CustomerResource;

impl Resource for CustomerResource {
    type Model = Customer;
    type Form = CustomerForm;

    fn declare() -> ResourceDef<Self> {
        let c = CustomerForm::controls();
        let vat = c.vat.visible_when(&c.kind, ["company"]);
        let logo = c.logo.visible_when(&c.kind, ["company"]);
        ResourceDef::new()
            .slug("customers")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Customer.name))))
            .form(Schema::new((c.name, c.kind, vat, logo)))
    }
}

/// The same customers, the company's fields in a tab a condition shows.
struct TabbedCustomerResource;

impl Resource for TabbedCustomerResource {
    type Model = Customer;
    type Form = CustomerForm;

    fn declare() -> ResourceDef<Self> {
        let c = CustomerForm::controls();
        let company = Tab::new("Company")
            .schema((c.vat, c.logo))
            .visible_when(&c.kind, ["company"]);
        ResourceDef::new()
            .slug("tabbed-customers")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Customer.name))))
            .form(Schema::new(Tabs::new([
                Tab::new("Profile").schema((c.name, c.kind)),
                company,
            ])))
    }
}

/// The same customers, the VAT number locked for a person.
struct LockedCustomerResource;

impl Resource for LockedCustomerResource {
    type Model = Customer;
    type Form = CustomerForm;

    fn declare() -> ResourceDef<Self> {
        let c = CustomerForm::controls();
        let vat = c.vat.default("none").disabled_when(&c.kind, ["person"]);
        ResourceDef::new()
            .slug("locked-customers")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Customer.name))))
            .form(Schema::new((c.name, c.kind, vat, c.logo)))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Customer)]
struct CustomerForm {
    name: String,
    kind: String,
    vat: Option<String>,
    #[form(file, optional)]
    logo: String,
}

async fn customers() -> (Db, Router) {
    let db = test_db(toasty::models!(Customer)).await;
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .uploads(KeepAll)
            .resource::<CustomerResource>()
            .resource::<TabbedCustomerResource>()
            .resource::<LockedCustomerResource>(),
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

/// The browser neither validates nor posts a hidden field: its fieldset is disabled exactly while
/// the condition hides it.
#[tokio::test]
async fn a_hidden_field_renders_in_a_disabled_fieldset() {
    let (db, router) = customers().await;
    let client = TestClient::new(&router);
    let vat_fieldset = |html: &str| {
        let input = html.find("name=\"vat\"").expect("the vat input");
        let start = html[..input].rfind("<fieldset").expect("its fieldset");
        html[start..start + html[start..].find('>').unwrap()].to_string()
    };

    let create = body_string(client.get("/admin/customers/-/create").await).await;
    assert!(vat_fieldset(&create).contains(" disabled"), "{create}");

    let response = client
        .submit(
            "/admin/customers/-/create",
            "name=Acme&kind=company&vat=FR1",
        )
        .await;
    assert!(response.status().is_redirection());
    let edit = format!("/admin/customers/{}/edit", only_customer(&db).await.id);
    let html = body_string(client.get(&edit).await).await;
    assert!(!vat_fieldset(&html).contains(" disabled"), "{html}");
}

/// A create that hides the field drops its posted key: the field takes its blank answer, and a
/// create that shows it writes the posted value.
#[tokio::test]
async fn a_create_drops_the_key_of_a_hidden_field() {
    let (db, router) = customers().await;
    let client = TestClient::new(&router);
    let response = client
        .submit(
            "/admin/customers/-/create",
            "name=Ada&kind=person&vat=FR123",
        )
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
            "/admin/customers/-/create",
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
            "/admin/customers/-/create",
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

/// A file field's carried upload is posted outside its disabled fieldset, so a hidden file field
/// drops it with its own key.
#[tokio::test]
async fn a_hidden_file_field_drops_its_carried_upload() {
    for (kind, logo) in [("company", "/uploads/logo.png"), ("person", "")] {
        let (db, router) = customers().await;
        let client = TestClient::new(&router).csrf(&Uuid::new_v4().to_string());
        // A refused submission carries the upload it stored.
        let refused = client
            .submit_multipart(
                "/admin/customers/-/create",
                &[
                    ("name", None, ""),
                    ("kind", None, "company"),
                    ("logo", Some("logo.png"), "PNG"),
                ],
            )
            .await;
        let carried =
            input_value(&body_string(refused).await, "keep_logo").expect("the upload is carried");
        let body = form_body(&[("name", "Acme"), ("kind", kind), ("keep_logo", &carried)]);
        let response = client.submit("/admin/customers/-/create", &body).await;
        assert!(
            response.status().is_redirection(),
            "kind={kind}: the create succeeds, got {} {}",
            response.status(),
            body_string(response).await
        );
        assert_eq!(only_customer(&db).await.logo, logo, "kind={kind}");
    }
}

/// An edit that does not post the watched field reads its stored value, as the parse does.
#[tokio::test]
async fn an_edit_without_the_watched_field_reads_its_stored_value() {
    let (db, router) = customers().await;
    let client = TestClient::new(&router);
    let response = client
        .submit(
            "/admin/customers/-/create",
            "name=Acme&kind=company&vat=FR1",
        )
        .await;
    assert!(response.status().is_redirection());
    let id = only_customer(&db).await.id;

    let response = client
        .submit(&format!("/admin/customers/{id}/edit"), "vat=FR2")
        .await;
    assert!(
        response.status().is_redirection(),
        "the edit succeeds, got {} {}",
        response.status(),
        body_string(response).await
    );
    assert_eq!(only_customer(&db).await.vat.as_deref(), Some("FR2"));
}

/// A tab's condition hides every field the tab holds: their fieldset is disabled while it fails,
/// and a create that hides them drops their keys.
#[tokio::test]
async fn a_tab_a_condition_hides_drops_its_fields() {
    let (db, router) = customers().await;
    let client = TestClient::new(&router);
    let create = body_string(client.get("/admin/tabbed-customers/-/create").await).await;
    let input = create.find("name=\"vat\"").expect("the vat input");
    let start = create[..input].rfind("<fieldset").expect("its fieldset");
    let fieldset = &create[start..start + create[start..].find('>').unwrap()];
    assert!(fieldset.contains(" disabled"), "{create}");

    let response = client
        .submit(
            "/admin/tabbed-customers/-/create",
            "name=Ada&kind=person&vat=FR123",
        )
        .await;
    assert!(response.status().is_redirection());
    assert_eq!(only_customer(&db).await.vat, None);
}

/// The fieldset around a control a condition disables is disabled exactly while the condition
/// holds, and the control still renders.
#[tokio::test]
async fn a_disabled_control_renders_in_a_disabled_fieldset() {
    let (db, router) = customers().await;
    let client = TestClient::new(&router);
    let vat_fieldset = |html: &str| {
        let input = html.find("name=\"vat\"").expect("the vat input");
        let start = html[..input].rfind("<fieldset").expect("its fieldset");
        html[start..start + html[start..].find('>').unwrap()].to_string()
    };
    for (kind, disabled) in [("person", true), ("company", false)] {
        let mut connection = db.clone();
        Customer::all()
            .delete()
            .exec(&mut connection)
            .await
            .unwrap();
        let body = format!("name=Ada&kind={kind}&vat=FR1");
        let response = client
            .submit("/admin/locked-customers/-/create", &body)
            .await;
        assert!(response.status().is_redirection());
        let edit = format!(
            "/admin/locked-customers/{}/edit",
            only_customer(&db).await.id
        );
        let html = body_string(client.get(&edit).await).await;
        let fieldset = vat_fieldset(&html);
        assert_eq!(
            fieldset.contains(" disabled"),
            disabled,
            "kind={kind}: {fieldset}"
        );
        assert!(!fieldset.contains(" hidden"), "kind={kind}: {fieldset}");
    }
}

/// A create that disables the field drops its posted key and stores its default; one that
/// enables it writes the posted value.
#[tokio::test]
async fn a_create_stores_the_default_of_a_field_a_condition_disables() {
    for (kind, vat) in [("person", "none"), ("company", "FR123")] {
        let (db, router) = customers().await;
        let client = TestClient::new(&router);
        let body = format!("name=Ada&kind={kind}&vat=FR123");
        let response = client
            .submit("/admin/locked-customers/-/create", &body)
            .await;
        assert!(
            response.status().is_redirection(),
            "kind={kind}: the create succeeds, got {} {}",
            response.status(),
            body_string(response).await
        );
        assert_eq!(
            only_customer(&db).await.vat.as_deref(),
            Some(vat),
            "kind={kind}"
        );
    }
}

/// An edit that disables the field keeps its stored value, whatever the submission posts for it.
#[tokio::test]
async fn an_edit_keeps_the_stored_value_of_a_field_a_condition_disables() {
    let (db, router) = customers().await;
    let client = TestClient::new(&router);
    let response = client
        .submit(
            "/admin/locked-customers/-/create",
            "name=Acme&kind=company&vat=FR123",
        )
        .await;
    assert!(response.status().is_redirection());
    let id = only_customer(&db).await.id;

    let response = client
        .submit(
            &format!("/admin/locked-customers/{id}/edit"),
            "name=Acme&kind=person&vat=FR999",
        )
        .await;
    assert!(
        response.status().is_redirection(),
        "the edit succeeds, got {} {}",
        response.status(),
        body_string(response).await
    );
    let customer = only_customer(&db).await;
    assert_eq!(customer.kind, "person", "the watched field is written");
    assert_eq!(customer.vat.as_deref(), Some("FR123"));
}
