//! `Panel::load`: a value the panel reads once for each request it admits, which a policy and a
//! `query_fn` read without awaiting.

use std::sync::atomic::{AtomicUsize, Ordering};

use tablo::{
    Ability, Auth, Load, NoForm, Panel, Resource, ResourceDef, Table, Tenancy, TenantId,
    TextColumn,
    auth::{AdminUser, AuthSession, create_admin},
    lens, loaded,
    testing::{body_string, rows},
};
use topcoat::{Result, context::Cx};
use uuid::Uuid;

use crate::common::{TestClient, mount, panel, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::PanelModel)]
struct Project {
    #[key]
    #[auto]
    id: Uuid,
    #[index]
    team: TenantId,
    owner: String,
    name: String,
}

/// What a team pays for, which no request carries.
#[derive(Debug, Clone, toasty::Model)]
struct Subscription {
    #[key]
    #[auto]
    id: Uuid,
    team: TenantId,
    /// The one owner whose projects the team's members see.
    shows: String,
}

const PAYING: TenantId = TenantId::new(Uuid::from_u128(1));
const LAPSED: TenantId = TenantId::new(Uuid::from_u128(2));

/// The subscription of the team the request acts for, read from the database.
struct Plan(Option<Subscription>);

impl Load for Plan {
    async fn load(cx: &Cx) -> Result<Self> {
        let Some(team) = tablo::tenant_id(cx) else {
            return Ok(Self(None));
        };
        let mut db = tablo::db::db(cx)?;
        let held = Subscription::filter(Subscription::fields().team().eq(team))
            .exec(&mut db)
            .await?;
        Ok(Self(held.into_iter().next()))
    }
}

/// Projects a subscribed team lists, narrowed to the owner its subscription shows.
struct ProjectResource;

impl Resource for ProjectResource {
    type Model = Project;
    type Form = NoForm<Project>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("projects")
            .tenancy(Tenancy::column(lens!(Project.team)))
            .policy_fn(|cx, ability| match ability {
                Ability::ViewAny | Ability::View(_) => {
                    loaded::<Plan>(cx).is_some_and(|plan| plan.0.is_some())
                }
                _ => false,
            })
            .query_fn(|cx| {
                let plan = loaded::<Plan>(cx)?.0.as_ref()?;
                Some(Project::fields().owner().eq(plan.shows.clone()))
            })
            .table(Table::new(TextColumn::new(lens!(Project.name))))
    }
}

