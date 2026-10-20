//! [`ComputedColumn`]: a display-only column projecting any text off the row.

use std::sync::Arc;

use derive_where::derive_where;
use topcoat::context::Cx;

use super::{Column, ColumnToggle, ColumnWidth, Includes};
use crate::display::Alignment;

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
#[derive_where(Clone, Debug)]
pub struct ComputedColumn<M> {
    name: String,
    label: String,
    #[derive_where(skip(Debug))]
    project: Arc<dyn Fn(&M) -> String + Send + Sync>,
    width: ColumnWidth,
    alignment: Alignment,
    toggle: ColumnToggle,
    /// Relations the projection reads.
    includes: Includes<M>,
}

impl<M> ComputedColumn<M>
where
    M: toasty::schema::Model,
{
    /// Declare a column headed `label` rendering `project(row)`.
    pub fn new(
        label: impl Into<String>,
        project: impl Fn(&M) -> String + Send + Sync + 'static,
    ) -> Self {
        let label = label.into();
        Self {
            name: label.to_lowercase(),
            label,
            project: Arc::new(project),
            width: ColumnWidth::Auto,
            alignment: Alignment::Start,
            toggle: ColumnToggle::Fixed,
            includes: Includes::new(),
        }
    }

    /// Declare a relation the projection reads, so the table loads it. The projection reads
    /// whatever the include loads, under no resource's query or policy, as an
    /// [`unscoped`](super::RelationColumn::unscoped) relation column does: another tenant's record,
    /// or one the user may not view, reaches it.
    pub fn include_unscoped<T>(mut self, relation: impl Into<toasty::stmt::Include<M, T>>) -> Self {
        self.includes = self.includes.with(relation);
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

    fn label(&self) -> &str {
        &self.label
    }

    fn text(&self, _cx: &Cx, row: &M) -> String {
        (self.project)(row)
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
}
