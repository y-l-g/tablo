# 0004 Mutations are transactional and per-record, with a hook on each side of the commit

Every mutation runs in a framework-owned transaction. The handler loads the target through
`scoped_query` (ADR-0002) and checks the policy on the loaded record inside the transaction,
never on the passed id alone; a bulk write checks every record. Relationship keys are re-checked
against the related resource inside the same transaction.

A custom `Action` runs the same way, authorized by the policy the panel mounted: `RunAny` with the
action's `NAME` before the body is read, then `View` and `Run` on each loaded record. `can_run` is
a state predicate on the record, asked after `Run`. A record refused `View` fails the whole POST;
a record refused `Run` or `can_run` is skipped from a selection, as a record refused `Delete` is
skipped from a bulk delete.

An action that asks for input (`Action::Input`) asks for it in a dialog, which carries no record
data, as the confirmation dialog does. Its submit POSTs the input to the action's route, which runs
every check before `run` receives the parsed value. A refused value renders the input as a form
page from that route, after the same checks and before any write, and rolls the transaction back;
the page posts back to the same route. As for a record
form, the input is parsed, validated and checked before the transaction opens, so a choice's
option query never waits on the connection the transaction holds, and a relationship choice is
re-checked inside it.

A record action renders wherever its `Places` say, a row, the detail or edit page's header or the
bulk bar, but the place is presentation only: the record route serves any action placed on a
record, and the bulk route any action placed on the bulk bar, through the same checks.

A header action runs on no record, so it has no `View` or `Run` to ask. On a resource it is
authorized by `ViewAny`, since its button is on the list, and `RunHeader` with its `NAME`, an
ability of its own, then its own `can_run(cx)`. On a page, which has no policy, the panel's sign-in
and the page's `PageDef::access` authorize it, as they do the page's `GET`, then `can_run(cx)`; an
action that writes a resource's records asks that resource's policy there. It runs in a
framework-owned transaction like any mutation, but nothing scopes it to the tenant: it reads a
tenant's records through `scoped_query` itself. The hooks receive its `NAME` and no record.

Every write a resource's panel makes, through its record fns, actions, header actions and
relation links, calls two hooks with the same `Change`: what was written, with the rows it names.
`Resource::before_commit` runs inside the transaction with its executor, so a row it writes, such
as an audit row, commits with the write or not at all; an error rolls everything back.
`Resource::after_commit` runs after the commit and before the response: the place for effects
outside the database that must not survive a rollback. A failing `after_commit` logs and does not
roll back, and a crash after the commit skips it.

`run` returns an `Outcome`: the notification the response flashes and the same-origin path it
lands on, each defaulting to the action's `success` and to where it ran from. The response applies
it only after the commit and the hook. A redirect off the site falls back to the default landing
and logs, since the write it follows has committed.

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
