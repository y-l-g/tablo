//! [`ResourceDef`]: everything a resource declares, as one value.

use std::sync::Arc;

use toasty::stmt::Path;
use topcoat::icon::IconData;

use super::{Action, Actions, PublicLink, Relation, Resource};
use crate::{
    DeclarationErrorKind,
    detail::Detail,
    navigation::NavigationItem,
    policy::{Deny, Policy},
    schema::Schema,
    table::Table,
    tenancy::Tenancy,
    toasty_compat::model::{self, ModelPath},
};

/// What a [`Resource`] declares: its names, navigation, policy, tenancy, table, form, detail page,
/// relations and actions.
///
/// [`Resource::declare`] returns one, and [`Panel::resource_with`](crate::Panel::resource_with)
/// adjusts it for one panel. Every setting has a default, so a resource sets only what differs:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, featured: bool }
/// # #[derive(Debug, Clone, tablo_core::RecordForm)]
/// # #[form(model = Post)]
/// # struct PostForm { title: String, featured: bool }
/// # struct PostResource;
/// use tablo_core::{ReadOnly, RecordForm, Resource, ResourceDef, TernaryFilter};
///
/// impl Resource for PostResource {
///     type Model = Post;
///     type Form = PostForm;
///
///     fn declare() -> ResourceDef<Self> {
///         ResourceDef::new()
///             .label("Blog post")
///             .policy(ReadOnly)
///             .table(PostForm::table().filters(TernaryFilter::new(Post::fields().featured())))
///     }
/// }
/// ```
///
/// The panel builds the def once when it mounts, binding the paths it names to the database
/// schema, and serves the result to every request.
pub struct ResourceDef<R: Resource> {
    pub(crate) slug: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) plural_label: Option<String>,
    pub(crate) icon: Option<IconData>,
    pub(crate) navigation_order: i32,
    pub(crate) navigation: Option<NavigationItem>,
    pub(crate) policy: Arc<dyn Policy<R::Model>>,
    pub(crate) tenancy: Tenancy<R::Model>,
    pub(crate) table: Option<Table<R::Model>>,
    pub(crate) form: Option<Schema>,
    pub(crate) view: Option<Detail<R::Model>>,
    pub(crate) record_label: Option<RecordLabel<R::Model>>,
    pub(crate) public_link: Option<PublicLinkFn<R::Model>>,
    pub(crate) relations: Vec<Relation<R::Model>>,
    pub(crate) actions: Actions<R>,
    /// The columns [`Self::create_column`] names, or why a path names no column.
    pub(crate) create_columns: Vec<Result<String, DeclarationErrorKind>>,
}

impl<R: Resource> Default for ResourceDef<R> {
    fn default() -> Self {
        Self {
            slug: None,
            label: None,
            plural_label: None,
            icon: None,
            navigation_order: 0,
            navigation: None,
            policy: Arc::new(Deny),
            tenancy: Tenancy::none(),
            table: None,
            form: None,
            view: None,
            record_label: None,
            public_link: None,
            relations: Vec::new(),
            actions: Actions::default(),
            create_columns: Vec::new(),
        }
    }
}

impl<R: Resource> ResourceDef<R> {
    /// A def with every default: the policy denies all, rows belong to no tenant, and the record
    /// form derives the table, the form and the detail page.
    pub fn new() -> Self {
        Self::default()
    }

    /// The URL slug for the resource's pages: `"users"` mounts the list at `{panel prefix}/users`.
    ///
    /// Defaults to the resource type's name without a trailing `Resource`, pluralized and
    /// kebab-cased: `UserResource` → `users`, `BlogPostResource` → `blog-posts`.
    #[must_use]
    pub fn slug(mut self, slug: impl Into<String>) -> Self {
        self.slug = Some(slug.into());
        self
    }

    /// One record's name, the noun in the "Create {label}" and "Edit {label}" titles.
    ///
    /// Defaults to the model's type name.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The list title and sidebar label.
    ///
    /// Defaults to the pluralized [`label`](Self::label): `Category` → `Categories`, `Person` →
    /// `People`.
    #[must_use]
    pub fn plural_label(mut self, label: impl Into<String>) -> Self {
        self.plural_label = Some(label.into());
        self
    }

    /// The icon of the resource's sidebar entry.
    #[must_use]
    pub fn icon(mut self, icon: IconData) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Where the sidebar entry sorts among the panel's entries, lowest first; defaults to `0`, and
    /// entries of equal order keep their registration order.
    #[must_use]
    pub fn navigation_order(mut self, order: i32) -> Self {
        self.navigation_order = order;
        self
    }

