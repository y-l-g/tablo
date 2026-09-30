#[cfg(feature = "auth")]
use toasty::Db;
#[cfg(feature = "auth")]
use topcoat::router::Body;

#[cfg(feature = "auth")]
use super::super::search::TABLE_SEARCH_PATH;
use super::*;
#[cfg(feature = "auth")]
use crate::Panel;

#[test]
fn list_url_prefers_panel_prefix_over_request_path() {
    use topcoat::context::CxTestBuilder;

    // With the panel prefix installed, the resource URL is derived from
    // the declaration — even on a path that is not the list route.
    let (parts, ()) = http::Request::builder()
        .uri("/admin/users/42/edit")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .app_context(PanelPrefix("/admin".to_string()))
        .build();
    assert_eq!(list_url(&cx, "users"), "/admin/users");

    let (parts, ()) = http::Request::builder()
        .uri("/backoffice/users")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .app_context(PanelPrefix("/backoffice".to_string()))
        .build();
    assert_eq!(list_url(&cx, "users"), "/backoffice/users");

    // Without a panel prefix (bare test builder), fall back to the
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
#[cfg(feature = "auth")]
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
    let router = Panel::new("admin")
        .app_context(db)
        .auth(crate::Auth::password())
        .build()
        .expect("panel builds");

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
