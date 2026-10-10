//! [`ResourceDef`]: everything a resource declares, as one value.

use std::sync::Arc;

use toasty::stmt::{Expr, Path};
use topcoat::{context::Cx, icon::IconData};

use super::{Action, Actions, HeaderAction, HeaderActions, PublicLink, Relation, Resource};
use crate::{
    DeclarationErrorKind, Lens,
    detail::Detail,
    form::FormScalar,
    navigation::NavigationItem,
    policy::{Deny, Policy},
    schema::Schema,
    table::{Table, contains_expr},
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
/// use tablo_core::{ReadOnly, RecordForm, Resource, ResourceDef, TernaryFilter, lens};
///
/// impl Resource for PostResource {
///     type Model = Post;
///     type Form = PostForm;
///
///     fn declare() -> ResourceDef<Self> {
///         ResourceDef::new()
///             .label("Blog post")
///             .policy(ReadOnly)
///             .table(PostForm::table().filters(TernaryFilter::new(lens!(Post.featured))))
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
    pub(crate) navigation_order: Option<i32>,
    pub(crate) navigation_group: Option<String>,
    pub(crate) navigation: Option<NavigationItem>,
    pub(crate) policy: Arc<dyn Policy<R::Model>>,
    pub(crate) tenancy: Tenancy<R::Model>,
    pub(crate) table: Option<Table<R::Model>>,
    pub(crate) form: Option<Schema<R::Form>>,
    pub(crate) view: Option<Detail<R::Model>>,
    pub(crate) record_title: Option<RecordTitle<R::Model>>,
    pub(crate) public_link: Option<PublicLinkFn<R::Model>>,
    pub(crate) relations: Vec<Relation<R::Model>>,
    pub(crate) actions: Actions<R>,
    pub(crate) header_actions: HeaderActions,
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
            navigation_order: None,
            navigation_group: None,
            navigation: None,
            policy: Arc::new(Deny),
            tenancy: Tenancy::none(),
            table: None,
            form: None,
            view: None,
            record_title: None,
            public_link: None,
            relations: Vec::new(),
            actions: Actions::default(),
            header_actions: HeaderActions::new(),
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
    /// Defaults to the model's type name in sentence case: `BlogPost` → `Blog post`, `APIKey` →
    /// `API key`.
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

    /// The icon of the resource's sidebar entry, replacing the icon of a
    /// [`navigation`](Self::navigation) item.
    #[must_use]
    pub fn icon(mut self, icon: IconData) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Where the sidebar entry sorts among the panel's entries, lowest first; defaults to `0`, and
    /// entries of equal order keep their registration order. It replaces the order of a
    /// [`navigation`](Self::navigation) item.
    #[must_use]
    pub fn navigation_order(mut self, order: i32) -> Self {
        self.navigation_order = Some(order);
        self
    }

    /// The labelled sidebar group the entry renders in, such as `"Content"`; defaults to none,
    /// above every group. It replaces the group of a [`navigation`](Self::navigation) item.
    #[must_use]
    pub fn navigation_group(mut self, group: impl Into<String>) -> Self {
        self.navigation_group = Some(group.into());
        self
    }

    /// Replaces the sidebar entry, for one that links somewhere other than the list page:
    /// `NavigationItem::at("Drafts", "/admin/posts?f.status=draft")`. [`icon`](Self::icon),
    /// [`navigation_order`](Self::navigation_order) and
    /// [`navigation_group`](Self::navigation_group) still apply to it.
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

    /// Arranges the create and edit forms: the record form's own controls, from its
    /// `controls()`, in layout blocks.
    ///
    /// A control the schema does not place follows the ones it does, in the record form's
    /// declaration order and outside every layout block, so the default form renders one control
    /// per field, and a form that adjusts one control places only that one:
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct User { #[key] #[auto] id: uuid::Uuid, name: String, email: String }
    /// # #[derive(Debug, Clone, tablo_core::RecordForm)]
    /// # #[form(model = User)]
    /// # struct UserForm { name: String, email: String }
    /// # struct UserResource;
    /// use tablo_core::{Resource, ResourceDef, Schema};
    ///
    /// impl Resource for UserResource {
    ///     type Model = User;
    ///     type Form = UserForm;
    ///
    ///     fn declare() -> ResourceDef<Self> {
    ///         // `email`, then `name`.
    ///         ResourceDef::new().form(Schema::new(UserForm::controls().email.email()))
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn form(mut self, form: Schema<R::Form>) -> Self {
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

    /// Titles each record with the text column `lens` reads, as in
    /// `.record_title(lens!(Post.title))`: the detail page's heading, each option of a
    /// [`relationship`](crate::ChoiceField::relationship) choice over this resource, and each cell
    /// of a [`RelationColumn::of`](crate::RelationColumn::of) it.
    ///
    /// A record whose column is empty, and every record by default, is titled with the
    /// [`label`](Self::label) and its key, as in `Blog Post 3f2a…`. A relationship choice over this
    /// resource also searches the column, besides the table's searchable columns.
    ///
    /// A title is display text, not a key: two records can share one, so it never replaces the
    /// primary key that keys the table's rows and the action routes. A lens names one of the
    /// model's own columns, never a relation, so the title reads on every record however it was
    /// loaded; a title built from several columns is a column the model stores.
    #[must_use]
    pub fn record_title<T>(mut self, lens: Lens<R::Model, T>) -> Self
    where
        T: FormScalar + toasty::schema::Field<Inner = String> + Send + Sync + 'static,
    {
        self.record_title = Some(RecordTitle::new(lens));
        self
    }

    /// Links a record's public page from its detail and edit pages, for each record `link`
    /// returns one for; defaults to no link.
    ///
    /// The edit page loads the record without relations, so a link that reads one renders on the
    /// detail page only when its relation is loaded there.
    #[must_use]
    pub fn public_link(
        mut self,
        link: impl Fn(&Cx, &R::Model) -> Option<PublicLink> + Send + Sync + 'static,
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
    /// The policy decides who may run it, through
    /// [`Ability::RunAny`](crate::Ability::RunAny) and [`Ability::Run`](crate::Ability::Run). An
    /// `A::NAME` that is not a single URL path segment does not compile.
    #[must_use]
    pub fn action<A: Action<R>>(mut self) -> Self {
        self.actions = self.actions.add::<A>();
        self
    }

    /// Adds the [`HeaderAction`] `A`, which acts on no record, to the list page's header, after
    /// the ones already declared.
    ///
    /// The policy decides who may run it, through [`Ability::ViewAny`](crate::Ability::ViewAny)
    /// and [`Ability::RunHeader`](crate::Ability::RunHeader) with its name, which no other action
    /// of the resource may share. An `A::NAME` that is not
    /// a single URL path segment does not compile.
    #[must_use]
    pub fn header_action<A: HeaderAction>(mut self) -> Self {
        self.header_actions = self.header_actions.add::<A>();
        self
    }

    /// Declares a column an overriding [`Resource::create_record`] sets itself, beyond the form's
    /// fields: `.create_column(lens!(Post.slug))`. Call it once per column.
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

/// A record's title column, as [`ResourceDef::record_title`] declares it.
pub(crate) struct RecordTitle<M> {
    read: Box<dyn Fn(&M) -> String + Send + Sync>,
    search: TitleSearch,
}

/// The substring match on a title column for a search term.
type TitleSearch = Box<dyn Fn(&str) -> Option<Expr<bool>> + Send + Sync>;

impl<M: toasty::schema::Model + Send + Sync + 'static> RecordTitle<M> {
    fn new<T>(lens: Lens<M, T>) -> Self
    where
        T: FormScalar + toasty::schema::Field<Inner = String> + Send + Sync + 'static,
    {
        let path = lens.clone();
        Self {
            read: Box::new(move |record| lens.read(record).to_label()),
            search: Box::new(move |term| contains_expr(&path, term)),
        }
    }

    /// The column's value on `record`, or `None` when it is empty.
    pub(crate) fn read(&self, record: &M) -> Option<String> {
        Some((self.read)(record)).filter(|title| !title.trim().is_empty())
    }

    /// The substring match on the column for `term`, or `None` for a blank term.
    pub(crate) fn search_expr(&self, term: &str) -> Option<Expr<bool>> {
        (self.search)(term)
    }
}

/// A record's public page, as [`ResourceDef::public_link`] declares it.
pub(crate) type PublicLinkFn<M> = Arc<dyn Fn(&Cx, &M) -> Option<PublicLink> + Send + Sync>;

impl<R: Resource> std::fmt::Debug for ResourceDef<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceDef")
            .field("slug", &self.slug)
            .field("label", &self.label)
            .field("plural_label", &self.plural_label)
            .field("relations", &self.relations)
            .field("actions", &self.actions)
            .field("header_actions", &self.header_actions)
            .finish_non_exhaustive()
    }
}
