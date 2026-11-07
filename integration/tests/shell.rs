//! The shell around a panel's pages: its brand, and the theme and sidebar its cookies restore.

use http::header::COOKIE;
use tablo::{
    Brand, Color, DeclarationErrorKind, Environment, Panel, Radius, ReadOnly, Resource,
    ResourceDef, Table, TextColumn, lens,
};
use topcoat::router::{Body, Router};
use uuid::Uuid;

use crate::common::{body_string, mount, panel, refusal, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

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

async fn books(panel: Panel) -> Router {
    let db = test_db(toasty::models!(Book)).await;
    mount(db, panel.resource::<BookResource>()).expect("the panel mounts")
}

/// The list page, its request carrying the raw `cookie` header.
async fn list_page(router: &Router, cookie: &str) -> String {
    let request = http::Request::builder()
        .uri("/admin/books")
        .header(COOKIE, cookie)
        .body(Body::empty())
        .unwrap();
    let response = router.handle(request).await;
    assert_eq!(response.status(), http::StatusCode::OK);
    body_string(response).await
}

/// The page's opening `<html>` tag.
fn html_tag(html: &str) -> &str {
    let start = html.find("<html").expect("a full document");
    &html[start..=start + html[start..].find('>').unwrap()]
}

/// A hostile brand is text in the sidebar header, and its logo stays inside its attribute.
#[tokio::test]
async fn the_brand_cannot_break_out_of_the_shell() {
    let router = books(
        panel().brand(Brand::new("<script>alert(1)</script>").logo("\"><script>alert(2)</script>")),
    )
    .await;
    let html = list_page(&router, "").await;
    assert!(
        !html.contains("\"><script>") && html.contains("src=\"&quot;><script>"),
        "the logo stays inside its attribute: {html}"
    );
    assert!(
        html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "the brand renders as text: {html}"
    );
}

/// Runtime navigation copies `<html>`'s attributes from the next page and runs no script, so the
/// server renders the stored theme, and only the panel's own cookies decide it.
#[tokio::test]
async fn the_theme_cookie_overrides_the_dark_mode_default() {
    let light = books(panel()).await;
    let dark = books(panel().dark_by_default()).await;
    for (router, cookie, tag) in [
        (
            &light,
            "tablo_theme=dark",
            "<html lang=\"en\" dir=\"ltr\" class=\"dark\">",
        ),
        (
            &dark,
            "tablo_sidebar=collapsed; tablo_theme=light",
            "<html lang=\"en\" dir=\"ltr\">",
        ),
        (
            &dark,
            "tablo_theme=sepia",
            "<html lang=\"en\" dir=\"ltr\" class=\"dark\">",
        ),
        // Another app's cookie on the same host decides nothing, even listed first.
        (
            &dark,
            "theme=light; xtablo_theme=light",
            "<html lang=\"en\" dir=\"ltr\" class=\"dark\">",
        ),
        (
            &light,
            "theme=light; tablo_theme=dark",
            "<html lang=\"en\" dir=\"ltr\" class=\"dark\">",
        ),
        // Under `system`, the scheme the theme script mirrors decides, and the default stands in
        // until it has.
        (
            &light,
            "tablo_theme=system; tablo_scheme=dark",
            "<html lang=\"en\" dir=\"ltr\" class=\"dark\">",
        ),
        (
            &dark,
            "tablo_theme=system; tablo_scheme=light",
            "<html lang=\"en\" dir=\"ltr\">",
        ),
        (
            &dark,
            "tablo_theme=system",
            "<html lang=\"en\" dir=\"ltr\" class=\"dark\">",
        ),
    ] {
        assert_eq!(html_tag(&list_page(router, cookie).await), tag, "{cookie}");
    }
}

/// The sidebar cookie seeds the first paint; from hydration on, the browser owns the state.
#[tokio::test]
async fn the_sidebar_cookie_restores_a_collapsed_sidebar() {
    let router = books(panel()).await;
    let open = list_page(&router, "").await;
    assert!(!open.contains("data-state=\"collapsed\""), "{open}");
    let collapsed = list_page(&router, "tablo_theme=dark; tablo_sidebar=collapsed").await;
    assert!(
        collapsed.contains("data-state=\"collapsed\"")
            && collapsed.contains("data-collapsible=\"icon\""),
        "{collapsed}"
    );
}

/// The sign-in page, which renders outside the shell, starts dark too.
#[tokio::test]
async fn the_sign_in_page_follows_the_dark_mode_default() {
    let db = test_db(toasty::models!(
        tablo::auth::AdminUser,
        tablo::auth::AuthSession
    ))
    .await;
    let router = mount(
        db,
        Panel::new("admin")
            .auth(tablo::Auth::password())
            .dark_by_default(),
    )
    .expect("the panel mounts");
    let response = crate::common::get(&router, "/admin/login").await;
    assert_eq!(response.status(), http::StatusCode::OK);
    assert_eq!(
        html_tag(&body_string(response).await),
        "<html lang=\"en\" dir=\"ltr\" class=\"dark\">"
    );
}

/// The brand's colors and radius reach the page as the theme's tokens, the dark ones winning while
/// it renders dark.
#[tokio::test]
async fn the_brand_paints_the_themes_primary_tokens() {
    let router = books(
        panel().brand(
            Brand::new("Acme")
                .color("#4f46e5")
                .dark_color("oklch(0.7 0.15 270)")
                .radius(Radius::Large),
        ),
    )
    .await;
    let html = list_page(&router, "").await;
    assert!(
        html.contains(
            ":root{--primary:#4f46e5;--ring:#4f46e5;--radius:0.75rem;}\
             :root.dark{--primary:oklch(0.7 0.15 270);--ring:oklch(0.7 0.15 270);}"
        ),
        "{html}"
    );
}

/// A color that could close the declaration and open another refuses the mount instead of
/// reaching the stylesheet.
#[tokio::test]
async fn the_mount_refuses_a_brand_color_that_is_no_color() {
    let db = test_db(toasty::models!(Book)).await;
    let hostile = "red;}body{display:none";
    let errors = refusal(mount(
        db,
        panel()
            .brand(Brand::new("Acme").color(hostile))
            .resource::<BookResource>(),
    ));
    assert!(
        errors.iter().any(|error| error.kind
            == DeclarationErrorKind::BrandColor {
                value: hostile.to_owned()
            }),
        "{errors:?}"
    );
}

/// An environment names itself on every page: its badge, and the start of the tab title.
#[tokio::test]
async fn an_environment_names_itself_on_every_page() {
    let router = books(panel().environment(Environment::new("Staging").color(Color::Info))).await;
    let html = list_page(&router, "").await;
    assert!(html.contains("<title>[Staging] "), "{html}");
    assert!(html.contains("data-environment=\"Staging\""), "{html}");
    let html = list_page(&books(panel()).await, "").await;
    assert!(
        !html.contains("data-environment"),
        "no environment, no badge"
    );
}
