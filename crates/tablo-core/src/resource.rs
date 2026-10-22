//! `Resource` — maps one Toasty [`Model`](toasty::schema::Model) to its admin UI.

use toasty::{
    Executor,
    stmt::{List, Query},
};
use topcoat::{Result, context::Cx};

use crate::form::{FieldErrors, Posted, RecordForm};

mod action;
mod check;
mod commit;
mod def;
mod field_access;
mod header_action;
mod mounted;
mod relation;
mod soft_delete;
mod write;
mod write_error;

pub use action::{Action, Places};
pub(crate) use action::{
    ActionEntry, Actions, ErasedInput, InputResult, InputSpec, RESERVED_KEYS, parse_validated,
};
pub(crate) use check::{ResourceCheck, check_inputs, check_resource};
pub use commit::Change;
pub(crate) use commit::{acted, run_after_commit};
pub use def::{PublicLink, ResourceDef};
pub use field_access::FieldAccess;
pub(crate) use field_access::{FieldMask, FieldRule};
pub(crate) use header_action::HeaderEntry;
pub use header_action::{HeaderAction, HeaderActions};
pub(crate) use mounted::{
    MountScope, Mounted, Mounts, is_mounted, is_restricted_model, is_tenant_model, model_rules,
    mounted, require_mounted, restricts_any,
};
pub use relation::{ForeignKey, Relation};
pub(crate) use relation::{Links, RelationKind};
pub use soft_delete::Trashed;
pub(crate) use soft_delete::{SoftDelete, TRASH_FILTER, TrashFilter};
pub use write::{write_create, write_update};
pub use write_error::WriteError;

