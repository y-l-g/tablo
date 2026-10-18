use super::*;
use crate::panel::test_support::{current_panel, panel_state};

#[test]
fn panel_prefix_prefers_the_panel_over_the_request_path() {
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
    assert_eq!(panel_prefix(&cx), "/admin");

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
    assert_eq!(panel_prefix(&cx), "/backoffice");

    // Without a panel (bare test builder), fall back to the
    // request path's first segment.
    let (parts, ()) = http::Request::builder()
        .uri("/admin/users/42/edit")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    assert_eq!(panel_prefix(&cx), "/admin");
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
