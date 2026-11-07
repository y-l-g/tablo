//! Areas: the resources and pages an area mounts under its URL, the sidebar it shows in place of
//! the panel's, the redirect at its root, the pages auth serves that it holds and their header
//! actions, and the mistakes mounting refuses.

use http::header::LOCATION;
use tablo::{
    Area, AreaFault, Auth, DeclarationErrorKind, FieldErrors, FormInput, HeaderAction, Membership,
    NoForm, Outcome, Page, PageDef, PasswordAuth, ReadOnly, Resource, ResourceDef,
    RouterBuilderPanelExt, Table, TextColumn, WriteError,
    auth::{AdminUser, AuthSession, TenantEditor, create_admin},
    lens,
    testing::{
        NavGroup, TestClient, action_label, area_back_link, find_href_with, form_body,
        mount_with_handle, nav_groups, nav_links, tenant_page_link, trail,
    },
};
use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    router::{Router, RouterBuilderDiscoverExt},
    view::{View, view},
};
use uuid::Uuid;

use crate::common::{
    body_string,
    members::{self, MemberAuth, NoteResource, SignedMember},
    mount, panel, refusal, test_db,
};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

/// Books in the panel's own sidebar, at `/admin/books`.
struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("books")
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Book.title))))
    }
}

/// The same books under the same slug, which an area's namespace keeps apart.
struct ShelfResource;

impl Resource for ShelfResource {
    type Model = Book;
    type Form = NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("books")
            .policy(ReadOnly)
            .navigation_group("Ignored")
            .table(Table::new(TextColumn::new(lens!(Book.title))))
    }
}

/// A page anyone may open.
struct AboutPage;

impl Page for AboutPage {
    fn declare() -> PageDef {
        PageDef::new().slug("about")
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { "about" })
    }
}

/// A page nobody may open.
struct ClosedPage;

impl Page for ClosedPage {
    fn declare() -> PageDef {
        PageDef::new().slug("closed").access(|_cx| false)
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { "closed" })
    }
}

/// Another page nobody may open.
struct SealedPage;

impl Page for SealedPage {
    fn declare() -> PageDef {
        PageDef::new().access(|_cx| false)
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { "sealed" })
    }
}

fn settings() -> Area {
    Area::new("settings").group("Library", |group| {
        group.resource::<ShelfResource>().page::<AboutPage>()
    })
}

/// The area mounts its entries under its URL, beside a panel entry of the same slug. The panel's
/// sidebar shows the area as one entry; inside the area, the sidebar shows the link back and the
/// area's group, and the trail starts at the area.
#[tokio::test]
async fn an_area_serves_its_entries_under_its_url_with_a_sidebar_of_its_own() {
    let db = test_db(toasty::models!(Book)).await;
    let (router, handle) =
        mount_with_handle(db, panel().resource::<BookResource>().area(settings()))
            .expect("the panel mounts");
    let client = TestClient::new(&router).panel(&handle);

    let books = body_string(client.get("/admin/books").await).await;
    assert_eq!(nav_links(&books), ["/admin/books", "/admin/settings"]);
    assert_eq!(area_back_link(&books), None);

    let shelf = client.get("/admin/settings/books").await;
    assert_eq!(shelf.status(), 200);
    let shelf = body_string(shelf).await;
    assert_eq!(
        nav_groups(&shelf),
        [NavGroup {
            label: "Library".to_string(),
            links: vec![
                "/admin/settings/books".to_string(),
                "/admin/settings/about".to_string()
            ],
        }]
    );
    assert_eq!(area_back_link(&shelf).as_deref(), Some("/admin"));
    assert_eq!(
        trail(&shelf).first(),
        Some(&("Settings".to_string(), Some("/admin/settings".to_string())))
    );

    let root = client.get("/admin/settings").await;
    assert_eq!(root.headers()[LOCATION], "/admin/settings/books");
}

/// The area's root opens the first entry the user may open, and an area holding none they may
/// open answers 404 at its root and leaves the panel's sidebar.
#[tokio::test]
async fn the_area_root_skips_what_the_user_may_not_open() {
    let db = test_db(toasty::models!(Book)).await;
    let (router, handle) = mount_with_handle(
        db,
        panel()
            .resource::<BookResource>()
            .area(Area::new("open").group("Pages", |group| {
                group.page::<ClosedPage>().page::<AboutPage>()
            }))
            .area(Area::new("sealed").group("Pages", |group| group.page::<SealedPage>())),
    )
    .expect("the panel mounts");
    let client = TestClient::new(&router).panel(&handle);

    let root = client.get("/admin/open").await;
    assert_eq!(root.headers()[LOCATION], "/admin/open/about");
    assert_eq!(client.get("/admin/sealed").await.status(), 404);
    let books = body_string(client.get("/admin/books").await).await;
    assert_eq!(nav_links(&books), ["/admin/books", "/admin/open"]);
}

