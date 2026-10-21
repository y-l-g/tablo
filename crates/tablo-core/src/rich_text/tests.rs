use super::{Tool, clean_with as clean, from_text, plain_text, text_length};

#[test]
fn cleaning_keeps_only_what_the_tools_produce() {
    let tools = [Tool::Bold, Tool::Link];
    assert_eq!(
        clean("<p><strong>a</strong> <em>b</em> <h2>c</h2></p>", tools),
        "<p><strong>a</strong> b </p>c",
        "a tool off the toolbar keeps its text and loses its element"
    );
    assert_eq!(
        clean(
            "<p><a href=\"https://x.test\" target=\"_blank\">x</a></p>",
            tools
        ),
        "<p><a href=\"https://x.test\" rel=\"noopener noreferrer nofollow\">x</a></p>"
    );
}

#[test]
fn cleaning_removes_scripts_handlers_styles_and_unsafe_urls() {
    let cleaned = clean(
        "<p style=\"position:fixed\" onclick=\"x()\">a<script>alert(1)</script>\
         <a href=\"javascript:alert(1)\">b</a><a href=\"data:text/html,x\">c</a></p>",
        Tool::ALL.iter().copied(),
    );
    assert_eq!(
        cleaned,
        "<p>a<a rel=\"noopener noreferrer nofollow\">b</a><a rel=\"noopener noreferrer nofollow\">c</a></p>"
    );
}

#[test]
fn a_document_without_text_is_blank() {
    assert_eq!(clean("<p></p>", Tool::ALL.iter().copied()), "");
    assert_eq!(
        clean(
            "<p> <br></p><ul><li><p></p></li></ul>",
            Tool::ALL.iter().copied()
        ),
        ""
    );
    assert_eq!(clean("<script>x</script>", Tool::ALL.iter().copied()), "");
    assert_eq!(
        clean("<p>\u{200b}&#8203;</p>", Tool::ALL.iter().copied()),
        "",
        "a zero-width character shows nothing"
    );
}

#[test]
fn the_empty_paragraphs_ending_a_document_drop() {
    assert_eq!(
        clean(
            "<ul><li><p>a</p></li></ul><p></p><p></p>",
            Tool::ALL.iter().copied()
        ),
        "<ul><li><p>a</p></li></ul>"
    );
}

#[test]
fn plain_text_breaks_lines_between_blocks_and_reads_entities() {
    let cleaned = clean(
        "<h2>Tea &amp; cake</h2><p>a&lt;b</p><ul><li><p>one</p></li><li><p>two</p></li></ul>",
        Tool::ALL.iter().copied(),
    );
    assert_eq!(plain_text(&cleaned), "Tea & cake\na<b\none\ntwo");
    assert_eq!(text_length(&cleaned), "Tea & cakea<bonetwo".chars().count());
}

#[test]
fn plain_text_reads_whitespace_as_a_browser_shows_it() {
    let cleaned = clean(
        "<p>a\n  b</p>\n<p> c </p><pre><code>x\n  y</code></pre>",
        Tool::ALL.iter().copied(),
    );
    assert_eq!(plain_text(&cleaned), "a b\nc\nx\n  y");
    assert_eq!(text_length(&cleaned), "a bcx\n  y".chars().count());
}

#[test]
fn a_plain_text_value_becomes_a_document_showing_it() {
    let html = from_text("Use Vec<String> & co.\r\nNext line\n\n\n Second ");
    assert_eq!(
        html,
        "<p>Use Vec&lt;String&gt; &amp; co.<br>Next line</p><p>Second</p>"
    );
    assert_eq!(clean(&html, []), html, "a toolbar of no tool keeps it");
    assert_eq!(from_text("  \n\n "), "");
}

#[test]
fn plain_text_skips_a_quoted_angle_bracket_in_an_attribute() {
    let cleaned = clean("<p><a href=\"/a>b\">link</a></p>", [Tool::Link]);
    assert_eq!(plain_text(&cleaned), "link");
}

proptest::proptest! {
    /// Cleaning a cleaned value changes nothing: the editor posts back what the server rendered.
    #[test]
    fn cleaning_is_idempotent(html in "(<[a-z]{1,3}( [a-z]{1,5}=\"[^\"]{0,8}\")?>|</[a-z]{1,3}>|[a-z &<>]{0,6}){0,12}") {
        let once = clean(&html, Tool::ALL.iter().copied());
        proptest::prop_assert_eq!(clean(&once, Tool::ALL.iter().copied()), once);
    }

    /// The text of a cleaned value holds no markup the value's text did not spell.
    #[test]
    fn plain_text_holds_no_tag(html in "(<(p|strong|em|h2|li|pre|code|br|a href=\"/x\")>|</(p|strong|em|h2|li|pre|code|a)>|[a-z \n&;]{0,6}){0,12}") {
        let text = plain_text(&clean(&html, Tool::ALL.iter().copied()));
        proptest::prop_assert!(!text.contains('<') && !text.contains('>'), "{text:?}");
    }
}
