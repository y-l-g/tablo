use super::*;
use crate::panel::test_support::{panel_state, serving};

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
        let panel = Arc::new(panel_state("/admin", crate::Auth::disabled()));
        let cx = serving(CxTestBuilder::new().request_context(parts), &panel).build();
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
