//! Registration beyond the showcase's own: the shipped `PasswordAuth` over `AdminUser`, an
//! account that may not enter the panel yet, the sign-up throttle, and the mistakes mounting
//! refuses.

use std::time::Duration;

use http::header::LOCATION;
use tablo::{
    ActionInput, Auth, DeclarationErrorKind, FieldErrors, LoginThrottle, NoForm,
    NotificationStatus, PanelUser, PasswordAuth, ReadOnly, Resource, ResourceDef, SignUpFault,
    Table, TextColumn,
    auth::{AdminUser, AuthSession, Registrar, SignUp, create_admin},
    lens,
    testing::form_body,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::framework::common::{body_string, flash, memory_db, mount, new_csrf, post, refusal};

#[derive(Debug, Clone, toasty::Model)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    body: String,
}

struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = NoForm<Note>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Note.body))))
    }
}

async fn db() -> Db {
    memory_db(toasty::models!(Note, AdminUser, AuthSession)).await
}

fn router(db: Db, auth: Auth) -> Router {
    mount(
        db,
        tablo::Panel::new("admin")
            .auth(auth)
            .resource::<NoteResource>(),
    )
    .expect("panel builds")
}

async fn submit(router: &Router, body: &str) -> http::Response<topcoat::router::Body> {
    let csrf = new_csrf();
    post(
        router,
        "/admin/register",
        &csrf,
        "application/x-www-form-urlencoded".to_string(),
        format!("{body}&csrf_token={csrf}"),
    )
    .await
}

fn sign_up(email: &str) -> String {
    form_body(&[
        ("name", "Ada"),
        ("email", email),
        ("password", "correct horse"),
        ("password_confirmation", "correct horse"),
    ])
}

async fn admins(db: &Db) -> Vec<AdminUser> {
    AdminUser::all().exec(&mut db.clone()).await.unwrap()
}

/// `PasswordAuth` registers an active `AdminUser` from a `SignUp` and signs it in.
#[tokio::test]
async fn password_auth_registers_an_admin_user_and_signs_it_in() {
    let db = db().await;
    let router = router(db.clone(), Auth::password().registration(PasswordAuth));

    let response = submit(&router, &sign_up("ada@example.com")).await;

    assert_eq!(response.status(), 303);
    assert_eq!(response.headers().get(LOCATION).unwrap(), "/admin");
    assert!(tablo::testing::session_cookie_value(&response).is_some());
    let admins = admins(&db).await;
    assert_eq!(admins.len(), 1);
    assert_eq!(admins[0].email, "ada@example.com");
    assert_eq!(admins[0].display_name, "Ada");
    assert!(admins[0].active);
    assert!(tablo::auth::verify_password(
        "correct horse",
        Some(&admins[0].password_hash)
    ));
}

/// `PasswordAuth` refuses an email another account holds, and nothing is written.
#[tokio::test]
async fn password_auth_refuses_a_taken_email() {
    let mut db = db().await;
    create_admin(&mut db, "ada@example.com", "secret", "Ada")
        .await
        .unwrap();
    let router = router(db.clone(), Auth::password().registration(PasswordAuth));

    let response = submit(&router, &sign_up("ada@example.com")).await;

    assert_eq!(response.status(), 422);
    let html = body_string(response).await;
    assert!(
        tablo::testing::field_error(&html, "email").is_some(),
        "{html}"
    );
    assert_eq!(admins(&db).await.len(), 1);
}

/// Without `Auth::registration`, the panel serves no sign-up route and the login page links to
/// none.
#[tokio::test]
async fn a_panel_without_registration_serves_no_sign_up() {
    let db = db().await;
    let router = router(db.clone(), Auth::password());
    let client = tablo::testing::TestClient::new(&router);

    let page = client.get("/admin/register").await;
    assert_eq!(page.status(), 404);
    let login = body_string(client.get("/admin/login").await).await;
    assert!(!login.contains("/admin/register"), "{login}");
    let response = submit(&router, &sign_up("ada@example.com")).await;
    assert_eq!(response.status(), 404);
    assert!(admins(&db).await.is_empty());
}

