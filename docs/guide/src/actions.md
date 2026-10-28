# Actions

An action is a mutation beyond create, update and delete. A **record action** runs on records: a
button on each row, in the header of a record's detail and edit pages, or an entry in the bulk bar
for the selection. A **header action** runs on no record: a button in the header of a resource's
list or of a custom [page](./panel-and-routing.md#pages).

## Record actions

A record action is a type implementing [`Action<R>`](https://y-l.fr/tablo/nightly/api/tablo/trait.Action.html),
added to the def with `ResourceDef::action`:

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

`run` holds only the write. [`filter_keys(posts)`](https://y-l.fr/tablo/nightly/api/tablo/fn.filter_keys.html)
selects the records it receives, so one statement publishes the whole selection. The framework owns everything around it, as it does for a delete: the
route and its CSRF check, the transaction the records load in and `run` writes through, the policy
checks on every record, the [hooks](./resources.md#hooks), and the success notification. The
[`Action`](https://y-l.fr/tablo/nightly/api/tablo/trait.Action.html) page lists each step, and
what a refused record or selection answers.

### Where it shows

`const PLACES` says where the button renders: each row, the detail and edit pages' headers, the
bulk bar, or any combination of them, by default all four. An action for the detail page and the
bulk bar only declares:

```rust
{{#include ../../../examples/guide/src/actions.rs:places}}
```

[`Places`](https://y-l.fr/tablo/nightly/api/tablo/struct.Places.html) names each place, and
[`Action::PLACES`](https://y-l.fr/tablo/nightly/api/tablo/trait.Action.html#associatedconstant.PLACES)
says which routes serve an action and which places no page renders. A run from a record's page
lands back on it, so an action that hides the record from the list, such as an archive, belongs
on rows and the bulk bar.

A bulk action runs on the rows checked. On a list whose rows span pages, the bulk bar also offers
**Select all matching**, which runs it on every record the list's search and filters select, up
to [`Limits::bulk_records`](https://y-l.fr/tablo/nightly/api/tablo/struct.Limits.html#method.bulk_records).

### Who runs it

Three checks decide, each asked for its own reason:

- the policy's `RunAny { action: NAME }` decides whether the resource offers the action at all;
- the policy's `Run` decides who may run it on a given record;
- the action's `can_run` reads the record's state: a published post refuses "publish".

`can_run` sees no user, so it never decides who may act. A panel that mounts the resource with
`ReadOnly` therefore offers none of its actions and refuses their POSTs
([Policy](./policy-auth-tenancy.md#policy)).

### What a run returns

`run` returns an [`Outcome`](https://y-l.fr/tablo/nightly/api/tablo/struct.Outcome.html), which
says what the response does once the write commits. `Outcome::done()` keeps the defaults: the
`success` notification, landing back where the action ran from. A "duplicate" action reports its
own message and opens the copy it made:

```rust
let outcome = Outcome::done().notify(Notification::success(format!("Duplicated {}", post.title)));
Ok(match tablo::url::edit::<PostResource>(cx, &copy) {
    Some(edit) => outcome.redirect(edit),
    None => outcome,
})
```

A file to download is a redirect to an app route that serves it.

A destructive action declares `const CONFIRM: bool = true` to ask first through the delete's
confirmation dialog.

## Header actions

A header action acts on no record: "publish every draft", "recount the tags", "clear the cache".
It is a type implementing [`HeaderAction`](https://y-l.fr/tablo/nightly/api/tablo/trait.HeaderAction.html):

```rust
{{#include ../../../examples/guide/src/actions.rs:header-action}}
```

`ResourceDef::header_action::<A>()` puts its button in the header of the resource's list, as the
post resource above does with `PublishDrafts`. The policy decides who runs it through `ViewAny`,
since the button is on the list, and
[`RunHeader`](https://y-l.fr/tablo/nightly/api/tablo/enum.Ability.html#variant.RunHeader), which
`ability.is_header_action::<PublishDrafts>()` matches. It is an ability of its own because a
policy that grants every record action and checks `Run` per record must not grant a header action
by accident: no record is there to check.

For the same reason, **nothing scopes a header action to the request's tenant**: an action over a
tenant's records reads them through `scoped_query`, as `PublishDrafts` does.

A [page](./panel-and-routing.md#pages) declares its header actions with
`PageDef::header_action::<A>()`. The default `render` shows their buttons in the page's header; a
page that [writes its own](./panel-and-routing.md#rendering-a-page) places them with
`header_action_buttons::<Self>(cx)`:

```rust
{{#include ../../../examples/guide/src/actions.rs:page-header-actions}}
```

**A page has no policy**: its header actions run for whoever may open it. An action that writes a
resource's records asks that resource's policy in its own `can_run`, as `PublishDrafts` does, so
it runs for the same users from the list and from the page.

## Asking for input

An action names what it asks for before it runs as `type Input`: `()` for nothing, or a struct
deriving [`FormInput`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.FormInput.html), which `run`
receives parsed. A record action and a header action ask the same way:

```rust
{{#include ../../../examples/guide/src/actions.rs:input-action}}
```

Its button opens the input's form in a dialog over the page instead of running. The derive picks
each field's control and its required mark as a record form does; `FormInput` lists its field
attributes and the names an input may not use.

A refusal lands on the input, never on a half-written record. `validate_input` refuses before the
transaction opens and sees no record, as `ChangeStatus` refuses to feature a draft. It names the
field it refuses by the enum the derive declares beside the input, `StatusChangeField`, so a
renamed field fails to compile rather than render no message. A rule that reads the database
refuses from `run` with `Err(WriteError::field("featured", ".."))`, under the field's form key,
and the transaction rolls back what `run` wrote. Either way the input renders as a page with the
error under its control.

## Names

An action's `NAME` is its URL segment. A name that is not one segment does not compile, and
mounting the panel refuses a name two actions of one resource, or two header actions of one page,
share.
