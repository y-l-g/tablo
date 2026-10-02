//! Relationship option loading — bounded, memoized, policy-checked.
//!
//! Every relationship choice field over one source shares a single bounded
//! load per `(request, tenant)`; the source's policy plus
//! the tenant gate fails the load closed instead of leaking labels.
//!
//! The loaders are generic over [`OptionSource`] — the source surface they
//! read — rather than over `Resource`, so the dependency runs one way:
//! `resource` depends on `schema`, and the blanket impl in
//! [`crate::resource`] makes every `Resource` an option source.

use toasty::stmt::{Expr, List, OrderByExpr, Query};
use topcoat::{Result, context::Cx};

use crate::policy::{Ability, Deny, Policy};

/// Everything the relationship option loaders read from the thing they load
/// options from.
///
/// The loaders ask for the **tenant-scoped** seed query, the policy, the
/// tenant declaration, a name for the log fields, and the source's search and
/// ordering expressions. It is a *source* surface rather than a second resource
/// trait, so `schema` names no part of `Resource`: every
/// [`Resource`](crate::resource::Resource) is one through the blanket impl in
/// [`crate::resource`], which forwards its policy and tenancy and composes the
/// framework's tenant-scoped query.
///
/// [`scoped_query`](Self::scoped_query) is **required**: a source states its own
/// scope, so a gated source cannot end up unscoped by omission. The rest have
/// the default that keeps a source declaring nothing honest: the policy denies,
/// the search and ordering expressions answer `None`,
/// [`requires_tenant`](Self::requires_tenant) is `false`, and
/// [`slug`](Self::slug) falls back to the type name.
///
/// It is public because [`ChoiceField::relationship`](crate::schema::ChoiceField::relationship)'s
/// bound names it. It is not re-exported at the crate root, because its method
/// names are `Resource`'s and a glob import would collide; it is reachable as
/// `schema::OptionSource`. Implement it directly only for a source that is not a
/// resource, and state the scope.
pub trait OptionSource: Sized + Send + Sync + 'static {
    /// The model whose rows become options.
    type Model: toasty::schema::Model + Send + Sync + 'static;

    /// The **tenant-scoped** seed query every option load starts from.
    ///
    /// Required, and stated by every implementor, so a source cannot be
    /// unscoped by omission. A source that scopes nothing says so in the body,
    /// `Ok(Query::all())`, and a source whose rows are tenant-owned ANDs the
    /// tenant predicate here, which is what [`Self::requires_tenant`] declares.
    ///
    /// An `Err` is a **permanent** misdeclaration, reported as
    /// `OptionLoadError::Misdeclared` rather than a retryable failure.
    fn scoped_query(cx: &Cx) -> Result<Query<List<Self::Model>>>;

    /// What the current user may see. [`Ability::ViewAny`] refused fails the
    /// whole option load closed, never an empty set that validates as
    /// "invalid"; [`Ability::View`] refused keeps a row out of the options, and
    /// out of validation, before its label renders.
    fn policy() -> impl Policy<Self::Model> {
        Deny
    }

    /// Whether the source's rows are tenant-owned: `true` fails a
    /// tenantless request closed, and is the declaration
    /// [`Self::scoped_query`] is expected to scope for.
    fn requires_tenant() -> bool {
        false
    }

    /// A short name for the loaders' log fields. A `Resource` forwards its
    /// route slug.
    fn slug() -> String {
        std::any::type_name::<Self>().to_string()
    }

    /// The related source's search predicate for `term`, or `None` when it
    /// declares no searchable column (D1). A resource answers from its
    /// declared `searchable()` columns.
    ///
    /// `None` on a non-blank term is the documented fallback: the option
    /// search runs the bounded head instead and overflows past the cap (D1a)
    /// rather than scanning the table.
    fn search_expr(_cx: &Cx, _term: &str) -> Option<Expr<bool>> {
        None
    }

    /// The related source's declared default ordering — its first sortable
    /// column, ascending, or `None`. A resource answers from its
    /// declared `sortable()` columns.
    ///
    /// The option search applies it so a narrowed result keeps the list's
    /// ordering; deliberately not a list-mode resolution, which would add a
    /// primary-key fallback this endpoint never had.
    fn order_by(_cx: &Cx) -> Option<OrderByExpr> {
        None
    }
}

