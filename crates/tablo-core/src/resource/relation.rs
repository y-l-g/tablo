//! Read-only relation rendering for detail pages.
//!
//! A detail page shows a record's related rows — a post's comments — from the
//! rows `Resource::query`'s `include` already loaded. This renders them, and
//! deliberately renders *only* them: no pager, no search, no row actions, no
//! bulk column. A relation on a record page is a fixed, already-loaded set,
//! and the list's chrome exists to narrow a query this page never runs.
//!
//! Two bounds apply to that set: the related resource's `can_view`
//! decides which rows the reader may see, and [`MAX_RELATION_ROWS`] caps how
//! many render. Both are in-memory decisions over the loaded rows, so the
//! relation still issues no query.
//!
//! Everything here is owned: a column's projection returns a `String`, so the
//! rendered view borrows the request context and nothing else. That is what
//! lets `Resource::view_relations(cx, record)` return a view that outlives the
//! record it read — the page is rendered before the handler's `record` binding
//! drops, and the borrow checker says so if it is not.

use std::borrow::Cow;

use tablo_ui::{table, table_body, table_cell, table_head, table_header, table_row};
use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, attributes, view},
};

use super::{
    ColumnWidth, Resource,
    column::{MinWidth, column_tuples, column_width_style},
};

/// The most rows a relation table renders.
///
/// The cap bounds the rendered page, not a query: the related rows are already
/// loaded, so what it removes is cell projection and DOM size. 50 covers the
/// one-to-many sets a detail page summarises while keeping a runaway relation
/// from making the page unusable; the table prints an overflow line when it
/// truncates, so a capped relation never reads as a complete one.
pub const MAX_RELATION_ROWS: usize = 50;

/// One column of a relation's read-only table.
///
/// The projection is the same shape a list column uses — a typed closure over
/// the related record — minus everything that only makes sense against a
/// query: no `sortable`, no `searchable`, no key.
pub struct RelationColumn<R> {
    label: String,
    display: Box<dyn Fn(&R) -> String + Send + Sync>,
    /// The width this column claims in the table's fixed layout.
    width: ColumnWidth,
}

impl<R> RelationColumn<R> {
    /// Declare a column by label and projection.
    ///
    /// Named for the shape it is, as
    /// [`TextColumn::computed`](crate::resource::TextColumn::computed) is on the list's side: a
    /// relation column has no lens to bind, because the related rows arrive as values rather
    /// than as a query. What a column *is* is its projection.
    pub fn computed(
        label: impl Into<String>,
        display: impl Fn(&R) -> String + Send + Sync + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            display: Box::new(display),
            width: ColumnWidth::Wide,
        }
    }

    /// Declare this column's width in the table's fixed layout:
    /// `.width(ColumnWidth::Percent(20))` for a column that knows its own
    /// measure.
    ///
    /// The default is [`Wide`](ColumnWidth::Wide): a relation column holds a
    /// related row's own text — a comment body, a name — so it takes a share
    /// of what the declared columns leave. Override it when the content
    /// disagrees: a status, date, or count is
    /// [`Narrow`](ColumnWidth::Narrow).
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.width = width;
        self
    }

    /// The width this column declares, which the renderer emits on its `th`
    /// and on every `td` of its column.
    pub fn column_width(&self) -> ColumnWidth {
        self.width
    }

    /// The column's heading.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// This column's cell for `row`.
    pub fn render_cell(&self, row: &R) -> String {
        (self.display)(row)
    }
}

impl<R> std::fmt::Debug for RelationColumn<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelationColumn")
            .field("label", &self.label)
            .field("width", &self.width)
            .finish_non_exhaustive()
    }
}

/// The columns of a relation's table, declared as a tuple or a single column.
///
/// A separate collection from the list's `IntoColumns` because the two tables
/// answer different questions: one is queryable, this one is not.
pub struct RelationColumns<R> {
    pub(crate) columns: Vec<RelationColumn<R>>,
}

impl<R> RelationColumns<R> {
    /// Collect one or more columns.
    pub fn columns(columns: impl IntoRelationColumns<R>) -> Self {
        columns.into_relation_columns()
    }
}

/// What can be collected into [`RelationColumns`].
pub trait IntoRelationColumns<R> {
    fn into_relation_columns(self) -> RelationColumns<R>;
}

impl<R> IntoRelationColumns<R> for RelationColumn<R> {
    fn into_relation_columns(self) -> RelationColumns<R> {
        RelationColumns {
            columns: vec![self],
        }
    }
}

column_tuples!(
    IntoRelationColumns,
    into_relation_columns,
    RelationColumn,
    RelationColumns<T>,
    |columns| RelationColumns { columns }
);

