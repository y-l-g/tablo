//! `Resource` — maps one Toasty [`Model`](toasty::schema::Model) to its admin UI.

use std::collections::HashMap;

use toasty::{
    Executor,
    stmt::{List, Query},
};
use topcoat::{Result, context::Cx};

use crate::{
    error::TabloError,
    form::{FieldErrors, Posted, RecordForm, write_create, write_update},
    policy::{Deny, Policy},
    tenancy::Tenancy,
};

mod action;
mod column;
mod commit;
mod declared;
mod filter;
pub(crate) mod naming;
mod navigation;
mod page;
mod relation;
mod state;
mod table;

pub(crate) use action::ActionEntry;
pub use action::{Action, Actions};
pub use column::{
    BooleanColumn, Column, ColumnWidth, ComputedColumn, Includes, IntoColumns, TextColumn,
};
pub(crate) use commit::run_after_commit;
pub use commit::{Committed, Mutation};
pub(crate) use declared::{Declarations, Declared, declared};
pub use filter::{
    DateFilter, Filter, FilterInput, IntoFilters, QueryFilter, SelectFilter, TernaryFilter,
};
use naming::{kebab_case, pluralize, type_short_name, type_stem};
pub(crate) use navigation::runtime_link;
pub use navigation::{NavTarget, NavigationItem};
pub use page::TablePage;
pub(crate) use page::{Past, row_exists_past};
pub(crate) use relation::BoundRelation;
pub use relation::{ForeignKey, Relation};
pub(crate) use state::{
    ACTION_ROUTE_PARAM, ACTIONS_ROUTE_SEGMENT, BULK_DELETE_ROUTE_SEGMENT, CREATE_ROUTE_SEGMENT,
    DELETE_ROUTE_SEGMENT, EDIT_ROUTE_SEGMENT, RECORD_ROUTE_PARAM, RETURN_PARAM, TableSignals,
    create_page_url, query_of, request_query, with_return,
};
pub use state::{Cursor, Sort, TableState};
pub use table::{DEFAULT_PAGE_SIZE, Table};
pub(crate) use table::{RowActions, TABLE_CARD_CLASS, TableAction, TableChrome};

#[cfg(test)]
pub(crate) use crate::query_term::MAX_QUERY_TERM;
pub(crate) use crate::query_term::clamp_query_term;

