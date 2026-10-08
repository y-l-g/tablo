use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    router::Body,
    view::{View, view},
};

use crate::{
    DeclarationError, DeclarationErrorKind, Page, Panel, SegmentFault, Site,
    navigation::NavigationItem,
    panel::test_support::{Dummy, dummy_table, mount, mount_without_db, refusal, response_html},
    resource::{Resource, ResourceDef},
    test_support::memory_db,
};

struct Dashboard;
impl Page for Dashboard {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <h1>"Welcome home"</h1> })
    }
}

struct ReportsPage;
impl Page for ReportsPage {
    fn navigation() -> NavigationItem {
        NavigationItem {
            order: -1,
            ..NavigationItem::for_page::<Self>()
        }
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <h1>"Quarterly reports"</h1> })
    }
}

struct DummyResource;
impl Resource for DummyResource {
    type Model = Dummy;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().table(dummy_table())
    }
}

async fn db() -> Db {
    let db = memory_db(toasty::models!(Dummy)).await;
    db
}

async fn get(router: &topcoat::router::Router, uri: &str) -> (http::StatusCode, String) {
    let request = http::Request::builder()
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    let response = router.handle(request).await;
    (response.status(), response_html(response).await)
}

/// A page and the home page mount under the panel's own prefix, and their
/// sidebar entries resolve from the same place. The home entry leads even
/// when registered last.
#[tokio::test]
async fn pages_mount_under_the_prefix_with_their_sidebar_entries() {
    let panel = || {
        Panel::new("backoffice")
            .auth(crate::Auth::disabled())
            .resource::<DummyResource>()
            .page::<ReportsPage>()
            .home::<Dashboard>()
    };
    let registered = panel().registered();
    let entries: Vec<_> = registered
        .nav_items
        .iter()
        .map(|item| (item.label.as_str(), item.url(), item.order))
        .collect();
    assert_eq!(
        entries,
        [
            ("Dashboard", Some("/backoffice"), 0),
            ("Dummies", Some("/backoffice/dummies"), 0),
            ("Reports", Some("/backoffice/reports"), -1),
        ]
    );
    let router = mount(db().await, panel()).expect("the panel builds");

    let (status, html) = get(&router, "/backoffice").await;
    assert_eq!(
        status,
        http::StatusCode::OK,
        "the home page replaces the redirect"
    );
    assert!(
        html.contains("Welcome home"),
        "home renders at the prefix: {html}"
    );

    let (status, html) = get(&router, "/backoffice/reports").await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(
        html.contains("Quarterly reports"),
        "the page renders: {html}"
    );
}

/// Pages and resources share one slug namespace, in either order.
#[test]
fn a_page_slug_that_a_resource_holds_does_not_build() {
    struct DummiesPage;
    impl Page for DummiesPage {
        async fn render(cx: &Cx) -> Result<impl View> {
            Ok(view! { cx => "dummies" })
        }
    }

    let duplicate = || DeclarationErrorKind::DuplicateSlug {
        slug: "dummies".to_string(),
    };
    let errors = refusal(mount_without_db(
        Panel::new("admin")
            .resource::<DummyResource>()
            .page::<DummiesPage>(),
    ));
    assert_eq!(
        errors[0],
        DeclarationError::of::<DummiesPage>(Site::Registration, duplicate())
    );
    let errors = refusal(mount_without_db(
        Panel::new("admin")
            .page::<DummiesPage>()
            .resource::<DummyResource>(),
    ));
    assert_eq!(
        errors[0],
        DeclarationError::of::<DummyResource>(Site::Registration, duplicate())
    );
}

#[test]
fn a_page_slug_that_is_not_one_segment_does_not_build() {
    struct NestedPage;
    impl Page for NestedPage {
        fn slug() -> String {
            "reports/q3".to_string()
        }

        async fn render(cx: &Cx) -> Result<impl View> {
            Ok(view! { cx => "nested" })
        }
    }

    let errors = refusal(mount_without_db(Panel::new("admin").page::<NestedPage>()));
    assert_eq!(
        errors[0].kind,
        DeclarationErrorKind::InvalidSegment {
            item: "Page::slug",
            segment: "reports/q3".to_string(),
            fault: SegmentFault::Char('/'),
        }
    );
}

#[test]
fn a_second_home_page_does_not_build() {
    let errors = refusal(mount_without_db(
        Panel::new("admin")
            .home::<Dashboard>()
            .home::<ReportsPage>(),
    ));
    assert_eq!(
        errors[0],
        DeclarationError::of::<ReportsPage>(Site::Registration, DeclarationErrorKind::SecondHome)
    );
}

/// The panel routes `{prefix}/login` and `{prefix}/logout` itself: a page
/// named after either is a registration error, not a duplicate-route panic
/// inside the router build.
#[test]
fn a_page_slug_the_panel_routes_itself_does_not_build() {
    struct LoginPage;
    impl Page for LoginPage {
        async fn render(cx: &Cx) -> Result<impl View> {
            Ok(view! { cx => "login" })
        }
    }

    let errors = refusal(mount_without_db(Panel::new("admin").page::<LoginPage>()));
    assert_eq!(
        errors[0],
        DeclarationError::of::<LoginPage>(
            Site::Registration,
            DeclarationErrorKind::ReservedSlug {
                slug: "login".to_string(),
            },
        )
    );
}

/// Without a home page, the root still redirects to the first resource, and
/// the redirect carries the clickjacking directive like every response.
#[tokio::test]
async fn without_a_home_page_the_root_redirects_to_the_first_resource() {
    let router = mount(
        db().await,
        Panel::new("admin")
            .auth(crate::Auth::disabled())
            .resource::<DummyResource>()
            .page::<ReportsPage>(),
    )
    .expect("the panel builds");
    let request = http::Request::builder()
        .uri("/admin")
        .body(Body::empty())
        .unwrap();
    let response = router.handle(request).await;
    assert_eq!(response.status(), http::StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers().get(http::header::LOCATION).unwrap(),
        "/admin/dummies"
    );
    assert!(
        response
            .headers()
            .get(http::header::CONTENT_SECURITY_POLICY)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|policy| policy.contains("frame-ancestors")),
        "the root redirect must carry the directive"
    );
}

/// A page re-checks the resolved user before rendering: with auth enforced
/// and no user, it answers the login redirect and never renders.
#[tokio::test]
async fn a_page_redirects_to_login_without_a_resolved_user() {
    use topcoat::{context::CxTestBuilder, router::response::IntoResponse, view::ViewExt};

    let (parts, ()) = http::Request::builder()
        .uri("/admin/reports")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .request_context(crate::panel::test_support::current_panel(
            crate::panel::test_support::panel_state("/admin", crate::Auth::password()),
        ))
        .build();
    let Err(error) = super::page_handler::<ReportsPage>(&cx, Body::empty())
        .single()
        .await
    else {
        panic!("a page must not render without a resolved user");
    };
    let response = error.into_response(&cx).expect("the gate redirect renders");
    let location = response
        .headers()
        .get(http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        location.starts_with("/admin/login"),
        "an anonymous page request redirects to login, got {location}"
    );
}
