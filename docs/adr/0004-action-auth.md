# 0004 Mutations are transactional and per-record

Every mutation runs in a framework-owned transaction. The handler loads the target through
`scoped_query` (ADR-0002) and checks the policy on the loaded record inside the transaction,
never on the passed id alone; a bulk write checks every record. Relationship keys are re-checked
against the related resource inside the same transaction.

A custom `Action` runs the same way, authorized by the policy the panel mounted: `RunAny` with the
action's `NAME` before the body is read, then `View` and `Run` on each loaded record. `can_run` is
a state predicate on the record, asked after `Run`. A record refused `View` fails the whole POST;
a record refused `Run` or `can_run` is skipped from a selection, as a record refused `Delete` is
skipped from a bulk delete.

An action that asks for input (`Action::Input`) renders it as a form page from the same POST
route, after the same checks and before any write, and rolls the transaction back. The page's
submit POSTs again with the input, which repeats every check inside a new transaction before `run`
receives the parsed value; a refused value re-renders the page and writes nothing. As for a record
form, the input is parsed, validated and checked before the transaction opens, so a choice's
option query never waits on the connection the transaction holds, and a relationship choice is
re-checked inside it.

A record action renders wherever its `Places` say, a row, the detail or edit page's header or the
bulk bar, but the place is presentation only: the record route serves any action placed on a
record, and the bulk route any action placed on the bulk bar, through the same checks.

A header action runs on no record, so it has no `View` or `Run` to ask. On a resource it is
authorized by `RunAny` with its `NAME`, then its own `can_run(cx)`; on a page, by the panel's
sign-in and `Page::can_access`, then `can_run(cx)`, the checks the page's `GET` runs. It runs in a
framework-owned transaction like any mutation, but nothing scopes it to the tenant: it reads a
tenant's records through `scoped_query` itself. `after_commit` receives its `NAME` and no record.

`Resource::after_commit` runs once per committed write, after the commit and before the
response: the place for side effects that must not survive a rollback. A failing hook logs and
does not roll back.

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
- A header action authorized by `Ability::Create` or `ViewAny`: neither says the user may run it,
  as neither says so for a record action.
- A page action gated by a policy: a page has no model to type one over, and `can_access` already
  answers who may use the page.
