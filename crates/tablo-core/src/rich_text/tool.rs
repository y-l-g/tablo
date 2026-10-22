//! [`Tool`]: one button of a rich text toolbar.

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
    /// `<ol>` of `<li>`, which may set `start` and `type`.
    OrderedList,
    /// `<blockquote>`.
    Blockquote,
    /// `<pre><code>`. The editor records a block's language as a class, which cleaning drops,
    /// and the allowlist names elements, not where they nest, so a forged post keeps inline
    /// `<code>` too.
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
    pub(super) fn elements(self) -> &'static [(&'static str, &'static [&'static str])] {
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
            Tool::OrderedList => &[("ol", &["start", "type"]), ("li", &[])],
            Tool::Blockquote => &[("blockquote", &[])],
            Tool::CodeBlock => &[("pre", &[]), ("code", &[])],
            Tool::HorizontalRule => &[("hr", &[])],
            Tool::Undo | Tool::Redo => &[],
        }
    }
}
