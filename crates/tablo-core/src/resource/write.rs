//! The derived record writes a resource's create and update default to.

use toasty::{Executor, stmt::IntoInsert};
use topcoat::{Result, context::Cx};

use super::{Mounted, Resource, require_mounted};
use crate::{
    form::{Posted, RecordForm, decode_list},
    tenancy::require_tenant,
    toasty_compat::{join::JoinTable, model::AppSchema, pk::pk_filter},
};

/// The derived create: the form's builder, the request tenant stamped on the
/// resource's [`Tenancy::column`](crate::Tenancy::column), executed through
/// `ex`, then a join row linking the record to each record its many-to-many fields hold.
///
/// A resource whose tenant is inherited ([`Tenancy::via`](crate::Tenancy::via))
/// has nothing to stamp.
///
/// # Errors
///
/// A tenantless request on a tenant-scoped resource (the handler answers 403 first),
/// a `column` tenancy whose lens names no field of the model (the mount refuses it first), a
/// resource the context's panel does not mount, or the driver's error.
pub async fn write_create<R: Resource>(
    cx: &Cx,
    form: R::Form,
    ex: &mut dyn Executor,
) -> Result<R::Model> {
    let resource = require_mounted::<R>(cx)?;
    let links = form.links();
    let mut insert = form.into_create().into_insert();
    match resource.tenancy.column_field() {
        Some(Ok(column)) => {
            insert.set(column.index, toasty::stmt::Value::from(require_tenant(cx)?));
        }
        Some(Err(error)) => {
            return Err(crate::error::declaration(
                crate::DeclarationError::of::<R>(crate::Site::Tenancy, error).to_string(),
            ));
        }
        None => {}
    }
    let record: R::Model = ex
        .exec(insert.into())
        .await
        .map_err(|error| -> topcoat::Error { error.into() })?;
    for (field, keys) in links {
        let join = join_table(cx, &resource, field)?;
        join.link(&record, &keys, &mut *ex)
            .await
            .map_err(|error| -> topcoat::Error { error.into() })?;
    }
    Ok(record)
}

/// The derived update: assign every named field and execute through `ex`, then link the record
/// to the records each named many-to-many field holds, and unlink it from the others.
///
/// A submission that names no field writes nothing. The instance update
/// reloads `record`, so the returned row is the written one.
///
/// # Errors
///
/// The driver's error.
pub async fn write_update<R: Resource>(
    cx: &Cx,
    mut record: R::Model,
    posted: Posted<R::Form>,
    ex: &mut dyn Executor,
) -> Result<R::Model> {
    let links: Vec<_> = posted
        .links()
        .into_iter()
        .filter(|(field, _)| posted.named(*field))
        .collect();
    if !links.is_empty() {
        let resource = require_mounted::<R>(cx)?;
        let mut stored = <R::Form as RecordForm>::hydrate(cx, &record);
        for (field, keys) in links {
            let join = join_table(cx, &resource, field)?;
            let key = form_key(&resource, field);
            if !stored.contains_key(key) {
                // A record loaded without its links, by an app's own update: read them now.
                stored = linked_now::<R>(cx, &record, &mut *ex).await?;
            }
            let held = stored
                .get(key)
                .and_then(|value| decode_list(value))
                .unwrap_or_default();
            let removed: Vec<String> = held
                .iter()
                .filter(|key| !keys.contains(key))
                .cloned()
                .collect();
            let added: Vec<String> = keys.into_iter().filter(|key| !held.contains(key)).collect();
            join.unlink(&record, &removed, &mut *ex)
                .await
                .map_err(|error| -> topcoat::Error { error.into() })?;
            join.link(&record, &added, &mut *ex)
                .await
                .map_err(|error| -> topcoat::Error { error.into() })?;
        }
    }
    // Build the execution future before awaiting: the builder itself is not
    // known to be `Send`, the future `exec_update` returns is.
    {
        let pending = posted
            .into_update(&mut record)
            .map(|update| <R::Form as RecordForm>::exec_update(update, &mut *ex));
        if let Some(pending) = pending {
            pending
                .await
                .map_err(|error| -> topcoat::Error { error.into() })?;
        }
    }
    Ok(record)
}

/// The form key the record form binds `field` to.
fn form_key<R: Resource>(resource: &Mounted<R>, field: <R::Form as RecordForm>::Field) -> &str {
    resource
        .fields
        .iter()
        .find(|claim| claim.field == field)
        .and_then(|claim| claim.keys.first())
        .map_or("", String::as_str)
}

/// The join model behind the many-to-many field `field`.
///
/// # Errors
///
/// A declaration error: no `Db` in `cx`, or a field the mount would have refused.
fn join_table<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    field: <R::Form as RecordForm>::Field,
) -> Result<JoinTable> {
    let key = form_key(resource, field);
    let schema = AppSchema::of(cx).ok_or_else(|| {
        crate::error::declaration(crate::DeclarationErrorKind::MissingDb.to_string())
    })?;
    JoinTable::of::<R::Model>(&schema, key).map_err(|fault| {
        crate::error::declaration(
            crate::DeclarationError::of::<R>(
                crate::Site::Form,
                crate::DeclarationErrorKind::ManyToMany {
                    field: key.to_string(),
                    fault,
                },
            )
            .to_string(),
        )
    })
}

/// `record` as the form spells it, reloaded with the links its many-to-many fields hold.
async fn linked_now<R: Resource>(
    cx: &Cx,
    record: &R::Model,
    ex: &mut dyn Executor,
) -> Result<std::collections::HashMap<String, String>> {
    let reloaded =
        toasty::stmt::Query::<toasty::stmt::List<R::Model>>::all().filter(pk_filter(record));
    let reloaded = <R::Form as RecordForm>::includes()
        .into_vec()
        .into_iter()
        .fold(reloaded, |query, include| query.include(include))
        .first()
        .exec(ex)
        .await
        .map_err(|error| -> topcoat::Error { error.into() })?;
    Ok(reloaded
        .map(|reloaded| <R::Form as RecordForm>::hydrate(cx, &reloaded))
        .unwrap_or_default())
}
