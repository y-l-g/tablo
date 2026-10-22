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

/// A name or a carried path is taken by its last extension, without case, and a path's query
/// or fragment is no part of it.
#[test]
fn a_file_is_taken_by_its_last_extension() {
    let file = FileControl {
        extensions: Some(vec!["png".into(), "pdf".into()]),
        max_bytes: None,
    };
    for taken in ["a.PNG", "scan.tar.pdf"] {
        assert!(file.takes_name(taken), "{taken}");
    }
    for refused in ["png", "a.png.html", "a.png.", "a.gif"] {
        assert!(!file.takes_name(refused), "{refused}");
    }
    assert!(file.takes_path("/uploads/a.PDF#page=2"));
    assert!(!file.takes_path("/uploads/page.html?x.png"));
}