/// Maps one Toasty `Model` to its admin UI.
///
/// # Contract
///
/// Requires [`Model`](Self::Model) and [`Form`](Self::Form); every other item has a default.
/// [`declare`](Self::declare) returns what the resource declares, as one [`ResourceDef`] value;
/// the methods scope, validate and write records for a request.
///
/// The panel builds the def once when it mounts, binding the paths it names to the database
/// schema, and serves the result to every request. Record fns run inside the handler transaction
/// and fail without partial writes.
pub trait Resource: Sized + Send + Sync + 'static {
    /// The persisted model this resource administers. Its [`ModelColumns`](crate::ModelColumns)
    /// give the table and detail page the resource declares none of.
    type Model: crate::ModelColumns
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

    /// What the resource declares: names, navigation, policy, tenancy, table, form, detail page,
    /// relations and actions. Defaults to [`ResourceDef::new`], whose policy denies all.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post {
    /// #     #[key] #[auto] id: uuid::Uuid,
    /// #     title: String,
    /// #     body: String,
    /// # }
    /// # #[derive(Debug, Clone, tablo_core::RecordForm)]
    /// # #[form(model = Post)]
    /// # struct PostForm { title: String, body: String }
    /// # struct PostResource;
    /// # use tablo_core::{ReadOnly, RecordForm, Resource, ResourceDef, Schema, Section};
    /// impl Resource for PostResource {
    ///     type Model = Post;
    ///     type Form = PostForm;
    ///
    ///     fn declare() -> ResourceDef<Self> {
    ///         let c = PostForm::controls();
    ///         ResourceDef::new().policy(ReadOnly).form(Schema::new(
    ///             Section::new("Content").schema((c.title, c.body.multiline(6))),
    ///         ))
    ///     }
    /// }
    /// ```
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
    }

    /// Base query — the seam for a resource's **own** row scoping (ADR-0002):
    /// soft deletes and row-level visibility. Every loader starts from it.
    ///
    /// **Relations are not this method's job.** The list, the export and the detail page load
    /// the relations their columns declare ([`Column::includes`](crate::extend::Column::includes)):
    /// a [`RelationColumn`](crate::RelationColumn) declares its own. Include a relation here
    /// only when a closure no column covers reads it on every loader's rows: one the
    /// [`policy`](ResourceDef::policy) reads, or one a table's `group_by` or row key reads without
    /// a column including it.
    ///
    /// **Tenancy is not this method's job either.** For a resource whose
    /// [`tenancy`](ResourceDef::tenancy) is scoped the framework ANDs the tenant filter
    /// onto whatever this returns, at every loader through [`scoped_query`]. Do
    /// not re-state it here. Code outside the framework's loaders starts from
    /// [`scoped_query`].
    ///
    /// Returns the raw typed statement query, which composes generically —
    /// `filter`, `order_by` and `Paginate::new` work on the raw form for any
    /// `M: Model` — so one panel handler drives every resource's list page.
    ///
    /// A list and its export replace any ordering this sets with the table's own sort: order a
    /// list with [`Table::default_sort`](crate::Table::default_sort).
    ///
    /// # Keep unique constraints in step with this scope
    ///
    /// The app-side unique pre-check probes through the same tenant-scoped
    /// query, so a `#[unique]` index *broader* than the scope is invisible: the
    /// probe misses the colliding row and the user gets a 500 instead of the
    /// inline "has already been taken". Scope the constraint to match —
    /// `#[unique(tenant_id, email)]` — or, when the index must stay broader, probe it in
    /// [`create_record`](Self::create_record) and [`update_record`](Self::update_record) and refuse
    /// with [`WriteError::Refused`]. Not checkable at declaration time, since a query's filters
    /// are not introspectable; tablo#117 tracks the fix.
    fn query(_cx: &Cx) -> toasty::stmt::Query<List<Self::Model>> {
        toasty::stmt::Query::<List<Self::Model>>::all()
    }

    /// App-level rules on the parsed form. The errors render inline with a
    /// 200 and nothing is written, so a range or cross-field rule belongs here. A rule that
    /// reads the database belongs in [`create_record`](Self::create_record) or
    /// [`update_record`](Self::update_record), which refuse through the transaction they write
    /// in.
    ///
    /// Each error names a field of the record form (`UserFormField::Age`) and renders under its
    /// control, or under an embedded value's first control.
    fn validate_record(
        _cx: &Cx,
        _form: &Self::Form,
    ) -> FieldErrors<<Self::Form as RecordForm>::Field> {
        FieldErrors::new()
    }

    /// Create a record from the parsed form, inside the handler's transaction.
    ///
    /// Defaults to the derived write, [`write_create`]; override to check
    /// something inside the transaction, then delegate. An override on a
    /// [`Tenancy::column`](crate::Tenancy::column) resource stamps the tenant
    /// only by delegating to [`write_create`]. `ex` is the open
    /// transaction: run every statement through it. Return the created row; the
    /// hooks receive it as [`Change::Create`].
    ///
    /// A rule that reads the database refuses the submission with [`WriteError::Refused`]: the
    /// transaction rolls back and the form renders again with the errors under their fields.
    fn create_record(
        cx: &Cx,
        form: Self::Form,
        ex: &mut dyn Executor,
    ) -> impl Future<Output = Result<Self::Model, WriteError<<Self::Form as RecordForm>::Field>>> + Send
    {
        async move { Ok(write_create::<Self>(cx, form, ex).await?) }
    }

    /// Update the already-authorized `record` from the posted form, inside the
    /// handler's transaction.
    ///
    /// Defaults to the derived write, [`write_update`]. `record` is the
    /// snapshot the handler loaded and policy-checked inside the transaction:
    /// use it, never re-query. Return the row as it now stands.
    ///
    /// Refuses the submission with [`WriteError::Refused`], as
    /// [`create_record`](Self::create_record) does.
    fn update_record(
        cx: &Cx,
        record: Self::Model,
        posted: Posted<Self::Form>,
        ex: &mut dyn Executor,
    ) -> impl Future<Output = Result<Self::Model, WriteError<<Self::Form as RecordForm>::Field>>> + Send
    {
        async move { Ok(write_update::<Self>(cx, record, posted, ex).await?) }
    }

    /// Delete the already-authorized `record`.
    ///
    /// The handler loads `record` through the tenancy-scoped query inside the
    /// framework transaction and checks the policy on that snapshot, then
    /// calls this with the same transaction as `ex`. The hooks receive the
    /// snapshot as [`Change::Delete`].
    ///
    /// The default deletes the row through [`scoped_query`], filtered to the
    /// record's primary key, or stamps its
    /// [`soft_delete`](ResourceDef::soft_delete) column with the time when the resource declares
    /// one. Override to delete another way.
    fn delete_record(
        cx: &Cx,
        record: &Self::Model,
        ex: &mut dyn toasty::Executor,
    ) -> impl std::future::Future<Output = Result<()>> + Send
    where
        Self: Sized,
    {
        write::delete::<Self>(cx, record, ex)
    }

    /// Restore the already-authorized trashed `record` of a resource that
    /// [soft-deletes](ResourceDef::soft_delete), inside the handler's transaction.
    ///
    /// The handler loads `record` through [`scoped_query_in`] with [`Trashed::Only`] and checks
    /// [`Ability::Restore`](crate::Ability::Restore) on it. The default empties the soft-delete
    /// column.
    fn restore_record(
        cx: &Cx,
        record: &Self::Model,
        ex: &mut dyn toasty::Executor,
    ) -> impl std::future::Future<Output = Result<()>> + Send
    where
        Self: Sized,
    {
        write::restore::<Self>(cx, record, ex)
    }

    /// Delete the already-authorized trashed `record` for good, inside the handler's
    /// transaction.
    ///
    /// The handler loads `record` through [`scoped_query_in`] with [`Trashed::Only`] and checks
    /// [`Ability::ForceDelete`](crate::Ability::ForceDelete) on it. The default removes the row.
    fn force_delete_record(
        cx: &Cx,
        record: &Self::Model,
        ex: &mut dyn toasty::Executor,
    ) -> impl std::future::Future<Output = Result<()>> + Send
    where
        Self: Sized,
    {
        write::force_delete::<Self>(cx, record, ex)
    }

    /// Bulk-delete the already-authorized `records`: the handler
    /// fetches through the tenancy-scoped `IN` query inside the framework
    /// transaction, checks `View` on every row, and passes the rows
    /// [`Ability::Delete`](crate::Ability::Delete) allows, skipping the rest
    /// as a bulk custom action does. The default deletes each record through
    /// [`Self::delete_record`] in order, through the same `ex` — any error rolls the whole
    /// batch back, so mid-loop failures delete zero rows. An override of `delete_record`,
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

    /// Work inside the write's transaction, after the record fn or action wrote: an audit row,
    /// a counter, a row in another table that must commit with the write or not at all.
    ///
    /// Called once per write, with the [`Change`] it made, through `ex`, the open transaction:
    /// run every statement through it, and notify through
    /// [`Notifications::send_in`](crate::notification::Notifications::send_in), which stores the
    /// notification only if the write commits. A second connection, through
    /// [`db`](crate::db::db) or [`Notifications::send`](crate::notification::Notifications::send),
    /// would write outside the transaction, and on a pool of one wait forever for the connection
    /// the transaction holds; a debug build panics on either instead.
    ///
    /// Every write this resource's panel routes make comes here: a create, once per row of a CSV
    /// import, an update, a delete or bulk delete, a restore or force delete, a custom or header
    /// action, and a many-to-many attach or detach. The profile and tenant-settings pages save
    /// their user and tenant rows themselves and do not. A validation error, a policy denial or
    /// a failed or refused record fn returns before it. An `Err` rolls the whole write back and
    /// fails the request with the write's failure toast.
    ///
    /// ```rust
    /// # use tablo_core::{Change, NoForm, Resource};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// # #[derive(Debug, toasty::Model)]
    /// # struct Audit { #[key] #[auto] id: uuid::Uuid, post_id: uuid::Uuid, change: String }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// async fn before_commit(
    ///     _cx: &Cx,
    ///     change: &Change<Post>,
    ///     ex: &mut dyn toasty::Executor,
    /// ) -> Result<()> {
    ///     let kind = match change {
    ///         Change::Create(_) => "create",
    ///         Change::Update { .. } => "update",
    ///         Change::Delete(_) => "delete",
    ///         Change::Restore(_) => "restore",
    ///         Change::ForceDelete(_) => "force delete",
    ///         Change::Action { name, .. } | Change::HeaderAction { name } => name,
    ///         Change::Attach { .. } => "attach",
    ///         Change::Detach { .. } => "detach",
    ///     };
    ///     for post in change.records() {
    ///         toasty::create!(Audit {
    ///             post_id: post.id,
    ///             change: kind.to_string()
    ///         })
    ///         .exec(&mut *ex)
    ///         .await?;
    ///     }
    ///     Ok(())
    /// }
    /// # }
    /// ```
    fn before_commit(
        _cx: &Cx,
        _change: &Change<Self::Model>,
        _ex: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send
    where
        Self: Sized,
    {
        async move { Ok(()) }
    }

    /// Work after the write commits, outside the database: an email, a webhook, cache
    /// invalidation.
    ///
    /// Called once per committed write, after `tx.commit()` and before the response, with the
    /// [`Change`] [`Self::before_commit`] received. It is never called when nothing committed, so
    /// a rollback cannot produce the effect. Nothing guarantees it runs either: an error it
    /// returns is logged and ignored, since the write is committed, and a crash between the
    /// commit and the hook skips it. A row that must exist whenever the write does belongs in
    /// [`Self::before_commit`]; retries are the app's to build. A panic surfaces as Topcoat's
    /// panic-isolated 500.
    ///
    /// ```rust
    /// # use tablo_core::{Change, NoForm, Resource};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// # async fn notify(_cx: &Cx, _post: &Post) -> Result<()> { Ok(()) }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// async fn after_commit(cx: &Cx, change: Change<Post>) -> Result<()> {
    ///     if let Change::Create(post) = &change {
    ///         notify(cx, post).await?;
    ///     }
    ///     Ok(())
    /// }
    /// # }
    /// ```
    fn after_commit(
        _cx: &Cx,
        _change: Change<Self::Model>,
    ) -> impl Future<Output = Result<()>> + Send
    where
        Self: Sized,
    {
        async move { Ok(()) }
    }
}

