use http::header::LOCATION;
use tablo_core::{
    Ability, Auth, Brand, DeclarationErrorKind, NavigationItem, Page, Panel, ReadOnly, Resource,
    ResourceDef, RouterBuilderPanelExt, Table, TextColumn,
    auth::{AdminUser, AuthSession, hash_password},
    lens,
};
use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    router::{
        Body, RouteFn, RouteFuture, Router, RouterBuilderDiscoverExt, Slot, response::IntoResponse,
    },
    view::{BoxView, View, ViewExt, view},
};
use uuid::Uuid;

use crate::framework::common::{
    body_string, cookie_header, get, get_with_cookies, memory_db, mount, new_csrf, post_form,
    refusal, response_cookies, rows,
};

#[derive(Debug, Clone, toasty::Model)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    body: String,
}

struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = tablo_core::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("books")
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Book.title))).paginate(25))
            .view(tablo_core::Schema::new(tablo_core::Field::text(
                Book::fields().title(),
            )))
    }
}

struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = tablo_core::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("notes")
            .policy(|_cx: &Cx, ability: Ability<'_, Note>| matches!(ability, Ability::ViewAny))
            .table(Table::new(TextColumn::new(lens!(Note.body))))
    }
}

struct UrlsPage;

impl Page for UrlsPage {
    fn slug() -> String {
        "urls".to_string()
    }

    fn navigation() -> NavigationItem {
        NavigationItem::for_page::<Self>()
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        let show = |url: Option<String>| url.unwrap_or_else(|| "none".to_string());
        let line = format!(
            "panel={} books={} notes={} urls={}",
            show(tablo_core::url::panel(cx)),
            show(tablo_core::url::resource::<BookResource>(cx)),
            show(tablo_core::url::resource::<NoteResource>(cx)),
            show(tablo_core::url::page::<UrlsPage>(cx)),
        );
        Ok(view! { cx => <p data-urls="">(line)</p> })
    }
}

