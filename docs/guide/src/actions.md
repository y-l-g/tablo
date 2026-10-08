# Actions

An action is a mutation beyond create, update and delete. A **record action** runs on records: a
button on each row, in the header of a record's detail and edit pages, or an entry in the bulk bar
for the selection. A **header action** runs on no record: a button in the header of a resource's
list or of a custom [page](./panel-and-routing.md#pages).

## Record actions

A record action is a type implementing `Action<R>`, added to the def with `ResourceDef::action`:

```rust
{{#include ../../../examples/guide/src/actions.rs:publish-action}}

impl Resource for PostResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:post-actions}}
    }
}
```

Its button reads `label`, by default `NAME` in sentence case: `"publish"` reads "Publish" and
`"send-invite"` reads "Send invite". Override `label(cx)` for any other text.

### Where it shows

`const PLACES` says where the button renders; it defaults to `Places::ALL`:

| Place | The button |
| --- | --- |
| `Places::ROW` | in each row's actions cell, the [related tables'](./detail-pages.md#related-tables) included |
| `Places::DETAIL` | in the header of the record's detail page |
| `Places::EDIT` | in the header of the record's edit page |
| `Places::BULK` | in the bulk bar, run on the selection |

`Places::RECORD` is the first three, and `with` combines places:

```rust
{{#include ../../../examples/guide/src/actions.rs:places}}
```

A place decides where the button shows, not who may run the action. The record's route serves an
action placed on any of `Places::RECORD`, and the bulk route one placed on `Places::BULK`; any
other POST answers 404.

A resource offers the action only when the policy allows `RunAny { action: NAME }`. A row or a
record's page then renders its button when the policy's `View` and `Run` and the action's
`can_run` allow the record, and the bulk bar renders it for the selection. `can_run` reads the
record's state; the policy decides who runs the action, so a panel that mounts the resource with
`ReadOnly` offers none of its actions and refuses their POSTs
([Policy](./policy-auth-tenancy.md#policy)).

### On a record's pages

The header of a record's detail page carries its actions placed on `Places::DETAIL`, and the edit
page's header those placed on `Places::EDIT`. Each one lands back on its page after it runs. Both
headers also carry Delete when the policy allows `DeleteAny`, `View` and `Delete` of the record; it
asks first through the confirmation dialog, and lands on the list, since the record is gone.

An action on the edit page posts a form of its own: it runs on the record as stored, and a change
typed into the edit form and not saved is lost.

### How it runs

The framework runs an action the way it runs a delete. The POST goes to
`{list}/{key}/-/actions/{NAME}` for one record and `{list}/-/actions/{NAME}` for the selection and
carries the CSRF token. The handler answers 403 before reading the body when the policy refuses
`RunAny`, then loads the records through the tenant-scoped query inside a transaction. Every record
must pass the policy's `View`, and `run` writes through the same transaction, so an error rolls
everything back. After the commit, `after_commit` receives `Mutation::Action(NAME)` with the
records and the page shows `Action::success`, by default the label and the record count.

A record the policy's `Run` or `can_run` refuses is not handed to `run`: a refused record answers
403, and a selection drops the refused records, runs the rest and appends the skipped count out of
the selection to `Action::success` (`"Publish: 3 records (2 of 5 skipped)"`). A selection every
record refuses writes nothing and returns to the list with an error notification. A record that
fails the policy's `View`, and one the scoped query no longer returns, fail the whole POST
instead: 403 and 404, and nothing is written.

A destructive action declares `const CONFIRM: bool = true` to ask first through the delete's
confirmation dialog; an unconfirmed POST answers 400. Confirmation dialogs need JavaScript: without
it their buttons do nothing.

## Header actions

A header action acts on no record: "publish every draft", "recount the tags", "clear the cache".
It is a type implementing `HeaderAction`:

```rust
{{#include ../../../examples/guide/src/actions.rs:header-action}}
```

`ResourceDef::header_action::<A>()` puts its button in the header of the resource's list, as the
post resource above does with `PublishDrafts`. The policy decides who runs it, through
`RunAny { action: NAME }`; `ability.is_header_action::<PublishDrafts>()` matches it, as
`is_action` matches a record action. A header action's name shares the resource's namespace with
its record actions: mounting refuses a name two of them share.

A [page](./panel-and-routing.md#pages) declares its header actions in `Page::header_actions`, and
places their buttons where it renders its header with `header_actions::<Self>(cx)`:

```rust
{{#include ../../../examples/guide/src/actions.rs:page-header-actions}}
```

A page's header action runs for whoever may open the page: the panel's sign-in and
`Page::can_access` gate it as they gate the page's `GET`.

On either, `HeaderAction::can_run(cx)` refuses a request beyond that, such as a feature flag or a
role the policy does not model. No button renders for an action the request may not run, and its
POST answers 403.

The POST goes to `{url}/-/actions/{NAME}` under the list or the page and carries the CSRF token.
`run` writes through a transaction the framework opens, so an error rolls everything back. **No
record is loaded, so nothing scopes `run` to the request's tenant**: an action over a tenant's
records reads them through `scoped_query`, as `PublishDrafts` does. After the commit, a resource's
`after_commit` receives `Mutation::Action(NAME)` with no record, and the list or the page shows
`HeaderAction::success`, by default the label and "done". A run from the list lands back on it as
it was left, its search, sort and filters kept. `CONFIRM` asks first, as for a record action.

## Asking for input

An action names what it asks for before it runs as `type Input`: `()` for nothing, or a struct
deriving `ActionInput`, which `run` receives parsed. A record action and a header action ask the
same way:

```rust
{{#include ../../../examples/guide/src/actions.rs:input-action}}
```

Its button opens an input page instead of running: the POST that would run the action renders
the input's form, after the same checks. A record action's page is titled with the label and the
record's title, or the record count for a selection, after the policy, `can_run` and tenancy
checks; a header action's is titled with its label. Its submit POSTs to the same route with the
input, and the action runs, on the records that pass the checks again for a record action, in one
transaction. A value the input refuses renders the page again with the error under its control
and writes nothing; a key the input does not declare answers 400. `validate_input` adds refusals
of its own, each under an input field's key, such as a reason too short to act on. An action with
input and `CONFIRM` confirms on the input page, which says the action cannot be undone and whose
submit renders destructive, instead of in the dialog. Cancel returns to the page the button was
on. The input page works without JavaScript.

Each field posts its own name and renders the control its type picks: a `bool` is a checkbox,
`#[form(options)]` a choice over the field type's `Options` and `#[form(options = T)]` one over
`T`'s, and any other `FormScalar` a text input. A field with no blank answer is required, as on a
record form: `#[form(blank = ..)]`, `#[form(optional)]` on a `String`, an `Option` or a `bool`
gives it one. `#[form(label = "..")]` labels the control, `#[form(placeholder = "..")]` sets a text
input's placeholder, and `#[form(multiline = N)]` makes it a `<textarea>`. An `Option` choice
names its options type, `#[form(options = PostStatus)]`. Mounting the panel refuses an input field
named `csrf_token`, `confirm`, `ids` or `-input`, which the action's POST carries itself, and a
file field, whose upload an action's POST does not read.

## Names

An action name that is not one URL segment does not compile, for `ResourceDef::action`,
`ResourceDef::header_action` and `HeaderActions::add` alike. Mounting the panel refuses a name two
actions of one resource share, record or header, and a name two header actions of one page share.
