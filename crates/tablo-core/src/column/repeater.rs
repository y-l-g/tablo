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
/// A detail page shows the items as a table, a row per item and a column per field. A table cell
/// lists the same fields as `label: value`, one item after another.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Embed, tablo_core::FormInput)]
/// # struct Link { label: String, url: String }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
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
    base_column_methods!();

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
        let headers: Vec<String> = schema
            .fields()
            .map(|field| field.label().localize(cx))
            .collect();
        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|values| {
                schema
                    .fields()
                    .map(|field| {
                        field
                            .read(cx, values.text(field.name()).unwrap_or_default())
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .collect();
        // The table takes the whole row of a detail page's grid, as its form control does.
        let entry = read_only(cx, &label, items_table(cx, headers, rows));
        view! { <div class="col-span-full">(entry)</div> }.boxed()
    }
}

/// `rows` of read values under `headers`, one column per field of the item: a repeater's entry.
/// A free function, so tablo-core compiles the view once rather than once per item type.
fn items_table(cx: &Cx, headers: Vec<String>, rows: Vec<Vec<String>>) -> BoxView<'_> {
    view! {
        <div
            class="mt-1 overflow-x-auto rounded-lg border border-border whitespace-normal"
        >
            <table class="w-full border-collapse text-sm">
                <thead class="bg-muted/50">
                    <tr class="border-b border-border">
                        for header in headers {
                            <th
                                scope="col"
                                class="px-3 py-2 text-start font-medium text-foreground"
                            >
                                (header)
                            </th>
                        }
                    </tr>
                </thead>
                <tbody class="divide-y divide-border">
                    for cells in rows {
                        <tr class="divide-x divide-border">
                            for cell in cells {
                                <td class="px-3 py-2 align-top text-foreground/75">
                                    (value_cell(cx, &cell))
                                </td>
                            }
                        </tr>
                    }
                </tbody>
            </table>
        </div>
    }
    .boxed()
}

impl<M, T> crate::column::ColumnInternals<M> for RepeaterColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormInput,
{
    base_internals!(binding);
}