/// Maps one Toasty `Model` to its admin UI.
///
/// # Contract
///
/// Requires [`Model`](Self::Model) and [`Form`](Self::Form); every other item has a default. The
/// record form derives the [`table`](Self::table), the [`form`](Self::form) and the
/// [`view`](Self::view), so a resource overrides one only to arrange or extend it.
///
/// Declarations take no arguments: the panel calls each once when it mounts, binding the paths
/// they name to the database schema, and serves the result to every request. Record fns run
/// inside the handler transaction and fail without partial writes. Row chrome follows the
/// declarations; the default policy denies all.
///
/// [`table`]: Self::table
/// [`form`]: Self::form
/// [`view`]: Self::view
/// [`relations`]: Self::relations
/// [`Schema::declaration_errors`]: crate::schema::Schema::declaration_errors
pub trait Resource: Sized + Send + Sync + 'static {
    /// The persisted model this resource administers.
    type Model: toasty::schema::Model
        + toasty::stmt::IntoExpr<Self::Model>
        + Send
        + Sync
        + Clone
        + 'static;

    /// The typed value the create and edit forms parse into.
    ///
    /// A resource with create and edit pages names its
    /// [`#[derive(RecordForm)]`](crate::RecordForm) struct; a list-only resource
    /// names [`NoForm<Self::Model>`](crate::NoForm), and
    /// [`Panel::resource`](crate::Panel::resource) registers no form route for
    /// it ([`RecordForm::HAS_FORM`]).
    type Form: RecordForm<Model = Self::Model>;

    /// The columns an overriding [`Self::create_record`] sets itself, beyond
    /// the form's fields.
    const CREATE_COLUMNS: &'static [&'static str] = &[];

    /// Gates every handler; defaults to [`Deny`]. Row scoping belongs in
    /// [`query`](Self::query), not policy.
    fn policy() -> impl Policy<Self::Model> {
        Deny
    }

    /// Declares how rows belong to a tenant; defaults to unscoped.
    fn tenancy() -> Tenancy<Self::Model> {
        Tenancy::none()
    }

    /// Declares the read-only detail schema; defaults to the [`form`](Self::form), and empty
    /// disables the detail route.
    fn view() -> crate::schema::Schema {
        Self::form()
    }

    /// Renders free-form content below [`view`](Self::view) and above
    /// [`relations`](Self::relations).
    fn view_content<'a>(_cx: &'a Cx, _record: &Self::Model) -> Option<topcoat::view::BoxView<'a>> {
        None
    }

    /// Declares related resources rendered as narrowed tables on detail and
    /// edit pages.
    fn relations() -> Vec<Relation<Self::Model>> {
        Vec::new()
    }

    /// The resource's custom [`Action`]s, in button order. Defaults to none.
    ///
    /// ```text
    /// fn actions() -> Actions<Self> {
    ///     Actions::new().add::<Publish>()
    /// }
    /// ```
    fn actions() -> Actions<Self> {
        Actions::new()
    }

    /// The record's label in the detail page's title, or `None` when
    /// the record has no label to show.
    ///
    /// The detail page titles itself with this label when a resource returns
    /// `Some`, and with [`label`](Self::label) plus the URL's record key when
    /// it returns `None`, the default. The showcase's `PostResource` returns
    /// the post's title, so its heading reads the title instead of
    /// `Blog Post <record key>`.
    ///
    /// `cx` is the request's context — the same one
    /// [`view_values`](Self::view_values) receives — so a label
    /// can read request state (a locale, a tenant). The default ignores both
    /// arguments and returns `None`.
    ///
    /// A label is display text, not a key. Two records can share one (two
    /// users named Ada), so it cannot replace the primary key that keys the
    /// table's rows and the action routes.
    fn record_label(_cx: &Cx, _record: &Self::Model) -> Option<String> {
        None
    }

    /// A public URL for one record, rendered as a "View public post"-style
    /// link on the detail and edit pages when `Some`.
    ///
    /// The default declares none, so no link renders. A resource whose records
    /// have a public page overrides this with its URL — the showcase's posts
    /// return their `/blog/{id}` page.
    fn public_url(_cx: &Cx, _record: &Self::Model) -> Option<String> {
        None
    }

    /// The URL slug for this resource's pages, e.g. `"users"` mounts the list
    /// at `{panel prefix}/users`.
    ///
    /// Defaults to the Filament convention (`HasRoutes::resolveDefaultSlug`):
    /// take the resource type's name, strip a trailing `Resource`, pluralize
    /// (`UserResource` → `Users`, `CategoryResource` → `Categories`), then
    /// kebab-case (`BlogPostResource` → `blog-posts`). Override for irregular
    /// naming the rules cannot guess (`UsersResource` pluralizes to
    /// `userses` — name resources singular, or override).
    fn slug() -> String {
        kebab_case(&pluralize(type_stem::<Self>("Resource")))
    }

    /// One record's name, e.g. `"User"`: the noun in the "Create {label}" and
    /// "Edit {label}" titles.
    ///
    /// Defaults to the `Model` type name (Filament's model label). Override for
    /// custom wording; [`navigation_label`](Self::navigation_label) pluralizes
    /// it.
    fn label() -> String {
        type_short_name::<Self::Model>().to_string()
    }

    /// The sidebar label and list title, e.g. `"Users"`.
    ///
    /// Defaults to the pluralized [`label`](Self::label) (Filament's plural
    /// model label): `User` → `Users`, `Category` → `Categories`, `Person` →
    /// `People`. Override for a plural the rules cannot guess.
    fn navigation_label() -> String {
        pluralize(&Self::label())
    }

    /// Base query — the seam for a resource's **own** row scoping (ADR-0002):
    /// soft deletes and row-level visibility. Every loader starts from it.
    ///
    /// **Relations are not this method's job.** The list and the export load
    /// the relations the table's columns declare ([`ComputedColumn::include`]), and
    /// the detail page loads [`Self::view_query`]. Include a relation here only
    /// when a closure no column covers reads it on every loader's rows: one
    /// [`policy`](Self::policy) reads, or one a table's `group_by` or row
    /// key reads without a column including it.
    ///
    /// **Tenancy is not this method's job either.** For a resource whose
    /// [`tenancy`](Self::tenancy) is scoped the framework ANDs the tenant filter
    /// onto whatever this returns, at every loader through [`scoped_query`]. Do
    /// not re-state it here. Code outside the framework's loaders starts from
    /// [`scoped_query`].
    ///
    /// Returns the raw typed statement query, which composes generically —
    /// `filter`, `order_by` and `Paginate::new` work on the raw form for any
    /// `M: Model` — so one panel handler drives every resource's list page.
    ///
    /// # Keep unique constraints in step with this scope
    ///
    /// The app-side unique pre-check probes through the same tenant-scoped
    /// query, so a `#[unique]` index *broader* than the scope is invisible: the
    /// probe misses the colliding row and the user gets a 500 instead of the
    /// inline "has already been taken". Scope the constraint to match —
    /// `#[unique(tenant_id, email)]`. Not checkable at declaration time, since
    /// a query's filters are not introspectable; upstream #117 is the fix.
    fn query(_cx: &Cx) -> toasty::stmt::Query<List<Self::Model>> {
        toasty::stmt::Query::<List<Self::Model>>::all()
    }

    /// The detail page's query: [`Self::query`] plus the relations the page
    /// reads off the loaded row — in [`view_values`](Self::view_values) or
    /// [`view_content`](Self::view_content) — so include them here. A
    /// [`relation`](Self::relations) table runs its own query and needs none.
    ///
    /// ```text
    /// fn view_query(cx: &Cx) -> Query<List<Post>> {
    ///     let author: Include<Post, Author> = Post::fields().author().into();
    ///     Self::query(cx).include(author)
    /// }
    /// ```
    ///
    /// The default is [`Self::query`] unchanged. The framework ANDs the tenant
    /// scope onto it, as it does onto [`Self::query`].
    fn view_query(cx: &Cx) -> toasty::stmt::Query<List<Self::Model>> {
        Self::query(cx)
    }

    /// Description of the list view.
    ///
    /// Defaults to the record form's derived table ([`RecordForm::table`]): one column per
    /// field a column can show. Override it to choose the columns, or to extend the derived
    /// table with filters or grouping:
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, featured: bool }
    /// # #[derive(Debug, Clone, tablo_core::RecordForm)]
    /// # #[form(model = Post)]
    /// # struct PostForm { title: String, featured: bool }
    /// # use tablo_core::{Table, TernaryFilter};
    /// fn table() -> Table<Post> {
    ///     PostForm::table().filters(TernaryFilter::new(Post::fields().featured()))
    /// }
    /// ```
    ///
    /// What depends on the request — which rows a user may act on — is the policies' business,
    /// which the panel wires per request.
    fn table() -> Table<Self::Model> {
        <Self::Form as RecordForm>::table()
    }

    /// The schema the create and edit forms render.
    ///
    /// Defaults to the record form's derived schema
    /// ([`RecordForm::schema`]): one control per field, in declaration order.
    /// Override it to arrange the controls into a layout — the derive's
    /// `controls()` hands each one over, ready for its modifiers:
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Post {
    /// #     #[key] #[auto] id: uuid::Uuid,
    /// #     title: String,
    /// #     body: String,
    /// # }
    /// # #[derive(Debug, Clone, tablo_core::RecordForm)]
    /// # #[form(model = Post)]
    /// # struct PostForm { title: String, body: String }
    /// # use tablo_core::{Schema, Section};
    /// fn form() -> Schema {
    ///     let c = PostForm::controls();
    ///     Schema::new(Section::new("Content").schema((c.title, c.body.multiline(6))))
    /// }
    /// ```
    ///
    /// The panel refuses a record form field this schema does not declare, and a schema on a
    /// resource whose [`Form`](Self::Form) is [`NoForm`](crate::NoForm).
    fn form() -> crate::schema::Schema {
        <Self::Form as RecordForm>::schema()
    }

    /// App-level rules on the parsed form. The errors render inline with a
    /// 200 and nothing is written; a record fn error keeps its own mapping, so
    /// a range or cross-field rule belongs here.
    ///
    /// Each error names the key it renders under: a control's own key, or a
    /// [`Repeater`](crate::Repeater) group's label. A key the submitted form
    /// renders nowhere fails the submit as a declaration error instead of
    /// writing past the rule.
    fn validate_record(_cx: &Cx, _form: &Self::Form) -> FieldErrors {
        FieldErrors::new()
    }

    /// Create a record from the parsed form, inside the handler's transaction.
    ///
    /// Defaults to the derived write, [`write_create`]; override to check
    /// something inside the transaction, then delegate. An override on a
    /// [`Tenancy::column`](crate::Tenancy::column) resource stamps the tenant
    /// only by delegating to [`write_create`]. `ex` is the open
    /// transaction: run every statement through it. Return the created row; it
    /// is what [`Self::after_commit`] receives.
    fn create_record(
        cx: &Cx,
        form: Self::Form,
        ex: &mut dyn Executor,
    ) -> impl Future<Output = Result<Self::Model>> + Send {
        write_create::<Self>(cx, form, ex)
    }

    /// Update the already-authorized `record` from the posted form, inside the
    /// handler's transaction.
    ///
    /// Defaults to the derived write, [`write_update`]. `record` is the
    /// snapshot the handler loaded and policy-checked inside the transaction:
    /// use it, never re-query. Return the row as it now stands.
    fn update_record(
        cx: &Cx,
        record: Self::Model,
        posted: Posted<Self::Form>,
        ex: &mut dyn Executor,
    ) -> impl Future<Output = Result<Self::Model>> + Send {
        write_update::<Self>(cx, record, posted, ex)
    }

    /// Sidebar entry for the resource.
    ///
    /// The default declares a label ([`Self::navigation_label`]) and no URL:
    /// the Panel that owns the resource resolves where it is mounted, so this
    /// entry never links at a mount the resource guessed.
    ///
    /// Override to curate this resource's sidebar entry: `Panel::resource`
    /// consumes the result through the panel-aware navigation seam, so a custom
    /// `order`, label or icon takes effect. Decorate the default with
    /// [`NavigationItem::for_resource`]
    /// (`NavigationItem { order: -1, ..NavigationItem::for_resource::<Self>() }`,
    /// or `NavigationItem::for_resource::<Self>().icon(..)`)
    /// to keep the panel-owned URL; spell a URL out yourself
    /// ([`NavigationItem::at`]) only to link somewhere other than this
    /// resource's list page — the Panel keeps such a URL verbatim.
    fn navigation() -> NavigationItem {
        NavigationItem::for_resource::<Self>()
    }

    /// Delete the already-authorized `record` (#86).
    ///
    /// The handler loads `record` through the tenancy-scoped query inside the
    /// framework transaction and checks the policy on that snapshot, then
    /// calls this with the same transaction as `ex`. The after-commit hook
    /// receives the snapshot once the delete commits.
    ///
    /// The default deletes the row through [`scoped_query`], filtered to the
    /// record's primary key. Override to delete another way, such as a soft
    /// delete.
    fn delete_record(
        cx: &Cx,
        record: &Self::Model,
        ex: &mut dyn toasty::Executor,
    ) -> impl std::future::Future<Output = Result<()>> + Send
    where
        Self: Sized,
    {
        let filter = crate::toasty_compat::pk::pk_filter(record);
        let query = scoped_query::<Self>(cx).map(|query| query.filter(filter));
        async move {
            query?
                .delete()
                .exec(ex)
                .await
                .map_err(|error| -> topcoat::Error { error.into() })?;
            Ok(())
        }
    }

    /// Bulk-delete the already-authorized `records`: the handler
    /// fetches through the tenancy-scoped `IN` query inside the framework
    /// transaction and checks the policy on every row before calling this.
    /// The default deletes each record through [`Self::delete_record`] in
    /// order, through the same `ex` — any error rolls the whole batch back,
    /// so mid-loop failures delete zero rows. An override of `delete_record`,
    /// such as a soft delete, therefore covers bulk delete too. Override this
    /// for a single-statement batch.
    fn bulk_delete_records(
        cx: &Cx,
        records: &[Self::Model],
        ex: &mut dyn toasty::Executor,
    ) -> impl std::future::Future<Output = Result<()>> + Send
    where
        Self: Sized,
    {
        async move {
            for record in records {
                Self::delete_record(cx, record, &mut *ex).await?;
            }
            Ok(())
        }
    }

    /// Post-commit work for a mutation this resource committed.
    ///
    /// The place for a side effect that must not survive a rollback: an email, a
    /// webhook, an audit row, cache invalidation. Called once per successful
    /// write, after `tx.commit()` and before the response — running it inside a
    /// record fn would leak the effect on rollback, and the write handlers' pool
    /// discipline forbids a second handle while the transaction is open.
    ///
    /// [`Committed`] names the mutation and the rows it wrote: the committed row
    /// a create or update returned, the rows a delete or bulk delete removed
    /// (one call with every row). It is never called when nothing committed: a
    /// validation error, a policy denial, a failed record fn, or a failed commit
    /// all leave the hook untouched, so a rollback cannot produce the effect. A
    /// hook that returns `Err` is logged and ignored — the write is committed,
    /// and retries are the app's to build; a panic surfaces as Topcoat's
    /// panic-isolated 500.
    ///
    /// ```text
    /// async fn after_commit(cx: &Cx, committed: Committed<Post>) -> Result<()> {
    ///     let mut db = db(cx); // a fresh handle is allowed here
    ///     for post in committed.records() { notify(post).await?; }
    ///     Ok(())
    /// }
    /// ```
    fn after_commit(
        _cx: &Cx,
        _committed: Committed<Self::Model>,
    ) -> impl std::future::Future<Output = Result<()>> + Send
    where
        Self: Sized,
    {
        async move { Ok(()) }
    }

    /// The record's values for the detail page, keyed by the name each
    /// [`view`](Self::view) field binds.
    ///
    /// The detail page reads the form's keys from
    /// [`RecordForm::hydrate`], and this adds any
    /// key only the view shows; the form's keys win on a collision. A
    /// resource with no form ([`NoForm`](crate::NoForm)) supplies every key its
    /// view shows here.
    ///
    /// `cx` carries the database, whose schema an embedded value's keys need
    /// ([`EmbeddedForm::write_form`](crate::schema::EmbeddedForm::write_form)).
    /// The default is empty.
    fn view_values(_cx: &Cx, _record: &Self::Model) -> HashMap<String, String> {
        HashMap::new()
    }
}

