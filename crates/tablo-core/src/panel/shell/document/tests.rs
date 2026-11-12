use super::font_style;

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
