//! [`RepeaterColumn`]: a `#[document]` list read off the record, one item at a time.

use derive_where::derive_where;
use toasty::stmt::List;
use topcoat::{context::Cx, view::*};

use super::{Column, ColumnBase, ColumnWidth};
use crate::{
    Lens,
    schema::{FormInput, read_only, value_cell},
    values::FormValues,
};

/// A column of a list of [`FormInput`] items.
///
/// A detail page shows each item as a block of its fields, each under its own label. A table cell
/// lists the same fields as `label: value`, one item after another.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Embed, tablo_core::FormInput)]
/// # struct Link { label: String, url: String }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[document] links: Vec<Link>,
/// # }
/// tablo_core::RepeaterColumn::new(tablo_core::lens!(Post.links));
/// ```
#[derive_where(Clone, Debug)]
pub struct RepeaterColumn<M, T> {
    lens: Lens<M, Vec<T>, List<T>>,
    base: ColumnBase,
}

impl<M, T> RepeaterColumn<M, T>
where
    M: toasty::schema::Model,
    T: FormInput,
{
    /// Bind the column to the list `lens` reads.
    pub fn new(lens: Lens<M, Vec<T>, List<T>>) -> Self {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Grow),
            lens,
        }
    }

    base_builders!();

    /// Each item's keys as its row posts them.
    fn spelled(&self, row: &M) -> Vec<FormValues> {
        self.lens
            .read(row)
            .iter()
            .map(|item| {
                let mut values = FormValues::new();
                item.write(&mut values);
                values
            })
            .collect()
    }
}

impl<M, T> Column<M> for RepeaterColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormInput,
{
    base_column_methods!(binding);

    fn text(&self, cx: &Cx, row: &M) -> String {
        let schema = T::schema();
        self.spelled(row)
            .iter()
            .map(|values| {
                schema
                    .fields()
                    .filter_map(|field| {
                        let value = values.text(field.name()).unwrap_or_default();
                        let read = field.read(cx, value)?;
                        Some(format!("{}: {read}", field.label().localize(cx)))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn entry<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let label = Column::label(self);
        let items = self.spelled(row);
        if items.is_empty() {
            return read_only(cx, &label, value_cell(cx, ""));
        }
        let schema = T::schema();
        let items: Vec<BoxView<'a>> = items
            .iter()
            .map(|values| {
                let fields: Vec<BoxView<'a>> = schema
                    .fields()
                    .map(|field| field.display(cx, values.text(field.name()).unwrap_or_default()))
                    .collect();
                view! {
                    cx =>
                    <li class="flex flex-col gap-3 rounded-lg border border-border p-4">
                        for field in fields {
                            (field)
                        }
                    </li>
                }
                .boxed()
            })
            .collect();
        read_only(
            cx,
            &label,
            view! {
                cx =>
                <ol class="flex flex-col gap-3 whitespace-normal">
                    for item in items {
                        (item)
                    }
                </ol>
            }
            .boxed(),
        )
    }
}
