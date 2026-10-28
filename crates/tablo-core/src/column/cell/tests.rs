use proptest::prelude::*;

use super::{LINK_SCHEMES, link_href, truncate};

#[test]
fn a_link_keeps_a_relative_or_web_url_and_refuses_a_script() {
    for href in [
        "/admin/users/1",
        "users/1",
        "?page=2",
        "https://example.com/a?b=c",
        "mailto:ada@example.com",
        "tel:+33123456789",
    ] {
        assert_eq!(link_href(href), Some(href), "{href}");
    }
    for href in [
        "javascript:alert(1)",
        "JavaScript:alert(1)",
        " javascript:alert(1)",
        "java\tscript:alert(1)",
        "\u{1}javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "vbscript:msgbox(1)",
    ] {
        assert_eq!(link_href(href), None, "{href:?}");
    }
}

proptest! {
    /// Whatever a row's closure returns, the URL a browser resolves a kept link to carries a web,
    /// mail or phone scheme.
    #[test]
    fn a_kept_link_resolves_to_an_allowed_scheme(href in any::<String>()) {
        if let Some(href) = link_href(&href) {
            let base = url::Url::parse("https://panel.example/admin/users").expect("a base URL");
            if let Ok(resolved) = base.join(href) {
                prop_assert!(LINK_SCHEMES.contains(&resolved.scheme()), "{href:?} → {resolved}");
            }
        }
    }

    #[test]
    fn a_kept_link_survives_a_script_scheme_prefix(rest in "[a-z(1)]{0,8}") {
        for scheme in ["javascript:", "JAVASCRIPT:", "\tjavascript:", "javas\ncript:"] {
            let href = format!("{scheme}{rest}");
            prop_assert_eq!(link_href(&href), None, "{:?}", href);
        }
    }
}

#[test]
fn truncation_cuts_on_a_character_and_leaves_a_short_value() {
    assert_eq!(truncate("Ada Lovelace", 3).as_deref(), Some("Ada…"));
    assert_eq!(truncate("Ada Lovelace", 4).as_deref(), Some("Ada…"));
    assert_eq!(truncate("éèêë", 2).as_deref(), Some("éè…"));
    assert_eq!(truncate("Ada", 3), None);
    assert_eq!(truncate("", 0), None);
}
