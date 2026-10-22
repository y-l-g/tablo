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
    naming::{pluralize, sentence_case, slug_case, type_short_name, type_stem},
    navigation::NavigationItem,
    policy::{Ability, Policy},
    schema::{Retype, Schema},
    table::{ModelColumns, Table},
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
    /// The rows [`ResourceDef::navigation_badge`] counts.
    navigation_badge: Option<toasty::stmt::Expr<bool>>,
    pub(crate) policy: Arc<dyn Policy<R::Model>>,
    pub(crate) tenancy: Tenancy<R::Model>,
    pub(crate) table: Arc<Table<R::Model>>,
    pub(crate) form: Arc<Schema>,
    pub(crate) detail: Detail<R::Model>,
    record_title: Option<RecordTitle<R::Model>>,
    pub(crate) global_search: Option<super::GlobalSearch<R::Model>>,
    public_link: Option<PublicLinkFn<R::Model>>,
    pub(crate) relations: Vec<Relation<R::Model>>,
    pub(crate) actions: Actions<R>,
    pub(crate) header_actions: HeaderActions,
    pub(crate) fields: Vec<FormField<<R::Form as RecordForm>::Field>>,
    pub(crate) create_columns: Vec<Result<String, DeclarationErrorKind>>,
    /// Whether the resource serves a create page: it has a record form and is not
    /// [`edit_only`](ResourceDef::edit_only).
    pub(crate) creates: bool,
    /// Whether the resource is [`importable`](ResourceDef::importable): mounting refuses one
    /// that serves no create page, so a mounted panel serves its import beside that page.
    pub(crate) imports: bool,
    pub(crate) soft_delete: Option<super::SoftDelete<R::Model>>,
    pub(crate) field_rules: Vec<super::FieldRule>,
}

impl<R: Resource> Mounted<R> {
    /// Fills in `def`'s defaults for a panel at `prefix`.
    pub(crate) fn new(def: ResourceDef<R>, prefix: &str) -> Self {
        let slug = def
            .slug
            .unwrap_or_else(|| slug_case(&pluralize(&sentence_case(type_stem::<R>("Resource")))));
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
        let mut table = def.table.unwrap_or_else(<R::Model as ModelColumns>::table);
        let mut actions = def.actions;
        if def.soft_delete.is_some() {
            table.push_filter(Arc::new(super::TrashFilter));
            actions.add_trash_actions();
        }
        let fields = <R::Form as RecordForm>::fields();
        let mut form: Schema = def.form.unwrap_or_default().retype();
        // A field the form does not place follows the ones it does, with its default control.
        for field in &fields {
            let placed = form
                .fields()
                .any(|control| field.keys.iter().any(|key| key == control.name()));
            if !placed {
                form.append(<R::Form as RecordForm>::control(field.field).retype());
            }
        }
        // The record form decides which controls an empty submission fails.
        form.require(
            &fields
                .iter()
                .flat_map(|field| field.required.iter().map(String::as_str))
                .collect(),
        );
        let detail = def
            .detail
            .unwrap_or_else(<R::Model as ModelColumns>::detail);
        Self {
            slug,
            url,
            label,
            plural_label,
            navigation,
            navigation_badge: def.navigation_badge,
            policy: def.policy,
            tenancy: def.tenancy,
            table: Arc::new(table),
            form: Arc::new(form),
            detail,
            record_title: def.record_title,
            global_search: def.global_search,
            public_link: def.public_link,
            relations: def.relations,
            actions,
            header_actions: def.header_actions,
            fields,
            create_columns: def.create_columns,
            creates: <R::Form as RecordForm>::HAS_FORM && !def.edit_only,
            imports: def.importable,
            soft_delete: def.soft_delete,
            field_rules: def.field_rules,
        }
    }

    /// Whether [`ResourceDef::navigation_badge`] declares a badge to count.
    pub(crate) fn counts_navigation_badge(&self) -> bool {
        self.navigation_badge.is_some()
    }

