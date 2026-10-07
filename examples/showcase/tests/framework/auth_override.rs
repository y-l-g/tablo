//! The override seam, proven end to end (spec #127, ticket #132).

use http::header::{COOKIE, LOCATION, SET_COOKIE};
use tablo::{
    Ability, Auth, Membership, PanelUser, Resource, ResourceDef, Table, Tenancy, TenantId,
    TextColumn,
    auth::{self, Authenticator, verify_password},
    lens, when,
};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Body, Router, response::Response},
};
use uuid::Uuid;

use crate::framework::common::{
    body_string, get_with_cookies, input_value, memory_db, mount, post_form, response_cookies,
    router_with,
};

/// A custom user table.
#[derive(Debug, Clone, toasty::Model)]
struct Member {
    #[key]
    #[auto]
    id: Uuid,
    #[unique]
    handle: String,
    password_hash: String,
    display_name: String,
    active: bool,
}

/// A member's seat in one tenant.
#[derive(Debug, Clone, toasty::Model)]
struct Seat {
    #[key]
    #[auto]
    id: Uuid,
    #[index]
    member_id: Uuid,
    tenant: Uuid,
    tenant_name: String,
}

const ACME: Uuid = Uuid::from_u128(7);
const GLOBEX: Uuid = Uuid::from_u128(8);
const INITECH: Uuid = Uuid::from_u128(9);

/// The signed-in member.
#[derive(Debug)]
struct SignedMember {
    member: Member,
    tenants: Vec<Membership>,
}

impl PanelUser for SignedMember {
    fn user_id(&self) -> String {
        self.member.id.to_string()
    }

    fn display_name(&self) -> &str {
        &self.member.display_name
    }

    fn can_access_panel(&self) -> bool {
        self.member.active
    }

    fn tenants(&self) -> &[Membership] {
        &self.tenants
    }
}

/// The one trait implementation an app with an existing user table writes.
struct MemberAuth;

impl MemberAuth {
    async fn signed(cx: &Cx, member: Member) -> topcoat::Result<SignedMember> {
        let mut db = tablo::db::db(cx);
        let tenants = Seat::filter(Seat::fields().member_id().eq(member.id))
            .exec(&mut db)
            .await?
            .into_iter()
            .map(|seat| Membership::new(seat.tenant, seat.tenant_name))
            .collect();
        Ok(SignedMember { member, tenants })
    }
}

impl Authenticator for MemberAuth {
    type User = SignedMember;

    async fn verify(
        &self,
        cx: &Cx,
        login: &str,
        password: &str,
    ) -> topcoat::Result<Option<SignedMember>> {
        let mut db = tablo::db::db(cx);
        let member = Member::filter(Member::fields().handle().eq(login.to_string()))
            .first()
            .exec(&mut db)
            .await?;
        if !verify_password(password, member.as_ref().map(|m| m.password_hash.as_str())) {
            return Ok(None);
        }
        match member {
            Some(member) => Ok(Some(Self::signed(cx, member).await?)),
            None => Ok(None),
        }
    }

    async fn find_by_id(&self, cx: &Cx, id: &str) -> topcoat::Result<Option<SignedMember>> {
        let Ok(id) = Uuid::parse_str(id) else {
            return Ok(None);
        };
        let mut db = tablo::db::db(cx);
        let member = Member::filter(Member::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await?;
        match member {
            Some(member) => Ok(Some(Self::signed(cx, member).await?)),
            None => Ok(None),
        }
    }
}

struct MemberResource;

impl Resource for MemberResource {
    type Model = Member;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(|_cx: &Cx, ability: Ability<'_, Member>| matches!(ability, Ability::ViewAny))
            .table(Table::new(TextColumn::new(lens!(Member.handle))).paginate(25))
    }
}

/// A tenant-owned note.
#[derive(Debug, Clone, toasty::Model)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    body: String,
}

/// Only Ada reads notes: the policy reads the app's own user type.
fn is_ada(cx: &Cx) -> bool {
    auth::user::<SignedMember>(cx).is_some_and(|signed| signed.member.handle == "ada")
}

struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(when(is_ada).and(tablo::ReadOnly))
            .tenancy(Tenancy::column(Note::fields().tenant_id()))
            .table(Table::new(TextColumn::new(lens!(Note.body))).paginate(25))
    }
}

/// Ada, with seats in Acme and Globex, and Grace, with none.
async fn seeded_db() -> Db {
    let mut db = memory_db(toasty::models!(
        Member,
        Seat,
        Note,
        tablo::auth::AuthSession
    ))
    .await;
    let hash = tablo::auth::hash_password("opensesame").expect("hash");
    let ada = toasty::create!(Member {
        handle: "ada".to_string(),
        password_hash: hash.clone(),
        display_name: "Ada Member".to_string(),
        active: true,
    })
    .exec(&mut db)
    .await
    .expect("seed member");
    toasty::create!(Member {
        handle: "grace".to_string(),
        password_hash: hash,
        display_name: "Grace Member".to_string(),
        active: true,
    })
    .exec(&mut db)
    .await
    .expect("seed member");
    for (tenant, name) in [(ACME, "Acme"), (GLOBEX, "Globex")] {
        toasty::create!(Seat {
            member_id: ada.id,
            tenant,
            tenant_name: name.to_string(),
        })
        .exec(&mut db)
        .await
        .expect("seed seat");
    }
    for (tenant, body) in [
        (ACME, "acme note"),
        (GLOBEX, "globex note"),
        (INITECH, "initech note"),
    ] {
        toasty::create!(Note {
            tenant_id: TenantId::from(tenant),
            body: body.to_string(),
        })
        .exec(&mut db)
        .await
        .expect("seed note");
    }
    db
}

/// A router gated by `MemberAuth`.
fn notes_router(db: Db) -> Router {
    mount(
        db,
        tablo::Panel::new("admin")
            .auth(Auth::custom(MemberAuth))
            .resource::<MemberResource>()
            .resource::<NoteResource>(),
    )
    .expect("panel builds")
}

fn cookie_value(response: &Response<Body>, name: &str) -> Option<String> {
    response_cookies(response)
        .into_iter()
        .find(|(cookie, _)| cookie == name)
        .map(|(_, value)| value)
}

/// Scrape the login page's CSRF pair.
async fn csrf_pair(router: &Router) -> (String, String) {
    let page = get_with_cookies(router, "/admin/login", &[]).await;
    let csrf_cookie = cookie_value(&page, tablo::csrf::COOKIE_NAME).expect("CSRF cookie");
    let html = body_string(page).await;
    let csrf = input_value(&html, "csrf_token").expect("CSRF field");
    (csrf_cookie, csrf)
}

/// Log in as Ada, returning the session cookie value.
async fn login_session(router: &Router) -> String {
    login_as(router, "ada").await
}

/// Log in as `handle`, returning the session cookie value.
async fn login_as(router: &Router, handle: &str) -> String {
    let (csrf_cookie, csrf) = csrf_pair(router).await;
    let login = post_form(
        router,
        "/admin/login",
        &[(tablo::csrf::COOKIE_NAME, csrf_cookie)],
        format!("email={handle}&password=opensesame&csrf_token={csrf}"),
    )
    .await;
    assert_eq!(login.status(), 303, "login succeeds for an active member");
    cookie_value(&login, "__Host-session").expect("session cookie")
}

