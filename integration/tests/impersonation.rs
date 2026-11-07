//! Acting as another user: off until the app gives a rule, asked again on every request, ended by
//! Stop or its lifetime, kept away from the target's credentials, and written to the audit log.

use std::time::Duration;

use tablo::{
    Allow, AuditEntry, Auth, Impersonate, Impersonation, NoForm, Panel, Resource, ResourceDef,
    Table, TextColumn,
    auth::{AdminUser, AuthSession, IMPERSONATE_FIELD, create_admin},
    lens,
    testing::{form_body, impersonation_banner, mount_with_handle},
};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{TestClient, body_string, test_db};

struct Users;

impl Resource for Users {
    type Model = AdminUser;
    type Form = NoForm<AdminUser>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("users")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(AdminUser.email))))
            .action::<Impersonate>()
    }
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Memo {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Memo)]
struct MemoForm {
    title: String,
}

struct Memos;

impl Resource for Memos {
    type Model = Memo;
    type Form = MemoForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("memos")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Memo.title))))
    }
}

/// Support staff may act as anyone; nobody else may act as anyone.
fn support_only() -> Impersonation<AdminUser> {
    Impersonation::new(|_cx, actor: &AdminUser, _target: &AdminUser| {
        actor.email.starts_with("support@")
    })
}

struct Fixture {
    db: Db,
    router: Router,
    handle: tablo::PanelHandle,
    support: AdminUser,
    bob: AdminUser,
}

impl Fixture {
    async fn new(auth: Auth) -> Self {
        Self::with(auth, |panel| panel).await
    }

    async fn with(auth: Auth, customize: impl FnOnce(Panel) -> Panel) -> Self {
        let mut db = test_db(toasty::models!(AdminUser, AuthSession, AuditEntry, Memo)).await;
        let support = create_admin(&mut db, "support@example.com", "pw-support", "Sam Support")
            .await
            .expect("seed support");
        let bob = create_admin(&mut db, "bob@example.com", "pw-bob", "Bob Builder")
            .await
            .expect("seed bob");
        let panel = Panel::new("admin")
            .auth(auth)
            .resource::<Users>()
            .resource::<Memos>();
        let (router, handle) =
            mount_with_handle(db.clone(), customize(panel)).expect("panel builds");
        Self {
            db,
            router,
            handle,
            support,
            bob,
        }
    }

    async fn as_user(&self, user: &AdminUser) -> TestClient<'_> {
        TestClient::new(&self.router)
            .panel(&self.handle)
            .sign_in(&self.db, "admin", user)
            .await
    }

    async fn sessions(&self) -> Vec<AuthSession> {
        AuthSession::all()
            .exec(&mut self.db.clone())
            .await
            .expect("query sessions")
    }
}

fn act_as(user: &AdminUser) -> String {
    form_body(&[(IMPERSONATE_FIELD, &user.id.to_string())])
}

async fn banner(client: &TestClient<'_>) -> Option<tablo::testing::ImpersonationBanner> {
    let page = client.get("/admin/memos").await;
    assert_eq!(page.status(), 200, "the panel serves the page");
    impersonation_banner(&body_string(page).await)
}

/// Without a rule, the routes are not there and no button offers it.
#[tokio::test]
async fn nobody_acts_as_another_until_the_app_gives_a_rule() {
    let app = Fixture::new(Auth::password()).await;
    let client = app.as_user(&app.support).await;
    let response = client.submit("/admin/impersonate", &act_as(&app.bob)).await;
    assert_eq!(response.status(), 404);
    let list = client.resource::<Users>().list().await;
    let action = client.resource::<Users>().action::<Impersonate>(&app.bob);
    assert!(tablo::testing::action_label(list.html(), action.url()).is_none());
    assert!(app.sessions().await[0].impersonating.is_none());
}

/// The action acts as the record's user; every page then reads them, under a banner whose Stop
/// restores the actor.
#[tokio::test]
async fn acting_as_a_user_until_stop() {
    let app = Fixture::new(Auth::password().impersonation(support_only())).await;
    let client = app.as_user(&app.support).await;
    let users = client.resource::<Users>();

    let list = users.list().await;
    let action = users.action::<Impersonate>(&app.bob);
    assert!(
        tablo::testing::action_label(list.html(), action.url()).is_some(),
        "the button shows on a user the rule allows"
    );
    let ran = action.submit().await;
    assert_eq!(ran.status(), 303);
    assert_eq!(ran.location(), Some("/admin"));

    let shown = banner(&client).await.expect("the banner shows");
    assert_eq!(shown.user_id, app.bob.id.to_string());
    assert!(shown.text.contains("Bob Builder"), "{}", shown.text);
    assert_eq!(shown.stop, "/admin/impersonate/stop");
    let row = app.sessions().await.remove(0);
    assert_eq!(
        row.user_id,
        app.support.id.to_string(),
        "the session stays the actor's"
    );
    assert_eq!(row.impersonating, Some(app.bob.id.to_string()));

    let stopped = client.submit("/admin/impersonate/stop", "").await;
    assert_eq!(stopped.status(), 303);
    assert!(banner(&client).await.is_none());
    assert!(app.sessions().await[0].impersonating.is_none());
}

