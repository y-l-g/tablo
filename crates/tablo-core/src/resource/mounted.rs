//! A resource as one panel mounted it, and the registry the panel's requests find it in.

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

use topcoat::context::{Cx, try_app_context};

use super::{Actions, Relation, Resource, ResourceDef};
use crate::{
    DeclarationError, DeclarationErrorKind, Site,
    error::TabloError,
    form::{FormField, RecordForm},
    naming::{kebab_case, pluralize, type_short_name, type_stem},
    navigation::NavigationItem,
    policy::{Ability, Policy},
    schema::{Schema, declare_with, schema_of},
    table::Table,
    tenancy::Tenancy,
};

/// `R`'s [`ResourceDef`] with every default filled in, built once when its panel mounts.
pub(crate) struct Mounted<R: Resource> {
    pub(crate) slug: String,
    /// The list URL, `{prefix}/{slug}`.
    pub(crate) url: String,
    pub(crate) label: String,
    pub(crate) plural_label: String,
    pub(crate) navigation: NavigationItem,
    pub(crate) policy: Arc<dyn Policy<R::Model>>,
    pub(crate) tenancy: Tenancy<R::Model>,
    pub(crate) table: Table<R::Model>,
    pub(crate) form: Arc<Schema>,
    /// The detail schema when it is not the form's.
    view: Option<Schema>,
    pub(crate) relations: Vec<Relation<R::Model>>,
    pub(crate) actions: Actions<R>,
    pub(crate) fields: Vec<FormField<<R::Form as RecordForm>::Field>>,
    pub(crate) create_columns: Vec<&'static str>,
}

impl<R: Resource> Mounted<R> {
    /// Fills in `def`'s defaults for a panel at `prefix`; call it with the app schema in scope.
    pub(crate) fn new(def: ResourceDef<R>, prefix: &str) -> Self {
        let slug = def
            .slug
            .unwrap_or_else(|| kebab_case(&pluralize(type_stem::<R>("Resource"))));
        let url = format!("{prefix}/{slug}");
        let label = def
            .label
            .unwrap_or_else(|| type_short_name::<R::Model>().to_string());
        let plural_label = def.plural_label.unwrap_or_else(|| pluralize(&label));
        let navigation = def
            .navigation
            .unwrap_or_else(|| NavigationItem {
                label: plural_label.clone(),
                order: def.navigation_order,
                icon: def.icon,
                ..NavigationItem::default()
            })
            .resolved(&url);
        Self {
            slug,
            url,
            label,
            plural_label,
            navigation,
            policy: def.policy,
            tenancy: def.tenancy,
            table: def.table.unwrap_or_else(<R::Form as RecordForm>::table),
            form: Arc::new(def.form.unwrap_or_else(<R::Form as RecordForm>::schema)),
            view: def.view,
            relations: def.relations,
            actions: def.actions,
            fields: <R::Form as RecordForm>::fields(),
            create_columns: def.create_columns,
        }
    }

    /// The detail page's schema: the def's view, else its form.
    pub(crate) fn view(&self) -> &Schema {
        self.view.as_ref().unwrap_or(&self.form)
    }

    /// Whether the resource declares a separate detail schema.
    pub(crate) fn has_own_view(&self) -> bool {
        self.view.is_some()
    }

    /// Whether the resource declares a detail page.
    pub(crate) fn viewed(&self) -> bool {
        !self.view().is_empty()
    }

    /// Whether the policy allows `ability`.
    pub(crate) fn can(&self, cx: &Cx, ability: Ability<'_, R::Model>) -> bool {
        self.policy.allows(cx, ability)
    }
}

/// The resources one panel mounted, by resource type.
#[derive(Default)]
pub(crate) struct Mounts(HashMap<TypeId, Arc<dyn Any + Send + Sync>>);

impl Mounts {
    pub(crate) fn insert<R: Resource>(&mut self, mounted: Arc<Mounted<R>>) {
        self.0.insert(TypeId::of::<R>(), mounted);
    }

    pub(crate) fn get<R: Resource>(&self) -> Option<Arc<Mounted<R>>> {
        let mounted = self.0.get(&TypeId::of::<R>())?;
        Arc::clone(mounted).downcast().ok()
    }

    pub(crate) fn contains(&self, resource: TypeId) -> bool {
        self.0.contains_key(&resource)
    }
}

/// Finds the [`Mounts`] of the request's panel; the first panel mounted on a router installs it
/// in the app context.
pub(crate) struct MountScope(pub(crate) fn(&Cx) -> Option<&Mounts>);

/// `R` as the request's panel mounted it, if it did.
///
/// A context with no panel at all, such as a test's or a background job's, answers from `R`'s own
/// [`declare`](Resource::declare).
pub(crate) fn mounted<R: Resource>(cx: &Cx) -> Option<Arc<Mounted<R>>> {
    match try_app_context::<MountScope>(cx) {
        Some(MountScope(mounts)) => mounts(cx)?.get::<R>(),
        None => Some(Arc::new(declare_with(schema_of(cx), || {
            Mounted::new(R::declare(), "")
        }))),
    }
}

/// `R` as the request's panel mounted it.
///
/// # Errors
///
/// A declaration error when the request's panel does not mount `R`.
pub(crate) fn require_mounted<R: Resource>(cx: &Cx) -> topcoat::Result<Arc<Mounted<R>>> {
    mounted::<R>(cx).ok_or_else(|| {
        TabloError::Declaration(
            DeclarationError::of::<R>(Site::Registration, DeclarationErrorKind::NotMounted)
                .to_string(),
        )
        .into()
    })
}
