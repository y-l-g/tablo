//! [`RepeaterColumn`]: a `#[document]` list read off the record, one item at a time.

use std::collections::HashMap;

use toasty::stmt::List;
use topcoat::{context::Cx, view::*};

use super::{Column, ColumnWidth};
use crate::{
    Lens,
    schema::{Binding, FieldResolver, RepeaterItem, read_only, value_cell},
};

/// A column of a list of [`RepeaterItem`] values.
///
/// A detail page shows each item as a block of its fields, each under its own label. A table cell
/// lists the same fields as `label: value`, one item after another.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Embed, tablo_core::RepeaterItem)]
/// # struct Link { label: String, url: String }
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[document] links: Vec<Link>,
/// # }
/// tablo_core::RepeaterColumn::new(tablo_core::lens!(Post.links));
/// ```
pub struct RepeaterColumn<M, T> {
    lens: Lens<M, Vec<T>, List<T>>,
    binding: Binding,
    /// The declared label, over the binding's.
    label: Option<String>,
    width: ColumnWidth,
}

impl<M, T> RepeaterColumn<M, T>
where
    M: toasty::schema::Model,
    T: RepeaterItem,
{
    /// Bind the column to the list `lens` reads.
    pub fn new(lens: Lens<M, Vec<T>, List<T>>) -> Self {
        let binding = Binding::of(lens.path());
        Self {
            lens,
            binding,
            label: None,
            width: ColumnWidth::Wide,
        }
    }

    /// Replace the label the field's name gives it.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Declare this column's width.
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.width = width;
        self
    }

    /// Each item's keys as its row posts them.
    fn spelled(&self, row: &M) -> Vec<HashMap<String, String>> {
        self.lens
            .read(row)
            .iter()
            .map(|item| {
                let mut values = HashMap::new();
                item.write(&mut values);
                values
            })
            .collect()
    }
}

impl<M, T> Column<M> for RepeaterColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: RepeaterItem,
{
    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(self.binding.label())
    }

    fn text(&self, _cx: &Cx, row: &M) -> String {
        let schema = T::schema();
        self.spelled(row)
            .iter()
            .map(|values| {
                schema
                    .fields()
                    .filter_map(|field| {
                        let value = values.get(field.name()).map_or("", String::as_str);
                        let read = field.read(value)?;
                        Some(format!("{}: {read}", field.label_str()))
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
            return read_only(cx, label, value_cell(cx, ""));
        }
        let schema = T::schema();
        let items: Vec<BoxView<'a>> = items
            .iter()
            .map(|values| {
                let fields: Vec<BoxView<'a>> = schema
                    .fields()
                    .map(|field| {
                        field.display(cx, values.get(field.name()).map_or("", String::as_str))
                    })
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
            label,
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

    fn column_width(&self) -> ColumnWidth {
        self.width
    }

    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }

    fn bind(&self, resolver: &FieldResolver) {
        self.binding.bind(resolver);
    }
}

impl<M, T> Clone for RepeaterColumn<M, T> {
    fn clone(&self) -> Self {
        Self {
            lens: self.lens.clone(),
            binding: self.binding.clone(),
            label: self.label.clone(),
            width: self.width,
        }
    }
}

impl<M, T> std::fmt::Debug for RepeaterColumn<M, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepeaterColumn")
            .field("name", &self.binding.name())
            .field(
                "label",
                &self.label.as_deref().unwrap_or(self.binding.label()),
            )
            .field("item", &std::any::type_name::<T>())
            .finish_non_exhaustive()
    }
}
