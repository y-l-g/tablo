//! Rich text (ADR-0032): the [`Tool`]s a toolbar offers, the HTML each produces, and the cleaning
//! that keeps a value to the elements its tools produce. A rich text field cleans what it reads
//! and [`RichTextColumn`](crate::RichTextColumn) what it renders; a page of the app's own renders
//! a stored value through [`clean`].

use std::collections::{HashMap, HashSet};

/// One button of a rich text field's toolbar. A tool is also a grant: the field keeps only the
/// elements and attributes its tools produce, so a tool left off the toolbar cannot be typed,
/// is dropped from a paste, and is removed from a forged post.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, body: String }
/// use tablo_core::{Field, Tool};
///
/// Field::rich_text(Post::fields().body()).toolbar(&[
///     &[Tool::Bold, Tool::Italic, Tool::Link],
///     &[Tool::BulletList, Tool::OrderedList],
/// ]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Tool {
    /// `<strong>`.
    Bold,
    /// `<em>`.
    Italic,
    /// `<u>`.
    Underline,
    /// `<s>`.
    Strike,
    /// Inline `<code>`.
    Code,
    /// `<a href>` to an `http`, `https` or `mailto` URL, or a relative one.
    Link,
    /// `<h1>`.
    Heading1,
    /// `<h2>`.
    Heading2,
    /// `<h3>`.
    Heading3,
    /// `<ul>` of `<li>`.
    BulletList,
    /// `<ol>` of `<li>`, which may set `start`.
    OrderedList,
    /// `<blockquote>`.
    Blockquote,
    /// `<pre><code>`.
    CodeBlock,
    /// `<hr>`.
    HorizontalRule,
    /// Undoes the last change; produces nothing.
    Undo,
    /// Redoes the last undone change; produces nothing.
    Redo,
}

impl Tool {
    /// The toolbar a rich text field shows unless it declares its own.
    pub const DEFAULT: &[&[Tool]] = &[
        &[
            Tool::Bold,
            Tool::Italic,
            Tool::Underline,
            Tool::Strike,
            Tool::Link,
        ],
        &[Tool::Heading2, Tool::Heading3],
        &[
            Tool::BulletList,
            Tool::OrderedList,
            Tool::Blockquote,
            Tool::CodeBlock,
        ],
        &[Tool::Undo, Tool::Redo],
    ];

    /// Every tool, whose union a column renders with.
    pub(crate) const ALL: &[Tool] = &[
        Tool::Bold,
        Tool::Italic,
        Tool::Underline,
        Tool::Strike,
        Tool::Code,
        Tool::Link,
        Tool::Heading1,
        Tool::Heading2,
        Tool::Heading3,
        Tool::BulletList,
        Tool::OrderedList,
        Tool::Blockquote,
        Tool::CodeBlock,
        Tool::HorizontalRule,
        Tool::Undo,
        Tool::Redo,
    ];

    /// The name `assets/rich_text.js` knows the tool by.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Tool::Bold => "bold",
            Tool::Italic => "italic",
            Tool::Underline => "underline",
            Tool::Strike => "strike",
            Tool::Code => "code",
            Tool::Link => "link",
            Tool::Heading1 => "heading-1",
            Tool::Heading2 => "heading-2",
            Tool::Heading3 => "heading-3",
            Tool::BulletList => "bullet-list",
            Tool::OrderedList => "ordered-list",
            Tool::Blockquote => "blockquote",
            Tool::CodeBlock => "code-block",
            Tool::HorizontalRule => "horizontal-rule",
            Tool::Undo => "undo",
            Tool::Redo => "redo",
        }
    }

    /// The button's accessible name.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Tool::Bold => "Bold",
            Tool::Italic => "Italic",
            Tool::Underline => "Underline",
            Tool::Strike => "Strikethrough",
            Tool::Code => "Code",
            Tool::Link => "Link",
            Tool::Heading1 => "Heading 1",
            Tool::Heading2 => "Heading 2",
            Tool::Heading3 => "Heading 3",
            Tool::BulletList => "Bulleted list",
            Tool::OrderedList => "Numbered list",
            Tool::Blockquote => "Quote",
            Tool::CodeBlock => "Code block",
            Tool::HorizontalRule => "Horizontal rule",
            Tool::Undo => "Undo",
            Tool::Redo => "Redo",
        }
    }

    /// The elements the tool produces, each with the attributes it may carry.
    fn elements(self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            Tool::Bold => &[("strong", &[])],
            Tool::Italic => &[("em", &[])],
            Tool::Underline => &[("u", &[])],
            Tool::Strike => &[("s", &[])],
            Tool::Code => &[("code", &[])],
            Tool::Link => &[("a", &["href"])],
            Tool::Heading1 => &[("h1", &[])],
            Tool::Heading2 => &[("h2", &[])],
            Tool::Heading3 => &[("h3", &[])],
            Tool::BulletList => &[("ul", &[]), ("li", &[])],
            Tool::OrderedList => &[("ol", &["start"]), ("li", &[])],
            Tool::Blockquote => &[("blockquote", &[])],
            Tool::CodeBlock => &[("pre", &[]), ("code", &[])],
            Tool::HorizontalRule => &[("hr", &[])],
            Tool::Undo | Tool::Redo => &[],
        }
    }
}

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
/// `rel="noopener noreferrer nofollow"`.
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
    let cleaned = ammonia::Builder::empty()
        .tags(tags)
        .tag_attributes(attributes)
        .generic_attributes(HashSet::new())
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .link_rel(Some("noopener noreferrer nofollow"))
        .clean(html)
        .to_string();
    if plain_text(&cleaned).is_empty() {
        String::new()
    } else {
        cleaned
    }
}

/// The text a reader sees in `cleaned`, the output of [`clean`], with a line break after each
/// block and the ends trimmed. It reads the serializer's output only: every tag closes its
/// quoted attributes, and text escapes only `&`, `<`, `>` and the no-break space.
pub(crate) fn plain_text(cleaned: &str) -> String {
    let mut text = String::with_capacity(cleaned.len());
    let mut rest = cleaned;
    while let Some(open) = rest.find(['<', '&']) {
        text.push_str(&rest[..open]);
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
        let name: String = rest[1..end]
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        if BLOCKS.contains(&name.as_str()) && !text.ends_with('\n') && !text.is_empty() {
            text.push('\n');
        }
        rest = &rest[(end + 1).min(rest.len())..];
    }
    text.push_str(rest);
    text.trim().to_string()
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

/// How many characters a reader sees in `cleaned`: its text without the line breaks between
/// blocks.
pub(crate) fn text_length(cleaned: &str) -> usize {
    plain_text(cleaned).chars().filter(|c| *c != '\n').count()
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