    /// The sidebar badge [`ResourceDef::navigation_badge`] declares: the count of the rows its
    /// filter matches, none for zero. A tenant-scoped resource shows none to a request without a
    /// tenant, whose list refuses it too. A failed count shows none either, logged: the sidebar
    /// renders on every page, which a badge must not fail.
    pub(crate) async fn navigation_badge(&self, cx: &Cx) -> Option<String> {
        let filter = self.navigation_badge.clone()?;
        if self.tenancy.is_scoped() && crate::tenant_id(cx).is_none() {
            return None;
        }
        let query = match self.scoped_query(cx) {
            Ok(query) => query.filter(filter),
            Err(error) => {
                tracing::error!(error = %error, "navigation badge query");
                return None;
            }
        };
        match query.count().exec(&mut crate::db::db(cx)).await {
            Ok(0) => None,
            Ok(count) => Some(count.to_string()),
            Err(error) => {
                tracing::error!(error = %error, "navigation badge count");
                None
            }
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

    /// The root field the record title reads.
    pub(crate) fn record_title_field(&self) -> Option<&str> {
        self.record_title.as_ref()?.field.as_deref()
    }

    /// What a relationship choice over the resource searches for `cx`: the searchable columns of
    /// its table and the title column.
    pub(crate) fn option_search_expr(
        &self,
        cx: &Cx,
        term: &str,
    ) -> Option<toasty::stmt::Expr<bool>> {
        self.term_search_expr(&self.table_for(cx), term)
    }

    /// What the global search matches for `cx`: each of `words` in a column
    /// [`option_search_expr`](Self::option_search_expr) searches; `None` for no words, or when the
    /// resource searches no column.
    pub(crate) fn words_search_expr(
        &self,
        cx: &Cx,
        words: &[String],
    ) -> Option<toasty::stmt::Expr<bool>> {
        let table = self.table_for(cx);
        let mut words = words.iter().map(|word| self.term_search_expr(&table, word));
        let first = words.next()??;
        words.try_fold(first, |all, word| Some(all.and(word?)))
    }

    /// `term` in one of `table`'s searchable columns or in the title column.
    fn term_search_expr(
        &self,
        table: &Table<R::Model>,
        term: &str,
    ) -> Option<toasty::stmt::Expr<bool>> {
        let title = self
            .record_title
            .as_ref()
            .and_then(|title| title.search_expr(term));
        match (table.search_expr(term), title) {
            (Some(table), Some(title)) => Some(table.or(title)),
            (table, title) => table.or(title),
        }
    }

    /// Whether the global search searches the resource: it did not opt out, it has a searchable
    /// column or a title column to match, and a result can open a page: its records have URLs, and
    /// it has a detail page or a form.
    pub(crate) fn globally_searched(&self) -> bool {
        self.global_search.is_some()
            && (self.record_title.is_some() || self.table.has_searchable_column())
            && self.table.is_addressable()
            && (self.viewed() || <R::Form as RecordForm>::HAS_FORM)
    }

    /// The fields the panel's [`ResourceDef::field_access`] rules let `cx` neither view nor change,
    /// every rule over the model from every resource the panel mounts over it, and the ones they
    /// let `cx` view only.
    pub(crate) fn field_mask(&self, cx: &Cx) -> super::FieldMask {
        let model = <R::Model as toasty::schema::Model>::id();
        super::FieldMask::of(cx, model_rules(cx, model, &self.field_rules))
    }

    /// The table `cx` reads: without what reads a field it may not view, of the records or of
    /// the related records a column shows.
    pub(crate) fn table_for(&self, cx: &Cx) -> Arc<Table<R::Model>> {
        let mask = self.field_mask(cx);
        if mask.hidden().is_empty() && !self.table.has_masked(cx) {
            Arc::clone(&self.table)
        } else {
            Arc::new(self.table.without(cx, mask.hidden()))
        }
    }

    /// The form keys `cx` may not see, and the ones it may see but not change.
    pub(crate) fn form_mask(&self, cx: &Cx) -> crate::schema::FormMask {
        self.field_mask(cx).form(&self.fields)
    }

    /// Whether `cx` may create a record: the policy allows it, and every field the form masks
    /// for `cx` and the record form requires has a default to take.
    pub(crate) fn can_create(&self, cx: &Cx) -> bool {
        if !self.can(cx, Ability::Create) {
            return false;
        }
        let mask = self.form_mask(cx);
        !self.form.fields().any(|field| {
            mask.keys().any(|key| key == field.name())
                && field.is_required()
                && field.default_value().is_none()
        })
    }

    /// The record's public page, if the resource links one.
    pub(crate) fn public_link(&self, cx: &Cx, record: &R::Model) -> Option<PublicLink> {
        self.public_link.as_ref().and_then(|link| link(cx, record))
    }

    /// Whether the policy allows `ability`. A force delete is a delete too, so the policy must
    /// also allow the delete: a policy that keeps a record from the trash keeps it whole.
    pub(crate) fn can(&self, cx: &Cx, ability: Ability<'_, R::Model>) -> bool {
        let implied = match ability {
            Ability::ForceDeleteAny => self.policy.allows(cx, Ability::DeleteAny),
            Ability::ForceDelete(record) => self.policy.allows(cx, Ability::Delete(record)),
            _ => true,
        };
        implied && self.policy.allows(cx, ability)
    }
}

/// The resources one panel mounted, by resource type, the models whose rows belong to a tenant:
/// a resource over one is tenant-scoped, or the model has a tenant column, and the field rules
/// the resources declare, by model.
#[derive(Default)]
pub(crate) struct Mounts {
    resources: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    tenant_models: std::collections::HashSet<crate::toasty_compat::model::ModelId>,
    /// Every mounted resource's field rules, by the model they restrict: a rule restricts its
    /// field for every resource over that model.
    field_rules: HashMap<crate::toasty_compat::model::ModelId, Vec<super::FieldRule>>,
}

impl Mounts {
    pub(crate) fn insert<R: Resource>(&mut self, mounted: Arc<Mounted<R>>) {
        if mounted.tenancy.is_scoped()
            || !<R::Model as crate::ModelColumns>::TENANT_COLUMNS.is_empty()
        {
            self.tenant_models
                .insert(<R::Model as toasty::schema::Model>::id());
        }
        self.field_rules
            .entry(<R::Model as toasty::schema::Model>::id())
            .or_default()
            .extend(mounted.field_rules.iter().cloned());
        self.resources.insert(TypeId::of::<R>(), mounted);
    }

    /// Whether the rows of `model` belong to a tenant, as a mounted resource over it says.
    pub(crate) fn is_tenant_model(&self, model: crate::toasty_compat::model::ModelId) -> bool {
        self.tenant_models.contains(&model)
    }

    /// The field rules every mounted resource over `model` declares.
    pub(crate) fn model_rules(
        &self,
        model: crate::toasty_compat::model::ModelId,
    ) -> &[super::FieldRule] {
        self.field_rules.get(&model).map_or(&[], Vec::as_slice)
    }

    /// Whether a mounted resource restricts who views a field of any model.
    pub(crate) fn restricts_any(&self) -> bool {
        self.field_rules
            .values()
            .flatten()
            .any(super::FieldRule::restricts_view)
    }

    /// Whether a mounted resource restricts who views a field of `model`.
    pub(crate) fn is_restricted_model(&self, model: crate::toasty_compat::model::ModelId) -> bool {
        self.model_rules(model)
            .iter()
            .any(super::FieldRule::restricts_view)
    }

    pub(crate) fn get<R: Resource>(&self) -> Option<Arc<Mounted<R>>> {
        let mounted = self.resources.get(&TypeId::of::<R>())?;
        Arc::clone(mounted).downcast().ok()
    }

    pub(crate) fn contains(&self, resource: TypeId) -> bool {
        self.resources.contains_key(&resource)
    }
}

/// Finds the [`Mounts`] of the request's panel; the first panel mounted on a router installs it
/// in the app context.
pub(crate) struct MountScope(pub(crate) fn(&Cx) -> Option<&Mounts>);

/// `R` as the context's panel mounted it, if it did: the request's panel, or the one a
/// [`PanelHandle::context`](crate::PanelHandle::context) holds.
pub(crate) fn mounted<R: Resource>(cx: &Cx) -> Option<Arc<Mounted<R>>> {
    let MountScope(mounts) = try_app_context::<MountScope>(cx)?;
    mounts(cx)?.get::<R>()
}

/// Whether the context's panel mounts a resource saying `model`'s rows belong to a tenant.
pub(crate) fn is_tenant_model(cx: &Cx, model: crate::toasty_compat::model::ModelId) -> bool {
    try_app_context::<MountScope>(cx)
        .and_then(|MountScope(mounts)| mounts(cx))
        .is_some_and(|mounts| mounts.is_tenant_model(model))
}

/// The field rules `cx`'s panel mounts over `model`, every resource's: `own` when the context
/// carries no panel, as a declaration check of one resource alone sees them.
pub(crate) fn model_rules<'a>(
    cx: &'a Cx,
    model: crate::toasty_compat::model::ModelId,
    own: &'a [super::FieldRule],
) -> &'a [super::FieldRule] {
    match try_app_context::<MountScope>(cx).and_then(|MountScope(mounts)| mounts(cx)) {
        Some(mounts) => mounts.model_rules(model),
        None => own,
    }
}

/// Whether the context's panel mounts a resource restricting who views a field of any model.
pub(crate) fn restricts_any(cx: &Cx) -> bool {
    try_app_context::<MountScope>(cx)
        .and_then(|MountScope(mounts)| mounts(cx))
        .is_some_and(|mounts| mounts.restricts_any())
}

/// Whether the context's panel mounts a resource restricting who views a field of `model`.
pub(crate) fn is_restricted_model(cx: &Cx, model: crate::toasty_compat::model::ModelId) -> bool {
    try_app_context::<MountScope>(cx)
        .and_then(|MountScope(mounts)| mounts(cx))
        .is_some_and(|mounts| mounts.is_restricted_model(model))
}

/// Whether the context's panel mounts the resource of type `resource`.
pub(crate) fn is_mounted(cx: &Cx, resource: TypeId) -> bool {
    try_app_context::<MountScope>(cx)
        .and_then(|MountScope(mounts)| mounts(cx))
        .is_some_and(|mounts| mounts.contains(resource))
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
