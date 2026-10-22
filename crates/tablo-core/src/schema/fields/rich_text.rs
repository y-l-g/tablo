//! The rich text control (ADR-0032): a `<textarea>` holding the HTML, which
//! `assets/rich_text.js` replaces with an editor whose toolbar the field declares.

use std::marker::PhantomData;

use tablo_ui::{icons, textarea as ui_textarea};
use toasty::stmt::Path;
use topcoat::{
    Result,
    context::Cx,
    icon::{IconData, icon},
    view::*,
};

use super::{ControlKind, Field, render_field};
use crate::{
    FieldRuleFault,
    rich_text::{PROSE, Tool, clean_with, text_length},
};

/// A rich text field: [`Field::rich_text`], or a record form's text control made one with
/// [`TextField::rich_text`](crate::TextField::rich_text) (`F` is the form).
///
/// - The toolbar is the schema. `.toolbar(..)` takes groups of `Tool`s, `Tool::DEFAULT` unless
///   declared. Each tool names the elements it produces: `Tool::Bold` a `<strong>`, `Tool::Link` an
///   `<a href>`, `Tool::BulletList` a `<ul>` of `<li>`. The editor registers only those, so a tool
///   left off the toolbar cannot be typed and drops from a paste. The editor takes a link only to a
///   URL the server keeps, and a code block's language, which the editor records as a class, is not
///   stored.
/// - The server cleans what it reads. A submission keeps only the elements and attributes its
///   field's tools produce, before the record form parses it: a `<script>`, a `style`, an `on…`
///   handler and a link to a scheme other than `http`, `https` or `mailto` never reach the column,
///   and every link gets `rel="noopener noreferrer nofollow"`. A document with no text reads as an
///   empty submission, so the field's blank answer applies, and the empty paragraphs the editor
///   leaves at a document's end are dropped.
/// - Length rules count the text. `.min_length(n)` and `.max_length(n)` count the characters a
///   reader sees, not the markup.
/// - Without JavaScript the control is a `<textarea>` of the HTML, cleaned the same way on submit.
///   The editor is TipTap, which the page loads only when it holds a rich text control.
/// - A text column moving to rich text needs its values moved too. The field reads what the column
///   stores as HTML, so `a<b` loses its `<b`, and each edit stores what it read. Rewrite each
///   stored value through `tablo::rich_text::from_text`, which escapes it and wraps its paragraphs,
///   before the field edits it.
pub struct RichTextField<F = ()>(pub(super) Field, pub(super) PhantomData<fn() -> F>);

/// The rich text control's toolbar, placeholder and length rules.
pub(crate) struct RichTextControl {
    pub(super) toolbar: Vec<Vec<Tool>>,
    pub(super) placeholder: Option<String>,
    pub(super) min_length: Option<usize>,
    pub(super) max_length: Option<usize>,
    /// The input type of the text control a field of a type other than text gave up, a mistake.
    pub(super) non_text: Option<&'static str>,
}

impl Default for RichTextControl {
    fn default() -> Self {
        Self {
            toolbar: Tool::DEFAULT.iter().map(|group| group.to_vec()).collect(),
            placeholder: None,
            min_length: None,
            max_length: None,
            non_text: None,
        }
    }
}

impl RichTextControl {
    fn tools(&self) -> impl Iterator<Item = Tool> + '_ {
        self.toolbar.iter().flatten().copied()
    }

    /// `posted` keeping only what the toolbar's tools produce, `""` when it holds no text.
    pub(crate) fn clean(&self, posted: &str) -> String {
        clean_with(posted, self.tools())
    }

    /// The sentence refusing `value`, a cleaned, non-empty submission, for the length rule it
    /// breaks. A length counts the characters a reader sees, not the markup.
    pub(crate) fn refusal(&self, label: &str, value: &str) -> Option<String> {
        let length = text_length(value);
        if let Some(min) = self.min_length.filter(|min| length < *min) {
            return Some(format!("{label} must be at least {min} characters"));
        }
        let max = self.max_length.filter(|max| length > *max)?;
        Some(format!("{label} must be at most {max} characters"))
    }

    /// What is wrong with the declared rules.
    pub(crate) fn rule_faults(&self) -> Vec<FieldRuleFault> {
        let mut faults: Vec<_> = self
            .non_text
            .map(FieldRuleFault::NotText)
            .into_iter()
            .collect();
        if let (Some(min), Some(max)) = (self.min_length, self.max_length)
            && min > max
        {
            faults.push(FieldRuleFault::EmptyRange);
        }
        faults
    }
}

