//! The derived record writes a resource's create and update default to.

use toasty::{Executor, stmt::IntoInsert};
use topcoat::{Result, context::Cx};

use super::{Resource, require_mounted};
use crate::{
    error::TabloError,
    form::{Posted, RecordForm},
    tenancy::require_tenant,
};

/// The derived create: the form's builder, the request tenant stamped on the
/// resource's [`Tenancy::column`](crate::Tenancy::column), executed through
/// `ex`.
///
/// A resource whose tenant is inherited ([`Tenancy::via`](crate::Tenancy::via))
/// has nothing to stamp.
///
/// # Errors
///
/// A tenantless request on a tenant-scoped resource (the handler answers 403 first),
/// a misdeclared tenant column (the mount refuses it first), a resource the request's panel does
/// not mount, or the driver's error.
pub async fn write_create<R: Resource>(
    cx: &Cx,
    form: R::Form,
    ex: &mut dyn Executor,
) -> Result<R::Model> {
    let resource = require_mounted::<R>(cx)?;
    let mut insert = form.into_create().into_insert();
    match resource.tenancy.column_field() {
        Some(Ok(column)) => {
            insert.set(
                column.index,
                toasty_core::stmt::Value::from(require_tenant(cx)?),
            );
        }
        Some(Err(error)) => {
            return Err(TabloError::Declaration(
                crate::DeclarationError::of::<R>(crate::Site::Tenancy, error).to_string(),
            )
            .into());
        }
        None => {}
    }
    ex.exec(insert.into())
        .await
        .map_err(|error| -> topcoat::Error { error.into() })
}

/// The derived update: assign every named field and execute through `ex`.
///
/// A submission that names no field writes nothing. The instance update
/// reloads `record`, so the returned row is the written one.
///
/// # Errors
///
/// The driver's error.
pub async fn write_update<R: Resource>(
    _cx: &Cx,
    mut record: R::Model,
    posted: Posted<R::Form>,
    ex: &mut dyn Executor,
) -> Result<R::Model> {
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
