//! Rich text (ADR-0032): the [`Tool`]s a toolbar offers, the HTML each produces, and
//! the cleaning that keeps a value to the elements its tools produce. A rich text field cleans what
//! it reads and [`RichTextColumn`](crate::RichTextColumn) what it renders; a page of the app's own
//! renders a stored value through [`clean`].

pub(crate) mod tool;

use std::collections::{HashMap, HashSet};

pub(crate) use self::tool::Tool;

/// The elements every document holds, whatever its tools: paragraphs and line breaks.
const ALWAYS: &[&str] = &["p", "br"];

/// The elements that end a line of a document's text.
const BLOCKS: &[&str] = &["p", "br", "h1", "h2", "h3", "li", "blockquote", "pre", "hr"];

/// `html` keeping only what some [`Tool`] produces, or `""` when it holds no text: HTML a page
/// may render unescaped, as a [`RichTextColumn`](crate::RichTextColumn) does.
///
/// ```rust
/// use tablo_core::rich_text::clean;
///
/// assert_eq!(
///     clean(r#"<p onclick="steal()">Hi <script>alert(1)</script><b>there</b></p>"#),
///     "<p>Hi there</p>",
/// );
/// ```
///
/// A rich text field stores its value cleaned already, with its own toolbar's tools; cleaning
/// again keeps a value the column's other writers stored to the same rules.
pub fn clean(html: &str) -> String {
    clean_with(html, Tool::ALL.iter().copied())
}

/// The text a reader sees in `html`, with a line break after each block: a summary, a search
/// index or a word count.
///
/// ```rust
/// use tablo_core::rich_text::text;
///
/// assert_eq!(
///     text("<h2>Tea &amp; cake</h2><p>Two <em>slices</em></p>"),
///     "Tea & cake\nTwo slices"
/// );
/// ```
pub fn text(html: &str) -> String {
    plain_text(&clean(html))
}

/// `html` keeping only what `tools` produce, or `""` when it holds no text, so a blank document
/// reads as an empty submission. No allowlist keeps a `style` or an event handler, a link takes
/// only an `http`, `https`, `mailto` or relative URL, and every link gets
/// `rel="noopener noreferrer nofollow"`. The empty paragraphs ending a document, which the
/// editor keeps after a list, a heading or a code block, are dropped.
pub(crate) fn clean_with(html: &str, tools: impl IntoIterator<Item = Tool>) -> String {
    let mut tags: HashSet<&str> = ALWAYS.iter().copied().collect();
    let mut attributes: HashMap<&str, HashSet<&str>> = HashMap::new();
    for tool in tools {
        for (tag, attrs) in tool.elements() {
            tags.insert(tag);
            if !attrs.is_empty() {
                attributes
                    .entry(tag)
                    .or_default()
                    .extend(attrs.iter().copied());
            }
        }
    }
    let mut cleaned = ammonia::Builder::empty()
        .tags(tags)
        .tag_attributes(attributes)
        .generic_attributes(HashSet::new())
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .link_rel(Some("noopener noreferrer nofollow"))
        .clean(html)
        .to_string();
    while let Some(kept) = cleaned.strip_suffix("<p></p>") {
        cleaned.truncate(kept.len());
    }
    if plain_text(&cleaned).chars().all(invisible) {
        String::new()
    } else {
        cleaned
    }
}

/// Whether `c` shows nothing: whitespace, or a zero-width character.
fn invisible(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}')
}

/// A rich text document showing `text`: a paragraph for each run of lines between blank lines, a
/// line break for each single newline, and `&`, `<` and `>` escaped. It moves a plain text column
/// to rich text, whose fields read their stored value as HTML.
///
/// ```rust
/// use tablo_core::rich_text::from_text;
///
/// assert_eq!(
///     from_text("Use Vec<String>.\nOr not.\n\nThe end"),
///     "<p>Use Vec&lt;String&gt;.<br>Or not.</p><p>The end</p>"
/// );
/// ```
pub fn from_text(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    text.split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| {
            let escaped = paragraph
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('\n', "<br>");
            format!("<p>{escaped}</p>")
        })
        .collect()
}

/// The text a reader sees in `cleaned`, the output of [`clean`], with a line break after each
/// block and the ends trimmed.
pub(crate) fn plain_text(cleaned: &str) -> String {
    read_text(cleaned).replace(BREAK, "\n")
}