/// The tenant-scoped base query.
///
/// [`Resource::query`] with the [`Resource::tenancy`] filter ANDed onto it, so a
/// resource that overrides `query` for soft deletes cannot drop the tenant
/// scope by forgetting to re-state it. Every framework loader and app code
/// start here.
///
/// # Errors
///
/// A tenant-scoped resource and no tenant in `cx`: 403, the same answer the
/// handler gate gives.
///
/// App code that loads rows itself must call this: on a scoped resource
/// [`Resource::query`] is the *tenant-unscoped* base by design.
pub fn scoped_query<R: Resource>(cx: &Cx) -> Result<Query<List<R::Model>>> {
    apply_tenant_scope::<R>(cx, R::query(cx))
}

/// [`Resource::view_query`] under the same tenant gate and filter as
/// [`scoped_query`]: the detail page's loader, and the entry point for a page
/// that owns its own detail view.
///
/// # Errors
///
/// The same as [`scoped_query`].
pub fn scoped_view_query<R: Resource>(cx: &Cx) -> Result<Query<List<R::Model>>> {
    apply_tenant_scope::<R>(cx, R::view_query(cx))
}

/// AND the resource's tenant filter onto `query`: the body of
/// [`scoped_query`] and [`scoped_view_query`].
fn apply_tenant_scope<R: Resource>(
    cx: &Cx,
    query: Query<List<R::Model>>,
) -> Result<Query<List<R::Model>>> {
    let tenancy = R::tenancy();
    if !tenancy.is_scoped() {
        return Ok(query);
    }
    if let Some(Err(error)) = tenancy.column_field() {
        return Err(TabloError::Declaration(format!(
            "resource `{}`'s `Tenancy::column` lens binds no column: {error}",
            std::any::type_name::<R>(),
        ))
        .into());
    }
    let tenant = crate::tenancy::require_tenant(cx)?;
    Ok(match tenancy.filter(tenant) {
        Some(filter) => query.filter(filter),
        None => query,
    })
}

