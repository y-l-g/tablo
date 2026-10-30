//! [`IconColumn`]: one field shown as its icon.

use std::sync::Arc;

use derive_where::derive_where;
use toasty::stmt::OrderByExpr;
use topcoat::{context::Cx, icon::IconData, view::*};

use super::{Column, ColumnBase, ColumnWidth, ReadFn, cell::icon_view, sort_order};
use crate::{Lens, display::Color, values::FormScalar};

/// A column of one field shown as an icon in its color, its label read to a screen reader and
/// shown when the pointer rests on it.
///
/// The icon and the color are an `Options` enum's `#[option(icon = …, color = "…")]`, unless
/// [`icon`](Self::icon) and [`color`](Self::color) say otherwise; a value with no icon shows its
/// label.
///
/// ```rust
/// # #[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo_core::Options)]
/// # enum Status {
/// #     #[option(color = "warning", icon = tablo_ui::icons::INFO)] Draft,
/// #     #[option(color = "success", icon = tablo_ui::icons::CIRCLE_CHECK)] Published,
/// # }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, status: Status }
/// tablo_core::IconColumn::new(tablo_core::lens!(Post.status)).sortable();
/// ```
#[derive_where(Clone, Debug)]
pub struct IconColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
    sortable: bool,
    #[derive_where(skip(Debug))]
    icon: ReadFn<T, Option<IconData>>,
    #[derive_where(skip(Debug))]
    color: ReadFn<T, Option<Color>>,
}

impl<M, T> IconColumn<M, T>
where
    M: toasty::schema::Model,
    T: FormScalar + Send + Sync + 'static,
{
    /// Bind the column to the field `lens` reads.
    pub fn new(lens: Lens<M, T>) -> Self {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Auto),
            lens,
            sortable: false,
            icon: Arc::new(T::icon),
            color: Arc::new(T::color),
        }
    }

    /// Show `icon(value)` for the value.
    pub fn icon(mut self, icon: impl Fn(&T) -> Option<IconData> + Send + Sync + 'static) -> Self {
        self.icon = Arc::new(icon);
        self
    }

    /// Color the icon by `color(value)`.
    pub fn color(mut self, color: impl Fn(&T) -> Option<Color> + Send + Sync + 'static) -> Self {
        self.color = Arc::new(color);
        self
    }

    base_builders!(sortable);
}

impl<M, T> Column<M> for IconColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_column_methods!();

    fn text(&self, cx: &Cx, row: &M) -> String {
        self.lens.read(row).to_label().localize(cx)
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let value = self.lens.read(row);
        let label = value.to_label().localize(cx);
        match (self.icon)(value) {
            Some(data) => icon_cell(cx, label, icon_view(cx, data, (self.color)(value))),
            None => crate::schema::value_cell(cx, &label),
        }
    }

    fn order_by(&self, descending: bool) -> Option<OrderByExpr> {
        self.sortable
            .then(|| sort_order(self.lens.path(), descending))
    }
}

impl<M, T> crate::column::ColumnInternals<M> for IconColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_internals!(binding);
}

/// `icon` titled `label`, which a screen reader reads in its place.
fn icon_cell<'a>(cx: &'a Cx, label: String, glyph: BoxView<'a>) -> BoxView<'a> {
    view! {
        cx =>
        <span class="inline-flex items-center" title=(label.clone())>
            (glyph)
            <span class="sr-only">(label)</span>
        </span>
    }
    .boxed()
}
