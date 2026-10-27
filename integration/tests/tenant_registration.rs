//! Tenant registration: what a user who acts for no tenant reaches, the tenant a registration
//! makes the session act for, the submissions it refuses, and the mistakes mounting refuses.

use http::header::LOCATION;
use tablo::{
    Auth, DeclarationErrorKind, FieldErrors, FormInput, Membership, NotificationStatus,
    TenantRegistration, TenantRegistrationFault,
    auth::{AdminUser, TenantRegistrar},
    testing::{ErrorKind, field_error, form_body},
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{
    TestClient, body_string, flash,
    members::{MemberAuth, Seat, SignedMember, db, member, mount, seats, session_tenant},
    refusal,
};

/// What the registration page asks for.
#[derive(FormInput)]
struct NewTeam {
    name: String,
}

/// Seats the member in a new tenant, refusing a name one of their tenants holds.
struct Teams;

impl TenantRegistrar for Teams {
    type Input = NewTeam;
    type User = SignedMember;

    async fn validate(
        &self,
        _cx: &Cx,
        signed: &SignedMember,
        team: &NewTeam,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<FieldErrors> {
        let mut errors = FieldErrors::new();
        if signed.tenants.iter().any(|m| m.name == team.name) {
            errors.add("name", "You have a tenant with that name");
        }
        Ok(errors)
    }

    async fn register(
        &self,
        _cx: &Cx,
        signed: &SignedMember,
        team: NewTeam,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Membership> {
        let tenant = Uuid::new_v4();
        toasty::create!(Seat {
            member_id: signed.member.id,
            tenant,
            tenant_name: team.name.clone(),
            owner: true,
        })
        .exec(ex)
        .await?;
        Ok(Membership::new(tenant.into(), team.name))
    }
}

fn router(db: Db) -> Router {
    mount(
        db,
        Auth::custom(MemberAuth).tenant_registration(TenantRegistration::new(Teams).required()),
    )
    .expect("panel builds")
}

/// A member who acts for no tenant reaches only the registration page; registering one makes
/// the session act for it, and the panel opens.
#[tokio::test]
async fn a_member_without_a_tenant_reaches_the_panel_once_they_register_one() {
    let db = db().await;
    let grace = member(&db, "Grace", &[]).await;
    let router = router(db.clone());
    let client = TestClient::new(&router).sign_in(&db, "admin", &grace).await;

    let page = client.get("/admin/notes").await;
    assert_eq!(page.status(), 307);
    assert_eq!(
        page.headers().get(LOCATION).unwrap(),
        "/admin/register-tenant"
    );
    assert_eq!(
        client.submit("/admin/notes/-/columns", "").await.status(),
        403
    );
    assert_eq!(client.get("/admin/register-tenant").await.status(), 200);

    let registered = client
        .submit("/admin/register-tenant", &form_body(&[("name", "Initech")]))
        .await;
    assert_eq!(registered.status(), 303);
    assert_eq!(registered.headers().get(LOCATION).unwrap(), "/admin");
    assert_eq!(flash(&registered).status, NotificationStatus::Success);
    let seats = seats(&mut db.clone(), grace.member.id).await.unwrap();
    assert_eq!(seats.len(), 1);
    assert_eq!(session_tenant(&db).await, Some(seats[0].tenant.into()));
    assert_eq!(client.get("/admin/notes").await.status(), 200);
}

/// A member who acts for a tenant reaches the panel, and the tenant menu leads to the page that
/// adds another, which the session then acts for.
#[tokio::test]
async fn a_member_registers_another_tenant_and_acts_for_it() {
    let db = db().await;
    let ada = member(&db, "Ada", &["Acme"]).await;
    let router = router(db.clone());
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;

    let list = body_string(client.get("/admin/notes").await).await;
    assert!(list.contains(r#"data-tenant-page="registration""#));

    client
        .submit("/admin/register-tenant", &form_body(&[("name", "Globex")]))
        .await;
    let globex = seats(&mut db.clone(), ada.member.id)
        .await
        .unwrap()
        .into_iter()
        .find(|seat| seat.tenant_name == "Globex")
        .expect("the registration seats Ada in Globex");
    assert_eq!(session_tenant(&db).await, Some(globex.tenant.into()));
}

/// The registrar's refusal renders under its field, and registers nothing.
#[tokio::test]
async fn a_refused_registration_renders_its_errors_and_registers_nothing() {
    let db = db().await;
    let ada = member(&db, "Ada", &["Acme"]).await;
    let router = router(db.clone());
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;

    let refused = client
        .submit("/admin/register-tenant", &form_body(&[("name", "Acme")]))
        .await;
    assert_eq!(refused.status(), 422);
    let html = body_string(refused).await;
    assert_eq!(field_error(&html, "name"), Some(ErrorKind::Invalid));
    assert_eq!(
        seats(&mut db.clone(), ada.member.id).await.unwrap().len(),
        1
    );
    assert_eq!(session_tenant(&db).await, None);
}

/// A panel whose registration is not required lets a member without a tenant in, where a resource
/// that needs one answers 403, and serves them the page; a panel that registers no tenant serves
/// no page.
#[tokio::test]
async fn only_a_required_registration_holds_back_a_member_without_a_tenant() {
    let db = db().await;
    let grace = member(&db, "Grace", &[]).await;
    let offered = Auth::custom(MemberAuth).tenant_registration(TenantRegistration::new(Teams));
    let router = mount(db.clone(), offered).expect("panel builds");
    let client = TestClient::new(&router).sign_in(&db, "admin", &grace).await;
    assert_eq!(client.get("/admin/notes").await.status(), 403);
    assert_eq!(client.get("/admin/register-tenant").await.status(), 200);

    let router = mount(db.clone(), Auth::custom(MemberAuth)).expect("panel builds");
    let client = TestClient::new(&router).sign_in(&db, "admin", &grace).await;
    assert_eq!(client.get("/admin/register-tenant").await.status(), 404);
}

/// Registers tenants for the shipped admin, on a panel whose auth is off.
struct AdminTeams;

impl TenantRegistrar for AdminTeams {
    type Input = NewTeam;
    type User = AdminUser;

    async fn register(
        &self,
        _cx: &Cx,
        _user: &AdminUser,
        _team: NewTeam,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Membership> {
        unreachable!("mounting refuses the registrar")
    }
}

/// A form posting the CSRF field's own key.
#[derive(FormInput)]
struct Forged {
    csrf_token: String,
}

/// Registers from a form the page cannot serve.
struct ForgedTeams;

impl TenantRegistrar for ForgedTeams {
    type Input = Forged;
    type User = SignedMember;

    async fn register(
        &self,
        _cx: &Cx,
        _user: &SignedMember,
        _input: Forged,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Membership> {
        unreachable!("mounting refuses the registrar")
    }
}

fn kinds(mounted: topcoat::Result<Router>) -> Vec<DeclarationErrorKind> {
    refusal(mounted)
        .into_iter()
        .map(|error| error.kind)
        .collect()
}

/// Mounting refuses a tenant registration it cannot serve.
#[tokio::test]
async fn mounting_refuses_a_tenant_registration_the_panel_cannot_serve() {
    let fault = DeclarationErrorKind::TenantRegistration;
    assert_eq!(
        kinds(mount(
            db().await,
            Auth::disabled().tenant_registration(TenantRegistration::new(AdminTeams))
        )),
        vec![fault(TenantRegistrationFault::AuthDisabled)]
    );
    assert_eq!(
        kinds(mount(
            db().await,
            Auth::custom(MemberAuth).tenant_registration(TenantRegistration::new(ForgedTeams))
        )),
        vec![fault(TenantRegistrationFault::ReservedField(
            "csrf_token".to_string()
        ))]
    );
}