/// How many characters a reader sees in `cleaned`: its text without the line breaks between
/// blocks.
pub(crate) fn text_length(cleaned: &str) -> usize {
    read_text(cleaned).chars().filter(|c| *c != BREAK).count()
}

/// Where [`read_text`] ends a block: a character no cleaned document holds, since the parser
/// drops every NUL.
const BREAK: char = '\0';

/// The text a reader sees in `cleaned`, a [`BREAK`] after each block and the ends trimmed.
/// Outside a `<pre>`, a run of whitespace reads as one space, as a browser shows it. It reads
/// the serializer's output only: every tag closes its quoted attributes, and text escapes only
/// `&`, `<`, `>` and the no-break space.
fn read_text(cleaned: &str) -> String {
    let mut text = String::with_capacity(cleaned.len());
    let mut pre = 0_usize;
    let mut rest = cleaned;
    while let Some(open) = rest.find(['<', '&']) {
        push_text(&mut text, &rest[..open], pre > 0);
        rest = &rest[open..];
        if rest.starts_with('&') {
            let end = rest.find(';').map_or(rest.len(), |end| end + 1);
            text.push_str(match &rest[..end] {
                "&amp;" => "&",
                "&lt;" => "<",
                "&gt;" => ">",
                "&quot;" => "\"",
                "&nbsp;" => "\u{a0}",
                other => other,
            });
            rest = &rest[end..];
            continue;
        }
        let end = tag_end(rest);
        let closing = rest[1..].starts_with('/');
        let name: String = rest[1..end]
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        if name == "pre" {
            pre = if closing {
                pre.saturating_sub(1)
            } else {
                pre + 1
            };
        }
        if BLOCKS.contains(&name.as_str()) && !text.is_empty() && !text.ends_with(['\n', BREAK]) {
            text.truncate(text.trim_end_matches(' ').len());
            text.push(BREAK);
        }
        rest = &rest[(end + 1).min(rest.len())..];
    }
    push_text(&mut text, rest, pre > 0);
    text.trim_matches(|c: char| c.is_whitespace() || c == BREAK)
        .to_string()
}

/// Appends the text node `raw` to `text`: as written inside a `<pre>`, else with each run of
/// whitespace read as one space and none at the start of a line.
fn push_text(text: &mut String, raw: &str, pre: bool) {
    if pre {
        text.push_str(raw);
        return;
    }
    for c in raw.chars() {
        if !c.is_ascii_whitespace() {
            text.push(c);
        } else if !(text.is_empty() || text.ends_with([' ', '\n', BREAK])) {
            text.push(' ');
        }
    }
}

/// The index of the `>` closing the tag `tag` starts with, past any quoted attribute value.
fn tag_end(tag: &str) -> usize {
    let mut quoted = false;
    for (at, c) in tag.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '>' if !quoted => return at,
            _ => {}
        }
    }
    tag.len()
}

/// The Tailwind classes a rich text value renders in, on a detail page and in its editor.
pub(crate) const PROSE: &str = "max-w-none text-sm leading-relaxed whitespace-normal \
    wrap-anywhere [&_p]:my-2 [&_h1]:mt-4 [&_h1]:mb-2 [&_h1]:text-2xl [&_h1]:font-semibold \
    [&_h2]:mt-4 [&_h2]:mb-2 [&_h2]:text-xl [&_h2]:font-semibold [&_h3]:mt-3 [&_h3]:mb-1.5 \
    [&_h3]:text-lg [&_h3]:font-semibold [&_ul]:my-2 [&_ul]:list-disc [&_ul]:pl-6 [&_ol]:my-2 \
    [&_ol]:list-decimal [&_ol]:pl-6 [&_li_p]:my-0.5 [&_blockquote]:my-2 [&_blockquote]:border-l-2 \
    [&_blockquote]:border-border [&_blockquote]:pl-4 [&_blockquote]:text-muted-foreground \
    [&_a]:text-primary [&_a]:underline [&_a]:underline-offset-2 [&_code]:rounded \
    [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[0.85em] \
    [&_pre]:my-2 [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:bg-muted [&_pre]:p-3 \
    [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_hr]:my-4 [&_hr]:border-border \
    [&>:first-child]:mt-0 [&>:last-child]:mb-0";

#[cfg(test)]
mod tests;
