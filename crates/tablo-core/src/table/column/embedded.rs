//! [`EmbeddedColumn`]: an embedded value read off the record, one leaf at a time.

use std::sync::Arc;

use derive_where::derive_where;
use topcoat::{context::Cx, view::*};

use super::{Column, ColumnBase, ColumnWidth};
use crate::{
    Lens,
    form::FormValues,
    schema::{EmbeddedForm, Schema},
};

/// A column of an [`EmbeddedForm`] value.
///
/// A detail page shows each of the value's leaves under its own label; an enum shows its variant's
/// name and that variant's leaves only. A table cell lists the same leaves as `label: value`.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Embed, tablo_core::EmbeddedForm)]
/// # struct Seo { title: String, description: String }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, seo: Seo }
/// tablo_core::EmbeddedColumn::new(tablo_core::lens!(Post.seo));
/// ```
#[derive_where(Clone, Debug)]
pub struct EmbeddedColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
    /// The value's schema.
    schema: Arc<Schema>,
}

impl<M, T> EmbeddedColumn<M, T>
where
    M: toasty::schema::Model,
    T: EmbeddedForm,
{
    /// Bind the column to the embedded value `lens` reads.
    pub fn new(lens: Lens<M, T>) -> Self {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Grow),
            schema: Arc::new(T::build_schema(lens.path().clone())),
            lens,
        }
    }

    base_builders!();

    /// The value's schema and its leaves as the form spells them.
    fn spelled(&self, row: &M) -> (&Schema, FormValues) {
        let mut values = FormValues::new();
        self.lens
            .read(row)
            .write_node(self.schema.embedded_root(), &mut values);
        (&self.schema, values)
    }
}

impl<M, T> Column<M> for EmbeddedColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: EmbeddedForm + Send + Sync + 'static,
{
    base_column_methods!();

    fn text(&self, _cx: &Cx, row: &M) -> String {
        let (schema, values) = self.spelled(row);
        schema
            .embedded_root()
            .shown(&schema.fields, &values)
            .into_iter()
            .filter_map(|index| {
                let field = &schema.fields[index];
                let value = values.text(field.name()).unwrap_or_default();
                let read = field.read(value)?;
                Some(format!("{}: {read}", field.label_str()))
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn entry<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let (schema, values) = self.spelled(row);
        schema.embedded_root().display(cx, &schema.fields, &values)
    }

    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        if let Some(error) = self.base.binding.misdeclared() {
            return Some(error);
        }
        self.schema.declaration_errors().into_iter().next()
    }
}
