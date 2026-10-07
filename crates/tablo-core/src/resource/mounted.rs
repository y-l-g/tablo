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
    form::{FormField, RecordForm},
    naming::{kebab_case, pluralize, type_short_name, type_stem},
    navigation::NavigationItem,
    policy::{Ability, Policy},
    schema::{FieldResolver, Schema},
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
    pub(crate) table: Arc<Table<R::Model>>,
    pub(crate) form: Arc<Schema>,
    /// The detail schema when it is not the form's.
    view: Option<Schema>,
    pub(crate) relations: Vec<Relation<R::Model>>,
    pub(crate) actions: Actions<R>,
    pub(crate) fields: Vec<FormField<<R::Form as RecordForm>::Field>>,
    pub(crate) create_columns: Vec<Result<String, DeclarationErrorKind>>,
}

impl<R: Resource> Mounted<R> {
    /// Fills in `def`'s defaults for a panel at `prefix` and binds its declarations through
    /// `resolver`'s app schema.
    pub(crate) fn new(def: ResourceDef<R>, prefix: &str, resolver: &FieldResolver) -> Self {
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
        let table = def.table.unwrap_or_else(<R::Form as RecordForm>::table);
        table.bind_with(resolver);
        let fields = <R::Form as RecordForm>::fields(resolver);
        let mut form = def.form.unwrap_or_else(<R::Form as RecordForm>::schema);
        form.bind_with(resolver);
        // The record form decides which controls an empty submission fails.
        form.require(
            &fields
                .iter()
                .flat_map(|field| field.required.iter().map(String::as_str))
                .collect(),
        );
        let view = def.view.map(|mut view| {
            view.bind_with(resolver);
            view
        });
        Self {
            slug,
            url,
            label,
            plural_label,
            navigation,
            policy: def.policy,
            tenancy: def.tenancy,
            table: Arc::new(table),
            form: Arc::new(form),
            view,
            relations: def.relations,
            actions: def.actions,
            fields,
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

/// `R` as the context's panel mounted it, if it did: the request's panel, or the one a
/// [`Panel::context`](crate::Panel::context) holds.
pub(crate) fn mounted<R: Resource>(cx: &Cx) -> Option<Arc<Mounted<R>>> {
    let MountScope(mounts) = try_app_context::<MountScope>(cx)?;
    mounts(cx)?.get::<R>()
}

/// `R` as the request's panel mounted it.
///
/// # Errors
///
/// A declaration error when the request's panel does not mount `R`.
pub(crate) fn require_mounted<R: Resource>(cx: &Cx) -> topcoat::Result<Arc<Mounted<R>>> {
    mounted::<R>(cx).ok_or_else(|| {
        crate::error::declaration(
            DeclarationError::of::<R>(Site::Registration, DeclarationErrorKind::NotMounted)
                .to_string(),
        )
    })
}
