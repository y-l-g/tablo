//! [`ComputedColumn`]: a display-only column projecting any text off the row.

use std::sync::Arc;

use derive_where::derive_where;
use topcoat::{context::Cx, view::BoxView};

use super::{
    Column, ColumnToggle, ColumnWidth, Includes,
    cell::{TextCell, text_cell},
};
use crate::display::{Alignment, Color};

/// A display-only column rendering any text from the row, with no query predicate.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, body: String }
/// tablo_core::ComputedColumn::new("Words", |p: &Post| {
///     p.body.split_whitespace().count().to_string()
/// });
/// ```
///
/// A closure that reads a relation declares it with [`include_unscoped`](Self::include_unscoped),
/// so the page loads it. One relation's record or count is a
/// [`RelationColumn`](super::RelationColumn) or a [`CountColumn`](super::CountColumn), which
/// declare their own and show only what the related resource shows the user.
///
/// It maps to no column, so it neither searches nor sorts.
///
/// A [`badge`](Self::badge) shows the text in an outline badge, with a dot in the
/// [`color`](Self::color) the row gives it:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Order { #[key] #[auto] id: uuid::Uuid, total_cents: i64 }
/// use tablo_core::{Color, ComputedColumn, Reads, lens};
///
/// ComputedColumn::new("Size", |o: &Order| {
///     if o.total_cents > 100_00 {
///         "Large"
///     } else {
///         "Small"
///     }
///     .to_string()
/// })
/// .badge()
/// .color(|o: &Order| (o.total_cents > 100_00).then_some(Color::Success))
/// .reading(Reads::of(lens!(Order.total_cents)));
/// ```
#[derive_where(Clone, Debug)]
pub struct ComputedColumn<M> {
    name: String,
    label: crate::Text,
    #[derive_where(skip(Debug))]
    project: Arc<dyn Fn(&M) -> String + Send + Sync>,
    width: ColumnWidth,
    alignment: Alignment,
    toggle: ColumnToggle,
    /// Relations the projection reads.
    includes: Includes<M>,
    /// The fields the projection reads, when declared.
    reads: Option<crate::lens::Reads<M>>,
    badge: bool,
    #[derive_where(skip(Debug))]
    color: Option<super::ReadFn<M, Option<Color>>>,
}

impl<M> ComputedColumn<M>
where
    M: toasty::schema::Model,
{
    /// Declare a column headed `label` rendering `project(row)`, named after the label: its
    /// plain text in lowercase, or its message's id.
    pub fn new(
        label: impl Into<crate::Text>,
        project: impl Fn(&M) -> String + Send + Sync + 'static,
    ) -> Self {
        let label = label.into();
        Self {
            name: label.key(),
            label,
            project: Arc::new(project),
            width: ColumnWidth::Auto,
            alignment: Alignment::Start,
            toggle: ColumnToggle::Fixed,
            includes: Includes::new(),
            reads: None,
            badge: false,
            color: None,
        }
    }

    /// Show the text as a badge, with a dot in its [`color`](Self::color).
    pub fn badge(mut self) -> Self {
        self.badge = true;
        self
    }

    /// Color the text by `color(row)`: its badge's dot, or the text itself when it is no badge.
    /// [`reading`](Self::reading) declares what it reads, beside the projection.
    pub fn color(mut self, color: impl Fn(&M) -> Option<Color> + Send + Sync + 'static) -> Self {
        self.color = Some(Arc::new(color));
        self
    }

    /// Declare a relation the projection reads, so the table loads it. The projection reads
    /// whatever the include loads, under no resource's query or policy, as an
    /// [`unscoped`](super::RelationColumn::unscoped) relation column does: another tenant's record,
    /// or one the user may not view, reaches it.
    pub fn include_unscoped<T>(mut self, relation: impl Into<toasty::stmt::Include<M, T>>) -> Self {
        self.includes = self.includes.with(relation);
        self
    }

    /// Declare the fields the projection and its [`color`](Self::color) read, which a resource
    /// restricting a field with [`field_access`](crate::ResourceDef::field_access) needs: the
    /// column shows only to a user who may view every one.
    pub fn reading(mut self, reads: crate::lens::Reads<M>) -> Self {
        self.reads = Some(reads);
        self
    }

    /// Declare this column's width.
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.width = width;
        self
    }

    /// Align the header and the cells.
    pub fn align(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    toggle_builders!(toggle);
}

impl<M> Column<M> for ComputedColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn label(&self) -> crate::Text {
        self.label.clone()
    }

    fn text(&self, _cx: &Cx, row: &M) -> String {
        (self.project)(row)
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let text = (self.project)(row);
        let color = self
            .color
            .as_ref()
            .filter(|_| !text.trim().is_empty())
            .and_then(|color| color(row));
        text_cell(
            cx,
            TextCell {
                text,
                color,
                badge: self.badge,
                ..TextCell::default()
            },
        )
    }

    fn column_width(&self) -> ColumnWidth {
        self.width
    }

    fn toggle(&self) -> ColumnToggle {
        self.toggle
    }

    fn alignment(&self) -> Alignment {
        self.alignment
    }

    fn includes(&self) -> Includes<M> {
        self.includes.clone()
    }

    fn reads(&self) -> Option<crate::lens::Reads<M>> {
        self.reads.clone()
    }
}