/// The route answers 403 alike to a user the rule refuses, to acting as oneself, to an inactive
/// target, to an unknown id, and to a request without the CSRF token.
#[tokio::test]
async fn the_rule_and_the_panel_refuse_alike() {
    let app = Fixture::new(Auth::password().impersonation(support_only())).await;
    let mut db = app.db.clone();
    let idle = create_admin(&mut db, "idle@example.com", "pw-idle", "Idle")
        .await
        .expect("seed idle");
    AdminUser::filter(AdminUser::fields().id().eq(idle.id))
        .update()
        .active(false)
        .exec(&mut db)
        .await
        .expect("deactivate idle");

    let bob = app.as_user(&app.bob).await;
    let refused = bob
        .submit("/admin/impersonate", &act_as(&app.support))
        .await;
    assert_eq!(refused.status(), 403, "bob is not support");
    let list = bob.resource::<Users>().list().await;
    let action = bob.resource::<Users>().action::<Impersonate>(&app.support);
    assert!(tablo::testing::action_label(list.html(), action.url()).is_none());

    let support = app.as_user(&app.support).await;
    for (case, body) in [
        ("oneself", act_as(&app.support)),
        ("an inactive user", act_as(&idle)),
        ("an unknown id", form_body(&[(IMPERSONATE_FIELD, "nobody")])),
        ("no user", String::new()),
    ] {
        let response = support.submit("/admin/impersonate", &body).await;
        assert_eq!(response.status(), 403, "{case}");
    }
    let forged = support
        .post_form("/admin/impersonate", act_as(&app.bob))
        .await;
    assert_eq!(forged.status(), 403, "a post without the CSRF token");
    assert!(
        app.sessions()
            .await
            .iter()
            .all(|row| row.impersonating.is_none())
    );
}

/// A user acting as another may not act as a third, nor open the pages that change the target's
/// credentials.
#[tokio::test]
async fn acting_as_another_reaches_no_credential_page_and_no_third_user() {
    let app = Fixture::new(
        Auth::password()
            .profile(tablo::PasswordAuth)
            .impersonation(support_only()),
    )
    .await;
    let client = app.as_user(&app.support).await;
    assert_eq!(client.get("/admin/profile").await.status(), 200);
    let started = client.submit("/admin/impersonate", &act_as(&app.bob)).await;
    assert_eq!(started.status(), 303);

    assert_eq!(client.get("/admin/profile").await.status(), 403);
    let posted = client
        .submit(
            "/admin/profile",
            &form_body(&[("name", "Mallory"), ("email", "mallory@example.com")]),
        )
        .await;
    assert_eq!(posted.status(), 403);
    let nested = client
        .submit("/admin/impersonate", &act_as(&app.support))
        .await;
    assert_eq!(nested.status(), 403, "no nesting");
    let shown = banner(&client).await.expect("still acting as bob");
    assert_eq!(shown.user_id, app.bob.id.to_string());
}

/// The rule is asked on every request: a target who loses panel access, or a lifetime that ran
/// out, returns the actor to themselves and clears the row.
#[tokio::test]
async fn acting_as_another_ends_when_the_target_or_the_clock_says_so() {
    let app = Fixture::new(Auth::password().impersonation(support_only())).await;
    let client = app.as_user(&app.support).await;
    client.submit("/admin/impersonate", &act_as(&app.bob)).await;
    assert!(banner(&client).await.is_some());
    AdminUser::filter(AdminUser::fields().id().eq(app.bob.id))
        .update()
        .active(false)
        .exec(&mut app.db.clone())
        .await
        .expect("deactivate bob");
    assert!(banner(&client).await.is_none(), "the actor is back");
    assert!(app.sessions().await[0].impersonating.is_none());

    let brief =
        Fixture::new(Auth::password().impersonation(support_only().lifetime(Duration::ZERO))).await;
    let client = brief.as_user(&brief.support).await;
    let started = client
        .submit("/admin/impersonate", &act_as(&brief.bob))
        .await;
    assert_eq!(started.status(), 303);
    assert!(banner(&client).await.is_none(), "the lifetime ran out");
}