/// A member whose panel access is revoked mid-session can still log out.
#[tokio::test]
async fn revoked_panel_access_can_still_log_out() {
    let db = seeded_db().await;
    let router = router_with::<MemberResource>(db.clone(), Auth::custom(MemberAuth));
    let session = login_session(&router).await;

    // Revoke the member's panel access.
    let mut db2 = db.clone();
    let mut member = Member::filter(Member::fields().handle().eq("ada".to_string()))
        .first()
        .exec(&mut db2)
        .await
        .unwrap()
        .expect("seeded member");
    toasty::update!(member { active: false })
        .exec(&mut db2)
        .await
        .unwrap();

    // Panel pages now 403 the de-permitted user.
    let response = get_with_cookies(
        &router,
        "/admin/members",
        &[("__Host-session", session.clone())],
    )
    .await;
    assert_eq!(response.status(), 403, "pages deny the de-permitted user");

    // Logout still answers.
    let logout_csrf = Uuid::new_v4().to_string();
    let logout = post_form(
        &router,
        "/admin/logout",
        &[
            ("__Host-session", session.clone()),
            (tablo::csrf::COOKIE_NAME, logout_csrf.clone()),
        ],
        format!("csrf_token={logout_csrf}"),
    )
    .await;
    assert_eq!(
        logout.status(),
        303,
        "a de-permitted user must still be able to log out"
    );
    assert_eq!(
        logout.headers().get(LOCATION).unwrap(),
        "/admin/login",
        "logout lands on the login page"
    );
    let cleared = logout
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("__Host-session="))
        .expect("logout clears the session cookie")
        .to_string();
    assert!(
        cleared.contains("Max-Age=0") || cleared.contains("Expires=Thu, 01 Jan 1970"),
        "the session cookie must be cleared: {cleared}"
    );
    let mut db2 = db.clone();
    assert!(
        tablo::auth::AuthSession::all()
            .exec(&mut db2)
            .await
            .unwrap()
            .is_empty(),
        "the session row must be deleted"
    );
}

#[tokio::test]
async fn custom_authenticator_completes_a_full_login_round_trip() {
    let db = seeded_db().await;
    let router = router_with::<MemberResource>(db, Auth::custom(MemberAuth));

    // No session: the panel gate redirects to login.
    let response = get_with_cookies(&router, "/admin/members", &[]).await;
    assert_eq!(response.status(), 307);
    assert_eq!(
        response.headers().get(LOCATION).unwrap(),
        "/admin/login?next=%2Fadmin%2Fmembers"
    );

    // Log in through the shipped login page.
    let (csrf_cookie, csrf) = csrf_pair(&router).await;

    let csrf_cookies = [(tablo::csrf::COOKIE_NAME, csrf_cookie)];
    let wrong = post_form(
        &router,
        "/admin/login",
        &csrf_cookies,
        format!("email=ada&password=wrong&csrf_token={csrf}"),
    )
    .await;
    assert_eq!(wrong.status(), 403);
    assert!(
        body_string(wrong)
            .await
            .contains("Invalid email or password.")
    );

    let login = post_form(
        &router,
        "/admin/login",
        &csrf_cookies,
        format!("email=ada&password=opensesame&csrf_token={csrf}"),
    )
    .await;
    assert_eq!(login.status(), 303, "success stays on the Ok path");
    let session = cookie_value(&login, "__Host-session").expect("session cookie");

    // The same request now serves the gated page.
    let response = get_with_cookies(
        &router,
        "/admin/members",
        &[("__Host-session", session.clone())],
    )
    .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(html.contains("ada"), "member list must render: {html}");

    // The session is server-side.
    let logout_csrf = Uuid::new_v4().to_string();
    let logout = router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/admin/logout")
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    COOKIE,
                    format!(
                        "__Host-session={session}; {}={logout_csrf}",
                        tablo::csrf::COOKIE_NAME
                    ),
                )
                .body(Body::from(format!("csrf_token={logout_csrf}")))
                .unwrap(),
        )
        .await;
    assert_eq!(logout.status(), 303);
    let response =
        get_with_cookies(&router, "/admin/members", &[("__Host-session", session)]).await;
    assert_eq!(response.status(), 307, "logout must revoke the session");
}

/// `POST /admin/tenant` with a fresh CSRF pair.
async fn switch_tenant(router: &Router, session: &str, tenant: Uuid) -> Response<Body> {
    let csrf = Uuid::new_v4().to_string();
    post_form(
        router,
        "/admin/tenant",
        &[
            ("__Host-session", session.to_string()),
            (tablo::csrf::COOKIE_NAME, csrf.clone()),
        ],
        format!("tenant={tenant}&csrf_token={csrf}"),
    )
    .await
}

/// The notes list as `session` sees it.
async fn notes(router: &Router, session: &str) -> (http::StatusCode, String) {
    let response = get_with_cookies(
        router,
        "/admin/notes",
        &[("__Host-session", session.to_string())],
    )
    .await;
    (response.status(), body_string(response).await)
}

