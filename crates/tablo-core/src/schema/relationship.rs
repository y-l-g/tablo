//! Relationship option loading — bounded, memoized, policy-checked.
//!
//! Every relationship choice field over one source shares a single bounded load per `(request,
//! tenant)` and fails closed instead of leaking labels. A choice offers the rows the source's
//! list shows: its scoped query's, when its policy allows listing them.

use toasty::stmt::{Expr, List, OrderByExpr, Query};
use topcoat::{Result, context::Cx};

use crate::BoxFuture;

/// Describes the source a relationship option loader reads.
///
/// Every [`Resource`](crate::Resource) is one, answering from its def as the context's panel
/// mounted it.
///
/// Every relationship choice over one source shares a single load per request and tenant,
/// bounded to [`MAX_RELATIONSHIP_OPTIONS`] rows of its [`scoped_query`](Self::scoped_query). A
/// load the source refuses fails closed: the choice offers nothing rather than leak a label.
pub trait OptionSource: Sized + Send + Sync + 'static {
    /// The model whose rows become options.
    type Model: toasty::schema::Model + toasty::stmt::IntoExpr<Self::Model> + Send + Sync + 'static;

    /// States the tenant-scoped seed query every option load starts from and reports an unscopable
    /// source as misdeclared.
    fn scoped_query(cx: &Cx) -> Result<Query<List<Self::Model>>>;

    /// Whether the current user may list the source's rows; refusing fails every load closed.
    /// Defaults to refusing. The rows the user may not see belong out of
    /// [`scoped_query`](Self::scoped_query).
    fn lists(_cx: &Cx) -> bool {
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

    /// Labels `record` wherever a relationship choice offers it or a
    /// [`RelationColumn::of`](crate::RelationColumn::of) shows it.
    ///
    /// A [`Resource`](crate::Resource) answers with its def's
    /// [`record_title`](crate::ResourceDef::record_title), else its label and the record's key.
    /// The record is loaded without its relations.
    fn label(cx: &Cx, record: &Self::Model) -> String;

    /// Whether the context's panel can load from the source: a resource only when the panel
    /// mounts it.
    #[doc(hidden)]
    fn available(_cx: &Cx) -> bool {
        true
    }

    /// The root fields of the source's records the request may not view: for a resource, the
    /// ones the panel's [`field_access`](crate::ResourceDef::field_access) hides from the request.
    #[doc(hidden)]
    fn hidden_fields(_cx: &Cx) -> Vec<String> {
        Vec::new()
    }

    /// Whether the source has a create page a choice could open: only a resource does.
    #[doc(hidden)]
    fn creates() -> bool {
        false
    }

    /// The source's create page, when the context's panel mounts it with one and lets the user
    /// create.
    #[doc(hidden)]
    fn create_page(_cx: &Cx) -> Option<String> {
        None
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

pub(crate) type RelationshipLoader = std::sync::Arc<
    dyn Fn(&Cx) -> BoxFuture<'static, Result<Vec<(String, String)>, OptionLoadError>> + Send + Sync,
>;

pub(crate) type RelationshipSearchLoader = std::sync::Arc<
    dyn Fn(
            &Cx,
            String,
            Option<Expr<bool>>,
        ) -> BoxFuture<'static, Result<Vec<(String, String)>, OptionLoadError>>
        + Send
        + Sync,
>;

pub(crate) type RelationshipChecker = std::sync::Arc<
    dyn for<'a> Fn(
            &'a Cx,
            String,
            Option<Expr<bool>>,
            &'a mut dyn toasty::Executor,
        ) -> BoxFuture<'a, Result<bool, OptionLoadError>>
        + Send
        + Sync,
>;

/// Refuses the option load closed when the source refuses the list or a tenant-owned source gets
/// a tenantless request.
pub(crate) fn ensure_option_access<R>(cx: &Cx) -> Result<(), OptionLoadError>
where
    R: OptionSource,
{
    if !R::lists(cx) {
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

/// Loads option records for one related resource, memoized per request, and checks the cap.
#[topcoat::context::memoize(as_ref)]
pub(crate) async fn related_records<R>(
    cx: &Cx,
    _tenant: Option<crate::TenantId>,
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

/// Fetches one row past the cap and refuses a set over the cap with `Overflow`.
async fn bounded_options<R>(
    cx: &Cx,
    query: Query<List<R::Model>>,
    failed: &str,
    overflow: &str,
) -> Result<Vec<R::Model>, OptionLoadError>
where
    R: OptionSource,
{
    let mut db = crate::db::db(cx).map_err(|_| OptionLoadError::Misdeclared)?;
    let records = query
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
    Ok(records)
}

/// Longest accepted search term, in chars; a relationship's option search and a table's `?q=`
/// search share it.
pub(crate) const MAX_QUERY_TERM: usize = 128;

/// Clamps a search term to [`MAX_QUERY_TERM`] chars, so a multibyte term truncates on a char
/// boundary.
pub(crate) fn clamp_query_term(term: &str) -> String {
    term.trim().chars().take(MAX_QUERY_TERM).collect()
}

/// Searches the related table's declared searchable columns, among the rows `scope` selects, in
/// one bounded round-trip and fails past the cap with `Overflow`.
pub(crate) async fn related_records_search<R>(
    cx: &Cx,
    q: String,
    scope: Option<Expr<bool>>,
) -> Result<Vec<R::Model>, OptionLoadError>
where
    R: OptionSource,
{
    ensure_option_access::<R>(cx)?;
    let term = clamp_query_term(&q);
    let mut query = option_query::<R>(cx)?;
    if let Some(scope) = scope {
        query = query.filter(scope);
    }
    if !term.is_empty()
        && let Some(expr) = R::search_expr(cx, &term)
    {
        query = query.filter(crate::toasty_compat::case_insensitive_search(
            &crate::db::db(cx).map_err(|_| OptionLoadError::Misdeclared)?,
            expr,
        ));
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

/// Whether one submitted key names a row of the source's scoped query among the rows `scope`
/// selects, read through the given executor.
pub(crate) async fn related_record_check<R>(
    cx: &Cx,
    value: String,
    scope: Option<Expr<bool>>,
    ex: &mut dyn toasty::Executor,
) -> Result<bool, OptionLoadError>
where
    R: OptionSource,
{
    ensure_option_access::<R>(cx)?;
    let trimmed = value.trim();
    let Some(expr) = crate::toasty_compat::pk::pk_eq_expr::<R::Model>(trimmed) else {
        return Ok(false);
    };
    let mut query = option_query::<R>(cx)?.filter(expr);
    if let Some(scope) = scope {
        query = query.filter(scope);
    }
    let row = query.first().exec(ex).await.map_err(|e| {
        tracing::warn!(
            resource = std::any::type_name::<R>(),
            error = %e,
            "relationship option check failed"
        );
        OptionLoadError::LoadFailed
    })?;
    Ok(row.is_some())
}

/// The keys among `keys` naming a row `R`'s scoped query holds, read in one query: what a
/// relationship filter admits past the cap. Memoized per request, so the list and its filter menu
/// ask once.
#[topcoat::context::memoize(as_ref)]
pub(crate) async fn held_keys<R>(
    cx: &Cx,
    _tenant: Option<crate::TenantId>,
    keys: &[String],
) -> Result<std::collections::HashSet<String>, OptionLoadError>
where
    R: OptionSource,
{
    ensure_option_access::<R>(cx)?;
    let Some(any) = keys
        .iter()
        .filter_map(|key| crate::toasty_compat::pk::pk_eq_expr::<R::Model>(key))
        .reduce(|any, one| any.or(one))
    else {
        return Ok(std::collections::HashSet::new());
    };
    let rows = option_query::<R>(cx)?
        .filter(any)
        .exec(&mut crate::db::db(cx).map_err(|_| OptionLoadError::Misdeclared)?)
        .await
        .map_err(|e| {
            tracing::warn!(
                resource = std::any::type_name::<R>(),
                error = %e,
                "relationship filter check failed"
            );
            OptionLoadError::LoadFailed
        })?;
    Ok(rows.iter().map(crate::toasty_compat::pk::pk_text).collect())
}

#[cfg(test)]
mod tests;
