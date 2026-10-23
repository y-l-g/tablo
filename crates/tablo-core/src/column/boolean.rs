//! [`BooleanColumn`]: a `bool` field rendered as an icon.

use derive_where::derive_where;
use toasty::stmt::OrderByExpr;
use topcoat::{context::Cx, icon::icon, view::*};

use super::{Column, ColumnBase, ColumnWidth, sort_order};
use crate::Lens;

/// A column of a `bool` field rendered as an icon.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, featured: bool }
/// tablo_core::BooleanColumn::new(tablo_core::lens!(Post.featured)).sortable();
/// ```
#[derive_where(Clone, Debug)]
pub struct BooleanColumn<M> {
    lens: Lens<M, bool>,
    base: ColumnBase,
    sortable: bool,
    labels: (crate::Text, crate::Text),
}

impl<M> BooleanColumn<M>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the `bool` field `lens` reads.
    pub fn new(lens: Lens<M, bool>) -> Self {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Auto),
            lens,
            sortable: false,
            labels: (
                crate::Text::message("tablo-yes"),
                crate::Text::message("tablo-no"),
            ),
        }
    }

    base_builders!(sortable);

    /// The words for `true` and `false`.
    pub fn labels(mut self, yes: impl Into<crate::Text>, no: impl Into<crate::Text>) -> Self {
        self.labels = (yes.into(), no.into());
        self
    }
}

impl<M> Column<M> for BooleanColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    base_column_methods!(binding);

    fn text(&self, cx: &Cx, row: &M) -> String {
        if *self.lens.read(row) {
            self.labels.0.localize(cx)
        } else {
            self.labels.1.localize(cx)
        }
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let value = *self.lens.read(row);
        let text = self.text(cx, row);
        // A no is the common case in most columns, so it stays quiet and the yeses stand out.
        let (data, class) = if value {
            (tablo_ui::icons::CIRCLE_CHECK, "size-4 text-success")
        } else {
            (tablo_ui::icons::MINUS, "size-4 text-muted-foreground/60")
        };
        view! {
            cx =>
            <span class="inline-flex items-center" data-boolean=(value.to_string())>
                icon(
                    data: data,
                    attrs: attributes! { class=(class) aria-hidden="true" }
                )
                <span class="sr-only">(text)</span>
            </span>
        }
        .boxed()
    }

    fn is_sortable(&self) -> bool {
        self.sortable
    }

    fn order_by(&self, descending: bool) -> Option<OrderByExpr> {
        self.sortable
            .then(|| sort_order(self.lens.path(), descending))
    }
}