/// Why a relationship option load produced no options.
///
/// Distinguishes a policy denial from a structural/transient failure so
/// `validate_async` can say "not available" instead of "retry", and so the
/// render path never re-labels a value the user may not view.
///
/// `Overflow` is distinct from `LoadFailed`: the related table
/// exceeds the option cap. A searchable choice degrades to "type to
/// search" instead of a retry error, while a genuine DB failure stays
/// retryable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OptionLoadError {
    /// The related resource refuses `ViewAny` (or has no tenant) for
    /// this request.
    Denied,
    /// The driver failed.
    LoadFailed,
    /// The related table overflows the option cap.
    Overflow,
    /// The source's [`scoped_query`](OptionSource::scoped_query) failed after
    /// the access check passed: a source that cannot state its own scope.
    ///
    /// Distinct from [`Self::LoadFailed`] because retrying cannot fix a broken
    /// declaration: the option UI must not offer a retry, and the search
    /// endpoint answers a 500 that names the misdeclaration.
    Misdeclared,
}

/// The boxed future a relationship loader returns — the bounded load and the
/// server-side *search* hand back the same shape, so they share one
/// alias.
pub(crate) type RelationshipLoadFuture = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<(String, String)>, OptionLoadError>> + Send>,
>;

#[allow(clippy::type_complexity)]
pub(crate) type RelationshipLoader =
    std::sync::Arc<dyn Fn(&Cx) -> RelationshipLoadFuture + Send + Sync>;

#[allow(clippy::type_complexity)]
pub(crate) type RelationshipSearchLoader =
    std::sync::Arc<dyn Fn(&Cx, String) -> RelationshipLoadFuture + Send + Sync>;

/// The boxed future a targeted existence check returns (D4).
pub(crate) type RelationshipCheckFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<RelatedCheck, OptionLoadError>> + Send + 'a>,
>;

/// The targeted existence check, run through the executor it is handed: a
/// pooled handle before a write, the write's transaction inside it.
#[allow(clippy::type_complexity)]
pub(crate) type RelationshipChecker = std::sync::Arc<
    dyn for<'a> Fn(&'a Cx, String, &'a mut dyn toasty::Executor) -> RelationshipCheckFuture<'a>
        + Send
        + Sync,
>;

/// Whether the option load may proceed: the related `ViewAny` plus the
/// related tenant gate fail closed (`Denied`). Shared by the bounded load,
/// the server-side search, and the targeted check so tenancy isolation never
/// rides on scope resolution in one place and not the others.
fn ensure_option_access<R>(cx: &Cx) -> Result<(), OptionLoadError>
where
    R: OptionSource,
{
    if !R::policy().allows(cx, Ability::ViewAny) {
        return Err(OptionLoadError::Denied);
    }
    if R::requires_tenant() && crate::tenancy::tenant_id(cx).is_none() {
        // The panel gates every handler through `enforce_tenant::<R>`;
        // option loads must not be the one tenantless path into the related
        // resource's scoped query.
        return Err(OptionLoadError::Denied);
    }
    Ok(())
}

/// The relationship option loaders' seed query: [`OptionSource::scoped_query`]
/// with the load's own error kind. An option label projects the related
/// record's own columns, so the seed carries no relation.
///
/// Every loader below starts here rather than at an unscoped base so option
/// loads inherit the framework's tenant scope (`ensure_option_access` above
/// already answered the tenantless case with `Denied`) and ask for only the
/// relations an option projection reads. The one error left for this step is a
/// source the framework cannot scope at all, which is a **permanent**
/// misdeclaration and therefore
/// [`Misdeclared`](OptionLoadError::Misdeclared) rather than a retryable load
/// failure.
fn option_query<R>(cx: &Cx) -> Result<Query<List<R::Model>>, OptionLoadError>
where
    R: OptionSource,
{
    R::scoped_query(cx).map_err(|error| {
        tracing::error!(
            resource = R::slug(),
            error = %error,
            "relationship option load cannot scope the related resource"
        );
        OptionLoadError::Misdeclared
    })
}

