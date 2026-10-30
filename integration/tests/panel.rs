use http::header::LOCATION;
use tablo::{
    Ability, Auth, Brand, DeclarationErrorKind, Page, PageDef, Panel, ReadOnly, Resource,
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

use crate::common::{
    body_string, cookie_header, get, get_with_cookies, mount, new_csrf, post_form, refusal,
    response_cookies, rows, test_db,
};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    body: String,
}

struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("books")
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Book.title))).page_size(25))
            .detail(tablo::Detail::new(TextColumn::new(lens!(Book.title))))
    }
}

struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("notes")
            .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny))
            .table(Table::new(TextColumn::new(lens!(Note.body))))
    }
}

struct UrlsPage;

impl Page for UrlsPage {
    fn declare() -> PageDef {
        PageDef::new().slug("urls")
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        let show = |url: Option<String>| url.unwrap_or_else(|| "none".to_string());
        let line = format!(
            "panel={} books={} notes={} urls={}",
            show(tablo::url::panel(cx)),
            show(tablo::url::resource::<BookResource>(cx)),
            show(tablo::url::resource::<NoteResource>(cx)),
            show(tablo::url::page::<UrlsPage>(cx)),
        );
        Ok(view! { cx => <p data-urls="">(line)</p> })
    }
}

async fn books_db() -> Db {
    let mut db = test_db(toasty::models!(Book, Note, AdminUser, AuthSession)).await;
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
        .with(db)
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
        password_hash: hash_password("opensesame").await.expect("hash"),
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
        &[(tablo::csrf::COOKIE_NAME, csrf.clone())],
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
        .with(books_db().await)
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
            .with(db.clone())
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
        Router::builder().discover().with(db).panel(
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
            .with(db.clone())
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
            .with(db.clone())
            .panel(Panel::new(prefix));
        assert_eq!(
            refusal(mounted)[0].kind,
            DeclarationErrorKind::PrefixOverlapsRuntime {
                prefix: format!("/{prefix}"),
            }
        );
    }
}

/// Books under the slug `S`, readable by all.
struct SluggedBooks<const STAR: bool>;

impl<const STAR: bool> Resource for SluggedBooks<STAR> {
    type Model = Book;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug(if STAR {
                "user*profiles"
            } else {
                "user-profiles_2"
            })
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Book.title))))
    }
}

/// A slug of URL-segment characters, a `*` among them, builds a list route that resolves.
#[tokio::test]
async fn a_slug_of_segment_characters_resolves() {
    let router = mount(
        test_db(toasty::models!(Book)).await,
        crate::common::panel()
            .resource::<SluggedBooks<false>>()
            .resource::<SluggedBooks<true>>(),
    )
    .expect("the slugs build");
    for uri in ["/admin/user-profiles_2", "/admin/user*profiles"] {
        let response = get(&router, uri).await;
        assert_eq!(response.status(), http::StatusCode::OK, "{uri}");
        assert!(body_string(response).await.contains("Books</h1>"), "{uri}");
    }
}

/// A panel page carries the panel's policy and hardening headers; a deployment replaces the
/// policy, or opts out of it and keeps the rest, and HSTS is sent only when asked.
#[tokio::test]
async fn a_panel_page_sends_the_security_headers() {
    async fn headers(panel: Panel) -> http::HeaderMap {
        let router = mount(
            test_db(toasty::models!(Book)).await,
            panel.auth(Auth::disabled()).resource::<BookResource>(),
        )
        .expect("panel builds");
        let response = get(&router, "/admin/books").await;
        assert_eq!(response.status(), http::StatusCode::OK);
        response.headers().clone()
    }
    let header = |headers: &http::HeaderMap, name| {
        headers
            .get(name)
            .map(|value: &http::HeaderValue| value.to_str().unwrap().to_owned())
    };

    let default = headers(Panel::new("admin")).await;
    let policy = header(&default, http::header::CONTENT_SECURITY_POLICY).unwrap();
    for directive in [
        "default-src 'self'",
        "script-src 'self' 'unsafe-inline' 'unsafe-eval';",
        "object-src 'none'",
        "base-uri 'self'",
        "form-action 'self'",
        "frame-ancestors 'self'",
    ] {
        assert!(policy.contains(directive), "{directive} in {policy}");
    }
    assert_eq!(
        header(&default, http::header::X_CONTENT_TYPE_OPTIONS).as_deref(),
        Some("nosniff")
    );
    assert_eq!(
        header(&default, http::header::REFERRER_POLICY).as_deref(),
        Some("same-origin")
    );
    assert_eq!(
        header(&default, http::header::CACHE_CONTROL).as_deref(),
        Some("no-store")
    );
    assert_eq!(
        header(&default, http::header::STRICT_TRANSPORT_SECURITY),
        None
    );

    let custom =
        headers(Panel::new("admin").content_security_policy("frame-ancestors https://a.example"))
            .await;
    assert_eq!(
        header(&custom, http::header::CONTENT_SECURITY_POLICY).as_deref(),
        Some("frame-ancestors https://a.example")
    );

    let off = headers(Panel::new("admin").without_content_security_policy()).await;
    assert_eq!(header(&off, http::header::CONTENT_SECURITY_POLICY), None);
    assert_eq!(
        header(&off, http::header::CACHE_CONTROL).as_deref(),
        Some("no-store"),
        "opting out of the policy keeps the other headers"
    );

    let hsts =
        headers(Panel::new("admin").strict_transport_security(std::time::Duration::from_secs(600)))
            .await;
    assert_eq!(
        header(&hsts, http::header::STRICT_TRANSPORT_SECURITY).as_deref(),
        Some("max-age=600")
    );
}

