//! Record fetchers through the tenancy-scoped query seam.
//!
//! One row by URL key — parsed against the model's primary-key type, so a
//! malformed or unknown id is a 404 — plus the policy-checked loader the
//! record pages share.

use topcoat::{Result, context::Cx};

use crate::{
    error::TabloError,
    policy::{Ability, can},
    resource::Resource,
};

/// Fetch one record by its URL `id` through the tenancy-scoped query seam.
///
/// The string id is parsed against the model's primary-key type and the PK
/// filter is ANDed onto the tenant-scoped
/// [`scoped_query`](crate::resource::scoped_query) (ADR-0002), so
/// tenancy and soft-delete scoping both hold. Fetches the one row by key
/// instead of loading every row and matching keys in memory — O(N) rows per
/// edit/delete, leaking the whole table before the policy check.
///
/// A malformed or unknown id maps to 404, not a query error.
///
/// Runs on the caller's executor: mutation handlers pass the open framework
/// transaction so the fetched snapshot is the checked snapshot.
pub(crate) async fn find_by_key<R: Resource>(
    cx: &Cx,
    id: &str,
    ex: &mut dyn toasty::Executor,
) -> Result<R::Model> {
    find_by_key_in::<R>(id, ex, || crate::resource::scoped_query::<R>(cx)).await
}

/// The error a resource with a composite primary key reports when the URL or
/// batch carries no single-key representation. `None` means the model
/// has a single-column key.
pub(super) fn composite_pk_error<R: Resource>() -> Option<topcoat::Error> {
    if !crate::toasty_compat::pk::pk_is_composite::<R::Model>() {
        return None;
    }
    tracing::error!(
        resource = R::slug(),
        "composite primary key has no URL representation"
    );
    Some(
        TabloError::Declaration(format!(
            "resource '{}' has a composite primary key, which has no URL representation",
            R::slug()
        ))
        .into(),
    )
}

/// The shared body of [`find_by_key`] and [`load_detail`]: parse the URL id
/// against the model's primary key, then fetch the one row through `seed`.
///
/// `seed` is a closure so the composite-PK misdeclaration is reported before
/// the scoped query is built.
async fn find_by_key_in<R: Resource>(
    id: &str,
    ex: &mut dyn toasty::Executor,
    seed: impl FnOnce() -> Result<toasty::stmt::Query<toasty::stmt::List<R::Model>>>,
) -> Result<R::Model> {
    let Some(expr) = crate::toasty_compat::pk::pk_eq_expr::<R::Model>(id) else {
        // Composite PKs have no URL representation: fail loudly so
        // the misconfiguration surfaces instead of 404ing every id.
        if let Some(error) = composite_pk_error::<R>() {
            return Err(error);
        }
        return Err(topcoat::router::error::not_found().into());
    };
    seed()?
        .filter(expr)
        .first()
        .exec(&mut *ex)
        .await
        .map_err(crate::error::unavailable)?
        .ok_or_else(topcoat::router::error::not_found)
        .map_err(Into::into)
}

/// Load the record the request names, scoped and policy-checked.
///
/// Reads the `{id}` path param, loads through the tenant-scoped query (which
/// turns an unknown *or* out-of-scope id into one 404), and returns 403 unless
/// `View` accepts the loaded snapshot.
///
/// Callers run [`gate`](super::super::gate) first and add their own policy on
/// top (`Update` for the edit page).
pub(crate) async fn load_viewable<R: Resource>(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
) -> Result<R::Model> {
    let id = topcoat::router::path_param_segment(cx, "id").to_string();
    check_viewable::<R>(cx, find_by_key::<R>(cx, &id, ex).await?)
}

/// [`load_viewable`] for the detail page: the same key, scope and policy, over
/// [`Resource::view_query`] so the relations the page reads off the record
/// arrive loaded. The caller
/// 404s a resource that declares no detail page (`R::viewed`).
pub(crate) async fn load_detail<R: Resource>(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
) -> Result<R::Model> {
    let id = topcoat::router::path_param_segment(cx, "id").to_string();
    let record =
        find_by_key_in::<R>(&id, ex, || crate::resource::scoped_view_query::<R>(cx)).await?;
    check_viewable::<R>(cx, record)
}

/// 403 unless `View` accepts the loaded snapshot.
fn check_viewable<R: Resource>(cx: &Cx, record: R::Model) -> Result<R::Model> {
    if !can::<R>(cx, Ability::View(&record)) {
        return Err(topcoat::router::error::forbidden().into());
    }
    Ok(record)
}

#[cfg(test)]
mod tests;