/// Max options a relationship choice field will load: the loader carries
/// `limit(Self + 1)` and fails past the cap instead of scanning a 10k-row
/// table per select per submit.
pub const MAX_RELATIONSHIP_OPTIONS: usize = 200;

/// The related model's primary key type — the identity a relationship option
/// stores. Fully qualified because the `Model` trait is a bound of
/// `OptionSource::Model`, not a supertrait of `OptionSource`.
pub(crate) type RelatedPrimaryKey<R> =
    <<R as OptionSource>::Model as toasty::schema::Model>::PrimaryKey;

/// Option records for one related resource, memoized per request.
///
/// Every relationship choice field over the same `R` shares one bounded load per
/// `(request, tenant)` instead of scanning the table per select per validate
/// plus re-render scans. `tenant` is an explicit cache key: memoize tracking
/// alone cannot distinguish header-tenanted callers sharing one `Parts`, so
/// tenancy isolation never rides on scope resolution. The tenant-scoped query
/// stays the only data seam; value and label mapping stay in the
/// caller so selects with different projections share the hit.
///
/// Policy is part of the load: `ViewAny` (and the related
/// resource's tenant gate) denies the whole load — fail closed, never an
/// empty set that validates as "invalid"; loaded rows are filtered through
/// `View` before any label is rendered.
///
/// The cap is checked on the **raw** bounded fetch, before `View`
/// filtering: counting filtered rows would let one hidden record
/// defeat the cap and silently truncate a larger table, misreporting
/// legitimate FKs as "invalid".
#[topcoat::context::memoize(as_ref)]
pub(crate) async fn related_records<R>(
    cx: &Cx,
    _tenant: Option<uuid::Uuid>,
) -> Result<Vec<R::Model>, OptionLoadError>
where
    R: OptionSource,
{
    ensure_option_access::<R>(cx)?;
    let mut query = option_query::<R>(cx)?;
    if let Some(ord) = R::order_by(cx) {
        query = query.order_by(ord);
    }
    bounded_options::<R>(
        cx,
        query,
        "relationship option load failed",
        "relationship option table overflows the cap",
    )
    .await
}

/// The bounded option load shared by [`related_records`] and
/// [`related_records_search`]: fetch one row past the cap, map a query failure
/// to `LoadFailed`, refuse a set over the cap with `Overflow`, then drop the
/// rows the caller cannot view.
///
/// The cap is checked on the **raw** bounded fetch, before `View`
/// filtering: counting filtered rows would let one hidden record
/// defeat the cap and silently truncate a larger table, misreporting
/// legitimate FKs as "invalid". `failed` and `overflow` are the warning
/// messages the caller's load reports.
async fn bounded_options<R>(
    cx: &Cx,
    query: Query<List<R::Model>>,
    failed: &str,
    overflow: &str,
) -> Result<Vec<R::Model>, OptionLoadError>
where
    R: OptionSource,
{
    let mut db = crate::db::db(cx);
    let mut records = query
        .limit(MAX_RELATIONSHIP_OPTIONS + 1)
        .exec(&mut db)
        .await
        .map_err(|e| {
            tracing::warn!(
                resource = R::slug(),
                error = %e,
                "{failed}"
            );
            OptionLoadError::LoadFailed
        })?;
    if records.len() > MAX_RELATIONSHIP_OPTIONS {
        // Fail visibly: validating against a silent truncation
        // would reject legitimate FKs as "invalid" while rendering a
        // misleading subset. Counted before policy filtering. Distinct
        // `Overflow` so searchable selects degrade to type-to-
        // search instead of a retry error.
        tracing::warn!(
            resource = R::slug(),
            max = MAX_RELATIONSHIP_OPTIONS,
            "{overflow}"
        );
        return Err(OptionLoadError::Overflow);
    }
    let policy = R::policy();
    records.retain(|record| policy.allows(cx, Ability::View(record)));
    Ok(records)
}