/// An area without navigation stays mounted, and the panel's sidebar does not list it.
#[tokio::test]
async fn an_area_without_navigation_leaves_the_panel_sidebar() {
    let db = test_db(toasty::models!(Book)).await;
    let (router, handle) = mount_with_handle(
        db,
        panel()
            .resource::<BookResource>()
            .area(settings().without_navigation()),
    )
    .expect("the panel mounts");
    let client = TestClient::new(&router).panel(&handle);

    let books = body_string(client.get("/admin/books").await).await;
    assert_eq!(nav_links(&books), ["/admin/books"]);
    assert_eq!(client.get("/admin/settings/about").await.status(), 200);
}

/// An area that holds the profile page serves it, and saves it, under the area: the user menu
/// links it there, and the prefix no longer serves it.
#[tokio::test]
async fn the_profile_page_moves_into_the_area_that_holds_it() {
    let db = test_db(toasty::models!(Book, AdminUser, AuthSession)).await;
    let admin = create_admin(&mut db.clone(), "ada@example.com", "a password", "Ada")
        .await
        .unwrap();
    let router = mount(
        db.clone(),
        tablo::Panel::new("admin")
            .auth(Auth::password().profile(PasswordAuth))
            .resource::<BookResource>()
            .area(Area::new("settings").group("Account", |group| group.profile())),
    )
    .expect("the panel mounts");
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    let books = body_string(client.get("/admin/books").await).await;
    assert!(find_href_with(&books, "/admin/settings/profile").is_some());
    let page = body_string(client.get("/admin/settings/profile").await).await;
    assert_eq!(
        tablo::testing::input_value(&page, "name").as_deref(),
        Some("Ada")
    );
    assert_eq!(area_back_link(&page).as_deref(), Some("/admin"));
    assert_eq!(client.get("/admin/profile").await.status(), 404);

    let saved = client
        .submit(
            "/admin/settings/profile",
            &form_body(&[("name", "Ada L."), ("email", "ada@example.com")]),
        )
        .await;
    assert_eq!(saved.headers()[LOCATION], "/admin/settings/profile");
}

/// Signs every session of the user out, from the profile's header.
struct SignOutEverywhere;

impl HeaderAction for SignOutEverywhere {
    type Input = ();
    const NAME: &'static str = "sign-out-everywhere";

    async fn run(
        _cx: &Cx,
        _input: (),
        _ex: &mut dyn toasty::Executor,
    ) -> Result<Outcome, WriteError<std::convert::Infallible>> {
        Ok(Outcome::done())
    }
}

/// A header action no request may run.
struct Never;

impl HeaderAction for Never {
    type Input = ();
    const NAME: &'static str = "never";

    fn can_run(_cx: &Cx) -> bool {
        false
    }

    async fn run(
        _cx: &Cx,
        _input: (),
        _ex: &mut dyn toasty::Executor,
    ) -> Result<Outcome, WriteError<std::convert::Infallible>> {
        Ok(Outcome::done())
    }
}

/// The profile page placed in an area renders its header actions over the form, serves them
/// under its URL and lands back on it, and refuses one the request may not run.
#[tokio::test]
async fn the_profile_page_in_an_area_runs_its_header_actions() {
    let db = test_db(toasty::models!(Book, AdminUser, AuthSession)).await;
    let admin = create_admin(&mut db.clone(), "ada@example.com", "a password", "Ada")
        .await
        .unwrap();
    let router = mount(
        db.clone(),
        tablo::Panel::new("admin")
            .auth(Auth::password().profile(PasswordAuth))
            .resource::<BookResource>()
            .area(Area::new("settings").group("Account", |group| {
                group.profile_with(|def| {
                    def.header_action::<SignOutEverywhere>()
                        .header_action::<Never>()
                })
            })),
    )
    .expect("the panel mounts");
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    let action = "/admin/settings/profile/-/actions/sign-out-everywhere";
    let page = body_string(client.get("/admin/settings/profile").await).await;
    assert!(action_label(&page, action).is_some());
    assert!(action_label(&page, "/admin/settings/profile/-/actions/never").is_none());
    assert_eq!(
        tablo::testing::input_value(&page, "name").as_deref(),
        Some("Ada")
    );

    let ran = client.submit(action, &form_body(&[])).await;
    assert_eq!(ran.headers()[LOCATION], "/admin/settings/profile");
    let refused = client
        .submit("/admin/settings/profile/-/actions/never", &form_body(&[]))
        .await;
    assert_eq!(refused.status(), 403);
}

/// What the tenant settings page asks for.
#[derive(FormInput)]
struct TeamName {
    name: String,
}

/// Lets a tenant's owner open its settings; saves nothing.
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
        _renamed: &TeamName,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<FieldErrors> {
        Ok(FieldErrors::new())
    }

    async fn save(
        &self,
        _cx: &Cx,
        _signed: &SignedMember,
        _team: &Membership,
        _renamed: TeamName,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        Ok(())
    }
}

