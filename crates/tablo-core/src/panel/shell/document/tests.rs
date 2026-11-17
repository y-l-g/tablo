use super::{face_style, font_style};
use crate::Theme;

#[test]
fn a_font_family_is_one_css_string_whatever_it_holds() {
    assert_eq!(
        font_style("Albert Sans"),
        ":root{--tablo-font:\"Albert Sans\"}"
    );
    assert_eq!(
        font_style("a\"b\\c"),
        ":root{--tablo-font:\"a\\\"b\\\\c\"}",
        "a quote or a backslash cannot end the string"
    );
}

#[test]
fn a_theme_with_faces_declares_each_at_its_asset_url() {
    let style = face_style(Theme::Ledger, |_| "/assets/face.woff2".to_owned());
    assert_eq!(style.matches("@font-face").count(), 2);
    assert!(style.contains("src:url(\"/assets/face.woff2\") format(\"woff2\")"));
    assert_eq!(
        face_style(Theme::Jade, |_| unreachable!("Jade has no faces")),
        ""
    );
}

#[test]
fn a_face_url_cannot_end_its_string_or_the_style_element() {
    let style = face_style(Theme::Ledger, |_| "/a\"b\\c\n</style>".to_owned());
    assert!(!style.contains("</style>"));
    assert!(style.contains("url(\"/a\\\"b\\\\c\\a \\3c /style>\")"));
}