/// Past the sign-up limit a client is refused with 429 before the registrar runs.
#[tokio::test]
async fn the_sign_up_throttle_refuses_a_client_past_its_limit() {
    let db = db().await;
    let auth = Auth::password()
        .registration(PasswordAuth)
        .sign_up_throttle(LoginThrottle::new(1, Duration::from_secs(60)));
    let router = router(db.clone(), auth);

    assert_eq!(
        submit(&router, &sign_up("one@example.com")).await.status(),
        303
    );
    let refused = submit(&router, &sign_up("two@example.com")).await;

    assert_eq!(refused.status(), 429);
    assert_eq!(admins(&db).await.len(), 1);
}

/// Registers inactive accounts, which an administrator approves.
struct Pending;

impl Registrar for Pending {
    type Input = SignUp;
    type User = AdminUser;

    async fn register(
        &self,
        _cx: &Cx,
        sign_up: SignUp,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<AdminUser> {
        let mut admin = create_admin(ex, &sign_up.email, &sign_up.password, &sign_up.name).await?;
        admin.update().active(false).exec(ex).await?;
        Ok(admin)
    }
}

/// An account that may not enter the panel is created but not signed in: the sign-up lands on
/// the login page with a notification, which the login page shows.
#[tokio::test]
async fn an_account_without_panel_access_lands_on_the_login_page() {
    let db = db().await;
    let router = router(db.clone(), Auth::password().registration(Pending));

    let response = submit(&router, &sign_up("ada@example.com")).await;

    assert_eq!(response.status(), 303);
    assert_eq!(response.headers().get(LOCATION).unwrap(), "/admin/login");
    assert!(tablo::testing::session_cookie_value(&response).is_none());
    assert_eq!(flash(&response).status, NotificationStatus::Success);
    let admins = admins(&db).await;
    assert_eq!(admins.len(), 1);
    assert!(!admins[0].active);
    let sessions = AuthSession::all().exec(&mut db.clone()).await.unwrap();
    assert!(sessions.is_empty());
}

/// A user type the panel's authenticator does not load.
struct Visitor;

impl PanelUser for Visitor {
    fn user_id(&self) -> String {
        String::new()
    }

    fn display_name(&self) -> &str {
        ""
    }
}

struct VisitorRegistrar;

impl Registrar for VisitorRegistrar {
    type Input = SignUp;
    type User = Visitor;

    async fn register(
        &self,
        _cx: &Cx,
        _input: SignUp,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Visitor> {
        Ok(Visitor)
    }
}

/// A sign-up form posting the keys the page carries itself.
#[derive(ActionInput)]
struct Clashing {
    next: String,
    #[form(email)]
    email: String,
}

struct ClashingRegistrar;

impl Registrar for ClashingRegistrar {
    type Input = Clashing;
    type User = AdminUser;

    async fn validate(
        &self,
        _cx: &Cx,
        _input: &Clashing,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<FieldErrors> {
        Ok(FieldErrors::new())
    }

    async fn register(
        &self,
        _cx: &Cx,
        input: Clashing,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<AdminUser> {
        create_admin(ex, &input.email, &input.next, "").await
    }
}

/// Mounting refuses a registrar of another user type, a registration without auth, and a
/// sign-up field named like a key the page posts itself.
#[tokio::test]
async fn mounting_refuses_a_registration_the_panel_cannot_serve() {
    let cases: [(Auth, SignUpFault); 3] = [
        (
            Auth::password().registration(VisitorRegistrar),
            SignUpFault::UserType {
                registrar: std::any::type_name::<Visitor>(),
            },
        ),
        (
            Auth::disabled().registration(PasswordAuth),
            SignUpFault::AuthDisabled,
        ),
        (
            Auth::password().registration(ClashingRegistrar),
            SignUpFault::ReservedField("next".to_string()),
        ),
    ];
    for (auth, fault) in cases {
        let errors = refusal(mount(
            db().await,
            tablo::Panel::new("admin")
                .auth(auth)
                .resource::<NoteResource>(),
        ));
        let kinds: Vec<_> = errors.iter().map(|error| error.kind.clone()).collect();
        assert_eq!(kinds, vec![DeclarationErrorKind::SignUp(fault)]);
    }
}
