//! What a scoped relation column runs once a page's rows load: it drops the related records its
//! source does not show the user.

use std::{collections::HashSet, sync::Arc};

use toasty::{Deferred, Executor};
use topcoat::{Result, context::Cx};

use super::ToOneRelation;
use crate::{BoxFuture, policy::Ability, schema::OptionSource, toasty_compat::pk};

/// Drops from a page's rows the related records a source does not show the user, through the
/// executor that loaded them.
pub(super) type Restrict<M> = Arc<
    dyn for<'a> Fn(&'a Cx, &'a mut [M], &'a mut dyn Executor) -> BoxFuture<'a, Result<()>>
        + Send
        + Sync,
>;

/// Drops a to-one relation's record from each row unless `S` shows it.
pub(super) fn restrict_one<M, S, F>(read: fn(&M) -> &F, write: fn(&mut M) -> &mut F) -> Restrict<M>
where
    M: Send + Sync + 'static,
    S: OptionSource,
    F: ToOneRelation<S::Model> + 'static,
{
    Arc::new(move |cx, rows, ex| {
        Box::pin(async move {
            let shown = {
                let loaded: Vec<&S::Model> = rows
                    .iter()
                    .filter_map(|row| read(row).loaded().flatten())
                    .collect();
                shown_keys::<S>(cx, &loaded, ex).await?
            };
            for row in rows.iter_mut() {
                write(row).retain(&|record| shown.contains(&pk::pk_text(record)));
            }
            Ok(())
        })
    })
}

/// Drops from each row's `has_many` list the records `S` does not show.
pub(super) fn restrict_many<M, S>(
    read: fn(&M) -> &Deferred<Vec<S::Model>>,
    write: fn(&mut M) -> &mut Deferred<Vec<S::Model>>,
) -> Restrict<M>
where
    M: Send + Sync + 'static,
    S: OptionSource,
{
    Arc::new(move |cx, rows, ex| {
        Box::pin(async move {
            let shown = {
                let loaded: Vec<&S::Model> = rows
                    .iter()
                    .map(read)
                    .filter(|records| !records.is_unloaded())
                    .flat_map(|records| records.get())
                    .collect();
                shown_keys::<S>(cx, &loaded, ex).await?
            };
            for row in rows.iter_mut() {
                let records = write(row);
                if !records.is_unloaded() {
                    let kept: Vec<S::Model> = std::mem::take(records)
                        .into_inner()
                        .into_iter()
                        .filter(|record| shown.contains(&pk::pk_text(record)))
                        .collect();
                    *records = Deferred::from(kept);
                }
            }
            Ok(())
        })
    })
}

/// How many keys one visibility query names, under every driver's bound-parameter limit.
const KEYS_PER_QUERY: usize = 500;

/// The keys of the `records` that `S` shows the user: its tenant-scoped query returns them and its
/// policy allows viewing them.
async fn shown_keys<S: OptionSource>(
    cx: &Cx,
    records: &[&S::Model],
    ex: &mut dyn Executor,
) -> Result<HashSet<String>> {
    let mut shown = HashSet::new();
    if records.is_empty() || !S::allows(cx, Ability::ViewAny) {
        return Ok(shown);
    }
    let mut seen = HashSet::new();
    let distinct: Vec<&S::Model> = records
        .iter()
        .copied()
        .filter(|record| seen.insert(pk::pk_text(*record)))
        .collect();
    for chunk in distinct.chunks(KEYS_PER_QUERY) {
        let rows = S::scoped_query(cx)?
            .filter(pk::pk_set_filter(chunk))
            .exec(&mut *ex)
            .await
            .map_err(crate::error::unavailable)?;
        shown.extend(
            rows.iter()
                .filter(|record| S::allows(cx, Ability::View(record)))
                .map(pk::pk_text),
        );
    }
    Ok(shown)
}
