//! A resource's declarations, built once.

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

use topcoat::context::{Cx, try_app_context};

use super::{Relation, Resource, Table};
use crate::{
    form::{FormField, RecordForm},
    schema::{Schema, declare_with, schema_of},
};

/// One resource's table, form schema, view schema, relations and record-form fields.
pub(crate) struct Declared<R: Resource> {
    pub(crate) table: Table<R::Model>,
    pub(crate) form: Arc<Schema>,
    pub(crate) view: Schema,
    pub(crate) relations: Vec<Relation<R::Model>>,
    pub(crate) fields: Vec<FormField<<R::Form as RecordForm>::Field>>,
}

impl<R: Resource> Declared<R> {
    /// Call the resource's declarations with `schema` in scope.
    pub(crate) fn build(schema: Option<Arc<toasty_core::Schema>>) -> Self {
        declare_with(schema, || Self {
            table: R::table(),
            form: Arc::new(R::form()),
            view: R::view(),
            relations: R::relations(),
            fields: <R::Form as RecordForm>::fields(),
        })
    }

    /// Whether the resource declares a detail page.
    pub(crate) fn viewed(&self) -> bool {
        !self.view.is_empty()
    }
}

/// Every registered resource's [`Declared`], keyed by the resource type.
#[derive(Default)]
pub(crate) struct Declarations(HashMap<TypeId, Arc<dyn Any + Send + Sync>>);

impl Declarations {
    pub(crate) fn insert<R: Resource>(&mut self, declared: Arc<Declared<R>>) {
        self.0.insert(TypeId::of::<R>(), declared);
    }

    /// Add another panel's declarations.
    pub(crate) fn extend(&mut self, other: Self) {
        for (resource, declared) in other.0 {
            self.0.entry(resource).or_insert(declared);
        }
    }
}

/// `R`'s declarations.
pub(crate) fn declared<R: Resource>(cx: &Cx) -> Arc<Declared<R>> {
    try_app_context::<Declarations>(cx)
        .and_then(|declarations| declarations.0.get(&TypeId::of::<R>()))
        .and_then(|declared| Arc::clone(declared).downcast::<Declared<R>>().ok())
        .unwrap_or_else(|| Arc::new(Declared::build(schema_of(cx))))
}