#[tokio::test]
async fn a_member_acts_for_their_first_tenant_until_they_switch() {
    let router = notes_router(seeded_db().await);
    let session = login_session(&router).await;

    let (status, html) = notes(&router, &session).await;
    assert_eq!(status, 200);
    assert!(html.contains("acme note"), "the first membership: {html}");
    assert!(!html.contains("globex note"));
    assert!(!html.contains("initech note"));
    // Two memberships: the top bar offers both.
    assert!(html.contains("data-tenant-switcher"), "{html}");
    assert!(html.contains("Globex"), "{html}");

    let switched = switch_tenant(&router, &session, GLOBEX).await;
    assert_eq!(switched.status(), 303);
    assert_eq!(switched.headers().get(LOCATION).unwrap(), "/admin");

    let (_, html) = notes(&router, &session).await;
    assert!(html.contains("globex note"), "the selected tenant: {html}");
    assert!(!html.contains("acme note"));
}

#[tokio::test]
async fn a_member_cannot_switch_to_a_tenant_they_do_not_belong_to() {
    let router = notes_router(seeded_db().await);
    let session = login_session(&router).await;

    let refused = switch_tenant(&router, &session, INITECH).await;
    assert_eq!(refused.status(), 403);
    let (_, html) = notes(&router, &session).await;
    assert!(
        html.contains("acme note"),
        "the selection is unchanged: {html}"
    );
    assert!(!html.contains("initech note"));

    // Without a CSRF field the switch is refused.
    let forged = post_form(
        &router,
        "/admin/tenant",
        &[("__Host-session", session.clone())],
        format!("tenant={GLOBEX}"),
    )
    .await;
    assert_eq!(forged.status(), 403);
    let (_, html) = notes(&router, &session).await;
    assert!(html.contains("acme note"), "{html}");
}

#[tokio::test]
async fn a_removed_membership_stops_applying_mid_session() {
    let db = seeded_db().await;
    let router = notes_router(db.clone());
    let session = login_session(&router).await;
    assert_eq!(switch_tenant(&router, &session, GLOBEX).await.status(), 303);

    let mut db = db;
    Seat::filter(Seat::fields().tenant().eq(GLOBEX))
        .delete()
        .exec(&mut db)
        .await
        .unwrap();

    let (status, html) = notes(&router, &session).await;
    assert_eq!(status, 200);
    assert!(
        html.contains("acme note") && !html.contains("globex note"),
        "the stored selection no longer applies: {html}"
    );
    // One membership left: nothing to switch.
    assert!(!html.contains("data-tenant-switcher"), "{html}");
}

#[tokio::test]
async fn a_member_without_a_tenant_is_refused_tenant_owned_records() {
    let router = notes_router(seeded_db().await);
    let session = login_as(&router, "grace").await;

    let (status, _) = notes(&router, &session).await;
    assert_eq!(status, 403, "no tenant: the scoped resource refuses");
    let members = get_with_cookies(
        &router,
        "/admin/members",
        &[("__Host-session", session.clone())],
    )
    .await;
    assert_eq!(members.status(), 200, "unscoped resources still answer");
    assert!(!body_string(members).await.contains("data-tenant-switcher"));
}

#[tokio::test]
async fn a_policy_reads_the_apps_own_user_type() {
    let db = seeded_db().await;
    let mut seed = db.clone();
    let grace = Member::filter(Member::fields().handle().eq("grace".to_string()))
        .first()
        .exec(&mut seed)
        .await
        .unwrap()
        .expect("grace");
    toasty::create!(Seat {
        member_id: grace.id,
        tenant: ACME,
        tenant_name: "Acme".to_string(),
    })
    .exec(&mut seed)
    .await
    .unwrap();
    let router = notes_router(db);

    let ada = login_as(&router, "ada").await;
    assert_eq!(notes(&router, &ada).await.0, 200);
    // Grace now has Acme's tenant, but the policy reads her handle.
    let grace = login_as(&router, "grace").await;
    assert_eq!(notes(&router, &grace).await.0, 403);
}
