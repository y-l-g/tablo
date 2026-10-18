//! A panel's custom pages: where they mount, who may open them, and their sidebar entries.

use tablo::{
    Page, PageDef, Panel, ReadOnly, Resource, ResourceDef, Table, TextColumn, lens,
    testing::find_href_with,
};
use topcoat::{
    Result,
    context::Cx,
    router::Router,
    view::{View, view},
};
use uuid::Uuid;

use crate::common::{body_string, csp, get, memory_db, mount};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

/// Books anyone may read.
struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("books")
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Book.title))))
    }
}

/// The same books under the default policy, which refuses every ability.
struct ClosedBookResource;

impl Resource for ClosedBookResource {
    type Model = Book;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("closed-books")
            .table(Table::new(TextColumn::new(lens!(Book.title))))
    }
}

struct Dashboard;

impl Page for Dashboard {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <h1>"Welcome home"</h1> })
    }
}

struct ReportsPage;

impl Page for ReportsPage {
    fn declare() -> PageDef {
        PageDef::new().navigation_order(-1)
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <h1>"Quarterly reports"</h1> })
    }
}

/// A page whose access refuses everyone.
struct SecretPage;

impl Page for SecretPage {
    fn declare() -> PageDef {
        PageDef::new().access(|_| false)
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <h1>"Secret"</h1> })
    }
}

/// A home page answering whether the request may open each page.
struct Probe;

impl Page for Probe {
    async fn render(cx: &Cx) -> Result<impl View> {
        let answers = format!(
            "reports={} secret={}",
            tablo::can_open::<ReportsPage>(cx),
            tablo::can_open::<SecretPage>(cx),
        );
        Ok(view! { cx => <p>(answers)</p> })
    }
}

async fn books(panel: Panel) -> Router {
    let db = memory_db(toasty::models!(Book)).await;
    mount(
        db,
        panel
            .auth(tablo::Auth::disabled())
            .resource::<BookResource>(),
    )
    .expect("the panel mounts")
}

async fn page(router: &Router, uri: &str) -> (http::StatusCode, String) {
    let response = get(router, uri).await;
    (response.status(), body_string(response).await)
}

/// A page and the home page mount under the panel's own prefix, and the sidebar lists them by
/// order, the home entry leading the entries of its order even when registered last.
#[tokio::test]
async fn pages_mount_under_the_prefix_with_their_sidebar_entries() {
    let router = books(
        Panel::new("backoffice")
            .page::<ReportsPage>()
            .home::<Dashboard>(),
    )
    .await;

    let (status, html) = page(&router, "/backoffice").await;
    assert_eq!(
        status,
        http::StatusCode::OK,
        "the home page replaces the redirect"
    );
    assert!(html.contains("Welcome home"), "{html}");
    let entry = |label: &str| {
        html.find(&format!("title=\"{label}\""))
            .unwrap_or_else(|| panic!("the sidebar lists {label}: {html}"))
    };
    assert!(
        entry("Reports") < entry("Dashboard") && entry("Dashboard") < entry("Books"),
        "{html}"
    );
    for href in ["/backoffice", "/backoffice/reports", "/backoffice/books"] {
        assert!(find_href_with(&html, href).is_some(), "{href}: {html}");
    }

    let (status, html) = page(&router, "/backoffice/reports").await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(html.contains("Quarterly reports"), "{html}");
    assert!(
        html.contains("data-theme-menu") && !html.contains("data-user-menu"),
        "with no session the footer offers the theme alone: {html}"
    );
}

/// `page_with` adjusts the page for its own panel: another panel mounts the page as it declares
/// itself.
#[tokio::test]
async fn page_with_adjusts_the_page_for_its_panel_only() {
    let portal = books(
        Panel::new("portal").page_with::<ReportsPage>(|def| def.slug("figures").access(|_| false)),
    )
    .await;
    assert_eq!(
        page(&portal, "/portal/figures").await.0,
        http::StatusCode::FORBIDDEN
    );
    assert_eq!(
        page(&portal, "/portal/reports").await.0,
        http::StatusCode::NOT_FOUND
    );

    let admin = books(Panel::new("admin").page::<ReportsPage>()).await;
    assert_eq!(page(&admin, "/admin/reports").await.0, http::StatusCode::OK);
}

/// `can_open` answers what the page's `GET` would: the request's panel mounts the page and its
/// access admits the request.
#[tokio::test]
async fn can_open_answers_for_the_requests_panel() {
    let admin = books(
        Panel::new("admin")
            .home::<Probe>()
            .page::<ReportsPage>()
            .page::<SecretPage>(),
    )
    .await;
    let (_, html) = page(&admin, "/admin").await;
    assert!(html.contains("reports=true secret=false"), "{html}");

    let portal = books(Panel::new("portal").home::<Probe>()).await;
    let (_, html) = page(&portal, "/portal").await;
    assert!(html.contains("reports=false secret=false"), "{html}");
}

/// Without a home page, the root redirects to the first resource, and the redirect carries the
/// clickjacking directive like every response.
#[tokio::test]
async fn without_a_home_page_the_root_redirects_to_the_first_resource() {
    let router = books(Panel::new("admin").page::<ReportsPage>()).await;
    let response = get(&router, "/admin").await;
    assert_eq!(response.status(), http::StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers().get(http::header::LOCATION).unwrap(),
        "/admin/books"
    );
    assert!(
        csp(&response).contains("frame-ancestors"),
        "{}",
        csp(&response)
    );
}

/// A page whose `access` refuses answers 403 and leaves the sidebar, as does a resource the
/// user cannot `ViewAny`; what the user may open stays.
#[tokio::test]
async fn the_sidebar_lists_only_what_the_user_may_open() {
    let router = books(
        Panel::new("admin")
            .home::<Dashboard>()
            .resource::<ClosedBookResource>()
            .page::<ReportsPage>()
            .page::<SecretPage>(),
    )
    .await;
    assert_eq!(
        page(&router, "/admin/secret").await.0,
        http::StatusCode::FORBIDDEN
    );

    let (status, html) = page(&router, "/admin").await;
    assert_eq!(status, http::StatusCode::OK);
    for (href, listed) in [
        ("/admin/reports", true),
        ("/admin/books", true),
        ("/admin/secret", false),
        ("/admin/closed-books", false),
    ] {
        assert_eq!(
            html.contains(&format!("href=\"{href}\"")),
            listed,
            "{href} listed: {listed}, got {html}"
        );
    }
}