/// Starting and stopping are written to the audit log for the actor, and every write made
/// meanwhile names the actor too.
#[tokio::test]
async fn the_audit_log_names_the_actor() {
    let app = Fixture::with(Auth::password().impersonation(support_only()), |panel| {
        panel.audit_log(|log| log.policy(Allow))
    })
    .await;
    let client = app.as_user(&app.support).await;
    client.submit("/admin/impersonate", &act_as(&app.bob)).await;
    let created = client
        .submit("/admin/memos/-/create", &form_body(&[("title", "Fixed")]))
        .await;
    assert_eq!(created.status(), 303);
    client.submit("/admin/impersonate/stop", "").await;

    let mut entries = AuditEntry::all()
        .exec(&mut app.db.clone())
        .await
        .expect("query entries");
    entries.sort_by_key(|entry| entry.created_at);
    let events: Vec<&str> = entries.iter().map(|entry| entry.event.as_str()).collect();
    assert_eq!(events, ["impersonate", "create", "stop_impersonating"]);
    let support = app.support.id.to_string();
    for entry in &entries {
        assert_eq!(entry.actor_id.as_ref(), Some(&support), "{entry:?}");
    }
    let bob = app.bob.id.to_string();
    assert_eq!(entries[0].record.as_ref(), Some(&bob));
    assert_eq!(entries[2].record.as_ref(), Some(&bob));
}

/// Acting as another that ends on its own, here because the target lost panel access, is written
/// to the audit log as a stop.
#[tokio::test]
async fn an_impersonation_that_ends_on_its_own_is_audited() {
    let app = Fixture::with(Auth::password().impersonation(support_only()), |panel| {
        panel.audit_log(|log| log.policy(Allow))
    })
    .await;
    let client = app.as_user(&app.support).await;
    client.submit("/admin/impersonate", &act_as(&app.bob)).await;
    AdminUser::filter(AdminUser::fields().id().eq(app.bob.id))
        .update()
        .active(false)
        .exec(&mut app.db.clone())
        .await
        .expect("deactivate bob");
    assert!(banner(&client).await.is_none(), "the actor is back");
    let mut entries = AuditEntry::all()
        .exec(&mut app.db.clone())
        .await
        .expect("query entries");
    entries.sort_by_key(|entry| entry.created_at);
    let events: Vec<&str> = entries.iter().map(|entry| entry.event.as_str()).collect();
    assert_eq!(events, ["impersonate", "stop_impersonating"]);
    assert_eq!(entries[1].actor_id, Some(app.support.id.to_string()));
}

/// On a panel that acts for tenants, the actor acts only as a user of their own tenants, unless
/// the rule says `across_tenants`; acting as one starts with no tenant chosen.
#[tokio::test]
async fn acting_as_another_stays_within_the_actors_tenants() {
    use crate::common::members::{self, MemberAuth, SignedMember};

    let support_rule = || {
        Impersonation::new(|_cx, actor: &SignedMember, _target: &SignedMember| {
            actor.member.name == "Support"
        })
    };
    let db = members::db().await;
    let support = members::member(&db, "Support", &["Acme"]).await;
    let alice = members::member(&db, "Alice", &[]).await;
    members::seat(&db, &alice.member, support.owned[0], "Acme", false).await;
    let mallory = members::member(&db, "Mallory", &["Globex"]).await;
    let target =
        |user: &SignedMember| form_body(&[(IMPERSONATE_FIELD, &user.member.id.to_string())]);

    let router = members::mount(
        db.clone(),
        Auth::custom(MemberAuth).impersonation(support_rule()),
    )
    .expect("panel builds");
    let client = TestClient::new(&router)
        .sign_in(&db, "admin", &support)
        .await;
    let refused = client.submit("/admin/impersonate", &target(&mallory)).await;
    assert_eq!(
        refused.status(),
        403,
        "Globex is not one of support's tenants"
    );
    let started = client.submit("/admin/impersonate", &target(&alice)).await;
    assert_eq!(started.status(), 303, "Acme is");
    assert_eq!(
        members::session_tenant(&db).await,
        None,
        "the tenant follows the target"
    );
    let page = body_string(client.get("/admin/notes").await).await;
    let shown = impersonation_banner(&page).expect("acting as alice");
    assert_eq!(shown.user_id, alice.member.id.to_string());

    let db = members::db().await;
    let support = members::member(&db, "Support", &["Acme"]).await;
    let mallory = members::member(&db, "Mallory", &["Globex"]).await;
    let rule = support_rule().across_tenants();
    let router = members::mount(db.clone(), Auth::custom(MemberAuth).impersonation(rule))
        .expect("panel builds");
    let client = TestClient::new(&router)
        .sign_in(&db, "admin", &support)
        .await;
    let started = client.submit("/admin/impersonate", &target(&mallory)).await;
    assert_eq!(
        started.status(),
        303,
        "across_tenants lets support act as any member"
    );
}