    /// Replaces the sidebar entry, for one that links somewhere other than the list page:
    /// `NavigationItem::at("Drafts", "/admin/posts?f.status=draft")`.
    #[must_use]
    pub fn navigation(mut self, item: NavigationItem) -> Self {
        self.navigation = Some(item);
        self
    }

    /// Gates every handler; defaults to [`Deny`]. Row scoping belongs in
    /// [`Resource::query`], not the policy.
    #[must_use]
    pub fn policy(mut self, policy: impl Policy<R::Model>) -> Self {
        self.policy = Arc::new(policy);
        self
    }

    /// How rows belong to a tenant; defaults to [`Tenancy::none`].
    #[must_use]
    pub fn tenancy(mut self, tenancy: Tenancy<R::Model>) -> Self {
        self.tenancy = tenancy;
        self
    }

    /// The list table; defaults to the record form's derived table ([`RecordForm::table`]).
    ///
    /// [`RecordForm::table`]: crate::RecordForm::table
    #[must_use]
    pub fn table(mut self, table: Table<R::Model>) -> Self {
        self.table = Some(table);
        self
    }

    /// The schema the create and edit forms render; defaults to the record form's derived schema
    /// ([`RecordForm::schema`]).
    ///
    /// The panel refuses a record form field this schema does not declare, and a schema on a
    /// resource whose [`Form`](Resource::Form) is [`NoForm`](crate::NoForm).
    ///
    /// [`RecordForm::schema`]: crate::RecordForm::schema
    #[must_use]
    pub fn form(mut self, form: Schema) -> Self {
        self.form = Some(form);
        self
    }

    /// The detail page: columns in layout blocks; defaults to the record form's derived detail
    /// page ([`RecordForm::detail`]), and [`Detail::empty`] turns the page off.
    ///
    /// The page loads the relations its columns declare.
    ///
    /// [`RecordForm::detail`]: crate::RecordForm::detail
    #[must_use]
    pub fn view(mut self, view: Detail<R::Model>) -> Self {
        self.view = Some(view);
        self
    }

    /// Titles the detail page with the record's label; defaults to the [`label`](Self::label)
    /// and the record's key, as in `Blog Post 3f2a…`.
    ///
    /// A label is display text, not a key: two records can share one, so it never replaces the
    /// primary key that keys the table's rows and the action routes.
    #[must_use]
    pub fn record_label(
        mut self,
        label: impl Fn(&R::Model) -> String + Send + Sync + 'static,
    ) -> Self {
        self.record_label = Some(Arc::new(label));
        self
    }

    /// Links a record's public page from its detail and edit pages, for each record `link`
    /// returns one for; defaults to no link.
    #[must_use]
    pub fn public_link(
        mut self,
        link: impl Fn(&R::Model) -> Option<PublicLink> + Send + Sync + 'static,
    ) -> Self {
        self.public_link = Some(Arc::new(link));
        self
    }

    /// Adds a related resource, rendered as its table narrowed to the record on the detail and
    /// edit pages.
    #[must_use]
    pub fn relation(mut self, relation: Relation<R::Model>) -> Self {
        self.relations.push(relation);
        self
    }

    /// Adds the custom [`Action`] `A` after the ones already declared.
    ///
    /// An `A::NAME` that is not a single URL path segment does not compile.
    #[must_use]
    pub fn action<A: Action<R>>(mut self) -> Self {
        self.actions = self.actions.add::<A>();
        self
    }

    /// Declares a column an overriding [`Resource::create_record`] sets itself, beyond the form's
    /// fields: `.create_column(Post::fields().slug())`. Call it once per column.
    ///
    /// Mounting the panel refuses a path that names no one field of the model, and the tenant
    /// column, which the framework stamps.
    #[must_use]
    pub fn create_column<T>(mut self, path: impl Into<Path<R::Model, T>>) -> Self {
        let path = ModelPath::of(&path.into());
        self.create_columns
            .push(model::field::<R::Model>(&path).map(|field| field.name));
        self
    }
}

/// A record's label, as [`ResourceDef::record_label`] declares it.
pub(crate) type RecordLabel<M> = Arc<dyn Fn(&M) -> String + Send + Sync>;

/// A record's public page, as [`ResourceDef::public_link`] declares it.
pub(crate) type PublicLinkFn<M> = Arc<dyn Fn(&M) -> Option<PublicLink> + Send + Sync>;

impl<R: Resource> std::fmt::Debug for ResourceDef<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceDef")
            .field("slug", &self.slug)
            .field("label", &self.label)
            .field("plural_label", &self.plural_label)
            .field("relations", &self.relations)
            .field("actions", &self.actions)
            .finish_non_exhaustive()
    }
}