impl<F> RichTextField<F> {
    fn rich(&mut self) -> &mut RichTextControl {
        match &mut self.0.control {
            ControlKind::RichText(rich) => rich,
            _ => unreachable!("a RichTextField holds a rich text control"),
        }
    }

    /// The toolbar's groups of [`Tool`]s, in order, which also decide what the field keeps: a
    /// submission loses every element no tool here produces. [`Tool::DEFAULT`] by default.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, summary: String }
    /// use tablo_core::{Field, Tool};
    ///
    /// Field::rich_text(Post::fields().summary()).toolbar(&[&[Tool::Bold, Tool::Italic, Tool::Link]]);
    /// ```
    pub fn toolbar(mut self, groups: &[&[Tool]]) -> Self {
        self.rich().toolbar = groups.iter().map(|group| group.to_vec()).collect();
        self
    }

    /// The text the empty editor shows.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.rich().placeholder = Some(placeholder.into());
        self
    }

    /// Refuse a value whose text holds fewer than `chars` characters. The markup does not count,
    /// and a value with no text is the field's blank answer, which no rule refuses.
    pub fn min_length(mut self, chars: usize) -> Self {
        self.rich().min_length = Some(chars);
        self
    }

    /// Refuse a value whose text holds more than `chars` characters. The markup does not count.
    pub fn max_length(mut self, chars: usize) -> Self {
        self.rich().max_length = Some(chars);
        self
    }
}