/// The tenant-scoped base query.
///
/// [`Resource::query`] with the [`tenancy`](ResourceDef::tenancy) filter ANDed onto it, so a
/// resource that overrides `query` cannot drop the tenant scope by forgetting to re-state it, and
/// with the trashed records of a resource that [soft-deletes](ResourceDef::soft_delete) left out.
/// Every framework loader and app code start here; [`scoped_query_in`] admits the trashed
/// records too.
///
/// # Errors
///
/// A tenant-scoped resource and no tenant in `cx`: 403, the same answer the
/// panel's handlers give. A declaration error when the context's panel does not mount `R`; a
/// background job builds its context with [`PanelHandle::context`](crate::PanelHandle::context).
///
/// App code that loads rows itself must call this: on a scoped resource
/// [`Resource::query`] is the *tenant-unscoped* base by design.
pub fn scoped_query<R: Resource>(cx: &Cx) -> Result<Query<List<R::Model>>> {
    scoped_query_in::<R>(cx, Trashed::Without)
}

/// [`scoped_query`] admitting the records `trashed` names, as a restore or a trash list loads
/// them, under the same tenant filter.
///
/// On a resource that declares no [`soft_delete`](ResourceDef::soft_delete) column every record
/// is live: [`Trashed::Only`] admits none, and the other scopes admit all.
///
/// # Errors
///
/// As [`scoped_query`].
pub fn scoped_query_in<R: Resource>(cx: &Cx, trashed: Trashed) -> Result<Query<List<R::Model>>> {
    require_mounted::<R>(cx)?.scoped_query_in(cx, trashed)
}

