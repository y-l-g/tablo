//! A resource's declarations, built once.
//!
//! [`Panel::build`](crate::panel::Panel::build) calls each resource's
//! [`table`](super::Resource::table), [`form`](super::Resource::form),
//! [`view`](super::Resource::view) and
//! [`relations`](super::Resource::relations) once, checks the values, and installs them
//! on the router's app context. Every handler reads them from there, so a
//! request builds no declaration and serves exactly what the checks saw.

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

use topcoat::context::{Cx, try_app_context};

use super::{Relation, Resource, Table};
use crate::schema::{DeclCx, Schema};

/// One resource's table, form schema, view schema and relations.
pub(crate) struct Declared<R: Resource> {
    pub(crate) table: Table<R::Model>,
    /// Shared, so a submission can hold the schema past the handler's
    /// borrow of the declarations.
    pub(crate) form: Arc<Schema>,
    pub(crate) view: Schema,
    pub(crate) relations: Vec<Relation<R::Model>>,
}

impl<R: Resource> Declared<R> {
    /// Call the resource's declarations with `dx`.
    pub(crate) fn build(dx: &DeclCx) -> Self {
        Self {
            table: R::table(),
            form: Arc::new(R::form(dx)),
            view: R::view(dx),
            relations: R::relations(),
        }
    }

    /// Whether the resource declares a detail page: a non-empty
    /// [`view`](Resource::view). The detail route 404s without one, and the
    /// row chrome leaves the View link off.
    pub(crate) fn viewed(&self) -> bool {
        !self.view.is_empty()
    }
}

/// Every registered resource's [`Declared`], keyed by the resource type: the
/// app-context value [`declared`] reads.
#[derive(Default)]
pub(crate) struct Declarations(HashMap<TypeId, Arc<dyn Any + Send + Sync>>);

impl Declarations {
    pub(crate) fn insert<R: Resource>(&mut self, declared: Arc<Declared<R>>) {
        self.0.insert(TypeId::of::<R>(), declared);
    }
}

/// `R`'s declarations: the ones the panel built, or, for a resource no panel
/// on this router registers, a fresh build from the request's app schema.
pub(crate) fn declared<R: Resource>(cx: &Cx) -> Arc<Declared<R>> {
    try_app_context::<Declarations>(cx)
        .and_then(|declarations| declarations.0.get(&TypeId::of::<R>()))
        .and_then(|declared| Arc::clone(declared).downcast::<Declared<R>>().ok())
        .unwrap_or_else(|| Arc::new(Declared::build(&DeclCx::from_cx(cx))))
}
