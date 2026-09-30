//! `Resource` — maps one Toasty [`Model`](toasty::schema::Model) to its admin UI.
//!
//! One `Model` → one `Resource`. The trait is the single seam for query
//! scoping (`query`), the form and table declarations, and navigation. See
//! `CONTEXT.md` and ADR-0002.
//!
//! Facade over the cohesive submodules split out in GH #133: `filter`,
//! `column`, `state`, `table` (+ `table::render`/`table::export`),
//! `navigation`, and `naming`. The re-export surface is unchanged.

use std::collections::HashMap;

use toasty::{
    Executor,
    stmt::{List, Query},
};
use topcoat::{Result, context::Cx};

use crate::form::{FieldErrors, Posted, RecordForm, write_create, write_update};

mod column;
mod commit;
mod filter;
mod naming;
mod navigation;
mod page;
mod relation;
mod state;
mod table;

pub use column::{ColumnWidth, IntoColumns, TextColumn};
pub(crate) use commit::run_after_commit;
pub use commit::{Committed, Mutation};
pub use filter::{DateFilter, Filter, IntoFilters, SelectFilter, TernaryFilter, VariantFilter};
use naming::{kebab_case, pluralize, type_short_name};
pub(crate) use navigation::runtime_link;
pub use navigation::{NavTarget, NavigationItem};
pub use page::TablePage;
pub(crate) use page::{Past, row_exists_past};
pub use relation::{
    IntoRelationColumns, MAX_RELATION_ROWS, RelationColumn, RelationColumns, render_relation,
};
pub(crate) use state::{
    BULK_DELETE_ROUTE_SEGMENT, CREATE_ROUTE_SEGMENT, DELETE_ROUTE_SEGMENT, EDIT_ROUTE_SEGMENT,
    RECORD_ROUTE_PARAM, create_page_url, cursor_after, cursor_before, cursor_none,
};
pub use state::{Sort, TableSignals, TableState};
pub use table::{DEFAULT_PAGE_SIZE, GroupKey, RowKey, Table};
pub(crate) use table::{RowActions, TableChrome};

#[cfg(test)]
pub(crate) use crate::query_term::MAX_QUERY_TERM;
pub(crate) use crate::query_term::clamp_query_term;

