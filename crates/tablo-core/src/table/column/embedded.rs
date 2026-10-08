//! [`EmbeddedColumn`]: an embedded value read off the record, one leaf at a time.

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use topcoat::{context::Cx, view::*};

use super::{Column, ColumnWidth};
use crate::{
    Lens,
    schema::{Binding, EmbeddedForm, FieldResolver, Schema},
};

/// A column of an [`EmbeddedForm`] value.
///
/// A detail page shows each of the value's leaves under its own label; an enum shows its variant's
/// name and that variant's leaves only. A table cell lists the same leaves as `label: value`.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Embed, tablo_core::EmbeddedForm)]
/// # struct Seo { title: String, description: String }
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, seo: Seo }
/// tablo_core::EmbeddedColumn::new(tablo_core::lens!(Post.seo));
/// ```
///
/// The value's leaves resolve through the app schema, which the column reads when it binds: a
/// panel binds the declarations it mounts, and [`Detail::bind`](crate::Detail::bind) binds one
/// built outside a panel.
pub struct EmbeddedColumn<M, T> {
    lens: Lens<M, T>,
    binding: Binding,
    /// The declared label, over the binding's.
    label: Option<String>,
    /// The value's schema, built when the column binds.
    schema: Arc<OnceLock<Schema>>,
    width: ColumnWidth,
}

impl<M, T> EmbeddedColumn<M, T>
where
    M: toasty::schema::Model,
    T: EmbeddedForm,
{
    /// Bind the column to the embedded value `lens` reads.
    pub fn new(lens: Lens<M, T>) -> Self {
        let binding = Binding::of(lens.path());
        Self {
            lens,
            binding,
            label: None,
            schema: Arc::default(),
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

    /// The value's schema and its leaves as the form spells them, once the column is bound.
    fn spelled(&self, row: &M) -> Option<(&Schema, HashMap<String, String>)> {
        let schema = self.schema.get()?;
        let mut values = HashMap::new();
        self.lens
            .read(row)
            .write_node(schema.embedded_root(), &mut values);
        Some((schema, values))
    }
}

impl<M, T> Column<M> for EmbeddedColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: EmbeddedForm + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(self.binding.label())
    }

    fn text(&self, _cx: &Cx, row: &M) -> String {
        let Some((schema, values)) = self.spelled(row) else {
            return String::new();
        };
        schema
            .embedded_root()
            .shown(&schema.fields, &values)
            .into_iter()
            .filter_map(|index| {
                let field = &schema.fields[index];
                let value = values.get(field.name()).map_or("", String::as_str);
                let read = field.read(value)?;
                Some(format!("{}: {read}", field.label_str()))
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn entry<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        match self.spelled(row) {
            Some((schema, values)) => schema.embedded_root().display(cx, &schema.fields, &values),
            None => ().boxed(),
        }
    }

    fn column_width(&self) -> ColumnWidth {
        self.width
    }

    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        if let Some(error) = self.binding.misdeclared() {
            return Some(error);
        }
        match self.schema.get() {
            Some(schema) => schema.declaration_errors().into_iter().next(),
            None => Some(crate::DeclarationErrorKind::Unbound {
                item: std::any::type_name::<T>(),
            }),
        }
    }

    fn bind(&self, resolver: &FieldResolver) {
        self.binding.bind(resolver);
        self.schema
            .get_or_init(|| T::build_schema(resolver, self.lens.path().clone()));
    }
}

impl<M, T> Clone for EmbeddedColumn<M, T> {
    fn clone(&self) -> Self {
        Self {
            lens: self.lens.clone(),
            binding: self.binding.clone(),
            label: self.label.clone(),
            schema: Arc::clone(&self.schema),
            width: self.width,
        }
    }
}

impl<M, T> std::fmt::Debug for EmbeddedColumn<M, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddedColumn")
            .field("name", &self.binding.name())
            .field(
                "label",
                &self.label.as_deref().unwrap_or(self.binding.label()),
            )
            .field("value", &std::any::type_name::<T>())
            .field("bound", &self.schema.get().is_some())
            .finish_non_exhaustive()
    }
}
