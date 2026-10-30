//! [`RichTextColumn`]: a rich text field's HTML, cleaned again as it renders (ADR-0032).

use derive_where::derive_where;
use topcoat::{context::Cx, view::*};

use super::{Column, ColumnBase, ColumnWidth};
use crate::{
    Lens,
    rich_text::{PROSE, clean, plain_text},
    values::FormScalar,
};

/// A column of a `String` field holding a [rich text](crate::Field::rich_text) field's HTML. A
/// detail page shows the formatted HTML; a table cell shows its text, cut to
/// [`excerpt`](Self::excerpt) characters; an export holds the text.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, body: String }
/// tablo_core::RichTextColumn::new(tablo_core::lens!(Post.body));
/// ```
///
/// The HTML is cleaned again as it renders, keeping what any [`Tool`](crate::Tool) produces, since
/// a seed, a migration or another app can write the column without the field's cleaning.
#[derive_where(Clone, Debug)]
pub struct RichTextColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
    excerpt: usize,
}

impl<M, T> RichTextColumn<M, T>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the `String` or `Option<String>` field `lens` reads.
    pub fn new(lens: Lens<M, T>) -> Self
    where
        T: toasty::schema::Field<Inner = String>,
    {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Grow),
            lens,
            excerpt: 120,
        }
    }

    /// The most characters of text a table cell shows, followed by `…` when the text runs on;
    /// 120 by default.
    pub fn excerpt(mut self, chars: usize) -> Self {
        self.excerpt = chars;
        self
    }

    base_builders!();
}

impl<M, T> RichTextColumn<M, T>
where
    T: FormScalar,
{
    fn html(&self, row: &M) -> String {
        clean(&self.lens.read(row).to_form())
    }
}

impl<M, T> Column<M> for RichTextColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_column_methods!();

    fn text(&self, _cx: &Cx, row: &M) -> String {
        plain_text(&self.html(row))
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        excerpt_cell(cx, &self.text(cx, row), self.excerpt)
    }

    fn entry<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        entry(cx, &self.label(), self.html(row))
    }
}

impl<M, T> crate::column::ColumnInternals<M> for RichTextColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_internals!(binding);
}

/// A table cell of `text` on one line, cut to `chars` characters with `…` when it runs on.
fn excerpt_cell<'a>(cx: &'a Cx, text: &str, chars: usize) -> BoxView<'a> {
    let text = text.replace('\n', " ");
    let cut = match text.char_indices().nth(chars) {
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
        None => text,
    };
    crate::schema::value_cell(cx, &cut)
}

/// A detail page's entry: `label` over `html`, the output of [`clean`].
fn entry<'a>(cx: &'a Cx, label: &crate::Text, html: String) -> BoxView<'a> {
    let value = if html.is_empty() {
        crate::schema::value_cell(cx, "")
    } else {
        // `clean` kept only the elements and attributes a tool produces.
        let html = Unescaped::new_unchecked(html);
        view! { cx => <div class=(PROSE) data-rich-text-value="">(html)</div> }.boxed()
    };
    crate::schema::read_only(cx, label, value)
}
