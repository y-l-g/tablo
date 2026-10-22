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
page's header those placed on `Places::EDIT`. Each one lands back on its page as it was left, its
query included, after it runs. An action that takes the record out of the resource's `query()` or
its `View`, such as an archive the list hides, lands on that page's 404: place it on rows and the
bulk bar instead. Both
headers also carry Delete when the policy allows `DeleteAny`, `View` and `Delete` of the record; it
asks first through the confirmation dialog, and lands on the list, since the record is gone.

An action on the edit page posts a form of its own: it runs on the record as stored, and a change
typed into the edit form and not saved is lost.

A place no page renders shows the button nowhere: `Places::EDIT` on a resource without a form, or
`Places::DETAIL` on one whose view is `Detail::empty()`.

### How it runs

The framework runs an action the way it runs a delete. The POST goes to
`{list}/{key}/-/actions/{NAME}` for one record and `{list}/-/actions/{NAME}` for the selection and
carries the CSRF token. The handler answers 403 before reading the body when the policy refuses
`RunAny`, then loads the records through the tenant-scoped query inside a transaction. Every record
must pass the policy's `View`, and `run` writes through the same transaction, so an error rolls
everything back. The resource's [hooks](./resources.md#hooks) receive `Change::Action` with the
`NAME` and the records, and after the commit the page shows `Action::success`, by default the
label and the record count.

A record the policy's `Run` or `can_run` refuses is not handed to `run`: a refused record answers
403, and a selection drops the refused records, runs the rest and appends the skipped count out of
the selection to `Action::success` (`"Publish: 3 records (2 of 5 skipped)"`). A selection every
record refuses writes nothing and returns to the list with an error notification. A record that
fails the policy's `View`, and one the scoped query no longer returns, fail the whole POST
instead: 403 and 404, and nothing is written.

### What a run returns

`run` returns an `Outcome`, which says what the response does once the write commits.
`Outcome::done()` keeps the defaults: the `success` notification, landing back where the action
ran from. `.notify(..)` flashes a notification of the run's own instead, such as
`Notification::warning("3 sent").description("1 bounced")`, and `.redirect(..)` lands on another
same-origin path, such as the record a "duplicate" action made:

```rust
let outcome = Outcome::done().notify(Notification::success(format!("Duplicated {}", post.title)));
Ok(match tablo::url::edit::<PostResource>(cx, &copy) {
    Some(edit) => outcome.redirect(edit),
    None => outcome,
})
```

A selection that skipped records appends the skipped count to the notification's title either
way. A redirect off the site, such as `https://…` or `//host`, lands where the action ran from
and logs an error: the write has committed by then, so the run does not fail. A file to download
is a redirect to an app route that serves it.

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
post resource above does with `PublishDrafts`. The policy decides who runs it: `ViewAny`, since
the button is on the list, and `RunHeader { action: NAME }`, which
`ability.is_header_action::<PublishDrafts>()` matches. `RunHeader` is an ability of its own, not
`RunAny`: a policy that grants every record action with `RunAny { .. } => true`, and checks
`Run` per record, grants no header action by it. A header action's name shares the resource's
namespace with its record actions: mounting refuses a name two of them share.

