//! [`ResourceDef`]: everything a resource declares, as one value.

use std::sync::Arc;

use toasty::stmt::{Expr, Path};
use topcoat::{context::Cx, icon::IconData};

use super::{Action, Actions, HeaderAction, HeaderActions, Relation, Resource, SoftDelete};
use crate::{
    DeclarationErrorKind, Lens,
    detail::Detail,
    form::FormScalar,
    navigation::NavigationItem,
    policy::{Ability, Deny, Policy},
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
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, featured: bool }
/// # #[derive(Debug, Clone, tablo_core::RecordForm)]
/// # #[form(model = Post)]
/// # struct PostForm { title: String, featured: bool }
/// # struct PostResource;
/// use tablo_core::{ModelColumns, ReadOnly, Resource, ResourceDef, TernaryFilter, lens};
///
/// impl Resource for PostResource {
///     type Model = Post;
///     type Form = PostForm;
///
///     fn declare() -> ResourceDef<Self> {
///         ResourceDef::new()
///             .label("Blog post")
///             .policy(ReadOnly)
///             .table(Post::table().filters(TernaryFilter::new(lens!(Post.featured))))
///     }
/// }
/// ```
///
/// The panel builds the def once when it mounts, binding the paths it names to the database
/// schema, and serves the result to every request.
pub struct ResourceDef<R: Resource> {
    pub(crate) slug: Option<String>,
    pub(crate) label: Option<crate::Text>,
    pub(crate) plural_label: Option<crate::Text>,
    pub(crate) icon: Option<IconData>,
    pub(crate) navigation_order: Option<i32>,
    pub(crate) navigation_group: Option<crate::Text>,
    pub(crate) navigation: Option<NavigationItem>,
    pub(crate) navigation_badge: Option<Expr<bool>>,
    pub(crate) policy: Arc<dyn Policy<R::Model>>,
    pub(crate) tenancy: Tenancy<R::Model>,
    pub(crate) table: Option<Table<R::Model>>,
    pub(crate) form: Option<Schema<R::Form>>,
    pub(crate) detail: Option<Detail<R::Model>>,
    pub(crate) record_title: Option<RecordTitle<R::Model>>,
    /// `None` once [`Self::without_global_search`] leaves the resource out.
    pub(crate) global_search: Option<super::GlobalSearch<R::Model>>,
    pub(crate) public_link: Option<PublicLinkFn<R::Model>>,
    pub(crate) relations: Vec<Relation<R::Model>>,
    pub(crate) actions: Actions<R>,
    pub(crate) header_actions: HeaderActions,
    /// The columns [`Self::create_column`] names, or why a path names no column.
    pub(crate) create_columns: Vec<Result<String, DeclarationErrorKind>>,
    /// Whether [`Self::edit_only`] turned the create page off.
    pub(crate) edit_only: bool,
    /// Whether [`Self::importable`] turned the CSV import on.
    pub(crate) importable: bool,
    pub(crate) soft_delete: Option<SoftDelete<R::Model>>,
    pub(crate) field_rules: Vec<super::FieldRule>,
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
            navigation_badge: None,
            policy: Arc::new(Deny),
            tenancy: Tenancy::undeclared(),
            table: None,
            form: None,
            detail: None,
            record_title: None,
            global_search: Some(super::GlobalSearch::default()),
            public_link: None,
            relations: Vec::new(),
            actions: Actions::default(),
            header_actions: HeaderActions::new(),
            create_columns: Vec::new(),
            edit_only: false,
            importable: false,
            soft_delete: None,
            field_rules: Vec::new(),
        }
    }
}

