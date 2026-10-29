//! Help text, disabled and hidden controls, and create defaults over HTTP.
//!
//! A disabled control, and a field hidden on its form, post nothing, and the panel drops a key
//! posted for one anyway: an edit keeps the stored value and a create takes the field's default.

use tablo::{
    Allow, Auth, DeclarationErrorKind, Panel, Resource, ResourceDef, Schema, Table, TextColumn,
    lens,
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{TestClient, body_string, control, mount, refusal, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Account {
    #[key]
    #[auto]
    id: Uuid,
    email: String,
    plan: String,
    credits: i64,
    coupon: Option<String>,
    referral: Option<String>,
}

struct AccountResource;

impl Resource for AccountResource {
    type Model = Account;
    type Form = AccountForm;

    fn declare() -> ResourceDef<Self> {
        let c = AccountForm::controls();
        let plan = c.plan.default("free").disabled();
        let coupon = c.coupon.visible_when(&plan, ["free"]);
        let referral = c.referral.visible_when(&plan, ["pro"]);
        ResourceDef::new()
            .slug("accounts")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Account.email))))
            .form(Schema::new((
                c.email
                    .disabled_on_edit()
                    .help("The sign-in address; it cannot change once the account exists."),
                plan,
                c.credits.default(10_i64),
                coupon,
                referral,
            )))
    }
}

/// The same accounts, the address hidden once set and the credits granted on create.
struct HiddenAccountResource;

impl Resource for HiddenAccountResource {
    type Model = Account;
    type Form = AccountForm;

    fn declare() -> ResourceDef<Self> {
        let c = AccountForm::controls();
        ResourceDef::new()
            .slug("hidden-accounts")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Account.email))))
            .form(Schema::new((
                c.email.hidden_on_edit(),
                c.plan,
                c.credits.default(5_i64).hidden_on_create(),
                c.coupon,
                c.referral,
            )))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Account)]
struct AccountForm {
    email: String,
    plan: String,
    credits: i64,
    coupon: Option<String>,
    referral: Option<String>,
}

async fn accounts() -> (Db, Router) {
    let db = test_db(toasty::models!(Account)).await;
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<AccountResource>()
            .resource::<HiddenAccountResource>(),
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

#[tokio::test]
async fn the_create_form_renders_defaults_help_and_disabled_controls() {
    let (_db, router) = accounts().await;
    let client = TestClient::new(&router);
    let html = body_string(client.get("/admin/accounts/-/create").await).await;

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
            "/admin/accounts/-/create",
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
            "/admin/accounts/-/create",
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
        ResourceDef::new().policy(Allow).form(Schema::new((
            c.email,
            c.plan.disabled(),
            c.credits,
            c.coupon,
            c.referral,
        )))
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

/// Conditions read a disabled field as its default on create and its stored value on edit, never
/// as what the submission posts for it.
#[tokio::test]
async fn a_condition_reads_a_disabled_field_as_the_browser_does() {
    let (db, router) = accounts().await;
    let client = TestClient::new(&router);
    let response = client
        .submit(
            "/admin/accounts/-/create",
            "email=ada%40example.com&credits=1&coupon=SAVE&plan=pro&referral=eve",
        )
        .await;
    assert!(
        response.status().is_redirection(),
        "the create succeeds, got {} {}",
        response.status(),
        body_string(response).await
    );
    let account = only_account(&db).await;
    assert_eq!(
        account.coupon.as_deref(),
        Some("SAVE"),
        "the default shows it"
    );
    assert_eq!(account.referral, None, "the default hides it");

    let response = client
        .submit(
            &format!("/admin/accounts/{}/edit", account.id),
            "credits=1&plan=pro&referral=eve",
        )
        .await;
    assert!(response.status().is_redirection());
    assert_eq!(
        only_account(&db).await.referral,
        None,
        "the stored plan hides it"
    );
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    body: String,
    secret: String,
    token: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Note)]
struct NoteForm {
    title: String,
    body: String,
    #[form(optional)]
    secret: String,
    #[form(optional)]
    token: String,
}

struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = NoteForm;

    fn declare() -> ResourceDef<Self> {
        let c = NoteForm::controls();
        ResourceDef::new()
            .slug("notes")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Note.title))))
            .form(Schema::new((
                c.title,
                c.body.multiline(4),
                c.secret.password(),
                c.token.password().multiline(3),
            )))
    }
}

