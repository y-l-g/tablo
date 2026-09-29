# Single resource registration

Closes #382. Lands before [`policy-derived-chrome.md`](policy-derived-chrome.md) (#383), which
reads `RecordForm::HAS_FORM` from this design.

Citations refer to `36f9382c`.

## Summary

`FormResource` and `Panel::form_resource` are deleted. A resource implements `Resource` alone and
registers with `Panel::resource`. It names its record form with the required associated type
`Resource::Form`; a list-only resource names `NoForm<Self::Model>`. `RecordForm::HAS_FORM`, `false`
on `NoForm` only, decides the route set at registration.

## Motivation

Today a resource with a form implements `Resource` + `FormResource` (`form.rs:401`) and must
register with `Panel::form_resource`. Registering it with `Panel::resource` compiles and serves a
list with no form; `Panel::build` catches it only when `can_create` or `editable()` answers true
(`panel/build.rs:410-425`). A generic `resource::<R: Resource>()` cannot detect a `FormResource`
impl outside its bounds, so the fix is to put the form on `Resource`.

## API

```rust
impl Resource for UserResource {
    type Model = User;
    type Form = UserForm; // #[derive(RecordForm)]

    fn table(cx: &Cx) -> Table<User> { /* … */ }

    fn form(_cx: &Cx) -> Schema {
        Schema::new((
            TextInput::r#for(User::fields().name()),
            TextInput::r#for(User::fields().email()).email().unique(),
        ))
    }
    // CREATE_COLUMNS, validate_record, create_record, update_record: optional, as today.
}

impl Resource for AuditResource {
    type Model = Audit;
    type Form = NoForm<Self::Model>;

    fn table(cx: &Cx) -> Table<Audit> { /* … */ }
}

Panel::new("admin")
    .resource::<UserResource>()
    .resource::<AuditResource>()
```

### `Resource` gains (from `FormResource`, unchanged unless noted)

| Item | Default |
| --- | --- |
| `type Form: RecordForm<Model = Self::Model>` | none, required (associated-type defaults are unstable, E0658) |
| `const CREATE_COLUMNS: &'static [&'static str]` | `&[]` |
| `fn form(cx: &Cx) -> Schema` | `Schema::empty()` (**new default**; was required) |
| `fn validate_record(cx, &Self::Form) -> FieldErrors<Self::Form>` | empty |
| `fn create_record(cx, Self::Form, ex)` | `write_create::<Self>` |
| `fn update_record(cx, Self::Model, Posted<Self::Form>, ex)` | `write_update::<Self>` |

### `RecordForm` gains

`const HAS_FORM: bool = true;`. The derive does not emit it.

### `NoForm<M>` (new, public, `tablo_core::NoForm`)

- `struct NoForm<M>(PhantomData<fn() -> M>)`, so it is `Send + 'static` for any `M`.
- `impl<M: Model + Send + Sync + 'static> RecordForm for NoForm<M>`:
  - `Model = M`, `Field = std::convert::Infallible`, `HAS_FORM = false`.
  - `fields` returns `vec![]`; `hydrate` returns an empty map.
  - `into_update` returns `None`, so `exec_update` is never reached.
  - `parse`, `into_create`, `exec_update`: no route calls them. They panic with
    ``"`NoForm<{type}>` has no form"``.

### Removed

`FormResource`, `Panel::form_resource`. `write_create` / `write_update` take `R: Resource`.

## Behavior

**Registration.** `Panel::resource::<R>()` registers the list, detail, delete, bulk delete, and
export routes, then the create, edit, and options routes when `R::Form::HAS_FORM`. The handlers
that bound on `FormResource` (`panel/forms/render.rs`, `panel/forms/submit.rs`,
`panel/actions/options.rs`) bound on `Resource`.

**List.** `resource_list` and `resource_list_live` drop their `const FORMS: bool` parameter and
read `R::Form::HAS_FORM`. The create link renders when `HAS_FORM && R::can_create(cx)`.

**Detail.** The page renders `view_values` extended by `R::Form::hydrate` for every resource;
`NoForm::hydrate` is empty. `DetailValues`, `ViewValues`, and `FormValues`
(`panel/detail.rs:28-52`) are deleted.

**Build checks.** One check per resource replaces `check_list_resource` / `check_form_resource`:

| Condition | Result |
| --- | --- |
| `HAS_FORM` and `form(cx)` is empty | refused: ``resource `X` names form `F` but `form()` declares no fields`` |
| `!HAS_FORM` and `form(cx)` is non-empty | refused: ``resource `X` declares a form schema but its `Form` is `NoForm` — name the record form in `type Form` `` |
| `!HAS_FORM` and `can_create(cx)` | refused: ``resource `X` allows create but has no form`` |
| `HAS_FORM` | every check `check_form_inner` runs today (`panel/build.rs:434-547`) |

Checks run under the Db-only context (`validation_cx`, `panel/build.rs:611`), as today.

Request-time policy, tenancy, export, and transactions do not change.

## Edge cases

- **Direct calls on a `NoForm` resource.** `R::create_record` / `R::update_record` reach
  `NoForm::into_create` / `parse` only through app code, which panics naming the type.
- **Request-dependent schema.** `form(cx)` is read at build and per request. Its field set must not
  depend on the request; the key-agreement check only sees the build answer.

## Alternatives

- **Detect `FormResource` inside `resource::<R: Resource>()`.** Trait selection happens when the
  generic body type-checks, so the probe always takes the fallback (verified on `rustc 1.98.0`).
- **`form() -> Option<Schema>`.** A second declaration of `HAS_FORM`, plus a per-request `None`
  branch no resource needs.
- **Two entry points** (`resource` + `with_form`) or **a mode parameter**
  (`resource::<R, WithForm>()`): both keep the registration choice this design removes.

## Implementation

1. `form.rs`: move the `FormResource` items onto `Resource` (`resource/mod.rs`); add `HAS_FORM` and
   `NoForm`; export `NoForm` from the crate root.
2. `panel/mod.rs`: fold `form_resource` into `resource`, branching on `HAS_FORM`.
3. `panel/list.rs`, `panel/detail.rs`, `panel/build.rs`: the Behavior changes above.
4. Migrate every `impl FormResource` and `form_resource` call: `crates/tablo-core` (src and
   tests), `examples/showcase`, `benchmarks/tablo` (sync its lockfile only if it changes).
5. Docs: `docs/guide/src/{resources,forms,detail-pages}.md`, `CONTEXT.md`, `README.md`,
   `docs/dev/architecture.md`, and ADR-0022 §2, which records the two-trait split this reverses.
6. Commit: `feat(core)!: …`, with a `BREAKING CHANGE:` footer naming `FormResource`,
   `Panel::form_resource`, the required `Resource::Form`, and `NoForm`.

## Out of scope

`RecordForm` parsing and its derive, tenant stamping, the `CREATE_COLUMNS` rule, and row chrome
(#383).
