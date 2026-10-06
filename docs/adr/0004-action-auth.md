# Action authorization is transactional and per-record

Date: 2026-08-19 — Status: accepted

## Decision

Every mutation runs in a framework-owned transaction. The handler fetches the target through
`scoped_query` (ADR-0002), checks the resource `Policy` for `View` and `Update` or `Delete` on
the loaded record inside the transaction, calls the record fn (`create_record` / `update_record`
/ `delete_record` / `bulk_delete_records`) with `&mut dyn toasty::Executor`, and commits. The
check reads the fetched row, never the passed ID alone, so an ID outside the tenant scope is not
found before any policy check. Bulk delete re-fetches every record through the same query and
checks each one. The resource's policy (`ResourceDef::policy`), asked one `Ability` at a time,
is the authorization vocabulary.

A create or update resolves each relationship key through the related resource's `scoped_query`
and `View` inside the same transaction, after the pre-write validation runs the same check
outside it. A related record deleted, moved, or hidden between the two refuses the write with a
field error and writes nothing.

A custom `Action` runs the same way. Its handler loads the row or selection through
`scoped_query`, checks `View` on every record and `can_run` on every record, and calls `Action::run`
with the same executor on the records that pass. A refused row is a 403; a selection drops the
refused records, runs the rest and reports the refused count as skipped, and a selection it refuses
on every record commits nothing and answers with an error notification on the list.

`create_record` and `update_record` return the written row; delete and bulk delete hand over the
removed rows. `Resource::after_commit(cx, Committed<Self::Model>)` receives one value per
committed write after `tx.commit()` and before the response, the place for side effects that must
not survive a rollback. A failing hook logs and ignores rather than rolling back. The hook opens
its own handle; retries and delivery carry no promise.
