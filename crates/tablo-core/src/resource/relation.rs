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

use super::{ColumnWidth, Resource};

/// The most rows a relation table renders.
///
/// The cap bounds the rendered page, not a query: the related rows are already
/// loaded, so what it removes is cell projection and DOM size. 50 covers the
/// one-to-many sets a detail page summarises while keeping a runaway relation
/// from making the page unusable; the table prints an overflow line when it
/// truncates, so a capped relation never reads as a complete one.
pub const MAX_RELATION_ROWS: usize = 50;

/// The readability floor one [`ColumnWidth::Wide`] relation column contributes
/// to the table's `min-width`, in whole rem.
///
/// The same floor the list table gives a wide column: a wide column declares no
/// width, so a sum of declared widths alone would let it crush to zero on a
/// narrow viewport. Six rem keeps body text readable and, summed across the
/// wide columns, trips the wrapper's horizontal scroll before the fixed layout
/// crushes them.
const WIDE_COLUMN_MIN_REM: u8 = 6;

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

/// Generate the flat tuple impls of [`IntoRelationColumns`] from one list per
/// arity.
///
/// Each list names the binding a tuple element moves through; the `@element`
/// rule supplies the single element type every position shares. Every element
/// is a [`RelationColumn<R>`], so `(a, (b, c))` is not a column list: a table's
/// columns sit in one flat tuple. Arity eight is the shared ceiling
/// [`IntoColumns`](super::IntoColumns) documents.
macro_rules! into_relation_columns_tuples {
    ($($v:ident),+ $(,)?) => {
        impl<R> IntoRelationColumns<R>
            for ($(into_relation_columns_tuples!(@element $v)),+)
        {
            fn into_relation_columns(self) -> RelationColumns<R> {
                let ($($v,)+) = self;
                RelationColumns {
                    columns: vec![$($v,)+],
                }
            }
        }
    };
    (@element $v:ident) => { RelationColumn<R> };
}

into_relation_columns_tuples!(a, b);
into_relation_columns_tuples!(a, b, c);
into_relation_columns_tuples!(a, b, c, d);
into_relation_columns_tuples!(a, b, c, d, e);
into_relation_columns_tuples!(a, b, c, d, e, f);
into_relation_columns_tuples!(a, b, c, d, e, f, g);
into_relation_columns_tuples!(a, b, c, d, e, f, g, h);

/// The width every column of one relation render declares: one `style` value
/// per declared column, in column order, plus the table-level floor.
///
/// A [`ColumnWidth::Narrow`] column claims its kind's share (10% nominally);
/// an explicit `Rem`/`Percent` is emitted as declared; a
/// [`Wide`](ColumnWidth::Wide) column declares nothing and takes a share of
/// what the declared columns leave. At most eight columns declare together,
/// so the kind defaults total at most 80%: they never claim the whole table
/// the way an unbounded set could, and every undeclared column keeps a share.
///
/// The table-level `min-width` is the sum of those declarations: every share
/// as emitted, every `Rem` verbatim, and one [`WIDE_COLUMN_MIN_REM`] per wide
/// column (which declares nothing and would otherwise crush to zero). With
/// `w-full` the table never exceeds its container on its own, so without the
/// floor the wrapper's `overflow-x-auto` never scrolls; with it the table
/// keeps its measure on a narrow viewport and the wrapper scrolls. Emitted
/// only when the sum carries a length — shares alone are a fraction of the
/// container and can never overflow it.
fn relation_widths<R>(
    columns: &[RelationColumn<R>],
) -> (Vec<Option<Cow<'static, str>>>, Option<Cow<'static, str>>) {
    let cells = columns
        .iter()
        .map(|col| {
            let width = col.column_width();
            width.explicit_css().or_else(|| {
                width
                    .default_percent()
                    .map(|percent| Cow::Owned(format!("width: {percent}%")))
            })
        })
        .collect();
    let mut percent_terms: Vec<u8> = Vec::new();
    let mut rem_total: u32 = 0;
    for col in columns {
        match col.column_width() {
            ColumnWidth::Wide => rem_total += u32::from(WIDE_COLUMN_MIN_REM),
            ColumnWidth::Narrow => {
                if let Some(share) = col.column_width().default_percent() {
                    percent_terms.push(share);
                }
            }
            ColumnWidth::Rem(rem) => rem_total += u32::from(rem),
            ColumnWidth::Percent(share) => percent_terms.push(share),
        }
    }
    let table_min_width = (rem_total > 0).then(|| {
        let mut parts: Vec<String> = percent_terms
            .iter()
            .map(|share| format!("{share}%"))
            .collect();
        parts.push(format!("{rem_total}rem"));
        if parts.len() == 1 {
            Cow::Owned(format!("min-width: {}", parts[0]))
        } else {
            Cow::Owned(format!("min-width: calc({})", parts.join(" + ")))
        }
    });
    (cells, table_min_width)
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
    // no record key — the list's `Table::id` contract exists for keyed diffs
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