/// A policy that is no valid header value refuses to mount.
#[tokio::test]
async fn a_policy_with_a_line_break_refuses_to_mount() {
    let error = mount(
        test_db(toasty::models!(Book)).await,
        Panel::new("admin")
            .auth(Auth::disabled())
            .content_security_policy("default-src 'self'\r\nSet-Cookie: x=1")
            .resource::<BookResource>(),
    )
    .err()
    .expect("the mount fails");
    assert!(
        error.to_string().contains("content_security_policy"),
        "{error}"
    );
}

static DECLARE_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[derive(tablo::RecordForm)]
#[form(model = Book)]
struct CountedForm {
    title: String,
}

/// Counts its declarations.
struct CountedResource;

impl Resource for CountedResource {
    type Model = Book;
    type Form = CountedForm;

    fn declare() -> ResourceDef<Self> {
        DECLARE_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ResourceDef::new()
            .slug("books")
            .policy(tablo::Allow)
            .table(Table::new(TextColumn::new(lens!(Book.title))))
            .detail(tablo::Detail::new(TextColumn::new(lens!(Book.title))))
    }
}

/// A resource declares once, when its panel mounts, and every page serves from that build.
#[tokio::test]
async fn a_resource_declares_once_when_its_panel_mounts() {
    use std::sync::atomic::Ordering;

    let mut db = test_db(toasty::models!(Book)).await;
    let book = toasty::create!(Book {
        title: "Dune".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = crate::common::panel_router::<CountedResource>(db);
    assert_eq!(DECLARE_CALLS.load(Ordering::SeqCst), 1);
    for uri in [
        "/admin/books".to_string(),
        "/admin/books/-/export".to_string(),
        format!("/admin/books/{}", book.id),
        "/admin/books/-/create".to_string(),
        format!("/admin/books/{}/edit", book.id),
    ] {
        assert_eq!(
            get(&router, &uri).await.status(),
            http::StatusCode::OK,
            "{uri}"
        );
    }
    assert_eq!(DECLARE_CALLS.load(Ordering::SeqCst), 1);
}

/// Counts the books `BookResource`'s panel lets the request see: through the `PanelHandle` the
/// router holds when the route is `/entered/books`, through the request alone otherwise.
fn book_count(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let cx = match topcoat::context::try_context::<tablo::PanelHandle>(cx) {
            Some(admin) if topcoat::router::request::uri(cx).path() == "/entered/books" => {
                admin.enter(cx)
            }
            _ => cx.clone(),
        };
        let count = tablo::scoped_query::<BookResource>(&cx)?
            .count()
            .exec(&mut tablo::db::db(&cx)?)
            .await?;
        let url = tablo::url::resource::<BookResource>(&cx).unwrap_or_default();
        format!("{count} {url}").into_response(&cx)
    })
}

#[tokio::test]
async fn a_route_outside_every_panel_answers_as_the_panel_it_enters() {
    let builder = Router::builder()
        .discover()
        .with(books_db().await)
        .route(RouteFn::new(http::Method::GET, "/books", book_count))
        .route(RouteFn::new(
            http::Method::GET,
            "/entered/books",
            book_count,
        ))
        .panel(
            Panel::new("admin")
                .auth(Auth::disabled())
                .resource::<BookResource>(),
        )
        .expect("the panel mounts");
    let admin = builder.panel_handle("admin").expect("mounted above");
    let router = builder.with(admin).build();

    let ambient = get(&router, "/books").await;
    assert_eq!(
        ambient.status(),
        500,
        "a request outside the prefix belongs to no panel, even on a router with one"
    );
    let entered = get(&router, "/entered/books").await;
    assert_eq!(entered.status(), 200);
    assert_eq!(
        body_string(entered).await,
        "1 /admin/books",
        "the entered panel scopes the query and spells its URLs"
    );
}
