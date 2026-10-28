//! Loading a mutation's targets, and refusing a target the mutation takes no record of.

use toasty::stmt::{List, Query};
use topcoat::{
    context::Cx,
    router::error::{bad_request, forbidden, not_found, see_other},
};

use super::{
    super::{
        super::gate::landing_url,
        fetch::{composite_pk_error, find_trashed_by_key},
    },
    Target,
};
use crate::{
    notification::{Notification, set_notification},
    resource::{ActionEntry, Mounted, Resource, Trashed},
    table::TableState,
};

/// Loads the records `ids` names through the tenant-scoped query inside the transaction, 404ing
/// unless every one is found.
///
/// One record loads as the mutation takes it, live or trashed. A selection loads both, since the
/// bulk bar selects across them, and the pipeline skips the ones the mutation does not take as it
/// skips the ones its policy refuses.
pub(super) async fn load_targets<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    ids: &[String],
    target: Target,
    trashed: Trashed,
    tx: &mut toasty::Transaction<'_>,
) -> Result<Vec<R::Model>, topcoat::Error> {
    let trashed = match target {
        Target::Record => trashed,
        Target::Bulk => Trashed::With,
    };
    if let Target::Record = target {
        return Ok(vec![
            find_trashed_by_key(cx, resource, &ids[0], trashed, tx).await?,
        ]);
    }
    let keys: Vec<&str> = ids.iter().map(String::as_str).collect();
    let Some(pk_filter) = crate::toasty_compat::pk::pk_in_expr::<R::Model>(&keys) else {
        if let Some(error) = composite_pk_error(resource) {
            return Err(error);
        }
        return Err(not_found().into());
    };
    let rows = resource
        .scoped_query_in(cx, trashed)?
        .filter(pk_filter)
        .exec(tx)
        .await
        .map_err(crate::error::unavailable)?;
    if rows.len() != ids.len() {
        return Err(not_found().into());
    }
    Ok(rows)
}

/// The records a select-all-matching POST names: the resource's list query narrowed by the search
/// and filters `query`, the list's query string, spells.
///
/// It runs before the transaction opens, since a filter's check may query. A filter the list
/// would ignore answers 400 rather than widen the selection to records the list did not show.
pub(super) async fn matching_query<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    query: &str,
) -> Result<Query<List<R::Model>>, topcoat::Error> {
    let table = resource.table_for(cx);
    let state = table.normalize_state(&TableState::from_query(query));
    let refused = table.refused_filters(cx, &state).await?;
    if !table.unapplied_filters(cx, &state, &refused).is_empty() {
        return Err(bad_request("the selection names a filter the list does not apply").into());
    }
    let list = resource.list_query(cx, &state)?;
    Ok(table.narrow(cx, &crate::db::db(cx)?, list, &state, &refused))
}

/// Loads the records `query` selects inside the transaction, or `None` when more than `max` do.
pub(super) async fn load_matching<M>(
    query: Query<List<M>>,
    max: usize,
    tx: &mut toasty::Transaction<'_>,
) -> Result<Option<Vec<M>>, topcoat::Error>
where
    M: toasty::schema::Model,
{
    let rows = query
        .limit(max.saturating_add(1))
        .exec(tx)
        .await
        .map_err(crate::error::unavailable)?;
    Ok((rows.len() <= max).then_some(rows))
}

/// Answers a target the mutation refuses on every record: a row with 403, a selection with an
/// error notification naming `refused` records.
pub(super) fn refuse<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    action: &ActionEntry<R>,
    target: Target,
    refused: usize,
) -> topcoat::Error {
    match target {
        Target::Record => forbidden().into(),
        Target::Bulk => {
            set_notification(
                cx,
                Notification::error(crate::t!(
                    cx,
                    "tablo-action-refused-all",
                    action = (action.label)(),
                    count = refused
                )),
            );
            see_other(landing_url(cx, &resource.url)).into()
        }
    }
}
