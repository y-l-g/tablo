//! [`ListColumn`]: the values a list column stores, a badge each.

use derive_where::derive_where;
use toasty::stmt::List;
use topcoat::{context::Cx, view::*};

use super::{
    Column, ColumnBase, ColumnWidth,
    cell::{TextCell, badges_cell},
};
use crate::{Lens, values::FormScalar};

/// A column of a `Vec` of [`FormScalar`] values that one list column stores, such as a
/// [multiple choice](crate::ChoiceField::multiple)'s.
///
/// A table cell and a detail page show a badge per value, in the stored order: its
/// [`FormScalar::to_label`], an [`Options`](crate::Options) enum's label with its color or icon,
/// or any other type's form spelling. An empty list shows the dash an empty text does. The text,
/// as an export writes it, joins the labels with `, `.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     keywords: Vec<String>,
/// # }
/// tablo_core::ListColumn::new(tablo_core::lens!(Post.keywords));
/// ```
#[derive_where(Clone, Debug)]
pub struct ListColumn<M, T> {
    lens: Lens<M, Vec<T>, List<T>>,
    base: ColumnBase,
}

impl<M, T> ListColumn<M, T>
where
    M: toasty::schema::Model,
    T: FormScalar,
{
    /// Bind the column to the list `lens` reads.
    pub fn new(lens: Lens<M, Vec<T>, List<T>>) -> Self {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Grow),
            lens,
        }
    }

    base_builders!();
}

impl<M, T> Column<M> for ListColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_column_methods!();

    fn text(&self, cx: &Cx, row: &M) -> String {
        self.lens
            .read(row)
            .iter()
            .map(|value| value.to_label().localize(cx))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let badges = self
            .lens
            .read(row)
            .iter()
            .map(|value| {
                let color = value.color();
                TextCell {
                    text: value.to_label().localize(cx),
                    color,
                    badge: true,
                    icon: value.icon().map(|icon| (icon, color)),
                    ..TextCell::default()
                }
            })
            .collect();
        badges_cell(cx, badges)
    }
}

impl<M, T> crate::column::ColumnInternals<M> for ListColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_internals!(binding);
}