async fn seeded() -> toasty::Db {
    let mut db = test_db(toasty::models!(Project, Subscription)).await;
    toasty::create!(Subscription {
        team: PAYING,
        shows: "ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    for (team, owner, name) in [
        (PAYING, "ada", "Engine"),
        (PAYING, "bob", "Loom"),
        (LAPSED, "ada", "Abacus"),
    ] {
        toasty::create!(Project {
            team: team,
            owner: owner.to_string(),
            name: name.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    db
}

/// The policy and the query both decide by a row the request loaded once, without awaiting.
#[tokio::test]
async fn a_policy_and_a_query_read_what_the_request_loaded() {
    let db = seeded().await;
    let router =
        mount(db, panel().load::<Plan>().resource::<ProjectResource>()).expect("panel builds");
    let client = TestClient::new(&router);

    let paying = client.tenant(PAYING).get("/admin/projects").await;
    assert_eq!(paying.status(), 200);
    let html = body_string(paying).await;
    assert_eq!(rows(&html).len(), 1, "{html}");
    assert!(html.contains("Engine"), "{html}");

    let lapsed = client.tenant(LAPSED).get("/admin/projects").await;
    assert_eq!(lapsed.status(), 403);
}

/// A panel that does not load the value refuses, since the policy reads `None`.
#[tokio::test]
async fn a_value_the_panel_does_not_load_reads_none() {
    let db = seeded().await;
    let router = mount(db, panel().resource::<ProjectResource>()).expect("panel builds");
    let response = TestClient::new(&router)
        .tenant(PAYING)
        .get("/admin/projects")
        .await;
    assert_eq!(response.status(), 403);
}

static FIRST_LOADS: AtomicUsize = AtomicUsize::new(0);

struct First(usize);

impl Load for First {
    async fn load(_cx: &Cx) -> Result<Self> {
        Ok(Self(FIRST_LOADS.fetch_add(1, Ordering::SeqCst) + 1))
    }
}

/// Reads [`First`], which the panel registers before it.
struct Second(Option<usize>);

impl Load for Second {
    async fn load(cx: &Cx) -> Result<Self> {
        Ok(Self(loaded::<First>(cx).map(|first| first.0)))
    }
}

struct OrderedResource;

impl Resource for OrderedResource {
    type Model = Project;
    type Form = NoForm<Project>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("ordered")
            .tenancy(Tenancy::none())
            .policy_fn(|cx, _| loaded::<Second>(cx).is_some_and(|second| second.0.is_some()))
            .table(Table::new(TextColumn::new(lens!(Project.name))))
    }
}

/// Each value loads once per request, however often a policy asks, and reads those before it.
#[tokio::test]
async fn values_load_once_per_request_in_registration_order() {
    let db = seeded().await;
    let panel = panel()
        .load::<First>()
        .load::<Second>()
        .load::<First>()
        .resource::<OrderedResource>();
    let router = mount(db, panel).expect("panel builds");
    let client = TestClient::new(&router);

    let response = client.get("/admin/ordered").await;
    assert_eq!(response.status(), 200);
    assert_eq!(rows(&body_string(response).await).len(), 3);
    assert_eq!(FIRST_LOADS.load(Ordering::SeqCst), 1);

    client.get("/admin/ordered").await;
    assert_eq!(FIRST_LOADS.load(Ordering::SeqCst), 2);
}

struct Broken;

impl Load for Broken {
    async fn load(_cx: &Cx) -> Result<Self> {
        Err(topcoat::Error::msg("the plan service is down"))
    }
}

/// A load that fails stops the request before a handler answers.
#[tokio::test]
async fn a_failed_load_fails_the_request() {
    let db = seeded().await;
    let router =
        mount(db, panel().load::<Broken>().resource::<ProjectResource>()).expect("panel builds");
    let response = TestClient::new(&router).get("/admin/projects").await;
    assert_eq!(response.status(), 500);
}

static GATED_LOADS: AtomicUsize = AtomicUsize::new(0);

/// Who the request is signed in as when it loads.
struct Greeting(Option<String>);

impl Load for Greeting {
    async fn load(cx: &Cx) -> Result<Self> {
        GATED_LOADS.fetch_add(1, Ordering::SeqCst);
        Ok(Self(
            tablo::auth::user::<AdminUser>(cx).map(|user| user.display_name.clone()),
        ))
    }
}

struct GreetedResource;

impl Resource for GreetedResource {
    type Model = Project;
    type Form = NoForm<Project>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("greeted")
            .tenancy(Tenancy::none())
            .policy_fn(|cx, _| {
                loaded::<Greeting>(cx).is_some_and(|greeting| greeting.0.as_deref() == Some("Ada"))
            })
            .table(Table::new(TextColumn::new(lens!(Project.name))))
    }
}

/// Behind the auth gate a load runs for an admitted user, whom it reads, and not for the login
/// page a signed-out visitor reaches.
#[tokio::test]
async fn a_load_runs_behind_the_auth_gate() {
    let mut db = test_db(toasty::models!(Project, AdminUser, AuthSession)).await;
    let ada = create_admin(&mut db, "ada@example.com", "secret", "Ada")
        .await
        .expect("seed the admin");
    let panel = Panel::new("admin")
        .auth(Auth::password())
        .load::<Greeting>()
        .resource::<GreetedResource>();
    let router = mount(db.clone(), panel).expect("panel builds");
    let visitor = TestClient::new(&router);

    assert_eq!(visitor.get("/admin/login").await.status(), 200);
    assert!(
        visitor
            .get("/admin/greeted")
            .await
            .status()
            .is_redirection()
    );
    assert_eq!(GATED_LOADS.load(Ordering::SeqCst), 0);

    let signed = visitor.sign_in(&db, "admin", &ada).await;
    assert_eq!(signed.get("/admin/greeted").await.status(), 200);
    assert_eq!(GATED_LOADS.load(Ordering::SeqCst), 1);
}