/// The width every column of one relation render declares: one `style` value
/// per declared column, in column order, plus the table-level `min-width`.
/// The arithmetic is the list table's, without its chrome columns: kind
/// defaults share one budget, explicit widths are verbatim, and a wide column
/// takes what the declared ones leave.
fn relation_widths<R>(
    columns: &[RelationColumn<R>],
) -> (Vec<Option<Cow<'static, str>>>, Option<Cow<'static, str>>) {
    let total: u32 = columns
        .iter()
        .filter_map(|col| col.column_width().default_percent())
        .map(u32::from)
        .sum();
    let cells = columns
        .iter()
        .map(|col| column_width_style(col.column_width(), total))
        .collect();
    let mut min_width = MinWidth::default();
    for col in columns {
        min_width.column(col.column_width(), total);
    }
    (cells, min_width.style())
}

/// Render `rows` as a titled, read-only table.
///
/// `R` is the related resource. Its [`can_view`](Resource::can_view) is applied
/// to every row before that row is projected, so a relation declared through the
/// framework cannot render a row the reader may not see; naming the resource at
/// the call (`render_relation::<CommentResource>(cx, …)`) is what makes the
/// policy the resource's, rather than a filter the caller writes and a later
/// edit drops. Only the per-row predicate runs: `can_view_any` gates the related
/// resource's own list page, which this table does not render.
///
/// `rows` is the related records the record already carries — the caller passes
/// `record.comments.get().iter().cloned().collect()` or the equivalent — so this
/// runs no query, and `can_view` is an in-memory predicate over those rows for
/// the same reason. At most [`MAX_RELATION_ROWS`] rows render, with a line
/// naming the truncation below the table. An empty set renders the title with an
/// honest "none" line rather than an empty table, which would read as a failure
/// to load.
///
/// The rows are rendered in the order given; a detail page shows what the query
/// loaded, and `Resource::query` owns that order.
pub fn render_relation<'a, R: Resource>(
    cx: &'a Cx,
    title: &str,
    columns: RelationColumns<R::Model>,
    rows: &[R::Model],
) -> BoxView<'a> {
    let title = title.to_string();
    // Own everything before the `view!` block: the emitted view must borrow
    // the request context and nothing else, or the caller's `record` (which
    // owns these rows) would have to outlive the page.
    let heads: Vec<String> = columns
        .columns
        .iter()
        .map(|c| c.label().to_string())
        .collect();
    // Policy first, then the cap: a row the reader may not see is not a row
    // this table shows, and it must not consume a slot a visible row needs.
    let visible: Vec<&R::Model> = rows.iter().filter(|row| R::can_view(cx, row)).collect();
    let total = visible.len();
    let cells: Vec<Vec<String>> = visible
        .iter()
        .take(MAX_RELATION_ROWS)
        .map(|row| {
            columns
                .columns
                .iter()
                .map(|column| column.render_cell(row))
                .collect()
        })
        .collect();
    let has_rows = !cells.is_empty();
    let overflow = (total > MAX_RELATION_ROWS).then(|| {
        format!("Showing the first {MAX_RELATION_ROWS} of {total} related rows you can view.")
    });
    // The declared widths are a property of the columns, not of the row, so
    // they are resolved once here: the same CSS for every row.
    let (cell_widths, table_min_width) = relation_widths(&columns.columns);
    // Row ids are positional: this table does not reorder or swap, so it needs
    // no record key — the list's table-key contract exists for keyed diffs
    // and action URLs, and neither exists here.
    view! {
        cx =>
        <section class="flex flex-col gap-3">
            <h2 class="text-base font-semibold">(title)</h2>
            if !has_rows {
                <p class="text-sm text-muted-foreground">"None."</p>
            } else {
                table(
                    attrs: attributes! { class="table-fixed" style=(table_min_width.as_deref()) },
                    table_header(
                        table_row(
                            for (head, width) in heads.iter().zip(&cell_widths) {
                                table_head(
                                    attrs: attributes! { style=(width.as_deref()) },
                                    (head.clone())
                                )
                            }
                        )
                    )
                    table_body(
                        for row in &cells {
                            table_row(
                                for (cell, width) in row.iter().zip(&cell_widths) {
                                    table_cell(
                                        attrs: attributes! { class="truncate" style=(width.as_deref()) },
                                        (cell.clone())
                                    )
                                }
                            )
                        }
                    )
                )
                if let Some(overflow) = overflow {
                    <p class="text-sm text-muted-foreground">(overflow)</p>
                }
            }
        </section>
    }
    .boxed()
}
