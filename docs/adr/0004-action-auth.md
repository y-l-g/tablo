# 0004 Mutations are transactional and per-record, with a hook on each side of the commit

Every mutation runs in a framework-owned transaction. The handler loads the target through
`scoped_query` (ADR-0002) and checks the policy on the loaded record inside the transaction, never
on the passed id alone; a bulk write checks every record, and relationship keys are re-checked
inside the same transaction.

A custom `Action` is authorized by the policy the panel mounted, through abilities of its own:
`RunAny` with its `NAME` before the body is read, then `View` and `Run` on each loaded record.
`can_run` is a state predicate on the record, asked after the policy. An action's input is parsed
and validated before the transaction opens, as a record form is, so no option query waits on the
connection the transaction holds. A record action's `Places` are presentation only: the routes
serve an action through the same checks wherever it shows.

A bulk action may take every record its list's search and filters select instead of the rows
checked: the POST carries the list's query, and the route re-runs the resource's list query inside
the transaction. Re-running the list asks what opening it asks, `ViewAny`, on top of `RunAny`:
otherwise a user who may not list the records could count them by search term through the
refusal note. The list shows a record its user may not view locked, so the selection skips one as
it skips a record the action refuses, where a checked key naming one answers 403.

A header action runs on no record. On a resource the policy authorizes it through `RunHeader`, an
ability apart from `RunAny`; on a page, which has no policy, the page's access does. Nothing scopes
it to a tenant: it reads through `scoped_query` itself.

Every write a resource's panel makes, through record fns, actions, header actions and relation
links, reaches two hooks with one `Change`: `Resource::before_commit` inside the transaction, so a
row it writes commits with the write or not at all, and `Resource::after_commit` after the commit,
for effects outside the database. An action's `Outcome` applies only after both.

## Rejected

- Authorizing an action through `View` and `can_run`: `can_run` is a static fn on the action
  type and never sees the mounted def's policy, so a panel that mounts the resource `ReadOnly`
  could not revoke its actions.
- `ViewAny` as an action's resource-wide ability: reading the list is not permission to write.
- Failing a whole selection on a record refused `Run`: the bulk bar offers a row a checkbox when
  any bulk write allows it, so a selection can mix records offered different actions.
- An action input as a `RecordForm`: a record form binds the model's columns and writes them,
  while an action's input is values the action reads, such as a rejection's reason.
- The input in the confirmation dialog: re-rendering a refused value needs a round trip the
  dialog does not make, and the dialog needs JavaScript.
- A place as authorization, a POST refused unless sent from a page the action is placed on: the
  place is where a button shows, and the policy already decides who may run the action.
- A selection of every matching record that posts their keys: the page holds only its own rows,
  and the keys a client posts are the ones it saw, not the ones that match when the write runs.
- A selection of every matching record past `Limits::bulk_records`, run in chunks: each chunk
  commits on its own, so a failure leaves part of the selection written.
- Select all matching on a relation table or on a page that owns its query: the bulk routes re-run
  the resource's own list query, which is not the query those tables show.
- Audit rows in `after_commit`: an error there is logged and dropped, and a crash between the
  commit and the hook loses the row while the write stands.
- Overriding each record fn and action `run` to write an audit row: every new write path needs its
  own override, and the relation links have no record fn to override.
- A built-in audit log: its table, retention, tenancy, redaction and viewer are the app's
  decisions, and `before_commit` with an app-owned resource covers it.
- A mutation kind beside a list of rows: an update could not carry the row it replaced, nor a
  link the records it linked.
- A header action authorized by `RunAny`: a policy that answers `RunAny { .. } => true` and
  checks `Run` per record would grant every header action added later. A separate `RunHeader`
  fails closed: an exhaustive policy stops compiling until it answers it.
- A header action authorized by `Ability::Create` or `ViewAny` alone: neither says the user may
  run it, as neither says so for a record action.
- `run` returning `Result<impl IntoOutcome>`, so an impl returning `Result<()>` kept compiling:
  a nested `impl Trait` an impl narrows trips `refining_impl_trait`, an error under
  `warnings = "deny"`.
- An outcome that answers with a file: a write's response is a redirect from a page handler,
  which Topcoat gives no way to carry a body; a redirect to an app route serves the file.
- A policy the framework asks of a page's action: a page has no model to type one over, and
  `PageDef::access` already answers who may use the page. An action that writes a resource's
  records asks that resource's policy in its `can_run`.