/// A multi-line control holds the stored text, so saving the form keeps it; a password, one line
/// or several, never renders a value, stored or just refused.
#[tokio::test]
async fn a_form_shows_the_stored_text_and_never_a_password() {
    let db = test_db(toasty::models!(Note)).await;
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<NoteResource>(),
    )
    .expect("panel builds");
    let note = toasty::create!(Note {
        title: "Plan".to_string(),
        body: "Line one\nLine two".to_string(),
        secret: "hunter22".to_string(),
        token: "s3cr3t-token".to_string(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap();
    let client = TestClient::new(&router);
    let edit = format!("/admin/notes/{}/edit", note.id);

    let html = body_string(client.get(&edit).await).await;
    assert!(html.contains("Line one\nLine two"), "{html}");
    for name in ["secret", "token"] {
        assert!(
            control(&html, name).contains("type=\"password\""),
            "{name}: {html}"
        );
    }
    assert!(
        !html.contains("hunter22") && !html.contains("s3cr3t-token"),
        "{html}"
    );

    let refused = body_string(
        client
            .submit(&edit, "title=&body=x&secret=typed-pass&token=typed-token")
            .await,
    )
    .await;
    assert_eq!(
        crate::common::field_error(&refused, "title"),
        Some(tablo::testing::ErrorKind::Required),
        "{refused}"
    );
    assert!(
        !refused.contains("typed-pass") && !refused.contains("typed-token"),
        "{refused}"
    );
}

/// A field hidden on the create form renders no control there, and the create stores its default
/// whatever the submission posts for it.
#[tokio::test]
async fn a_create_stores_the_default_of_a_field_hidden_on_create() {
    let (db, router) = accounts().await;
    let client = TestClient::new(&router);
    let html = body_string(client.get("/admin/hidden-accounts/-/create").await).await;
    assert!(!html.contains("name=\"credits\""), "{html}");

    let response = client
        .submit(
            "/admin/hidden-accounts/-/create",
            "email=ada%40example.com&plan=pro&credits=99",
        )
        .await;
    assert!(
        response.status().is_redirection(),
        "the create succeeds, got {} {}",
        response.status(),
        body_string(response).await
    );
    assert_eq!(only_account(&db).await.credits, 5);

    let id = only_account(&db).await.id;
    let html = body_string(
        client
            .get(&format!("/admin/hidden-accounts/{id}/edit"))
            .await,
    )
    .await;
    assert!(
        !control(&html, "credits").contains("disabled=\"\""),
        "the edit form edits it"
    );
}

/// A field hidden on the edit form renders no control there, and an edit keeps its stored value
/// whatever the submission posts for it.
#[tokio::test]
async fn an_edit_keeps_the_stored_value_of_a_field_hidden_on_edit() {
    let (db, router) = accounts().await;
    let client = TestClient::new(&router);
    let response = client
        .submit(
            "/admin/hidden-accounts/-/create",
            "email=ada%40example.com&plan=pro",
        )
        .await;
    assert!(response.status().is_redirection());
    let edit = format!("/admin/hidden-accounts/{}/edit", only_account(&db).await.id);
    let html = body_string(client.get(&edit).await).await;
    assert!(!html.contains("name=\"email\""), "{html}");

    let response = client
        .submit(&edit, "email=eve%40example.com&plan=team&credits=7")
        .await;
    assert!(
        response.status().is_redirection(),
        "the edit succeeds, got {} {}",
        response.status(),
        body_string(response).await
    );
    let account = only_account(&db).await;
    assert_eq!(account.email, "ada@example.com");
    assert_eq!((account.plan.as_str(), account.credits), ("team", 7));
}

struct HiddenPlan;

impl Resource for HiddenPlan {
    type Model = Account;
    type Form = AccountForm;

    fn declare() -> ResourceDef<Self> {
        let c = AccountForm::controls();
        ResourceDef::new().policy(Allow).form(Schema::new((
            c.email,
            c.plan.hidden_on_create(),
            c.credits,
            c.coupon,
            c.referral,
        )))
    }
}

/// A required field hidden on create, with no default, is one no create could fill.
#[tokio::test]
async fn a_required_field_hidden_on_create_without_a_default_refuses_the_panel() {
    let (db, _) = accounts().await;
    let errors = refusal(mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<HiddenPlan>(),
    ));
    assert!(
        errors.iter().any(|error| matches!(
            &error.kind,
            DeclarationErrorKind::DisabledWithoutValue { field } if field == "plan"
        )),
        "{errors:?}"
    );
}
