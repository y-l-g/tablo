use toasty::Db;
use topcoat::router::Body;

use super::{super::search::TABLE_SEARCH_PATH, *};
use crate::{
    Panel,
    panel::test_support::{current_panel, mount, panel_state},
};

#[test]
fn list_url_prefers_panel_prefix_over_request_path() {
    use topcoat::context::CxTestBuilder;

    // On a panel's request, the resource URL is derived from the panel — even on a path that is not
    // the list route.
    let (parts, ()) = http::Request::builder()
        .uri("/admin/users/42/edit")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .request_context(current_panel(panel_state(
            "/admin",
            crate::Auth::disabled(),
        )))
        .build();
    assert_eq!(list_url(&cx, "users"), "/admin/users");

    let (parts, ()) = http::Request::builder()
        .uri("/backoffice/users")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .request_context(current_panel(panel_state(
            "/backoffice",
            crate::Auth::disabled(),
        )))
        .build();
    assert_eq!(list_url(&cx, "users"), "/backoffice/users");

    // Without a panel (bare test builder), fall back to the
    // request path's first segment.
    let (parts, ()) = http::Request::builder()
        .uri("/admin/users/42/edit")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    assert_eq!(list_url(&cx, "users"), "/admin/users");
}

/// The named runtime endpoints answer the gate: a request
/// without a session to a shard's fixed path is refused with 401, not a
/// login redirect and not the shard's content. The path is stable, so the
/// refusal is the only thing that keeps it from being probed.
#[tokio::test]
async fn named_shard_endpoints_answer_401_without_a_session() {
    let db = Db::builder()
        .models(toasty::models!(
            crate::auth::AdminUser,
            crate::auth::AuthSession
        ))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router =
        mount(db, Panel::new("admin").auth(crate::Auth::password())).expect("panel builds");

    for path in [TABLE_SEARCH_PATH, crate::notification::LIVE_TOASTER_PATH] {
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(path)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}".to_owned()))
            .unwrap();
        let response = router.handle(request).await;
        assert_eq!(
            response.status(),
            http::StatusCode::UNAUTHORIZED,
            "an unauthenticated shard request must answer 401: {path}"
        );
    }
}

/// `?return=` is followed only to a path under the panel prefix: never off
/// the origin, never to a sibling mount, and never through a dot segment the
/// browser would resolve out of the prefix.
#[test]
fn return_target_accepts_only_paths_under_the_prefix() {
    use topcoat::context::CxTestBuilder;

    let target = |value: &str| {
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("return", value)
            .finish();
        let (parts, ()) = http::Request::builder()
            .uri(format!("/admin/comments/1/delete?{query}"))
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new()
            .request_context(parts)
            .request_context(current_panel(panel_state(
                "/admin",
                crate::Auth::disabled(),
            )))
            .build();
        return_target(&cx)
    };
    for accepted in ["/admin", "/admin/posts/1", "/admin/posts/1?comments.q=a"] {
        assert_eq!(target(accepted).as_deref(), Some(accepted));
    }
    for refused in [
        "//evil.example",
        "https://evil.example",
        "/\\evil.example",
        "/adminx",
        "/elsewhere",
        "/admin/../logout",
        "/admin/%2E%2e/logout",
        "/admin/./posts",
    ] {
        assert_eq!(target(refused), None, "{refused} must not be followed");
    }
}
