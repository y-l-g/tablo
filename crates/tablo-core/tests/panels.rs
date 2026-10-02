use http::header::LOCATION;
use tablo_core::{
    Ability, Auth, Brand, NavigationItem, Page, Panel, Policy, ReadOnly, Resource,
    RouterBuilderPanelExt, Table, TextColumn,
    auth::{AdminUser, AuthSession, hash_password},
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
    body_string, get, get_with_cookies, memory_db, mount, new_csrf, post_form, response_cookies,
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

    fn slug() -> String {
        "books".to_string()
    }

    fn policy() -> impl Policy<Book> {
        ReadOnly
    }

    fn table() -> Table<Book> {
        Table::new(
            |book: &Book| book.id.to_string(),
            TextColumn::r#for(Book::fields().title(), |book: &Book| book.title.clone()),
        )
        .paginate(25)
        .live_search()
    }

    fn view(_dx: &tablo_core::DeclCx) -> tablo_core::Schema {
        tablo_core::Schema::new(tablo_core::Field::text(Book::fields().title()))
    }
}

struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = tablo_core::NoForm<Self::Model>;

    fn slug() -> String {
        "notes".to_string()
    }

    fn policy() -> impl Policy<Note> {
        |_cx: &Cx, ability: Ability<'_, Note>| matches!(ability, Ability::ViewAny)
    }

    fn table() -> Table<Note> {
        Table::new(
            |note: &Note| note.id.to_string(),
            TextColumn::r#for(Note::fields().body(), |note: &Note| note.body.clone()),
        )
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

fn shard(path: &str, cookies: Option<&str>) -> http::Request<Body> {
    let args = format!(
        r#"[{},{{"t":"Signal","id":"{:032x}","v":""}},{{"t":"Signal","id":"{:032x}","v":""}}]"#,
        serde_json::to_string(path).unwrap(),
        1,
        2
    );
    let mut request = http::Request::builder()
        .method(http::Method::POST)
        .uri("/_topcoat/runtime/shards/tablo-table-search")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(topcoat::router::request::IDENTITY_HEADER, "A".repeat(22));
    if let Some(cookies) = cookies {
        request = request.header(http::header::COOKIE, cookies);
    }
    request
        .body(Body::from(format!(r#"{{"args":{args},"signals":{{}}}}"#)))
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
    assert!(
        portal.contains("href=\"/portal/books/"),
        "the row links stay in the portal"
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
async fn a_live_table_shard_re_renders_for_the_panel_its_path_names() {
    let router = two_panels(books_db().await, Auth::disabled);

    let response = router.handle(shard("/portal/books", None)).await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(
        html.contains("Dune"),
        "the shard renders the row, got {html}"
    );
    assert!(
        html.contains("/portal/books/"),
        "the row links stay in the portal"
    );
    assert!(
        !html.contains("/admin/"),
        "nothing of the admin panel, got {html}"
    );

    let response = router.handle(shard("/elsewhere/books", None)).await;
    assert_eq!(response.status(), 404, "a path no panel serves");
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

    let header = tablo_test::cookie_header(jar.iter().map(|(n, v)| (n.as_str(), v.as_str())));
    let own = router
        .handle(shard("/admin/books", header.as_deref()))
        .await;
    assert_eq!(
        own.status(),
        200,
        "the session re-renders its own panel's table"
    );
    let _ = body_string(own).await;
    let other = router
        .handle(shard("/portal/books", header.as_deref()))
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
        let error = mount_both(second).expect_err("an overlapping prefix is refused");
        assert!(
            error
                .to_string()
                .contains("overlaps the panel mounted at '/admin'"),
            "got {error}"
        );
    }
    assert!(
        mount_both("administration").is_ok(),
        "a sibling prefix is fine"
    );

    let error = Router::builder()
        .discover()
        .app_context(db.clone())
        .panel(Panel::new("_topcoat/runtime/x"))
        .map(|_| ())
        .expect_err("the runtime endpoints are Topcoat's");
    assert!(
        error.to_string().contains("runtime endpoints"),
        "got {error}"
    );

    let error = Router::builder()
        .discover()
        .app_context(db)
        .panel(Panel::new("_topcoat"))
        .map(|_| ())
        .expect_err("a panel above the runtime endpoints wraps them");
    assert!(
        error.to_string().contains("runtime endpoints"),
        "got {error}"
    );
}
