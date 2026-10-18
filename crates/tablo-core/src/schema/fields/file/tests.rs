use super::*;

/// A stored value becomes an `href` only when it is a rooted path or an absolute `http(s)` URL,
/// and an image column's `src` only when it is rooted. Every other spelling, such as a
/// `javascript:` or `data:` scheme, the scheme-relative `//host` or a bare basename, stays text:
/// the framework stores what it is handed, so the render is where a scheme is refused.
#[test]
fn only_a_rooted_or_http_value_links() {
    for refused in [
        "javascript:alert(1)",
        "JavaScript:alert(1)",
        "data:text/html,x",
        "//evil.example/x.png",
        // A browser reads `\` as `/` and drops tabs, so each would load from another host.
        "/\\evil.example/x.png",
        "/\t/evil.example/x.png",
        "https://evil.example\\@site/x.png",
        "report.pdf",
        "",
    ] {
        assert!(!is_linkable(refused), "{refused:?} must not link");
        assert!(!is_rooted(refused), "{refused:?} is not on this site");
    }
    for rooted in ["/uploads/a.png", "/media/photo.JPG?token=abc"] {
        assert!(is_linkable(rooted) && is_rooted(rooted), "{rooted}");
    }
    let remote = "https://cdn.example/a.png";
    assert!(is_linkable(remote), "an http(s) URL links");
    assert!(!is_rooted(remote), "an http(s) URL is another host");
}