async fn books_db() -> Db {
    let mut db = memory_db(toasty::models!(Book, Note, AdminUser, AuthSession)).await;
    toasty::create!(Book {
        title: "Dune".to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed a book");
    db
}

fn two_panels(db: Db, auth: fn() -> Auth) -> Router {
    Router::builder()
        .discover()
        .app_context(db)
        .panel(
            Panel::new("admin")
                .brand(Brand::new("Back office"))
                .auth(auth())
                .resource::<BookResource>()
                .resource::<NoteResource>()
                .page::<UrlsPage>(),
        )
        .expect("the admin panel mounts")
        .panel(
            Panel::new("portal")
                .brand(Brand::new("Reader portal"))
                .auth(auth())
                .resource::<BookResource>()
                .page::<UrlsPage>(),
        )
        .expect("the portal mounts")
        .build()
}

/// A page rerun of `path`: the request the runtime sends when a table's query changes.
fn rerun(path: &str, cookies: Option<&str>) -> http::Request<Body> {
    let mut request = http::Request::builder()
        .method(http::Method::POST)
        .uri(path)
        .header("X-Topcoat-Runtime", "true")
        .header(http::header::CONTENT_TYPE, "application/json");
    if let Some(cookies) = cookies {
        request = request.header(http::header::COOKIE, cookies);
    }
    request
        .body(Body::from(r#"{"signals":{}}"#.to_owned()))
        .unwrap()
}

fn urls_line(html: &str) -> &str {
    let start = html.find("data-urls=\"\">").expect("the URL page renders") + 13;
    &html[start..start + html[start..].find('<').expect("the line ends")]
}

#[tokio::test]
async fn each_panel_serves_its_own_resources_in_its_own_shell() {
    let router = two_panels(books_db().await, Auth::disabled);

    let admin = body_string(get(&router, "/admin/books").await).await;
    assert!(
        admin.contains("Back office"),
        "the admin brand, got {admin}"
    );
    assert!(
        admin.contains("href=\"/admin/notes\""),
        "the admin sidebar lists notes"
    );
    assert!(admin.contains("Dune"), "the admin list renders the row");

    let portal = get(&router, "/portal/books").await;
    assert_eq!(portal.status(), 200);
    let portal = body_string(portal).await;
    assert!(
        portal.contains("Reader portal"),
        "the portal brand, got {portal}"
    );
    assert!(
        !portal.contains("Back office"),
        "no admin brand on the portal"
    );
    assert!(
        !portal.contains("/admin/"),
        "no admin URL on the portal, got {portal}"
    );
    let found = rows(&portal);
    assert_eq!(found.len(), 1, "the portal lists its one row: {portal}");
    assert!(
        found[0].cells.iter().any(|cell| cell == "Dune"),
        "the row carries the title: {portal}"
    );
    assert!(
        found[0]
            .actions
            .view
            .as_deref()
            .is_some_and(|view| view.starts_with("/portal/books/")),
        "the row links stay in the portal: {portal}"
    );

    assert_eq!(get(&router, "/portal/notes").await.status(), 404);
}

#[tokio::test]
async fn url_helpers_answer_for_the_requests_panel() {
    let router = two_panels(books_db().await, Auth::disabled);

    let admin = body_string(get(&router, "/admin/urls").await).await;
    assert_eq!(
        urls_line(&admin),
        "panel=/admin books=/admin/books notes=/admin/notes urls=/admin/urls"
    );
    let portal = body_string(get(&router, "/portal/urls").await).await;
    assert_eq!(
        urls_line(&portal),
        "panel=/portal books=/portal/books notes=none urls=/portal/urls",
        "a resource the portal does not register has no URL there"
    );
}

#[tokio::test]
async fn a_session_belongs_to_the_panel_that_signed_it_in() {
    let mut db = books_db().await;
    toasty::create!(AdminUser {
        email: "ada@example.com".to_string(),
        password_hash: hash_password("opensesame").expect("hash"),
        display_name: "Ada".to_string(),
        active: true,
        created_at: jiff::Timestamp::now(),
    })
    .exec(&mut db)
    .await
    .expect("seed the admin");
    let router = two_panels(db.clone(), Auth::password);

    let csrf = new_csrf();
    let login = post_form(
        &router,
        "/admin/login",
        &[(tablo_core::csrf::COOKIE_NAME, csrf.clone())],
        format!("email=ada%40example.com&password=opensesame&csrf_token={csrf}"),
    )
    .await;
    assert!(login.status().is_redirection(), "the sign-in succeeds");
    let jar = response_cookies(&login);
    let cookies: Vec<(&str, String)> = jar
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();

    let mut db = db;
    let sessions = AuthSession::all()
        .exec(&mut db)
        .await
        .expect("read sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].panel, "/admin",
        "the row names the panel that issued it"
    );

    let admin = get_with_cookies(&router, "/admin/books", &cookies).await;
    assert_eq!(
        admin.status(),
        200,
        "the session opens the panel that issued it"
    );
    let _ = body_string(admin).await;

    let portal = get_with_cookies(&router, "/portal/books", &cookies).await;
    assert!(
        portal.status().is_redirection(),
        "another panel's session opens nothing"
    );
    let location = portal.headers().get(LOCATION).unwrap().to_str().unwrap();
    assert!(
        location.starts_with("/portal/login"),
        "the portal sends to its own login, got {location}"
    );

    let header = cookie_header(jar.iter().map(|(n, v)| (n.as_str(), v.as_str())));
    let own = router
        .handle(rerun("/admin/books", header.as_deref()))
        .await;
    assert_eq!(own.status(), 200, "the session reruns its own panel's list");
    let _ = body_string(own).await;
    let other = router
        .handle(rerun("/portal/books", header.as_deref()))
        .await;
    assert_eq!(other.status(), 401, "and not another panel's");
}

fn bare_layout<'a>(cx: &'a Cx, slot: Slot<'a>) -> BoxView<'a> {
    view! { cx => <div data-bare-layout="">(slot)</div> }.boxed()
}

fn wrapped_layout<'a>(cx: &'a Cx, slot: Slot<'a>) -> BoxView<'a> {
    let page = view! { cx => <div data-app-frame="">(slot)</div> }.boxed();
    Panel::layout_shell(cx, page.into())
}

#[tokio::test]
async fn the_panel_frames_its_pages_in_its_own_layout() {
    let db = books_db().await;
    let shell = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<BookResource>(),
    )
    .expect("panel builds");
    let html = body_string(get(&shell, "/admin/books").await).await;
    assert!(
        html.contains("data-sidebar=\"provider\""),
        "the shipped shell frames the list, got {html}"
    );

    let wrapped = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<BookResource>()
            .layout(wrapped_layout),
    )
    .expect("panel builds");
    let html = body_string(get(&wrapped, "/admin/books").await).await;
    assert!(
        html.contains("data-app-frame") && html.contains("data-sidebar=\"provider\""),
        "a layout that calls the shell keeps it around its own frame"
    );

    let custom = mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<BookResource>()
            .layout(bare_layout),
    )
    .expect("panel builds");
    let html = body_string(get(&custom, "/admin/books").await).await;
    assert!(
        html.contains("data-bare-layout"),
        "the app's layout frames the list"
    );
    assert!(
        !html.contains("data-sidebar=\"provider\""),
        "the app's layout replaces the shell"
    );
}

fn status(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move { "ok".into_response(cx) })
}