/// Maps one Toasty `Model` to its admin UI.
///
/// # Contract
///
/// **Every item but [`Model`](Self::Model), [`Form`](Self::Form) and
/// [`table`](Self::table) has a default**, so a resource compiles as soon as it
/// declares its model, its record form (or [`NoForm`](crate::NoForm)) and its
/// list view — and an omission must fail loudly rather than silently:
///
/// - **Checked at [`Panel::build`](crate::panel::Panel::build)**: the declared table must serve a
///   list, and [`form`](Self::form) must agree with [`Form`](Self::Form): a record form's fields
///   are the schema's controls, and a [`NoForm`](crate::NoForm) resource declares no schema. A
///   resource with no form must not allow [`can_create`](Self::can_create). These are declarations,
///   checked with a Db-only context.
/// - **Loud at request time**: [`delete_record`](Self::delete_record) defaults to an error naming
///   the type, so a resource that never implemented delete says so instead of writing nothing
///   quietly. [`bulk_delete_records`](Self::bulk_delete_records) loops `delete_record` by default,
///   so it stays loud through the same stub.
/// - **Chrome follows the declarations**: the row Delete control and the bulk column render when
///   [`can_delete_any`](Self::can_delete_any) allows, the Edit link when the resource has a record
///   form, and the View link when it declares [`view`](Self::view); the row predicates then gate
///   each row.
/// - **Default-deny is untouched**: every `can_*` defaults to `false`, except `can_delete`, which
///   defaults to `can_delete_any`; an unconfigured resource exposes no data and no mutation.
pub trait Resource: Sized + Send + Sync + 'static {
    /// The persisted model this resource administers.
    ///
    /// `Send + Sync` holds for every data-only model struct and is required
    /// for concurrent rendering of the resource's pages.
    ///
    /// `Clone` is part of the contract because a committed mutation names its
    /// rows: a handler keeps a copy of the rows it loaded while the
    /// record fn consumes them, so the hook can be handed what was written.
    type Model: toasty::schema::Model + Send + Sync + Clone + 'static;

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
    ///
    /// [`Panel::build`](crate::Panel::build) refuses a resource that allows
    /// create when a non-nullable column is neither a form field, nor filled by
    /// toasty (`#[auto]`, `#[default(..)]`), nor the stamped tenant column: the
    /// create would fail at the driver on every submit. A record fn that sets
    /// such a column by hand names it here.
    const CREATE_COLUMNS: &'static [&'static str] = &[];

    /// Whether the current user may view the list page.
    ///
    /// Also gates relationship option loads: a related resource
    /// that denies this cannot offer its records as options at all.
    fn can_view_any(_cx: &Cx) -> bool {
        false
    }

    /// Whether the current user may view the given record.
    ///
    /// Checked on the edit page (GET), the edit POST (which requires both
    /// `can_view` and `can_update`), per row in CSV export, on each
    /// record behind a relationship `Select`'s options, on each row
    /// of a detail page's relation table, and on the list page as
    /// the per-row gate of every action link (GH #235: the View link, and the
    /// `can_view` half of Edit and Delete). Note both hooks default-deny: a
    /// resource used as a relationship target for option loads must allow
    /// `can_view_any` **and** `can_view` (overriding one does not imply the
    /// other), while a relation table consults `can_view` alone. The
    /// list page deliberately checks only
    /// `can_view_any` for *membership*: `can_view` is an in-memory Rust
    /// predicate that cannot run in SQL, and filtering rows after cursor
    /// pagination would mislabel pages. Row-level visibility that must hold on
    /// the list belongs in [`Self::query`].
    fn can_view(_cx: &Cx, _record: &Self::Model) -> bool {
        false
    }

    /// Whether the current user may create a new record.
    ///
    /// `Panel::build` calls this with a Db-only context to decide which
    /// declaration checks apply, so a predicate that reads the request (a
    /// tenant, a user) answers as it would for an anonymous request there. The
    /// list page links to the create page only for a resource with a record
    /// form ([`RecordForm::HAS_FORM`]), whatever this answers.
    fn can_create(_cx: &Cx) -> bool {
        false
    }

    /// Whether the current user may update the given record.
    fn can_update(_cx: &Cx, _record: &Self::Model) -> bool {
        false
    }

    /// Whether the current user may delete records of this resource at all.
    ///
    /// Decides whether the list renders the row Delete control, the bulk
    /// column, and the bulk bar, and gates the single-delete and bulk-delete
    /// POSTs before any record loads. The column decision takes no record, so
    /// the streamed skeleton and the table agree on their columns. The default
    /// [`can_delete`](Self::can_delete) also calls it once per row.
    fn can_delete_any(_cx: &Cx) -> bool {
        false
    }

    /// Whether the current user may delete the given record.
    ///
    /// Defaults to [`can_delete_any`](Self::can_delete_any); override to
    /// refuse some rows. Checked on the single-delete and bulk-delete POSTs
    /// after `can_delete_any` and together with `can_view` — the edit
    /// contract: a record that cannot be viewed cannot be deleted by
    /// UUID-guessing the route. A row it refuses renders no Delete control and
    /// no bulk checkbox.
    fn can_delete(cx: &Cx, _record: &Self::Model) -> bool {
        Self::can_delete_any(cx)
    }

    /// How one record is displayed on the detail page, read-only.
    ///
    /// The same [`Schema`](crate::schema::Schema) a form uses, rendered for reading: a `TextInput`
    /// shows its stored value instead of an `<input>`, a `Select` shows the
    /// option label the form offered, and a layout block keeps the structure it
    /// declares. Declaring a view is what turns the detail page on — the default
    /// declares nothing, so the route 404s and no `View` row action renders.
    ///
    /// Values come from [`view_values`](Self::view_values). A
    /// relation is not one of these fields — it is a list of records, not a
    /// string — and renders through [`view_relations`](Self::view_relations).
    ///
    /// Read-only is a promise, not a disabled form: nothing here validates or
    /// submits, and no field renders a required marker or an error slot.
    fn view(_cx: &Cx) -> crate::schema::Schema {
        crate::schema::Schema::empty()
    }

    /// The related records on this resource's detail page.
    ///
    /// [`view`](Self::view) renders from the record's *string projection* — one
    /// `HashMap<String, String>` — because that is what every field binds. A
    /// relation is not a string and may be a list of records, so it cannot ride
    /// that map; this hook renders it instead, with the loaded record in hand.
    ///
    /// This is the half that makes [`Self::query`]'s `include` pay: the related
    /// rows are already loaded on the record, so a hook that reads
    /// `record.comments.get()` issues no query at all. Touching an un-included
    /// relation panics ([`Deferred::get`](toasty::Deferred)), so check
    /// [`Deferred::is_unloaded`](toasty::Deferred::is_unloaded) — the same marker
    /// the list columns check.
    ///
    /// Returns `None` (the default) for a resource with no related records to
    /// show, which renders nothing. The two lifetimes are deliberately separate:
    /// the returned view may borrow the request context, never the record — a
    /// view holding the record would pin the handler's local binding for as long
    /// as the page, which does not compile, and the projections return owned
    /// strings ([`render_relation`]), so nothing needs to.
    fn view_relations<'a>(
        _cx: &'a Cx,
        _record: &Self::Model,
    ) -> Option<topcoat::view::BoxView<'a>> {
        None
    }

    /// Whether this resource declares a detail page.
    ///
    /// Derived from [`view`](Self::view) rather than declared twice, so the
    /// route and the row link cannot disagree with the schema that renders
    /// them. The detail handler uses it to 404 a resource that declares
    /// nothing, and the row chrome uses it to leave the link off.
    fn viewed(cx: &Cx) -> bool {
        !Self::view(cx).is_empty()
    }

    /// The record's label in the detail page's title, or `None` when
    /// the record has no label to show.
    ///
    /// The detail page titles itself with this label when a resource returns
    /// `Some`, and with [`navigation_label`](Self::navigation_label) plus the
    /// URL's record key when it returns `None` — the default, so a resource
    /// that declares nothing keeps the title it has. The showcase's
    /// `PostResource` is the worked example: it returns the post's title, so
    /// its heading reads the title instead of `Blog Posts <record key>`.
    ///
    /// `cx` is the request's context — the same one [`view`](Self::view) and
    /// [`view_values`](Self::view_values) receive — so a label
    /// can read request state (a locale, a tenant). The default ignores both
    /// arguments and returns `None`.
    ///
    /// A label is display text, not a key. Two records can share one (two
    /// users named Ada), so it cannot replace the table key, whose projection
    /// must stay injective within a page for keyed diffs and bulk selection,
    /// or the record key, which the action routes resolve as the
    /// model's typed PK.
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
        let name = type_short_name::<Self>();
        let singular = name.strip_suffix("Resource").unwrap_or(name);
        kebab_case(&pluralize(singular))
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
    /// the relations the table's columns declare ([`TextColumn::include`]), and
    /// the detail page loads [`Self::view_query`]. Include a relation here only
    /// when a closure no column covers reads it on every loader's rows: one
    /// [`can_view`](Self::can_view) reads, or one a table's `group_by` or row
    /// key reads without a column including it.
    ///
    /// **Tenancy is not this method's job either.** When
    /// [`requires_tenant`](Self::requires_tenant) is `true` the framework ANDs
    /// the tenant filter, derived from the model's `tenant_id` column, onto
    /// whatever this returns, at every loader through [`scoped_query`]. Do not
    /// re-state `tenant_id().eq(tenant_id(cx))`: the copy is redundant, and one
    /// that disagreed with the derived column would hide rows rather than widen
    /// access. Code outside the framework's loaders starts from
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
    /// reads. [`view_relations`](Self::view_relations) renders related records
    /// off the loaded row, so include them here:
    /// `Self::query(cx).include(Post::fields().comments())`.
    ///
    /// The default is [`Self::query`] unchanged. The framework ANDs the tenant
    /// scope onto it, as it does onto [`Self::query`].
    fn view_query(cx: &Cx) -> toasty::stmt::Query<List<Self::Model>> {
        Self::query(cx)
    }

    /// Whether this resource requires a tenant in every handler.
    ///
    /// Opt-in and default-open: `false` preserves [`Self::query`] exactly as
    /// written. `true` means two things:
    ///
    /// 1. **The gate.** Every handler 403s when the request carries no tenant, instead of leaking
    ///    unscoped rows or minting nil-tenant orphans.
    /// 2. **The scope.** Every loader ANDs `tenant_id = <request tenant>` onto the base query,
    ///    deriving the column from the model's own schema or the resource's [`Self::tenant_scope`].
    ///    A gated resource that declares neither a `tenant_id` UUID column nor an override is
    ///    refused by [`Panel::build`](crate::Panel::build) at boot rather than served unscoped or
    ///    failing per request.
    ///
    /// A resource that must genuinely serve more than the request tenant
    /// declares `false` and scopes in [`Self::query`] by hand, giving up the
    /// gate above along with the derived filter.
    fn requires_tenant() -> bool {
        false
    }

    /// The predicate the framework ANDs onto this resource's base query to
    /// scope it to `tenant`, or `None` when there is nothing to AND.
    ///
    /// The default derives it from the model: `tenant_id = tenant`, on the
    /// field named `tenant_id` whose type is a UUID (see [`crate::tenancy`]).
    /// Override it when the resource's tenancy is not a column on its own
    /// model — a row that inherits its parent's tenant states the relation
    /// path here instead, and the framework applies it exactly as it applies
    /// the derived one. The showcase's comments are the worked example.
    ///
    /// Only consulted when [`requires_tenant`](Self::requires_tenant) is
    /// `true`. `None` from a gated resource is a **misdeclaration**, not a way
    /// to be unscoped: [`Panel::build`](crate::Panel::build) refuses it at boot
    /// and every loader keeps answering an error naming the resource
    /// rather than running its query without a tenant predicate — the backstop
    /// for a predicate that is only `None` for some tenants. There is
    /// deliberately no override that *removes* the scope — a resource that
    /// must serve more than one tenant declares `requires_tenant() = false`
    /// and owns the scope in [`Self::query`], visibly, with the gate given up.
    fn tenant_scope(tenant: uuid::Uuid) -> Option<toasty::stmt::Expr<bool>> {
        crate::tenancy::derived_tenant_filter::<Self::Model>(tenant)
    }

    /// Description of the list view.
    ///
    /// Every resource declares its table with [`Table::new`]: columns and the
    /// row key the list renders (see [`Table::render`]).
    fn table(_cx: &Cx) -> Table<Self::Model>;

    /// The schema the create and edit forms render.
    ///
    /// [`Panel::build`](crate::Panel::build) refuses a record form field this
    /// schema does not declare, and a schema on a resource whose
    /// [`Form`](Self::Form) is [`NoForm`](crate::NoForm).
    fn form(_cx: &Cx) -> crate::schema::Schema {
        crate::schema::Schema::empty()
    }

    /// App-level rules on the parsed form. The errors render inline with a
    /// 200 and nothing is written; a record fn error is a 500, so a range or
    /// cross-field rule belongs here.
    fn validate_record(_cx: &Cx, _form: &Self::Form) -> FieldErrors<Self::Form> {
        FieldErrors::new()
    }

    /// Create a record from the parsed form, inside the handler's transaction.
    ///
    /// Defaults to the derived write, [`write_create`]; override to check
    /// something inside the transaction, then delegate. `ex` is the open
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
    /// `order` or a custom label takes effect. Decorate the default with
    /// [`NavigationItem::for_resource`]
    /// (`NavigationItem { order: -1, ..NavigationItem::for_resource::<Self>() }`)
    /// to keep the panel-owned URL; spell a URL out yourself
    /// ([`NavigationItem::at`]) only to link somewhere other than this
    /// resource's list page — the Panel keeps such a URL verbatim.
    fn navigation() -> NavigationItem {
        NavigationItem::for_resource::<Self>()
    }

    /// Delete the already-authorized `record` (#86).
    ///
    /// The handler loads `record` through the tenancy-scoped query inside the
    /// framework transaction and checks `can_delete` on that snapshot, then
    /// calls this with the same transaction as `ex`. The after-commit hook
    /// receives the snapshot once the delete commits.
    ///
    /// The default deletes the row through [`scoped_query`], filtered to the
    /// record's key: the table's record key, which is the model's primary key
    /// (see [`Table::new_split`]). Override to delete another way, such as a
    /// soft delete.
    fn delete_record(
        cx: &Cx,
        record: &Self::Model,
        ex: &mut dyn toasty::Executor,
    ) -> impl std::future::Future<Output = Result<()>> + Send
    where
        Self: Sized,
    {
        let key = Self::table(cx).record_key_of(record);
        let query = scoped_query::<Self>(cx).and_then(|query| {
            let filter = crate::schema::pk_eq_expr::<Self::Model>(&key).ok_or_else(|| {
                std::io::Error::other(format!(
                    "{}: record key `{key}` is not a single-column primary key; override \
                     delete_record",
                    std::any::type_name::<Self>()
                ))
            })?;
            Ok(query.filter(filter))
        });
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
    /// transaction and checks `can_delete` on every row before calling this.
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
    /// ```ignore
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
    /// `cx` carries the app schema, which an embedded value's keys need
    /// ([`write_embedded`](crate::schema::write_embedded)). The default is
    /// empty.
    fn view_values(_cx: &Cx, _record: &Self::Model) -> HashMap<String, String> {
        HashMap::new()
    }
}