impl Field {
    /// A rich text field over a `String` or `Option<String>` column, storing HTML its toolbar's
    /// [`Tool`]s produce: the server removes anything else from a submission before it parses it.
    /// A record form's text control becomes one with
    /// [`TextField::rich_text`](super::TextField::rich_text).
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, body: String }
    /// use tablo_core::Field;
    ///
    /// Field::rich_text(Post::fields().body()).placeholder("The full story…");
    /// ```
    pub fn rich_text<M, T>(lens: impl Into<Path<M, T>>) -> RichTextField
    where
        M: toasty::schema::Model,
        T: toasty::schema::Field<Inner = String>,
    {
        RichTextField::new(Self::bound(
            super::Binding::of::<M, T>(&lens.into()),
            ControlKind::RichText(RichTextControl::default()),
        ))
    }

    /// A rich text field posting `name`, a key no column binds: a
    /// [`FormInput`](crate::FormInput) field.
    pub fn rich_text_input(name: impl Into<String>) -> RichTextField {
        RichTextField::new(Self::bound(
            Self::named(name.into()),
            ControlKind::RichText(RichTextControl::default()),
        ))
    }

    /// Renders a rich text field's control: the toolbar and the editor the script mounts, over
    /// the `<textarea>` that holds the HTML and posts it.
    pub(super) fn render_rich_text<'a>(
        &self,
        rich: &RichTextControl,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&crate::values::FieldError>,
        id: String,
        disabled: bool,
    ) -> Result<BoxView<'a>> {
        let required = self.is_required() && !disabled;
        let chrome = self.chrome(id.clone(), error, None);
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome.described_by();
        let name = self.name().to_string();
        let placeholder = rich.placeholder.clone();
        let value = value.unwrap_or("").to_string();
        let tools = rich.tools().map(Tool::key).collect::<Vec<_>>().join(" ");
        let groups: Vec<Vec<Tool>> = rich.toolbar.clone();
        let content_class = format!("{PROSE} {EDITOR}");
        let control = view! {
            cx =>
            <div
                class="overflow-hidden rounded-lg border border-border focus-within:ring-2 focus-within:ring-ring focus-within:ring-offset-2 focus-within:ring-offset-background has-[[aria-invalid=true]]:border-destructive"
                data-rich-text=""
                data-rich-text-tools=(tools)
            >
                <div
                    class="flex flex-wrap gap-1 border-b border-border bg-muted/40 p-1"
                    role="toolbar"
                    aria-label="Formatting"
                    aria-controls=(id.clone())
                    data-rich-text-toolbar=""
                    hidden=(true)
                >
                    for group in groups {
                        <div class="flex gap-0.5" role="group">
                            for tool in group {
                                <button
                                    type="button"
                                    class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 aria-pressed:bg-accent aria-pressed:text-foreground [&>svg]:size-4"
                                    tabindex="-1"
                                    title=(tool.label())
                                    aria-label=(tool.label())
                                    aria-pressed=(toggles(tool).then_some("false"))
                                    disabled=(disabled)
                                    data-rich-text-tool=(tool.key())
                                >
                                    icon(data: tool_icon(tool))
                                </button>
                            }
                        </div>
                    }
                </div>
                <div class=(content_class) data-rich-text-content="" hidden=(true)></div>
                ui_textarea(
                    attrs: attributes! {
                        id=(id)
                        class="rounded-none border-0 focus-visible:ring-0 focus-visible:ring-offset-0"
                        name=(name)
                        placeholder=(placeholder)
                        rows=(8)
                        required=(required)
                        disabled=(disabled)
                        aria-required=(required.then_some("true"))
                        aria-invalid=(aria_invalid)
                        aria-describedby=(described_by)
                        data-rich-text-source=""
                    },
                    (value)
                )
            </div>
        }
        .boxed();
        render_field(
            cx,
            &chrome,
            self.label_str(),
            required,
            attributes! {},
            control,
        )
    }
}

/// The editing surface's own classes, beside [`PROSE`]: its padding and height, and the
/// placeholder TipTap marks on an empty document.
const EDITOR: &str = "px-3 py-2 [&_.ProseMirror]:min-h-32 [&_.ProseMirror]:outline-none \
    [&_.is-editor-empty:first-child]:before:pointer-events-none \
    [&_.is-editor-empty:first-child]:before:float-left [&_.is-editor-empty:first-child]:before:h-0 \
    [&_.is-editor-empty:first-child]:before:text-muted-foreground \
    [&_.is-editor-empty:first-child]:before:content-[attr(data-placeholder)]";

/// Whether the tool's button shows a state, pressed while the selection is in what it produces.
fn toggles(tool: Tool) -> bool {
    !matches!(tool, Tool::Undo | Tool::Redo | Tool::HorizontalRule)
}

fn tool_icon(tool: Tool) -> IconData {
    match tool {
        Tool::Bold => icons::BOLD,
        Tool::Italic => icons::ITALIC,
        Tool::Underline => icons::UNDERLINE,
        Tool::Strike => icons::STRIKETHROUGH,
        Tool::Code => icons::CODE,
        Tool::Link => icons::LINK,
        Tool::Heading1 => icons::HEADING_1,
        Tool::Heading2 => icons::HEADING_2,
        Tool::Heading3 => icons::HEADING_3,
        Tool::BulletList => icons::LIST,
        Tool::OrderedList => icons::LIST_ORDERED,
        Tool::Blockquote => icons::TEXT_QUOTE,
        Tool::CodeBlock => icons::SQUARE_CODE,
        Tool::HorizontalRule => icons::MINUS,
        Tool::Undo => icons::UNDO_2,
        Tool::Redo => icons::REDO_2,
    }
}

#[cfg(test)]
mod tests;