impl<R: Resource> ResourceDef<R> {
    /// A def with every default: the policy denies all, rows belong to no tenant, the model's
    /// [`ModelColumns`](crate::ModelColumns) give the table and the detail page, and the record
    /// form gives the form.
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
    /// Defaults to the message `resource-{model}` of the panel's catalogs, else the model's type
    /// name in sentence case: `BlogPost` reads `resource-blog-post`, else `Blog post`, and
    /// `APIKey` reads `resource-api-key`, else `API key`.
    #[must_use]
    pub fn label(mut self, label: impl Into<crate::Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The list title and sidebar label.
    ///
    /// Defaults to the plural of the [`label`](Self::label): a plain label pluralized in English,
    /// `Category` reading `Categories` and `Person` `People`, and a message's attribute `plural`.
    /// The default label's message is `resource-{model}`, so its plural is
    /// `resource-{model}.plural`, else the derived English noun pluralized.
    ///
    /// ```ftl
    /// resource-blog-post = Article
    ///     .plural = Articles
    /// ```
    #[must_use]
    pub fn plural_label(mut self, label: impl Into<crate::Text>) -> Self {
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
    pub fn navigation_group(mut self, group: impl Into<crate::Text>) -> Self {
        self.navigation_group = Some(group.into());
        self
    }

    /// Replaces the sidebar entry, for one that links somewhere other than the list page:
    /// `NavigationItem::at("Drafts", "/admin/posts?f.status=draft")`. [`icon`](Self::icon),
    /// [`navigation_order`](Self::navigation_order),
    /// [`navigation_group`](Self::navigation_group) and
    /// [`navigation_badge`](Self::navigation_badge) still apply to it.
    #[must_use]
    pub fn navigation(mut self, item: NavigationItem) -> Self {
        self.navigation = Some(item);
        self
    }

    /// Badge the sidebar entry with the number of rows of [`scoped_query`](crate::scoped_query)
    /// that `filter` matches, so a tenant counts its own rows. The panel counts them each time its
    /// sidebar renders, on a page load and on each runtime rerun such as a sort or a page step,
    /// one query per badge run together, and shows no badge for zero. It replaces the
    /// badge of a [`navigation`](Self::navigation) item.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, draft: bool }
    /// # struct PostResource;
    /// # impl tablo_core::Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = tablo_core::NoForm<Post>;
    /// fn declare() -> tablo_core::ResourceDef<Self> {
    ///     // "Posts 3" while three drafts wait.
    ///     tablo_core::ResourceDef::new().navigation_badge(Post::fields().draft().eq(true))
    /// }
    /// # }
    /// ```
    #[must_use]
    pub fn navigation_badge(mut self, filter: Expr<bool>) -> Self {
        self.navigation_badge = Some(filter);
        self
    }

    /// Authorizes every handler; defaults to [`Deny`]. Row scoping belongs in
    /// [`Resource::query`], not the policy.
    #[must_use]
    pub fn policy(mut self, policy: impl Policy<R::Model>) -> Self {
        self.policy = Arc::new(policy);
        self
    }

    /// Authorizes every handler with a closure, as [`policy`](Self::policy) does, inferring the
    /// closure's argument types from the def's model:
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, locked: bool }
    /// # struct PostResource;
    /// # impl tablo_core::Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = tablo_core::NoForm<Post>;
    /// # }
    /// use tablo_core::{Ability, Panel};
    ///
    /// Panel::new("portal").resource_with::<PostResource>(|def| {
    ///     // No update, delete, restore or record action on a locked post.
    ///     def.policy_fn(|_cx, ability| ability.written_record().is_none_or(|post| !post.locked))
    /// });
    /// ```
    ///
    /// In [`Resource::declare`], the chain on `ResourceDef::new()` learns its resource from the
    /// return type, after the closure is checked, so a closure that reads the record names the
    /// ability's type, `|_cx, ability: Ability<'_, Post>|`, or the chain starts from
    /// `ResourceDef::<Self>::new()`.
    ///
    /// [`policy`](Self::policy) takes a closure too, but infers neither argument type: Rust infers
    /// a closure's signature from an `Fn` bound, not from [`Policy`]'s blanket impl.
    #[must_use]
    pub fn policy_fn(
        self,
        policy: impl Fn(&Cx, Ability<'_, R::Model>) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.policy(policy)
    }

    /// Deletes by stamping `lens`'s column with the time instead of removing the row, as in
    /// `.soft_delete(lens!(Post.deleted_at))`, and keeps the stamped records out of every loader
    /// unless it asks for them.
    ///
    /// The list hides the trashed records behind a trash control, and offers Restore, which
    /// empties the column, and Force delete, which removes the row, on each one: the policy decides
    /// who may through [`Ability::Restore`] and [`Ability::ForceDelete`]. The detail page opens a
    /// trashed record; its edit page and every other action refuse one until it is restored.
    /// [`scoped_query`](crate::scoped_query) admits the live records only, and
    /// [`scoped_query_in`](crate::scoped_query_in) the ones a [`Trashed`](crate::Trashed) scope
    /// names, each under the tenant filter.
    ///
    /// The unique pre-check probes the trashed records too, since a unique index covers them.
    /// Mounting the panel refuses a lens that is not one field of the model, and a record form
    /// that writes the column.
    #[must_use]
    pub fn soft_delete(mut self, lens: Lens<R::Model, Option<jiff::Timestamp>>) -> Self {
        self.soft_delete = Some(SoftDelete::new(lens));
        self
    }

    /// Restricts who views and who changes the field `path` names, as in
    /// `.field_access(lens!(Employee.salary), FieldAccess::new().view(is_hr))`: see
    /// [`FieldAccess`](super::FieldAccess).
    ///
    /// The rule restricts the field of its model within the panel: every resource the panel mounts
    /// over the model drops what reads a field the request may not view, the table's and the detail
    /// page's columns, the filters and the grouping, the export's columns, the searches of the list
    /// and of the relationship choices over those resources, and the relation columns showing it.
    /// Each form renders nothing for the field, or a disabled control for one it may view but not
    /// change, and refuses a submission posting either with 400. A create the field needs a value
    /// for, with no default, is refused.
    ///
    /// Hiding a `belongs_to` relation hides its foreign keys, and hiding a key hides the relation.
    /// Mounting the panel refuses a path that is not one field of the model, a second declaration
    /// over one field, a primary key with a `view` rule, a record title or a form condition reading
    /// a restricted field, and a column or a filter that does not say which fields it
    /// [`reads`](crate::Reads), and the declarations [`FieldAccessFault`](crate::FieldAccessFault)
    /// lists that would show a restricted record no panel can mask.
    ///
    /// A validation error on a field the request may not view shows as an alert that a hidden
    /// field refuses the save, without its message.
    #[must_use]
    pub fn field_access<T>(
        mut self,
        path: impl Into<toasty::stmt::Path<R::Model, T>>,
        access: super::FieldAccess,
    ) -> Self {
        self.field_rules
            .push(super::FieldRule::new(&path.into(), access));
        self
    }

    /// How rows belong to a tenant. A resource that declares none is served to every tenant, and
    /// mounting the panel refuses it when its model has a [`TenantId`](crate::TenantId) column.
    #[must_use]
    pub fn tenancy(mut self, tenancy: Tenancy<R::Model>) -> Self {
        self.tenancy = tenancy;
        self
    }

    /// The list table; defaults to the model's ([`ModelColumns::table`]).
    ///
    /// [`ModelColumns::table`]: crate::ModelColumns::table
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
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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

    /// The detail page: columns in layout blocks; defaults to the model's
    /// ([`ModelColumns::detail`]), and [`Detail::empty`] turns the page off: its route answers
    /// 404 and no row links to it. A non-empty detail adds a View action to each row.
    ///
    /// The page, at `{list}/{key}`, carries in its header the record's title, a link back to the
    /// list, the actions placed on [`Places::DETAIL`](crate::Places::DETAIL) and Delete as the
    /// policy allows them on the record, and Edit when the resource has a form and the policy
    /// allows `Update`. The [related tables](Self::relation) follow its columns.
    ///
    /// The record loads through the resource's tenant-scoped [`query`](Resource::query) with every
    /// relation the columns declare, once each, wherever its block sits; no other relation
    /// loads. An unknown key and a key outside the request's tenant are the same 404, and a
    /// record the policy refuses `View` is a 403. The [`form`](Self::form) does not shape the
    /// page: a resource that arranges its form declares a detail to show the same arrangement.
    ///
    /// [`ModelColumns::detail`]: crate::ModelColumns::detail
    #[must_use]
    pub fn detail(mut self, detail: Detail<R::Model>) -> Self {
        self.detail = Some(detail);
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
    ///
    /// ```rust,compile_fail,E0080
    /// # use tablo_core::{HeaderAction, NoForm, Resource, ResourceDef};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// # }
    /// struct RecountTags;
    ///
    /// impl HeaderAction for RecountTags {
    ///     type Input = ();
    ///     const NAME: &'static str = "recount/tags";
    ///
    ///     async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError> {
    ///         Ok(tablo_core::Outcome::done())
    ///     }
    /// }
    ///
    /// let _def = ResourceDef::<PostResource>::new().header_action::<RecountTags>();
    /// ```
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

    /// Serves the edit page but no create page, for records that arrive another way: an import,
    /// a seed, an action.
    ///
    /// A resource with a record form creates through it unless it opts out here; the policy
    /// decides who may create, per request, but never whether the page exists. Without the page,
    /// mounting skips the check that a create fills every non-nullable column, and the list and
    /// the relations offer no create button.
    #[must_use]
    pub fn edit_only(mut self) -> Self {
        self.edit_only = true;
        self
    }

    /// Serves a CSV import beside the create page: the list links **Import CSV** to a page that
    /// takes a file whose header names the form's keys, and creates one record per row through
    /// the create form, all in one transaction or none.
    ///
    /// Each row is checked as a create submission is, under the same policy, field access and
    /// tenant. Mounting refuses an importable resource that serves no create page.
    ///
    /// - The first row names the columns, each by its form key or its field's label in any case:
    ///   the controls the request may change, but file fields, repeaters and controls disabled on
    ///   the create page. A header naming any other column, or a label two fields carry, refuses
    ///   the file. **Download template** at `{list}/-/import/template` returns the header row.
    /// - Each cell posts what the create form's control would: an option's value, `true` or `false`
    ///   for a checkbox, a date and time in the request's time zone. A blank cell posts a blank
    ///   value, and a column the header leaves out takes the field's `default`. A multiple choice
    ///   takes one column per value, its key repeated; no other column may repeat.
    /// - The file is UTF-8 text with commas or semicolons between cells, of at most
    ///   [`MAX_IMPORT_ROWS`](crate::MAX_IMPORT_ROWS) rows, 500 cells a row and 10 MiB. A row whose
    ///   every cell is blank is skipped.
    /// - [`Resource::before_commit`] receives each row's [`Change::Create`](crate::Change::Create)
    ///   inside the transaction. A row [`Resource::create_record`] refuses rolls back to a
    ///   savepoint taken before it, so the rows after it are checked without its writes. When any
    ///   row is refused nothing is written, and the page lists each refusal by row number, as a
    ///   spreadsheet numbers it, and column. Once every row passes, the import lands on the list
    ///   and [`Resource::after_commit`] receives each row's change, in file order.
    ///
    /// The transaction lasts the whole file: on SQLite it holds the write lock that long. The
    /// import reads what the form posts, not what the export writes: an exported `$1,234.50`,
    /// `Yes` or option label does not import back.
    #[must_use]
    pub fn importable(mut self) -> Self {
        self.importable = true;
        self
    }
}

/// A record's title column, as [`ResourceDef::record_title`] declares it.
pub(crate) struct RecordTitle<M> {
    read: Box<dyn Fn(&M) -> String + Send + Sync>,
    search: TitleSearch,
    /// The model's root field the title reads.
    pub(crate) field: Option<String>,
}

/// The substring match on a title column for a search term.
type TitleSearch = Box<dyn Fn(&str) -> Option<Expr<bool>> + Send + Sync>;

impl<M: toasty::schema::Model + Send + Sync + 'static> RecordTitle<M> {
    fn new<T>(lens: Lens<M, T>) -> Self
    where
        T: FormScalar + toasty::schema::Field<Inner = String> + Send + Sync + 'static,
    {
        let path = lens.clone();
        let field = crate::toasty_compat::model::root_field::<M>(
            &crate::toasty_compat::model::ModelPath::of(lens.path()),
        );
        Self {
            read: Box::new(move |record| lens.read(record).to_label()),
            search: Box::new(move |term| contains_expr(&path, term)),
            field,
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

/// A record's public page, linked from its detail and edit pages.
#[derive(Debug, Clone)]
pub struct PublicLink {
    /// The public page's URL.
    pub url: String,
    /// The link's text.
    pub label: &'static str,
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