/// The tenant-scoped base query.
///
/// [`Resource::query`] with the predicate from [`Resource::tenant_scope`]
/// ANDed onto it, so a resource that overrides `query` for soft deletes cannot
/// drop the tenant scope by forgetting to re-state it. Every framework loader
/// and app code start here.
///
/// # Errors
///
/// - A gated resource and no tenant in `cx` → 403, the same fail-closed answer the handler gate
///   gives.
/// - A gated resource that supplies no tenant predicate — no discoverable `tenant_id` UUID column,
///   no [`Resource::tenant_scope`] override → an error naming the resource and the model.
///   [`Panel::build`](crate::Panel::build) refuses that declaration at boot, so this is the
///   backstop for a predicate that is `None` for the request's tenant, and for app code outside a
///   panel. It is deliberately **not** a fallback to the unscoped query: a silent miss would be the
///   leak [`Resource::requires_tenant`] exists to prevent.
///
/// App code that loads rows itself must call this — on a gated resource
/// [`Resource::query`] is the *tenant-unscoped* base by design, so calling it
/// directly is safe only for rows whose tenant membership is already settled (a
/// write by id against a record the framework loaded and authorized).
pub fn scoped_query<R: Resource>(cx: &Cx) -> Result<Query<List<R::Model>>> {
    apply_tenant_scope::<R>(cx, R::query(cx))
}

