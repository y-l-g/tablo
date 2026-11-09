//! Which related records a scoped relation column shows: once a page's rows load, it checks the
//! records they hold against its source and records the answer for the request; a cell then shows
//! only the records the request recorded as shown. The rows themselves stay as loaded, so a policy
//! or a title reading the same relation sees what the query returned.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock, Mutex},
};

use toasty::{Deferred, Executor};
use topcoat::{Result, context::Cx};

use super::ToOneRelation;
use crate::{
    BoxFuture,
    schema::{OptionSource, ensure_option_access},
    toasty_compat::pk,
};

/// Checks against a source the related records a page's rows hold, through the executor that
/// loaded them.
pub(super) type Restrict<M> = Arc<
    dyn for<'a> Fn(&'a Cx, &'a [M], &'a mut dyn Executor) -> BoxFuture<'a, Result<()>>
        + Send
        + Sync,
>;

/// Checks the record each row's to-one relation holds.
pub(super) fn restrict_one<M, S, F>(read: fn(&M) -> &F) -> Restrict<M>
where
    M: Send + Sync + 'static,
    S: OptionSource,
    F: ToOneRelation<S::Model> + 'static,
{
    Arc::new(move |cx, rows, ex| {
        Box::pin(async move {
            let loaded: Vec<&S::Model> = rows
                .iter()
                .filter_map(|row| read(row).loaded().flatten())
                .collect();
            check::<S>(cx, &loaded, ex).await
        })
    })
}

/// Checks the records each row's `has_many` list holds.
pub(super) fn restrict_many<M, S>(read: fn(&M) -> &Deferred<Vec<S::Model>>) -> Restrict<M>
where
    M: Send + Sync + 'static,
    S: OptionSource,
{
    Arc::new(move |cx, rows, ex| {
        Box::pin(async move {
            let loaded: Vec<&S::Model> = rows
                .iter()
                .map(read)
                .filter(|records| !records.is_unloaded())
                .flat_map(|records| records.get())
                .collect();
            check::<S>(cx, &loaded, ex).await
        })
    })
}

/// Whether the request recorded `record` as one `S` shows the user. A record no load checked
/// reads as hidden, so a row loaded around [`restrict`](crate::column::Column::restrict) shows
/// none.
pub(super) fn shows<S: OptionSource>(cx: &Cx, record: &S::Model) -> bool {
    checked::<S>(cx, crate::tenancy::tenant_id(cx))
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&pk::pk_text(record))
        .copied()
        .unwrap_or(false)
}

/// The records of `S` the request has checked, by primary key, and whether `S` shows each. One
/// answer holds for the whole request, so every column over `S` shares it and checks a record
/// once.
#[derive(Default)]
pub(super) struct Checked(Mutex<HashMap<String, bool>>);

/// The request's [`Checked`] records of `S` in `tenant`.
#[topcoat::context::memoize]
fn checked<S: OptionSource>(cx: &Cx, _tenant: Option<crate::TenantId>) -> Arc<Checked> {
    let _ = cx;
    Arc::default()
}

/// Logs one warning per process for each source a relation column could not load from: its
/// cells render empty, as a record the source hides does, which no status check tells apart.
fn warn_denied(source: &'static str) {
    if first_denial(source) {
        tracing::warn!(
            source,
            "a relation column's source refuses the request, so its cells render empty: let the \
             source list for the user, or use `RelationColumn::unscoped`"
        );
    }
}

/// Whether `source` is denied for the first time in this process.
fn first_denial(source: &'static str) -> bool {
    static DENIED: LazyLock<Mutex<HashSet<&'static str>>> = LazyLock::new(Mutex::default);
    DENIED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(source)
}

/// How many keys one visibility query names, under every driver's bound-parameter limit.
const KEYS_PER_QUERY: usize = 500;

/// Record whether `S` shows each of `records` the request has not checked yet: its scoped query
/// returns it. A source the request may not load from,
/// such as a tenant-scoped one without a tenant, shows none, as its relationship choices offer
/// none.
async fn check<S: OptionSource>(
    cx: &Cx,
    records: &[&S::Model],
    ex: &mut dyn Executor,
) -> Result<()> {
    let store = &checked::<S>(cx, crate::tenancy::tenant_id(cx)).0;
    let lock = || {
        store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    let unchecked: Vec<&S::Model> = {
        let known = lock();
        let mut seen = HashSet::new();
        records
            .iter()
            .copied()
            .filter(|record| {
                let key = pk::pk_text(*record);
                !known.contains_key(&key) && seen.insert(key)
            })
            .collect()
    };
    if unchecked.is_empty() {
        return Ok(());
    }
    let mut answers: HashMap<String, bool> = unchecked
        .iter()
        .map(|record| (pk::pk_text(*record), false))
        .collect();
    if ensure_option_access::<S>(cx).is_err() {
        warn_denied(std::any::type_name::<S>());
    } else {
        for chunk in unchecked.chunks(KEYS_PER_QUERY) {
            let rows = S::scoped_query(cx)?
                .filter(pk::pk_set_filter(chunk))
                .exec(&mut *ex)
                .await
                .map_err(crate::error::unavailable)?;
            for record in &rows {
                answers.insert(pk::pk_text(record), true);
            }
        }
    }
    lock().extend(answers);
    Ok(())
}

#[cfg(test)]
mod tests;
