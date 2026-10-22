//! The built-in entries: delete, and the restore and force delete of a soft-deleting resource.

use topcoat::context::Cx;

use super::{ActionEntry, ErasedInput, InputSpec, Places};
use crate::{
    BoxFuture,
    policy::Ability,
    resource::{Change, Resource, Trashed, WriteError},
};

impl<R: Resource> ActionEntry<R> {
    /// Restore, on the trashed records [`Ability::Restore`] allows.
    pub(super) fn restore() -> Self {
        Self {
            name: "restore",
            label: |_| "Restore".to_string(),
            places: Places::ALL,
            resource_wide: Ability::RestoreAny,
            can_run: |resource, cx, record| resource.can(cx, Ability::Restore(record)),
            input: InputSpec::none(),
            run: restore_erased::<R>,
            success: |_, count| {
                if count == 1 {
                    "Restored".to_string()
                } else {
                    format!("Restored {count} records")
                }
            },
            change: Change::Restore,
            failure: "restore the records",
            confirm: false,
            trashed: Trashed::Only,
            removes: false,
            prompt: "Restore this record?",
        }
    }

    /// Force delete, on the trashed records [`Ability::ForceDelete`] allows.
    pub(super) fn force_delete() -> Self {
        Self {
            name: "force-delete",
            label: |_| "Delete permanently".to_string(),
            places: Places::ALL,
            resource_wide: Ability::ForceDeleteAny,
            can_run: |resource, cx, record| resource.can(cx, Ability::ForceDelete(record)),
            input: InputSpec::none(),
            run: force_delete_erased::<R>,
            success: |_, count| {
                if count == 1 {
                    "Deleted permanently".to_string()
                } else {
                    format!("Deleted {count} records permanently")
                }
            },
            change: Change::ForceDelete,
            failure: "delete the records permanently",
            confirm: true,
            trashed: Trashed::Only,
            removes: true,
            prompt: "Delete this record permanently?",
        }
    }

    /// One record's Delete, from its row or its detail or edit page: [`Resource::delete_record`],
    /// on a record [`Ability::Delete`] allows.
    pub(crate) fn delete() -> Self {
        Self::deleting(false)
    }

    /// The bulk bar's Delete: [`Resource::bulk_delete_records`], on the selected records
    /// [`Ability::Delete`] allows.
    pub(crate) fn bulk_delete() -> Self {
        Self::deleting(true)
    }

    fn deleting(bulk: bool) -> Self {
        Self {
            name: "delete",
            label: |_| "Delete".to_string(),
            places: if bulk { Places::BULK } else { Places::RECORD },
            resource_wide: Ability::DeleteAny,
            can_run: |resource, cx, record| resource.can(cx, Ability::Delete(record)),
            input: InputSpec::none(),
            run: if bulk {
                bulk_delete_erased::<R>
            } else {
                delete_erased::<R>
            },
            success: if bulk {
                |_, _| "Bulk deleted".to_string()
            } else {
                |_, _| "Deleted".to_string()
            },
            change: Change::Delete,
            failure: if bulk {
                "delete the selected rows"
            } else {
                "delete the record"
            },
            confirm: true,
            trashed: Trashed::Without,
            removes: true,
            prompt: "Delete this record?",
        }
    }
}

/// [`Resource::delete_record`] on each of `records`, behind a function pointer.
fn delete_erased<'a, R: Resource>(
    cx: &'a Cx,
    records: &'a [R::Model],
    _input: ErasedInput,
    ex: &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<crate::Outcome, WriteError>> {
    Box::pin(async move {
        for record in records {
            R::delete_record(cx, record, &mut *ex).await?;
        }
        Ok(crate::Outcome::done())
    })
}

/// [`Resource::restore_record`] on each of `records`, behind a function pointer.
fn restore_erased<'a, R: Resource>(
    cx: &'a Cx,
    records: &'a [R::Model],
    _input: ErasedInput,
    ex: &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<crate::Outcome, WriteError>> {
    Box::pin(async move {
        for record in records {
            R::restore_record(cx, record, &mut *ex).await?;
        }
        Ok(crate::Outcome::done())
    })
}

/// [`Resource::force_delete_record`] on each of `records`, behind a function pointer.
fn force_delete_erased<'a, R: Resource>(
    cx: &'a Cx,
    records: &'a [R::Model],
    _input: ErasedInput,
    ex: &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<crate::Outcome, WriteError>> {
    Box::pin(async move {
        for record in records {
            R::force_delete_record(cx, record, &mut *ex).await?;
        }
        Ok(crate::Outcome::done())
    })
}

/// [`Resource::bulk_delete_records`] behind a function pointer.
fn bulk_delete_erased<'a, R: Resource>(
    cx: &'a Cx,
    records: &'a [R::Model],
    _input: ErasedInput,
    ex: &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<crate::Outcome, WriteError>> {
    Box::pin(async move {
        R::bulk_delete_records(cx, records, ex).await?;
        Ok(crate::Outcome::done())
    })
}
