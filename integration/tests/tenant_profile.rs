//! Tenant settings: the page that edits the tenant the request acts for, who may open it, the
//! submissions it refuses, and the mistakes mounting refuses.

use http::header::LOCATION;
use tablo::{
    Auth, DeclarationErrorKind, FieldErrors, FormInput, Membership, NotificationStatus,
    TenantProfileFault,
    auth::{AdminUser, TenantEditor},
    testing::{ErrorKind, field_error, form_body, input_value},
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};

use crate::common::{
    TestClient, body_string, flash,
    members::{MemberAuth, Seat, SignedMember, db, member, mount, seat, seats},
    refusal,
};

/// What the settings page asks for.
#[derive(FormInput)]
struct TeamName {
    name: String,
}

/// Renames a tenant its owner acts for, refusing a blank name.
struct Teams;

impl TenantEditor for Teams {
    type Input = TeamName;
    type User = SignedMember;

    fn may_edit(&self, _cx: &Cx, signed: &SignedMember, team: &Membership) -> bool {
        signed.owned.contains(&team.tenant.get())
    }

    async fn fill(
        &self,
        _cx: &Cx,
        _signed: &SignedMember,
        team: &Membership,
    ) -> topcoat::Result<TeamName> {
        Ok(TeamName {
            name: team.name.clone(),
        })
    }

