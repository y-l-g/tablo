# Single resource registration

Closes #382.

Line citations refer to the tree this design was written against, `f9750974`.

## Summary

`Panel` exposes one registration method, `Panel::resource`. A resource
declares its form with `Resource::form`, which returns `Some(schema)` for a
CRUD resource and `None` for a list-only one. `Panel::build` reads that return
value with the Db-only context and registers the create, edit, and options
routes only when it finds a schema. `FormResource` remains the typed-write
extension trait; this design does not merge its associated type.

## Motivation

A CRUD resource needs two traits and one of two registrations: `Resource` plus
`FormResource` (`crates/tablo-core/src/form.rs:401`), registered with
`Panel::form_resource` (`crates/tablo-core/src/panel/mod.rs:277`), while a
list-only resource uses `Resource` plus `Panel::resource` (`:252`). The two
registration paths share `register_common` (`:324-383`: list, detail, delete,
bulk delete, export); the form path adds create, edit, and options (`:284-316`).
Registering a resource whose `can_create` or `editable()` is on through
`Panel::resource` fails at `Panel::build` (`panel/build.rs:411-425`), and that
check reads the policy flags rather than the trait, so the compiler never
points at the wrong call.

## User-facing API

A CRUD resource implements `Resource` with a form:

```rust
impl Resource for UserResource {
    type Model = User;

    fn table(cx: &Cx) -> Table<User> { /* unchanged */ }

    fn form(_cx: &Cx) -> Option<Schema> {
        Some(Schema::new((
            TextInput::r#for(User::fields().name()),
            TextInput::r#for(User::fields().email()).email().unique(),
        )))
    }
}

impl FormResource for UserResource {
    type Form = UserForm;
    // validate_record / create_record / update_record keep current defaults
}

Panel::new("admin").resource::<UserResource>()
```

A list-only resource omits `form` and its `FormResource` impl, and registers
the same way. The `form()` / `table()` symmetry is deliberate: both are pure
declarations built with the Db-only context.

Before and after: `Panel::form_resource::<UserResource>()` becomes
`Panel::resource::<UserResource>()`; `fn form(cx) -> Schema` becomes
`fn form(cx) -> Option<Schema>` wrapped in `Some`. List-only resources delete
nothing and change no call.

## Behavior

`Panel::build` calls `R::form` once with `validation_cx`
(`panel/build.rs:612-616`, Db only), next to the existing `table` (`:398-399`),
`form` (`:436`), and `can_create` (`:413`, `:510`) calls. `Some` registers the
create, edit, and options routes; `None` registers the list, detail, delete,
bulk delete, and export routes only. The four `check_form_inner` checks (key
agreement, blank agreement, tenant ownership, create columns) run when the
return is `Some` and are skipped when it is `None`. A resource returning `Some`
without a `FormResource` impl fails `Panel::build` naming the resource and the
missing impl. Detection uses the autoref trick; a spike proves the encoding
compiles on the pinned toolchain before the implementation merges. The
list-only guard (`:411-425`) is deleted: `form()` returning `Some`
is the declaration that create or edit exists.

## Edge cases

- **Request-dependent schemas.** `form()` runs at build with no request, so the
  returned key set is fixed across requests, matching the existing rule the
  build checks rely on.
- **Policy-gated create.** A resource whose `can_create` denies for the request
  still registers form routes when `form()` returns `Some`; page and POST
  handlers keep their `can_*` checks, so the routes answer 403.
- **Relationship options.** The options endpoint registers exactly when the
  edit page does, as today.

## Alternatives

- **Default associated type** (`type Form: RecordForm = NoForm`). Associated-type
  defaults are unstable on the pinned toolchain (`rustc 1.98.0`, `E0658`), no
  `NoForm` type exists, and ADR-0022 records this rejection. Discarded until the
  language feature stabilizes.
- **Additive second call** (`resource::<R>().with_form::<R>()`). Compiles today
  but keeps two method names and repeats the type parameter; fallback encoding
  when the spike fails.

## Out of scope

`RecordForm` parsing, completion, tenant stamping, and the `CREATE_COLUMNS`
rule are unchanged. No column, filter, or Schema API changes ride along.
