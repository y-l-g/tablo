# 0029 Soft deletes are a declared column the framework scopes

A resource that keeps deleted records declares the column a delete stamps,
`ResourceDef::soft_delete(lens!(Post.deleted_at))`, a lens to an `Option<jiff::Timestamp>`. That one
declaration gives the delete that stamps it, the live-only default of every loader, the list's
trash control, and Restore and Delete permanently on the trashed records.

The mount owns the scope as it owns tenancy (ADR-0002): `scoped_query_in(cx, Trashed)` ANDs the
tenant filter and the trash predicate after the `ResourceDef::query` narrowings, and `scoped_query` is its
`Trashed::Without`. A restore and a force delete load through `Trashed::Only` under the same
tenant filter, so neither is a second, hand-built query that could drop it. The detail page loads
`Trashed::With`, so the trash list's rows link somewhere; the edit page and every other mutation
load `Trashed::Without`, so a trashed record is restored before it is written again. The unique
probe reads `Trashed::With`, since a database index covers the trashed rows.

Restore and Delete permanently are the pipeline's own action entries (ADR-0004), placed like a
custom action on the row, the detail page and the bulk bar, and named `restore` and
`force-delete`, which a custom action of the resource cannot share. Each entry names the records
it takes, live or trashed: a row offers the entries that take it, a selection loads both and
skips the records an entry does not take as it skips the ones its policy refuses, and the bulk bar
offers the entries that take the records the trash control shows. The policy decides through
`RestoreAny`, `Restore`, `ForceDeleteAny` and `ForceDelete`, and the record fns `restore_record`
and `force_delete_record` write, as `delete_record` does.

The trash control is not a `Filter` predicate: a filter ANDs onto the base query, and the control
replaces the live-only default the base query holds. It renders in the filter menu and posts
`f.trashed`, which the list and the export read to pick the base query.

## Rejected

- Soft deletes as a `ResourceDef::query` narrowing with a hand-written restore: the restore needs the
  tenant filter, which only the mount builds, and the trashed records reach no page.
- A trashed filter ANDed onto a query that admits every record: every loader but the list then
  needs to re-add the live-only predicate, and forgetting it serves trashed rows.
- A boolean column: the time of a delete is what an audit asks first, and one type keeps the
  predicate one shape.
- A force delete of a live record: a hard delete is two deliberate steps, and a resource that
  wants one step does not soft-delete.
- Bulk record fns for restore and force delete: each loops its one-record fn in the transaction,
  as `bulk_delete_records` does by default, and an override of the one-record fn covers both.
