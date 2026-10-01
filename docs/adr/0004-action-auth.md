# Action authorization is transactional and per-record

Date: 2026-08-19 — Status: accepted — Amended: 2026-10-01

## Decision

Every mutation runs in a framework-owned transaction: the handler fetches the target through the
tenant-scoped query (`scoped_query`, ADR-0002), checks
`Resource::can_view`/`can_update`/`can_delete` against that loaded record inside the transaction,
calls the `Resource` record fn
(`create_record` / `update_record` / `delete_record` / `bulk_delete_records`) with
`&mut dyn toasty::Executor`, and commits. Inputs are untrusted: the check always runs against the
fetched row, never the passed ID alone, so an id outside the request's tenant scope is not found
before any policy check runs. Bulk delete re-fetches through the same query and checks every
record. There is no `Policy` trait and no `shouldSkipAuthorization`: the `can_*` methods are the one
authorization vocabulary.

A custom `Action` (`Resource::actions`) runs the same way. Its handler loads the row, or the bulk
selection, through `scoped_query` inside the transaction, checks `can_view` and the action's own
`can_run` on every loaded record, and calls `Action::run` with the same executor. A record the
caller cannot view, or a row the action refuses, is a 403. A selection holding a record the action
(or, for bulk delete, `can_delete`) refuses commits nothing and answers with an error notification
on the list, because the bulk bar offers each operation for the whole selection.

`create_record` and `update_record` return the row they wrote (the generated key, or the state the
instance update reloaded); delete and bulk delete hand over the rows they removed as they were.
That is what `Resource::after_commit(cx, Committed<Self::Model>)` receives — default no-op, called
by every write handler, custom actions included (`Mutation::Action(NAME)`), after `tx.commit()`
and before the response — the only place a side
effect that must not survive a rollback belongs. One `Committed` per committed write (a bulk delete
is a single value), never produced when nothing committed, and a failing hook is logged and ignored
rather than rolling the write back. The transaction is gone by then, so the hook may open its own
handle; retries and delivery guarantees are not promised. `Resource::Model` is `Clone` so a handler
can keep the rows it loaded while the record fn consumes them.