/// [`Resource::view_query`] under the same tenant gate and predicate as
/// [`scoped_query`]: the detail page's loader, and the entry point for a page
/// that owns its own detail view.
///
/// # Errors
///
/// The same as [`scoped_query`].
pub fn scoped_view_query<R: Resource>(cx: &Cx) -> Result<Query<List<R::Model>>> {
    apply_tenant_scope::<R>(cx, R::view_query(cx))
}

/// AND the framework's tenant predicate onto `query`.
///
/// The body of [`scoped_query`] and [`scoped_view_query`], split out so both
/// seeds share one gate, one predicate, and one fail-closed error.
/// It is crate-internal because a caller outside the crate always has a
/// `Resource`, and so always wants [`scoped_query`] or [`scoped_view_query`].
pub(crate) fn apply_tenant_scope<R: Resource>(
    cx: &Cx,
    query: Query<List<R::Model>>,
) -> Result<Query<List<R::Model>>> {
    if !R::requires_tenant() {
        return Ok(query);
    }
    let tenant = crate::tenancy::require_tenant(cx)?;
    let Some(filter) = R::tenant_scope(tenant) else {
        // Fail closed and loudly: the resource declared a gate whose scope the
        // framework cannot derive and the resource did not state, and running
        // the query unscoped is the one outcome that declaration exists to
        // prevent. `Panel::build` already refused the resource if *no* tenant
        // could scope it; this is the backstop for a `tenant_scope`
        // that answers `None` only for this tenant, and for callers outside a
        // panel.
        tracing::error!(
            resource = R::slug(),
            model = std::any::type_name::<R::Model>(),
            "requires_tenant is true but the resource supplies no tenant predicate: no `tenant_id` \
             UUID column on the model and no `tenant_scope` override (GH #223)"
        );
        return Err(std::io::Error::other(format!(
            "resource '{}' requires a tenant, but the framework cannot scope it: {} declares no \
             `tenant_id` UUID column to derive the filter from, and the resource does not override \
             `tenant_scope` (GH #223); declare the column, override `tenant_scope`, or drop \
             `requires_tenant` and scope in `query`",
            R::slug(),
            std::any::type_name::<R::Model>(),
        ))
        .into());
    };
    Ok(query.filter(filter))
}

