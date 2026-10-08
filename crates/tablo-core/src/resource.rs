//! `Resource` — maps one Toasty [`Model`](toasty::schema::Model) to its admin UI.

use toasty::{
    Executor,
    stmt::{List, Query},
};
use topcoat::{Result, context::Cx};

use crate::form::{FieldErrors, Posted, RecordForm};

mod action;
mod commit;
mod def;
mod link;
mod mounted;
mod relation;
mod write;

#[doc(hidden)]
pub use action::required_input;
pub use action::{Action, ActionInput};
pub(crate) use action::{ActionEntry, Actions, ErasedInput, RESERVED_KEYS, SUBMITTED_KEY};
pub(crate) use commit::run_after_commit;
pub use commit::{Committed, Mutation};
pub use def::ResourceDef;
pub use link::PublicLink;
pub(crate) use mounted::{MountScope, Mounted, Mounts, mounted, require_mounted};
pub use relation::{ForeignKey, Relation};
pub use write::{write_create, write_update};

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

    /// What the resource declares: names, navigation, policy, tenancy, table, form, detail page,
    /// relations and actions. Defaults to [`ResourceDef::new`], whose policy denies all.
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

    /// App-level rules on the parsed form. The errors render inline with a
    /// 200 and nothing is written; a record fn error keeps its own mapping, so
    /// a range or cross-field rule belongs here.
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
}

/// The tenant-scoped base query.
///
/// [`Resource::query`] with the [`tenancy`](ResourceDef::tenancy) filter ANDed onto it, so a
/// resource that overrides `query` for soft deletes cannot drop the tenant
/// scope by forgetting to re-state it. Every framework loader and app code
/// start here.
///
/// # Errors
///
/// A tenant-scoped resource and no tenant in `cx`: 403, the same answer the
/// handler gate gives. A declaration error when the context's panel does not mount `R`; a
/// background job builds its context with [`Panel::context`](crate::Panel::context).
///
/// App code that loads rows itself must call this: on a scoped resource
/// [`Resource::query`] is the *tenant-unscoped* base by design.
pub fn scoped_query<R: Resource>(cx: &Cx) -> Result<Query<List<R::Model>>> {
    require_mounted::<R>(cx)?.scoped_query(cx)
}

impl<R: Resource> Mounted<R> {
    /// [`scoped_query`] for this mount.
    pub(crate) fn scoped_query(&self, cx: &Cx) -> Result<Query<List<R::Model>>> {
        self.tenant_scope(cx, R::query(cx))
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
/// [`scoped_query`], so an option load inherits the tenant gate and filter
/// exactly as every other loader does. The search expression and default ordering come from the
/// resource's [`table`](ResourceDef::table), which is where "the option search searches the
/// related resource's searchable columns" lives, and each record's label from its
/// [`record_label`](ResourceDef::record_label).
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
        mounted::<R>(cx)?.table.search_expr(term)
    }

    fn order_by(cx: &Cx) -> Option<toasty::stmt::OrderByExpr> {
        mounted::<R>(cx)?.table.order_by(false)
    }

    fn label(cx: &Cx, record: &R::Model) -> String {
        let key = crate::toasty_compat::pk::pk_text(record);
        match mounted::<R>(cx) {
            Some(mounted) => mounted.record_title(cx, record, &key),
            None => key,
        }
    }

    fn available(cx: &Cx) -> bool {
        mounted::<R>(cx).is_some()
    }
}

#[cfg(test)]
mod tests;