/// Every `Resource` is an [`OptionSource`](crate::schema::OptionSource): the
/// bridge that lets the relationship option loaders be generic over the source
/// surface instead of over `Resource`, so `schema` does not depend on
/// `resource`.
///
/// [`scoped_query`](crate::schema::OptionSource::scoped_query) forwards to
/// [`scoped_query`], so an option load inherits the tenant gate and filter
/// exactly as every other loader does. The policy and the tenancy forward
/// unchanged, and the search expression and default ordering come from the
/// resource's declared [`table`](Resource::table), which is where "the option
/// search searches the related resource's searchable columns" lives.
impl<R: Resource> crate::schema::OptionSource for R {
    type Model = R::Model;

    fn scoped_query(cx: &Cx) -> Result<Query<List<R::Model>>> {
        scoped_query::<R>(cx)
    }

    fn policy() -> impl Policy<R::Model> {
        <R as Resource>::policy()
    }

    fn requires_tenant() -> bool {
        <R as Resource>::tenancy().is_scoped()
    }

    fn slug() -> String {
        <R as Resource>::slug()
    }

    fn search_expr(cx: &Cx, term: &str) -> Option<toasty::stmt::Expr<bool>> {
        declared::<R>(cx).table.search_expr(term)
    }

    fn order_by(cx: &Cx) -> Option<toasty::stmt::OrderByExpr> {
        declared::<R>(cx).table.order_by(false)
    }
}

#[cfg(test)]
mod tests;
