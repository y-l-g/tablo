use topcoat::{
    Result,
    context::Cx,
    router::Body,
    view::{View, view},
};

use crate::Page;

struct ReportsPage;
impl Page for ReportsPage {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <h1>"Quarterly reports"</h1> })
    }
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
        .app_context(crate::panel::test_support::host())
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
