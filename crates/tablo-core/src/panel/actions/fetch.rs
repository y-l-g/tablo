//! Record fetchers through the tenancy-scoped query seam.
//!
//! One row by URL key — parsed against the model's primary-key type, so a
//! malformed or unknown id is a 404 — plus the policy-checked loader the
//! record pages share.

use topcoat::{Result, context::Cx};

use crate::resource::Resource;

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

/// [`find_by_key`] for a loader that reads only the record's own columns: the
/// same PK filter over
/// [`scoped_query_with`](crate::resource::scoped_query_with) with an empty
/// [`IncludeNeeds`](crate::resource::IncludeNeeds), so the edit handler and
/// delete do not load the relations the record's list or detail page reads. A
/// resource that overrides nothing keeps its full
/// [`query`](crate::resource::Resource::query).
pub(crate) async fn find_by_key_narrowed<R: Resource>(
    cx: &Cx,
    id: &str,
    ex: &mut dyn toasty::Executor,
) -> Result<R::Model> {
    find_by_key_in::<R>(id, ex, || {
        crate::resource::scoped_query_with::<R>(cx, &crate::resource::IncludeNeeds::default())
    })
    .await
}

/// The error a resource with a composite primary key reports when the URL or
/// batch carries no single-key representation. `None` means the model
/// has a single-column key.
pub(super) fn composite_pk_error<R: Resource>() -> Option<topcoat::Error> {
    if !crate::schema::pk_is_composite::<R::Model>() {
        return None;
    }
    tracing::error!(
        resource = R::slug(),
        "composite primary key has no URL representation"
    );
    Some(topcoat::Error::from(std::io::Error::other(format!(
        "resource '{}' has a composite primary key, which has no URL representation (GH #95)",
        R::slug()
    ))))
}

/// The shared body of [`find_by_key`] and [`find_by_key_narrowed`]: parse the
/// URL id against the model's primary key, then fetch the one row through
/// `seed`.
///
/// `seed` is a closure so the composite-PK misdeclaration is reported before
/// the scoped query is built.
async fn find_by_key_in<R: Resource>(
    id: &str,
    ex: &mut dyn toasty::Executor,
    seed: impl FnOnce() -> Result<toasty::stmt::Query<toasty::stmt::List<R::Model>>>,
) -> Result<R::Model> {
    let Some(expr) = crate::schema::pk_eq_expr::<R::Model>(id) else {
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
        .map_err(crate::db::unavailable)?
        .ok_or_else(topcoat::router::error::not_found)
        .map_err(Into::into)
}

/// Load the record the request names, scoped and policy-checked.
///
/// Reads the `{id}` path param, loads through the tenant-scoped query (which
/// turns an unknown *or* out-of-scope id into one 404), and returns 403 unless
/// `can_view` accepts the loaded snapshot.
///
/// Callers run [`gate`](super::super::gate) first, add their own policy on top
/// (`can_update` for the edit page), and 404 a page the resource does not
/// declare (`R::viewed` for the view page).
pub(crate) async fn load_viewable<R: Resource>(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
) -> Result<R::Model> {
    load_viewable_in::<R>(cx, ex, false).await
}

/// [`load_viewable`] for a loader that reads only the record's own columns:
/// the edit page hydrates its fields from the record, so it does
/// not load the relations the record's list or detail page reads.
pub(crate) async fn load_viewable_narrowed<R: Resource>(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
) -> Result<R::Model> {
    load_viewable_in::<R>(cx, ex, true).await
}

/// The shared body of [`load_viewable`] and [`load_viewable_narrowed`]: read
/// the `{id}` path param, load through the tenant-scoped query, then 403
/// unless `can_view` accepts the snapshot.
///
/// `narrowed` selects the loader that reads only the record's own columns.
async fn load_viewable_in<R: Resource>(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    narrowed: bool,
) -> Result<R::Model> {
    let id = topcoat::router::path_param_segment(cx, "id").to_string();
    let record = if narrowed {
        find_by_key_narrowed::<R>(cx, &id, ex).await?
    } else {
        find_by_key::<R>(cx, &id, ex).await?
    };
    if !R::can_view(cx, &record) {
        return Err(topcoat::router::error::forbidden().into());
    }
    Ok(record)
}

#[cfg(test)]
mod tests;