    async fn validate(
        &self,
        _cx: &Cx,
        _signed: &SignedMember,
        _team: &Membership,
        renamed: &TeamName,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<FieldErrors> {
        let mut errors = FieldErrors::new();
        if renamed.name.trim() == "-" {
            errors.add("name", "Pick a name");
        }
        Ok(errors)
    }

    async fn save(
        &self,
        _cx: &Cx,
        _signed: &SignedMember,
        team: &Membership,
        renamed: TeamName,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        Seat::filter(Seat::fields().tenant().eq(team.tenant.get()))
            .update()
            .tenant_name(renamed.name)
            .exec(ex)
            .await?;
        Ok(())
    }
}

fn router(db: Db) -> Router {
    mount(db, Auth::custom(MemberAuth).tenant_profile(Teams)).expect("panel builds")
}

/// The name of every seat in the member's first tenant.
async fn names(db: &Db, signed: &SignedMember) -> Vec<String> {
    seats(&mut db.clone(), signed.member.id)
        .await
        .unwrap()
        .into_iter()
        .map(|seat| seat.tenant_name)
        .collect()
}

/// The owner opens the settings of the tenant they act for, filled from it, from the tenant menu,
/// and saves a new name.
#[tokio::test]
async fn an_owner_renames_the_tenant_they_act_for() {
    let db = db().await;
    let ada = member(&db, "Ada", &["Acme"]).await;
    let router = router(db.clone());
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;

    let list = body_string(client.get("/admin/notes").await).await;
    assert!(list.contains(r#"data-tenant-page="profile""#));
    let page = body_string(client.get("/admin/tenant/profile").await).await;
    assert_eq!(input_value(&page, "name").as_deref(), Some("Acme"));
    let acme = ada.tenants[0].tenant.to_string();
    assert_eq!(input_value(&page, "editing_tenant"), Some(acme.clone()));

    let saved = client
        .submit(
            "/admin/tenant/profile",
            &form_body(&[("name", "Acme Corp"), ("editing_tenant", &acme)]),
        )
        .await;
    assert_eq!(saved.status(), 303);
    assert_eq!(
        saved.headers().get(LOCATION).unwrap(),
        "/admin/tenant/profile"
    );
    assert_eq!(flash(&saved).status, NotificationStatus::Success);
    assert_eq!(names(&db, &ada).await, ["Acme Corp"]);
}

/// The editor's refusal renders under its field, and saves nothing.
#[tokio::test]
async fn a_refused_save_renders_its_errors_and_changes_nothing() {
    let db = db().await;
    let ada = member(&db, "Ada", &["Acme"]).await;
    let router = router(db.clone());
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;

    let acme = ada.tenants[0].tenant.to_string();
    let refused = client
        .submit(
            "/admin/tenant/profile",
            &form_body(&[("name", "-"), ("editing_tenant", &acme)]),
        )
        .await;
    let html = body_string(refused).await;
    assert_eq!(field_error(&html, "name"), Some(ErrorKind::Invalid));
    assert_eq!(names(&db, &ada).await, ["Acme"]);
}

/// A member the editor does not admit gets no link and a 403, and so does a member who acts for
/// no tenant; a panel without the page answers 404.
#[tokio::test]
async fn only_an_admitted_member_of_the_tenant_reaches_its_settings() {
    let db = db().await;
    let ada = member(&db, "Ada", &["Acme"]).await;
    let grace = member(&db, "Grace", &[]).await;
    seat(
        &db,
        &grace.member,
        ada.tenants[0].tenant.get(),
        "Acme",
        false,
    )
    .await;
    let router = router(db.clone());

    let client = TestClient::new(&router).sign_in(&db, "admin", &grace).await;
    let list = body_string(client.get("/admin/notes").await).await;
    assert!(!list.contains(r#"data-tenant-page="profile""#));
    assert_eq!(client.get("/admin/tenant/profile").await.status(), 403);
    let forged = client
        .submit("/admin/tenant/profile", &form_body(&[("name", "Grace's")]))
        .await;
    assert_eq!(forged.status(), 403);
    assert_eq!(names(&db, &ada).await, ["Acme"]);

    let alan = member(&db, "Alan", &[]).await;
    let client = TestClient::new(&router).sign_in(&db, "admin", &alan).await;
    assert_eq!(client.get("/admin/tenant/profile").await.status(), 403);

    let router = mount(db.clone(), Auth::custom(MemberAuth)).expect("panel builds");
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    assert_eq!(client.get("/admin/tenant/profile").await.status(), 404);
}

/// A member who switched to another of their tenants edits that one only.
#[tokio::test]
async fn the_page_edits_the_tenant_the_request_acts_for() {
    let db = db().await;
    let ada = member(&db, "Ada", &["Acme", "Globex"]).await;
    let router = router(db.clone());
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let globex = ada.tenants[1].tenant.to_string();
    client
        .submit("/admin/tenant", &form_body(&[("tenant", &globex)]))
        .await;

    let page = body_string(client.get("/admin/tenant/profile").await).await;
    assert_eq!(input_value(&page, "name").as_deref(), Some("Globex"));
    client
        .submit(
            "/admin/tenant/profile",
            &form_body(&[("name", "Globex Inc"), ("editing_tenant", &globex)]),
        )
        .await;
    let mut names = names(&db, &ada).await;
    names.sort();
    assert_eq!(names, ["Acme", "Globex Inc"]);
}

/// A form filled for one tenant, posted after another tab switched to a second, saves nothing.
#[tokio::test]
async fn a_form_for_another_tenant_saves_nothing() {
    let db = db().await;
    let ada = member(&db, "Ada", &["Acme", "Globex"]).await;
    let router = router(db.clone());
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let acme = ada.tenants[0].tenant.to_string();
    let globex = ada.tenants[1].tenant.to_string();
    client
        .submit("/admin/tenant", &form_body(&[("tenant", &globex)]))
        .await;

    let stale = client
        .submit(
            "/admin/tenant/profile",
            &form_body(&[("name", "Acme Corp"), ("editing_tenant", &acme)]),
        )
        .await;
    assert_eq!(
        stale.headers().get(LOCATION).unwrap(),
        "/admin/tenant/profile"
    );
    assert_eq!(flash(&stale).status, NotificationStatus::Warning);
    let mut names = names(&db, &ada).await;
    names.sort();
    assert_eq!(names, ["Acme", "Globex"]);
}

/// Edits tenants for the shipped admin, whom `MemberAuth` never loads.
struct AdminTeams;

impl TenantEditor for AdminTeams {
    type Input = TeamName;
    type User = AdminUser;

    async fn fill(
        &self,
        _cx: &Cx,
        _user: &AdminUser,
        _team: &Membership,
    ) -> topcoat::Result<TeamName> {
        unreachable!("mounting refuses the editor")
    }

    async fn save(
        &self,
        _cx: &Cx,
        _user: &AdminUser,
        _team: &Membership,
        _input: TeamName,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        unreachable!("mounting refuses the editor")
    }
}

fn kinds(mounted: topcoat::Result<Router>) -> Vec<DeclarationErrorKind> {
    refusal(mounted)
        .into_iter()
        .map(|error| error.kind)
        .collect()
}

/// Mounting refuses a tenant settings page it cannot serve.
#[tokio::test]
async fn mounting_refuses_a_tenant_settings_page_the_panel_cannot_serve() {
    let fault = DeclarationErrorKind::TenantProfile;
    assert_eq!(
        kinds(mount(db().await, Auth::disabled().tenant_profile(Teams))),
        vec![fault(TenantProfileFault::AuthDisabled)]
    );
    assert_eq!(
        kinds(mount(
            db().await,
            Auth::custom(MemberAuth).tenant_profile(AdminTeams)
        )),
        vec![fault(TenantProfileFault::UserType {
            editor: std::any::type_name::<AdminUser>(),
        })]
    );
}
