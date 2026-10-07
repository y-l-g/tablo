//! Relationship option loading — bounded, memoized, policy-checked.
//!
//! Every relationship choice field over one source shares a single bounded load per `(request,
//! tenant)` and fails closed instead of leaking labels.

use toasty::stmt::{Expr, List, OrderByExpr, Query};
use topcoat::{Result, context::Cx};

use crate::policy::Ability;

/// Describes the source a relationship option loader reads.
///
/// Every [`Resource`](crate::Resource) is one, answering from its def as the context's panel
/// mounted it.
pub trait OptionSource: Sized + Send + Sync + 'static {
    /// The model whose rows become options.
    type Model: toasty::schema::Model + toasty::stmt::IntoExpr<Self::Model> + Send + Sync + 'static;

    /// States the tenant-scoped seed query every option load starts from and reports an unscopable
    /// source as misdeclared.
    fn scoped_query(cx: &Cx) -> Result<Query<List<Self::Model>>>;

    /// Whether the current user may see the rows `ability` names; refusing `ViewAny` fails the
    /// whole load closed. Defaults to refusing everything.
    fn allows(_cx: &Cx, _ability: Ability<'_, Self::Model>) -> bool {
        false
    }

    /// Declares whether the source's rows are tenant-owned and fails a tenantless request closed.
    ///
    /// A [`Resource`](crate::Resource) answers from its def's
    /// [`tenancy`](crate::ResourceDef::tenancy). An implementor that is not one declares no def and
    /// no tenant lens the framework could read, so it states the boolean and scopes
    /// [`scoped_query`](Self::scoped_query) itself.
    fn requires_tenant(_cx: &Cx) -> bool {
        false
    }

    /// States the related source's search predicate for `term`, or `None` when it declares no
    /// searchable column.
    fn search_expr(_cx: &Cx, _term: &str) -> Option<Expr<bool>> {
        None
    }

    /// States the related source's declared default ordering.
    fn order_by(_cx: &Cx) -> Option<OrderByExpr> {
        None
    }

    /// Whether the context's panel can load from the source: a resource only when the panel
    /// mounts it.
    #[doc(hidden)]
    fn available(_cx: &Cx) -> bool {
        true
    }
}

/// Distinguishes a policy denial and an over-cap table from a retryable load failure so validation
/// answers correctly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OptionLoadError {
    Denied,
    LoadFailed,
    Overflow,
    Misdeclared,
}

pub(crate) type RelationshipLoadFuture = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<(String, String)>, OptionLoadError>> + Send>,
>;

#[allow(clippy::type_complexity)]
pub(crate) type RelationshipLoader =
    std::sync::Arc<dyn Fn(&Cx) -> RelationshipLoadFuture + Send + Sync>;

#[allow(clippy::type_complexity)]
pub(crate) type RelationshipSearchLoader =
    std::sync::Arc<dyn Fn(&Cx, String) -> RelationshipLoadFuture + Send + Sync>;

pub(crate) type RelationshipCheckFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<RelatedCheck, OptionLoadError>> + Send + 'a>,
>;

#[allow(clippy::type_complexity)]
pub(crate) type RelationshipChecker = std::sync::Arc<
    dyn for<'a> Fn(&'a Cx, String, &'a mut dyn toasty::Executor) -> RelationshipCheckFuture<'a>
        + Send
        + Sync,
>;

/// Refuses the option load closed when `ViewAny` is refused or a tenant-owned source gets a
/// tenantless request.
fn ensure_option_access<R>(cx: &Cx) -> Result<(), OptionLoadError>
where
    R: OptionSource,
{
    if !R::allows(cx, Ability::ViewAny) {
        return Err(OptionLoadError::Denied);
    }
    if R::requires_tenant(cx) && crate::tenancy::tenant_id(cx).is_none() {
        return Err(OptionLoadError::Denied);
    }
    Ok(())
}

/// Starts every option loader from the source's tenant-scoped seed query and reports an unscopable
/// source as misdeclared.
fn option_query<R>(cx: &Cx) -> Result<Query<List<R::Model>>, OptionLoadError>
where
    R: OptionSource,
{
    R::scoped_query(cx).map_err(|error| {
        tracing::error!(
            resource = std::any::type_name::<R>(),
            error = %error,
            "relationship option load cannot scope the related resource"
        );
        OptionLoadError::Misdeclared
    })
}

/// Caps the options a relationship choice field loads instead of scanning a large table per select.
pub const MAX_RELATIONSHIP_OPTIONS: usize = 200;

/// Loads option records for one related resource, memoized per request, and checks the cap on the
/// raw fetch before filtering rows through `View`.
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

/// Fetches one row past the cap, refuses a set over the cap with `Overflow`, and drops rows the
/// caller cannot view.
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
                resource = std::any::type_name::<R>(),
                error = %e,
                "{failed}"
            );
            OptionLoadError::LoadFailed
        })?;
    if records.len() > MAX_RELATIONSHIP_OPTIONS {
        tracing::warn!(
            resource = std::any::type_name::<R>(),
            max = MAX_RELATIONSHIP_OPTIONS,
            "{overflow}"
        );
        return Err(OptionLoadError::Overflow);
    }
    records.retain(|record| R::allows(cx, Ability::View(record)));
    Ok(records)
}

/// Searches the related table's declared searchable columns in one bounded round-trip and fails
/// past the cap with `Overflow`.
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
    if !term.is_empty()
        && let Some(expr) = R::search_expr(cx, &term)
    {
        query = query.filter(expr);
    }
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

/// Reports the outcome of the targeted existence check for overflowed selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RelatedCheck {
    FoundViewable,
    FoundHidden,
    NotFound,
}

/// Checks one submitted key through the given executor against the tenant-scoped query and `View`.
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
    let Some(expr) = crate::toasty_compat::pk::pk_eq_expr::<R::Model>(trimmed) else {
        return Ok(RelatedCheck::NotFound);
    };
    let row = option_query::<R>(cx)?
        .filter(expr)
        .first()
        .exec(ex)
        .await
        .map_err(|e| {
            tracing::warn!(
                resource = std::any::type_name::<R>(),
                error = %e,
                "relationship option check failed"
            );
            OptionLoadError::LoadFailed
        })?;
    match row {
        None => Ok(RelatedCheck::NotFound),
        Some(record) => {
            if R::allows(cx, Ability::View(&record)) {
                Ok(RelatedCheck::FoundViewable)
            } else {
                Ok(RelatedCheck::FoundHidden)
            }
        }
    }
}

#[cfg(test)]
mod tests;