A [page](./panel-and-routing.md#pages) declares its header actions with `PageDef::header_action::<A>()`, and
places their buttons where it renders its header with `header_action_buttons::<Self>(cx)`:

```rust
{{#include ../../../examples/guide/src/actions.rs:page-header-actions}}
```

A page's header action runs for whoever may open the page: the panel's sign-in and
the page's `PageDef::access` cover it as they cover the page's `GET`. **A page has no policy**, so an action
that writes a resource's records asks that resource's policy in its own `can_run`, as
`PublishDrafts` does: it then runs for the same users from the list and from the page.

On either, `HeaderAction::can_run(cx)` refuses a request beyond that, such as a feature flag, or
the policy of a resource the action writes. No button renders for an action the request may not
run, and its POST answers 403.

The POST goes to `{url}/-/actions/{NAME}` under the list or the page and carries the CSRF token.
`run` writes through a transaction the framework opens, so an error rolls everything back. **No
record is loaded, so nothing scopes `run` to the request's tenant**: an action over a tenant's
records reads them through `scoped_query`, as `PublishDrafts` does. A resource's
[hooks](./resources.md#hooks) receive `Change::HeaderAction` with the `NAME`, and after the commit
the list or the page shows
`HeaderAction::success`, by default the label and "done". A run from the list lands back on it as
it was left, its search, sort and filters kept; its `Outcome` changes either, as a record
action's does. `CONFIRM` asks first, as for a record action.

## Asking for input

An action names what it asks for before it runs as `type Input`: `()` for nothing, or a struct
deriving `FormInput`, which `run` receives parsed. A record action and a header action ask the
same way:

```rust
{{#include ../../../examples/guide/src/actions.rs:input-action}}
```

Its button opens the input's form in a dialog over the page instead of running, titled with the
action's label; on the bulk bar, the dialog counts the selection. The dialog submits the input to
the action's route, and the action runs, on the records that pass the policy, `can_run` and
tenancy checks for a record action, in one transaction. Cancel and Escape close the dialog, and
opening it again starts from a blank form. A table renders one dialog per action, which each row's
button points at its own record.

A value the input refuses renders the input as a page, with the error under its control, and
writes nothing; a key the input does not declare answers 400. The page posts to the same route.
`validate_input` adds refusals of its own, each under an input field's key, such as a reason too
short to act on; a refusal under a key no control renders is a declaration error, so a misspelled
key fails loudly instead of leaving a page with no message. `validate_input` runs before the
transaction opens and sees no record; a rule that reads the database refuses from `run`, which
returns `Err(WriteError::field("reason", ".."))`: the transaction rolls back what `run` wrote,
and the input page renders with the error. An action that asks for nothing has no control to
render one under, so its refusal is a declaration error. A record action's page is titled with
the label and the record's title, or the record count for a selection; a header action's is
titled with its label. Cancel returns to the
page the button was on. An action with input and `CONFIRM` confirms in its input dialog and on its
page, which say the action cannot be undone and whose submit renders destructive, instead of in
the confirmation dialog. Like the confirmation dialog, the input dialog needs JavaScript.

A searchable relationship choice in the input searches `GET {url}/-/actions/{NAME}/options` under
the list or the page, behind the same checks as the action's POST before it loads a record.

Each field posts its own name and renders the control its type picks: a `bool` is a checkbox,
`#[form(options)]` a choice over the field type's `Options` and `#[form(options = T)]` one over
`T`'s, and any other `FormScalar` a text input. A field with no blank answer is required, as on a
record form: an `Option` field has one, `#[form(optional)]` gives a `String` the empty string, and
`#[form(blank = ..)]` gives any other type a value. `#[form(label = "..")]` labels the control, `#[form(placeholder = "..")]` sets a text
input's placeholder, and `#[form(multiline = N)]` makes it a `<textarea>`. `#[form(email)]`
refuses a malformed email address, `#[form(url)]` what is not an `http` or `https` URL,
`#[form(tel)]` renders a telephone input, and `#[form(password)]` masks a `String` input, reads it
untrimmed, and renders it empty when the form comes back refused. An `Option` choice names its
options type, `#[form(options = PostStatus)]`. Mounting the panel refuses an input field
named `csrf_token`, `confirm` or `ids`, which the action's POST carries itself, and a
file field, whose upload an action's POST does not read.

## Names

An action name that is not one URL segment does not compile, for `ResourceDef::action`,
`ResourceDef::header_action` and `HeaderActions::add` alike. Mounting the panel refuses a name two
actions of one resource share, record or header, and a name two header actions of one page share.
