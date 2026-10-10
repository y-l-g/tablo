//! Help text, disabled controls and create defaults over HTTP.
//!
//! A disabled control posts nothing, and the panel drops a key posted for it anyway: an edit
//! keeps the stored value and a create takes the field's default.

use tablo::{
    Ability, Auth, DeclarationErrorKind, Panel, Resource, ResourceDef, Schema, Table, TextColumn,
    lens,
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::{
    common::{TestClient, body_string, mount},
    framework::common::refusal,
};

#[derive(Debug, Clone, toasty::Model)]
struct Account {
    #[key]
    #[auto]
    id: Uuid,
    email: String,
    plan: String,
    credits: i64,
}

struct AccountResource;

impl Resource for AccountResource {
    type Model = Account;
    type Form = AccountForm;

    fn declare() -> ResourceDef<Self> {
        let c = AccountForm::controls();
        ResourceDef::new()
            .slug("accounts")
            .policy(|_cx: &topcoat::context::Cx, _ability: Ability<'_, Account>| true)
            .table(Table::new(TextColumn::new(lens!(Account.email))))
            .form(Schema::new((
                c.email
                    .disabled_on_edit()
                    .help("The sign-in address; it cannot change once the account exists."),
                c.plan.default("free").disabled(),
                c.credits.default(10_i64),
            )))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Account)]
struct AccountForm {
    email: String,
    plan: String,
    credits: i64,
}

async fn accounts() -> (Db, Router) {
    let db = Db::builder()
        .models(toasty::models!(Account))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    db.push_schema().await.expect("push schema");
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<AccountResource>(),
    )
    .expect("panel builds");
    (db, router)
}

async fn only_account(db: &Db) -> Account {
    let mut db = db.clone();
    let mut all = Account::all().exec(&mut db).await.unwrap();
    assert_eq!(all.len(), 1, "one account");
    all.remove(0)
}

/// The opening tag of the control posting `name`.
fn control<'h>(html: &'h str, name: &str) -> &'h str {
    let needle = format!("name=\"{name}\"");
    let at = html
        .find(&needle)
        .unwrap_or_else(|| panic!("no {name} control in {html}"));
    let start = html[..at].rfind('<').unwrap();
    &html[start..at + html[at..].find('>').unwrap()]
}

#[tokio::test]
async fn the_create_form_renders_defaults_help_and_disabled_controls() {
    let (_db, router) = accounts().await;
    let client = TestClient::new(&router);
    let html = body_string(client.get("/admin/accounts/create").await).await;

    let email = control(&html, "email");
    assert!(
        !email.contains("disabled=\"\""),
        "editable on create: {email}"
    );
    assert!(
        email.contains("aria-describedby=\"email-description\""),
        "the help describes the control: {email}"
    );
    assert!(
        html.contains("id=\"email-description\"")
            && html.contains("it cannot change once the account exists"),
        "the help renders: {html}"
    );
    let plan = control(&html, "plan");
    assert!(
        plan.contains("disabled=\"\"") && plan.contains("value=\"free\""),
        "{plan}"
    );
    assert!(
        !plan.contains("required=\"\""),
        "a disabled control is not required: {plan}"
    );
    assert!(control(&html, "credits").contains("value=\"10\""));
}

/// A create stores a disabled field's default whatever the submission posts for it, and the
/// posted value of an editable field that has one.
#[tokio::test]
async fn a_create_stores_the_default_of_a_disabled_field() {
    let (db, router) = accounts().await;
    let client = TestClient::new(&router);
    let response = client
        .submit(
            "/admin/accounts/create",
            "email=ada%40example.com&plan=enterprise&credits=3",
        )
        .await;
    assert!(
        response.status().is_redirection(),
        "the create succeeds, got {} {}",
        response.status(),
        body_string(response).await
    );
    let account = only_account(&db).await;
    assert_eq!(account.plan, "free", "the posted plan is dropped");
    assert_eq!(
        account.credits, 3,
        "an editable default takes the posted value"
    );
}

/// An edit keeps the stored value of a field disabled on the edit form, whatever it posts.
#[tokio::test]
async fn an_edit_keeps_the_stored_value_of_a_disabled_field() {
    let (db, router) = accounts().await;
    let client = TestClient::new(&router);
    let response = client
        .submit(
            "/admin/accounts/create",
            "email=ada%40example.com&credits=3",
        )
        .await;
    assert!(response.status().is_redirection());
    let id = only_account(&db).await.id;

    let edit = format!("/admin/accounts/{id}/edit");
    let html = body_string(client.get(&edit).await).await;
    let email = control(&html, "email");
    assert!(
        email.contains("disabled=\"\"") && email.contains("value=\"ada@example.com\""),
        "{email}"
    );

    let response = client
        .submit(&edit, "email=eve%40example.com&plan=enterprise&credits=7")
        .await;
    assert!(
        response.status().is_redirection(),
        "the edit succeeds, got {} {}",
        response.status(),
        body_string(response).await
    );
    let account = only_account(&db).await;
    assert_eq!(account.email, "ada@example.com");
    assert_eq!(account.plan, "free");
    assert_eq!(account.credits, 7, "the editable field is written");
}

struct NoPlan;

impl Resource for NoPlan {
    type Model = Account;
    type Form = AccountForm;

    fn declare() -> ResourceDef<Self> {
        let c = AccountForm::controls();
        ResourceDef::new()
            .policy(|_cx: &topcoat::context::Cx, _ability: Ability<'_, Account>| true)
            .form(Schema::new((c.email, c.plan.disabled(), c.credits)))
    }
}

/// A required field disabled on create, with no default, is one no create could fill.
#[tokio::test]
async fn a_required_disabled_field_without_a_default_refuses_the_panel() {
    let (db, _) = accounts().await;
    let errors = refusal(mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<NoPlan>(),
    ));
    assert!(
        errors.iter().any(|error| matches!(
            &error.kind,
            DeclarationErrorKind::DisabledWithoutValue { field } if field == "plan"
        )),
        "{errors:?}"
    );
}