/// Bounded server-side option search.
///
/// Reuses the related table's declared `searchable()` columns via
/// `R::search_expr(cx, q)` (D1): documented as "option search searches
/// the related resource's declared searchable columns". No option-specific
/// hook until a real caller needs it.
///
/// * Empty/blank `q` → bounded head (same cap as [`related_records`]).
/// * `q` non-empty but `search_expr` is `None` (no searchable columns) → fallback to the hard-cap
///   path (D1a): unfiltered bounded load, `Overflow` when over the cap. Non-searchable selects use
///   the hard-cap path.
/// * Filtered fetch carries `limit(MAX+1)` and fails with `Overflow` past the cap instead of
///   scanning the table — one bounded round-trip per keystroke burst, never the whole table.
/// * Policy mirrors the base load: `ViewAny` + tenant gate fail closed (`Denied`), rows filter
///   through `View` before labels.
/// * `q` is clamped to [`crate::query_term::MAX_QUERY_TERM`] chars (same bound as `?q=`), trimmed.
/// * One bounded round-trip per call, never the whole table; not memoized (`q` is unbounded per
///   keystroke, and the endpoint serves one field and one term per request, so sharing would only
///   grow the per-request cache).
pub(crate) async fn related_records_search<R>(
    cx: &Cx,
    q: String,
) -> Result<Vec<R::Model>, OptionLoadError>
where
    R: OptionSource,
{
    ensure_option_access::<R>(cx)?;
    let term = crate::query_term::clamp_query_term(&q);
    let mut query = option_query::<R>(cx)?;
    if !term.is_empty() {
        // D1: reuse the related table's declared searchable columns. When it
        // declares none, `search_expr` is None and we fall through unfiltered
        // to the capped exec below (D1a fallback: hard-cap path, `Overflow`
        // on large tables). Non-searchable selects use the hard-cap path.
        if let Some(expr) = R::search_expr(cx, &term) {
            query = query.filter(expr);
        }
    }
    // The declared default ordering only — the first sortable column, asc, or
    // nothing. Deliberately not a list-mode resolution: the
    // option search has no table state, and the PK fallback a paginated table
    // would add is an ordering change this endpoint never had.
    if let Some(ord) = R::order_by(cx) {
        query = query.order_by(ord);
    }
    bounded_options::<R>(
        cx,
        query,
        "relationship option search failed",
        "relationship option search overflows the cap",
    )
    .await
}

/// Outcome of the targeted existence check for overflowed selects (D4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RelatedCheck {
    /// PK parses and resolves through the tenant-scoped query and passes
    /// `View`.
    FoundViewable,
    /// PK resolves but `View` is refused (maps to "invalid", never leaks).
    FoundHidden,
    /// PK does not parse or no row matches (maps to "invalid").
    NotFound,
}

/// Targeted FK existence check, through `ex`.
///
/// Validates one submitted key directly: parse via `pk_eq_expr`, fetch through
/// the tenant-scoped query, then `View`. The pre-write validation runs it for
/// an overflowed set, which membership in the bounded set cannot validate, and
/// the write runs it for every relationship key inside its transaction, so the
/// key is checked against the rows the write sees.
/// * `Denied` when `ViewAny` is refused or tenant is missing (maps to "not available").
/// * `LoadFailed` on driver failure (maps to retry).
/// * `Ok(FoundViewable/FoundHidden/NotFound)` otherwise.
/// * Single targeted round-trip per call, not memoized (one value per validation; sharing would
///   only grow the per-request cache).
pub(crate) async fn related_record_check<R>(
    cx: &Cx,
    value: String,
    ex: &mut dyn toasty::Executor,
) -> Result<RelatedCheck, OptionLoadError>
where
    R: OptionSource,
{
    ensure_option_access::<R>(cx)?;
    let trimmed = value.trim();
    let Some(expr) = crate::schema::pk_eq_expr::<R::Model>(trimmed) else {
        return Ok(RelatedCheck::NotFound);
    };
    let row = option_query::<R>(cx)?
        .filter(expr)
        .first()
        .exec(ex)
        .await
        .map_err(|e| {
            tracing::warn!(
                resource = R::slug(),
                error = %e,
                "relationship option check failed"
            );
            OptionLoadError::LoadFailed
        })?;
    match row {
        None => Ok(RelatedCheck::NotFound),
        Some(record) => {
            if R::policy().allows(cx, Ability::View(&record)) {
                Ok(RelatedCheck::FoundViewable)
            } else {
                Ok(RelatedCheck::FoundHidden)
            }
        }
    }
}

#[cfg(test)]
mod tests;
