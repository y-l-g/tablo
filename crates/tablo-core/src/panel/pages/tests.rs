use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    router::Body,
    view::{View, view},
};

use crate::{
    Page, Panel,
    panel::test_support::{Dummy, dummy_table, response_html},
    resource::{NavigationItem, Resource},
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

    fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
        dummy_table(cx)
    }
}

async fn db() -> Db {
    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
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
    let panel = Panel::new("backoffice")
        .app_context(db().await)
        .auth(crate::Auth::disabled())
        .resource::<DummyResource>()
        .page::<ReportsPage>()
        .home::<Dashboard>();
    let entries: Vec<_> = panel
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
    let router = panel.build().expect("the panel builds");

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

    let Err(error) = Panel::new("admin")
        .resource::<DummyResource>()
        .page::<DummiesPage>()
        .build()
    else {
        panic!("a page over a resource's slug must not build");
    };
    assert!(
        error.to_string().contains("duplicate slug 'dummies'"),
        "got {error}"
    );
    let Err(error) = Panel::new("admin")
        .page::<DummiesPage>()
        .resource::<DummyResource>()
        .build()
    else {
        panic!("a resource over a page's slug must not build");
    };
    assert!(
        error
            .to_string()
            .contains("duplicate resource slug 'dummies'"),
        "got {error}"
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

    let Err(error) = Panel::new("admin").page::<NestedPage>().build() else {
        panic!("a slug with a slash must not build");
    };
    assert!(error.to_string().contains("Page::slug"), "got {error}");
}

#[test]
fn a_second_home_page_does_not_build() {
    let Err(error) = Panel::new("admin")
        .home::<Dashboard>()
        .home::<ReportsPage>()
        .build()
    else {
        panic!("two home pages must not build");
    };
    assert!(
        error
            .to_string()
            .contains("a home page is already registered"),
        "got {error}"
    );
}