/// Every `Resource` is an [`OptionSource`](crate::schema::OptionSource) — the
/// bridge that lets the relationship option loaders be generic over the source
/// surface instead of over `Resource`, so `schema` does not depend on
/// `resource`.
///
/// A resource answers the loaders here, so the loaders never name `Resource`.
/// [`scoped_query`](crate::schema::OptionSource::scoped_query), the one required
/// method, forwards to [`scoped_query`], so an option load inherits the tenant
/// gate and the derived tenant predicate exactly as every other loader does; it
/// is deliberately not [`Resource::query`], which on a gated resource is the
/// *tenant-unscoped* base.
///
/// The policy predicates and the tenant declaration forward unchanged, and the
/// search expression and default ordering come from the resource's declared
/// [`table`](Resource::table), which is where "the option search searches the
/// related resource's searchable columns" lives.
impl<R: Resource> crate::schema::OptionSource for R {
    type Model = R::Model;

    fn scoped_query(cx: &Cx) -> Result<Query<List<R::Model>>> {
        scoped_query::<R>(cx)
    }

    fn can_view_any(cx: &Cx) -> bool {
        <R as Resource>::can_view_any(cx)
    }

    fn can_view(cx: &Cx, record: &R::Model) -> bool {
        <R as Resource>::can_view(cx, record)
    }

    fn requires_tenant() -> bool {
        <R as Resource>::requires_tenant()
    }

    fn slug() -> String {
        <R as Resource>::slug()
    }

    fn search_expr(cx: &Cx, term: &str) -> Option<toasty::stmt::Expr<bool>> {
        <R as Resource>::table(cx).search_expr(term)
    }

    fn order_by(cx: &Cx) -> Option<toasty::stmt::OrderByExpr> {
        <R as Resource>::table(cx).order_by(false)
    }
}

#[cfg(test)]
mod tests;