impl<R: Resource> Mounted<R> {
    /// [`scoped_query`] for this mount.
    pub(crate) fn scoped_query(&self, cx: &Cx) -> Result<Query<List<R::Model>>> {
        self.scoped_query_in(cx, Trashed::Without)
    }

    /// [`scoped_query_in`] for this mount.
    pub(crate) fn scoped_query_in(
        &self,
        cx: &Cx,
        trashed: Trashed,
    ) -> Result<Query<List<R::Model>>> {
        let query = self.tenant_scope(cx, R::query(cx))?;
        Ok(match &self.soft_delete {
            Some(soft_delete) => match soft_delete.filter(trashed) {
                Some(filter) => query.filter(filter),
                None => query,
            },
            None if trashed == Trashed::Only => query.filter(crate::toasty_compat::never()),
            None => query,
        })
    }

    /// The records a list in `state` admits: the ones its trash control selects, else the live
    /// ones.
    pub(crate) fn list_trashed(&self, state: &crate::TableState) -> Trashed {
        if self.soft_delete.is_none() {
            return Trashed::Without;
        }
        Trashed::from_filter(state.filters.get(TRASH_FILTER)).unwrap_or_default()
    }

    /// [`scoped_query_in`] for the list in `state`.
    pub(crate) fn list_query(
        &self,
        cx: &Cx,
        state: &crate::TableState,
    ) -> Result<Query<List<R::Model>>> {
        self.scoped_query_in(cx, self.list_trashed(state))
    }

