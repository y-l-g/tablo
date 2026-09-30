//! Relationship option loading — bounded, memoized, policy-checked.
//!
//! Every relationship `Select` over one source shares a single bounded
//! load per `(request, tenant)`; policy (`can_view_any`/`can_view` plus
//! the tenant gate) fails the load closed instead of leaking labels.
//!
//! The loaders are generic over [`OptionSource`] — the source surface they
//! read — rather than over `Resource`, so the dependency runs one way:
//! `resource` depends on `schema`, and the blanket impl in
//! [`crate::resource`] makes every `Resource` an option source.

use toasty::stmt::{Expr, List, OrderByExpr, Query};
use topcoat::{Result, context::Cx};

/// Everything the relationship option loaders read from the thing they load
/// options from.
///
/// The loaders ask for the **tenant-scoped** seed query, the narrowed
/// option-load seed, the policy predicates, the tenant declaration, a name for
/// the log fields, and the source's search and ordering expressions. It is a
/// *source* surface rather than a second resource trait, so `schema` names no
/// part of `Resource`: for a `Resource`, the policy and declaration methods
/// forward straight through and the two query methods compose the framework's
/// tenant-scoped query (see the blanket impl in [`crate::resource`]).
///
/// [`scoped_query`](Self::scoped_query) is **required**: a source states its own
/// scope, so a gated source cannot end up unscoped by omission. The rest have
/// the default that keeps a source declaring nothing honest — the policy
/// predicates deny, the search and ordering expressions answer `None`,
/// [`requires_tenant`](Self::requires_tenant) is `false`, and
/// [`slug`](Self::slug) falls back to the type name.
///
/// It is public because [`Select::relationship`](crate::schema::Select::relationship)'s
/// bound names it, and a `pub(crate)` trait there is a `private_bounds` warning.
/// It is not re-exported at the crate root, because its method names are
/// `Resource`'s and a glob import would collide; it is reachable as
/// `schema::OptionSource`. Every [`Resource`](crate::resource::Resource) is one
/// through the blanket impl, so a real resource keeps the tenant gate and
/// derived filter on every option load; implement it directly only for a source
/// that is not a resource, and state the scope.
pub trait OptionSource: Sized + Send + Sync + 'static {
    /// The model whose rows become options.
    type Model: toasty::schema::Model + Send + Sync + 'static;

    /// The **tenant-scoped** seed query every option load starts from.
    ///
    /// Required, and stated by every implementor, so a source cannot be
    /// unscoped by omission. A source that scopes nothing says so in the body —
    /// `Ok(Query::all())`, what a resource's default
    /// [`query`](crate::resource::Resource::query) returns — and a source whose
    /// rows are tenant-owned ANDs the tenant predicate here, which is what
    /// [`Self::requires_tenant`] declares. A `Resource` never writes this by
    /// hand: the blanket impl forwards to the framework's `scoped_query`.
    ///
    /// An `Err` is a **permanent** misdeclaration — a source that declared a
    /// tenant gate the framework cannot satisfy — reported as
    /// `OptionLoadError::Misdeclared` rather than a retryable failure.
    fn scoped_query(cx: &Cx) -> Result<Query<List<Self::Model>>>;

    /// The seed query an **option load** runs.
    ///
    /// An option load renders a value and a label per row. Both projections are
    /// opaque closures the framework cannot inspect, and the loaders read no
    /// relation of their own, so the query only has to carry the related
    /// record's own columns. The default returns [`Self::scoped_query`]
    /// unchanged — a source states its scope once — and narrowing is the
    /// blanket impl's job: for a
    /// [`Resource`](crate::resource::Resource) it forwards to the resource's
    /// needs-aware base query with an empty set, so a resource that overrides
    /// [`query_with`](crate::resource::Resource::query_with) narrows option
    /// loads too, while a resource that overrides nothing keeps its full base
    /// query.
    ///
    /// The contract this states: an option label projects the related record's
    /// own columns. A source whose option label reads a relation cannot declare
    /// that here, and a narrowed source that does so panics in `Deferred::get`.
    fn options_query(cx: &Cx) -> Result<Query<List<Self::Model>>> {
        Self::scoped_query(cx)
    }

    /// Whether the current user may see the source's records at all: `false`
    /// fails the whole option load closed, never an empty set that
    /// validates as "invalid".
    fn can_view_any(_cx: &Cx) -> bool {
        false
    }

    /// Whether the current user may view one loaded row: `false` keeps it out
    /// of the options — and out of validation — before its label renders.
    fn can_view(_cx: &Cx, _record: &Self::Model) -> bool {
        false
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
/// exceeds the option cap. A searchable `Select` degrades to "type to
/// search" instead of a retry error, while a genuine DB failure stays
/// retryable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OptionLoadError {
    /// The related resource denies `can_view_any` (or has no tenant) for
    /// this request.
    Denied,
    /// The driver failed.
    LoadFailed,
    /// The related table overflows the option cap.
    Overflow,
    /// The related resource's tenancy cannot be scoped at all: it requires a
    /// tenant, its model has no derivable `tenant_id`, and it declares no
    /// `tenant_scope`.
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
pub(crate) type RelationshipCheckFuture = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<RelatedCheck, OptionLoadError>> + Send>,
>;

#[allow(clippy::type_complexity)]
pub(crate) type RelationshipChecker =
    std::sync::Arc<dyn Fn(&Cx, String) -> RelationshipCheckFuture + Send + Sync>;

/// Whether the option load may proceed: related `can_view_any` plus the
/// related tenant gate fail closed (`Denied`). Shared by the bounded load,
/// the server-side search, and the targeted check so tenancy isolation never
/// rides on scope resolution in one place and not the others.
fn ensure_option_access<R>(cx: &Cx) -> Result<(), OptionLoadError>
where
    R: OptionSource,
{
    if !R::can_view_any(cx) {
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

/// The relationship option loaders' seed query: [`OptionSource::options_query`]
/// with the load's own error kind.
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
    R::options_query(cx).map_err(|error| {
        tracing::error!(
            resource = R::slug(),
            error = %error,
            "relationship option load cannot scope the related resource (GH #223)"
        );
        OptionLoadError::Misdeclared
    })
}

/// Max options a relationship `Select` will load: the loader carries
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
/// Every relationship `Select` over the same `R` shares one bounded load per
/// `(request, tenant)` instead of scanning the table per select per validate
/// plus re-render scans. `tenant` is an explicit cache key: memoize tracking
/// alone cannot distinguish header-tenanted callers sharing one `Parts`, so
/// tenancy isolation never rides on scope resolution. The tenant-scoped query
/// stays the only data seam; value and label mapping stay in the
/// caller so selects with different projections share the hit.
///
/// Policy is part of the load: `can_view_any` (and the related
/// resource's tenant gate) denies the whole load — fail closed, never an
/// empty set that validates as "invalid"; loaded rows are filtered through
/// `can_view` before any label is rendered.
///
/// The cap is checked on the **raw** bounded fetch, before `can_view`
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
    bounded_options::<R>(
        cx,
        option_query::<R>(cx)?,
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
/// The cap is checked on the **raw** bounded fetch, before `can_view`
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
    records.retain(|record| R::can_view(cx, record));
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
/// * Policy mirrors the base load: `can_view_any` + tenant gate fail closed (`Denied`), rows filter
///   through `can_view` before labels.
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
    // nothing (GH #210 removed `order_bys()`, so this is the column-level
    // helper that replaced it). Deliberately not a list-mode resolution: the
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
    /// `can_view`.
    FoundViewable,
    /// PK resolves but `can_view` denies it (maps to "invalid", never leaks).
    FoundHidden,
    /// PK does not parse or no row matches (maps to "invalid").
    NotFound,
}

/// Targeted FK existence check for overflowed sets (D4).
///
/// Membership in the bounded set cannot validate overflowed selects (the full
/// set exceeds the cap), so validate the submitted value directly: parse via
/// `pk_eq_expr`, fetch through the tenant-scoped query, then `can_view`.
/// * `Denied` when `can_view_any` fails or tenant is missing (maps to "not available").
/// * `LoadFailed` on driver failure (maps to retry).
/// * `Ok(FoundViewable/FoundHidden/NotFound)` otherwise.
/// * Single targeted round-trip per call, not memoized (one value per validation; sharing would
///   only grow the per-request cache).
pub(crate) async fn related_record_check<R>(
    cx: &Cx,
    value: String,
) -> Result<RelatedCheck, OptionLoadError>
where
    R: OptionSource,
{
    ensure_option_access::<R>(cx)?;
    let trimmed = value.trim();
    let Some(expr) = crate::schema::pk_eq_expr::<R::Model>(trimmed) else {
        return Ok(RelatedCheck::NotFound);
    };
    let mut db = crate::db::db(cx);
    let row = option_query::<R>(cx)?
        .filter(expr)
        .first()
        .exec(&mut db)
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
            if R::can_view(cx, &record) {
                Ok(RelatedCheck::FoundViewable)
            } else {
                Ok(RelatedCheck::FoundHidden)
            }
        }
    }
}

#[cfg(test)]
mod tests;