#[tokio::test]
async fn the_apps_own_routes_stay_outside_the_panel() {
    let router = Router::builder()
        .discover()
        .app_context(books_db().await)
        .route(RouteFn::new(http::Method::GET, "/status", status))
        .panel(Panel::new("admin").resource::<BookResource>())
        .expect("the panel mounts")
        .build();

    let page = get(&router, "/admin/books").await;
    assert!(page.status().is_redirection(), "the panel is gated");
    let app = get(&router, "/status").await;
    assert_eq!(
        app.status(),
        200,
        "the app's route answers without a session"
    );
    assert!(
        app.headers()
            .get(http::header::CONTENT_SECURITY_POLICY)
            .is_none(),
        "the panel's frame-ancestors stays under its prefix"
    );
}

#[tokio::test]
async fn mounting_refuses_a_directory_another_panel_serves() {
    let db = books_db().await;
    let dir = std::env::temp_dir();
    let errors = refusal(
        Router::builder()
            .discover()
            .app_context(db.clone())
            .panel(
                Panel::new("admin")
                    .auth(Auth::disabled())
                    .serve_dir("/uploads/{*file}", &dir),
            )
            .expect("the first panel mounts")
            .panel(
                Panel::new("portal")
                    .auth(Auth::disabled())
                    .serve_dir("/uploads/{*path}", &dir),
            ),
    );
    assert_eq!(
        errors[0].kind,
        DeclarationErrorKind::ServeDirTaken {
            path: "/uploads/{*path}".to_string(),
            panel: "/admin".to_string(),
        }
    );

    let errors = refusal(
        Router::builder().discover().app_context(db).panel(
            Panel::new("admin")
                .auth(Auth::disabled())
                .serve_dir("/uploads/{*file}", &dir)
                .serve_dir("/uploads/{*file}", &dir),
        ),
    );
    assert_eq!(
        errors[0].kind,
        DeclarationErrorKind::ServeDirTwice {
            path: "/uploads/{*file}".to_string(),
        }
    );
}

#[tokio::test]
async fn mounting_refuses_a_prefix_another_panel_covers() {
    let db = books_db().await;
    let mount_both = |second: &'static str| {
        Router::builder()
            .discover()
            .app_context(db.clone())
            .panel(Panel::new("admin").auth(Auth::disabled()))
            .expect("the first panel mounts")
            .panel(Panel::new(second).auth(Auth::disabled()))
            .map(|_| ())
    };
    for second in ["admin", "admin/reports"] {
        assert_eq!(
            refusal(mount_both(second))[0].kind,
            DeclarationErrorKind::PrefixOverlapsPanel {
                other: "/admin".to_string(),
            }
        );
    }
    assert!(
        mount_both("administration").is_ok(),
        "a sibling prefix is fine"
    );

    for prefix in ["_topcoat/runtime/x", "_topcoat"] {
        let mounted = Router::builder()
            .discover()
            .app_context(db.clone())
            .panel(Panel::new(prefix));
        assert_eq!(
            refusal(mounted)[0].kind,
            DeclarationErrorKind::PrefixOverlapsRuntime {
                prefix: format!("/{prefix}"),
            }
        );
    }
}