    /// Whether `record` is trashed: never on a resource that does not soft-delete.
    pub(crate) fn is_trashed(&self, record: &R::Model) -> bool {
        self.soft_delete
            .as_ref()
            .is_some_and(|soft_delete| soft_delete.is_trashed(record))
    }

    /// AND the resource's tenant filter onto `query`.
    fn tenant_scope(&self, cx: &Cx, query: Query<List<R::Model>>) -> Result<Query<List<R::Model>>> {
        if !self.tenancy.is_scoped() {
            return Ok(query);
        }
        if let Some(Err(error)) = self.tenancy.column_field() {
            return Err(crate::error::declaration(
                crate::DeclarationError::of::<R>(crate::Site::Tenancy, error).to_string(),
            ));
        }
        let tenant = crate::tenancy::require_tenant(cx)?;
        Ok(match self.tenancy.filter(tenant) {
            Some(filter) => query.filter(filter),
            None => query,
        })
    }
}

/// Whether `R`'s policy, as the context's panel mounted it, allows `ability`; `false` when the
/// panel does not mount `R`. Does not check sign-in or tenant scope.
pub fn can<R: Resource>(cx: &Cx, ability: crate::Ability<'_, R::Model>) -> bool {
    mounted::<R>(cx).is_some_and(|resource| resource.can(cx, ability))
}

/// Every `Resource` is an [`OptionSource`](crate::extend::OptionSource), answering from its def
/// as the context's panel mounted it.
///
/// [`scoped_query`](crate::extend::OptionSource::scoped_query) forwards to
/// [`scoped_query`], so an option load inherits the tenant check and filter
/// exactly as every other loader does. The search expression and default ordering come from the
/// resource's [`table`](ResourceDef::table), whose searchable columns the option search searches
/// with the [`record_title`](ResourceDef::record_title) column, and each record's label is its
/// title.
impl<R: Resource> crate::schema::OptionSource for R {
    type Model = R::Model;

    fn scoped_query(cx: &Cx) -> Result<Query<List<R::Model>>> {
        scoped_query::<R>(cx)
    }

    fn allows(cx: &Cx, ability: crate::Ability<'_, R::Model>) -> bool {
        can::<R>(cx, ability)
    }

    fn requires_tenant(cx: &Cx) -> bool {
        mounted::<R>(cx).is_some_and(|mounted| mounted.tenancy.is_scoped())
    }

    fn search_expr(cx: &Cx, term: &str) -> Option<toasty::stmt::Expr<bool>> {
        mounted::<R>(cx)?.option_search_expr(cx, term)
    }

    fn order_by(cx: &Cx) -> Option<toasty::stmt::OrderByExpr> {
        mounted::<R>(cx)?.table_for(cx).first_order_by()
    }

    /// The record's title, as its detail page shows it.
    fn label(cx: &Cx, record: &R::Model) -> String {
        let key = crate::toasty_compat::pk::pk_text(record);
        match mounted::<R>(cx) {
            Some(mounted) => mounted.record_title(record, &key),
            None => key,
        }
    }

    fn available(cx: &Cx) -> bool {
        mounted::<R>(cx).is_some()
    }

    fn hidden_fields(cx: &Cx) -> Vec<String> {
        mounted::<R>(cx).map_or_else(Vec::new, |mounted| mounted.field_mask(cx).hidden().to_vec())
    }
}

#[cfg(test)]
mod tests;