fn team_router(db: Db) -> Router {
    Router::builder()
        .discover()
        .with(db)
        .panel(
            tablo::Panel::new("admin")
                .auth(Auth::custom(MemberAuth).tenant_profile(Teams))
                .area(Area::new("settings").tenant_group(|group| {
                    group
                        .tenant_profile_with(|def| {
                            def.slug("team")
                                .label("General")
                                .header_action::<SignOutEverywhere>()
                        })
                        .resource::<NoteResource>()
                })),
        )
        .expect("the panel mounts")
        .build()
}

/// The tenant group renders under the name of the tenant the request acts for, holds the tenant
/// settings page the tenant menu links, and shows a member who acts for no tenant nothing.
#[tokio::test]
async fn the_tenant_group_is_headed_by_the_tenant_the_request_acts_for() {
    let db = members::db().await;
    let ada = members::member(&db, "Ada", &["Acme"]).await;
    let router = team_router(db.clone());
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;

    let page = body_string(client.get("/admin/settings/team").await).await;
    assert_eq!(
        nav_groups(&page),
        [NavGroup {
            label: "Acme".to_string(),
            links: vec![
                "/admin/settings/team".to_string(),
                "/admin/settings/notes".to_string()
            ],
        }]
    );
    assert_eq!(
        tenant_page_link(&page, "profile").as_deref(),
        Some("/admin/settings/team")
    );
    assert_eq!(client.get("/admin/tenant/profile").await.status(), 404);

    let alan = members::member(&db, "Alan", &[]).await;
    let client = TestClient::new(&router).sign_in(&db, "admin", &alan).await;
    assert_eq!(client.get("/admin/settings").await.status(), 404);
}

/// The tenant settings page placed in an area serves its header actions to whom auth offers the
/// page, the tenant's owner, and refuses them to a member who may not edit the tenant.
#[tokio::test]
async fn the_tenant_settings_actions_follow_who_may_edit_the_tenant() {
    let db = members::db().await;
    let ada = members::member(&db, "Ada", &["Acme"]).await;
    let alan = members::member(&db, "Alan", &[]).await;
    members::seat(&db, &alan.member, ada.owned[0], "Acme", false).await;
    let router = team_router(db.clone());
    let action = "/admin/settings/team/-/actions/sign-out-everywhere";

    let owner = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let page = body_string(owner.get("/admin/settings/team").await).await;
    assert!(action_label(&page, action).is_some());
    let ran = owner.submit(action, &form_body(&[])).await;
    assert_eq!(ran.headers()[LOCATION], "/admin/settings/team");

    let member = TestClient::new(&router).sign_in(&db, "admin", &alan).await;
    let refused = member.submit(action, &form_body(&[])).await;
    assert_eq!(refused.status(), 403);
}

/// An area holding a page auth does not install, or the same page twice, or a def declaring
/// what the page does not render, refuses to mount; so does a slug taken twice in one area.
#[tokio::test]
async fn mounting_refuses_an_area_holding_what_it_cannot_serve() {
    let db = test_db(toasty::models!(Book)).await;
    let kinds = |area: Area| {
        refusal(mount(
            db.clone(),
            panel().resource::<BookResource>().area(area),
        ))
        .into_iter()
        .map(|error| error.kind)
        .collect::<Vec<_>>()
    };

    assert_eq!(
        kinds(Area::new("settings").group("Account", |group| group.profile())),
        [DeclarationErrorKind::Area(AreaFault::AuthPageNotOffered(
            "profile"
        ))]
    );
    assert_eq!(
        kinds(
            Area::new("settings")
                .group("One", |group| group.two_factor())
                .group("Two", |group| group.two_factor())
        )
        .first(),
        Some(&DeclarationErrorKind::Area(AreaFault::AuthPagePlacedTwice(
            "two-factor"
        )))
    );
    assert_eq!(
        kinds(Area::new("settings").group("Account", |group| {
            group.profile_with(|def| def.access(|_cx| false))
        })),
        [DeclarationErrorKind::Area(AreaFault::AuthPageDeclares {
            page: "profile",
            feature: "an access check",
        })]
    );
    assert_eq!(
        kinds(Area::new("settings").group("Account", |group| {
            group.two_factor_with(|def| def.header_action::<SignOutEverywhere>())
        })),
        [DeclarationErrorKind::Area(AreaFault::AuthPageDeclares {
            page: "two-factor",
            feature: "header actions",
        })]
    );
    assert_eq!(
        kinds(Area::new("settings").group("Library", |group| {
            group
                .resource::<ShelfResource>()
                .page_with::<AboutPage>(|def| def.slug("books"))
        })),
        [DeclarationErrorKind::DuplicateSlug {
            slug: "books".to_string()
        }]
    );
    assert_eq!(
        kinds(Area::new("books")),
        [DeclarationErrorKind::DuplicateSlug {
            slug: "books".to_string()
        }]
    );
}
