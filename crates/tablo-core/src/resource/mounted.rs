//! A resource as one panel mounted it, and the registry the panel's requests find it in.

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

use topcoat::context::{Cx, try_app_context};

use super::{
    Actions, HeaderActions, PublicLink, Relation, Resource, ResourceDef,
    def::{PublicLinkFn, RecordTitle},
};
use crate::{
    DeclarationError, DeclarationErrorKind, Site,
    detail::Detail,
    form::{FormField, RecordForm},
    naming::{kebab_case, pluralize, sentence_case, type_short_name, type_stem},
    navigation::NavigationItem,
    policy::{Ability, Policy},
    schema::{FieldResolver, Retype, Schema},
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
    pub(crate) detail: Detail<R::Model>,
    /// Whether the def declares the detail page, rather than the record form deriving it.
    pub(crate) declares_detail: bool,
    record_title: Option<RecordTitle<R::Model>>,
    public_link: Option<PublicLinkFn<R::Model>>,
    pub(crate) relations: Vec<Relation<R::Model>>,
    pub(crate) actions: Actions<R>,
    pub(crate) header_actions: HeaderActions,
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
            .unwrap_or_else(|| sentence_case(type_short_name::<R::Model>()));
        let plural_label = def.plural_label.unwrap_or_else(|| pluralize(&label));
        let mut navigation = def.navigation.unwrap_or_else(|| NavigationItem {
            label: plural_label.clone(),
            ..NavigationItem::default()
        });
        if let Some(order) = def.navigation_order {
            navigation.order = order;
        }
        if let Some(icon) = def.icon {
            navigation.icon = Some(icon);
        }
        if let Some(group) = def.navigation_group {
            navigation.group = Some(group);
        }
        let navigation = navigation.resolved(&url);
        let table = def.table.unwrap_or_else(<R::Form as RecordForm>::table);
        table.bind_with(resolver);
        let fields = <R::Form as RecordForm>::fields(resolver);
        let mut form: Schema = def.form.unwrap_or_default().retype();
        form.bind_with(resolver);
        // A field the form does not place follows the ones it does, with its default control.
        for field in &fields {
            let placed = form
                .fields()
                .any(|control| field.keys.iter().any(|key| key == control.name()));
            if !placed {
                let mut control = <R::Form as RecordForm>::control(field.field).retype();
                control.bind_with(resolver);
                form.append(control);
            }
        }
        // The record form decides which controls an empty submission fails.
        form.require(
            &fields
                .iter()
                .flat_map(|field| field.required.iter().map(String::as_str))
                .collect(),
        );
        let declares_detail = def.detail.is_some();
        let detail = def.detail.unwrap_or_else(<R::Form as RecordForm>::detail);
        detail.bind_with(resolver);
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
            detail,
            declares_detail,
            record_title: def.record_title,
            public_link: def.public_link,
            relations: def.relations,
            actions: def.actions,
            header_actions: def.header_actions,
            fields,
            create_columns: def.create_columns,
        }
    }

    /// Whether the resource declares a detail page.
    pub(crate) fn viewed(&self) -> bool {
        !self.detail.is_empty()
    }

    /// The title of `record`: its title column, else the resource's label and `key`.
    pub(crate) fn record_title(&self, record: &R::Model, key: &str) -> String {
        self.record_title
            .as_ref()
            .and_then(|title| title.read(record))
            .unwrap_or_else(|| format!("{} {key}", self.label))
    }

    /// What a relationship choice over the resource searches: the table's searchable columns and
    /// the title column.
    pub(crate) fn option_search_expr(&self, term: &str) -> Option<toasty::stmt::Expr<bool>> {
        let title = self
            .record_title
            .as_ref()
            .and_then(|title| title.search_expr(term));
        match (self.table.search_expr(term), title) {
            (Some(table), Some(title)) => Some(table.or(title)),
            (table, title) => table.or(title),
        }
    }

    /// The record's public page, if the resource links one.
    pub(crate) fn public_link(&self, cx: &Cx, record: &R::Model) -> Option<PublicLink> {
        self.public_link.as_ref().and_then(|link| link(cx, record))
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
