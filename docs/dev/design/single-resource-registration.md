# Single resource registration

Closes #382.

Line citations refer to the tree this design was written against, `f9750974`.

## Summary

An app implements one trait, `Resource`, and registers it once with
`Panel::resource`. The resource names its typed record form with
`Resource::Form` — `NoForm<Self::Model>` when it has none — and declares its
schema with `Resource::form`, which returns `Some(schema)` for a resource with
create and edit pages and `None` for a list-only one.

The form type carries `RecordForm::HAS_FORM` (`false` on `NoForm`), so
`Panel::resource` decides the route set as it registers: `false` registers the
list, detail, delete, bulk delete, and export routes, `true` adds create, edit,
and options. `Panel::build` reconciles `Resource::Form` with `Resource::form`
and runs the declaration checks for a resource that has a form. `FormResource`
and `Panel::form_resource` are deleted.

The associated type carries no default: associated-type defaults are unstable
(`E0658`), so every list-only resource writes
`type Form = NoForm<Self::Model>;`.

## Motivation

A resource with a form implements two traits and picks one of two
registrations: `Resource` plus `FormResource`
(`crates/tablo-core/src/form.rs:401`), registered with `Panel::form_resource`
(`crates/tablo-core/src/panel/mod.rs:277`). A list-only resource uses
`Resource` plus `Panel::resource` (`:252`). Both paths share `register_common`
(`:324-383`: list, detail, delete, bulk delete, export); the form path adds
create, edit, and options (`:284-316`).

Registration is the part that can be wrong, and the check for it reads the
policy flags rather than the trait. `Panel::build` refuses a `Panel::resource`
registration whose `can_create` or `editable()` answers true
(`panel/build.rs:411-425`), so a resource with a `FormResource` impl and both
flags off builds through `Panel::resource` and serves a list with no form: the
compiler accepts it and the build reports nothing. #382 states this case.

One trait and one registration remove the wrong registration instead of
reporting it. `Resource::form` returning `Some` is the declaration that the
create and edit pages exist, and `Panel::build` holds the resource to it.

## User-facing API

A resource with a form implements `Resource` alone:

```rust
impl Resource for UserResource {
    type Model = User;
    type Form = UserForm;

    fn table(cx: &Cx) -> Table<User> { /* unchanged */ }

    fn form(_cx: &Cx) -> Option<Schema> {
        Some(Schema::new((
            TextInput::r#for(User::fields().name()),
            TextInput::r#for(User::fields().email()).email().unique(),
        )))
    }

    // CREATE_COLUMNS, validate_record, create_record, and update_record move
    // here unchanged; can_* are unchanged.
}

Panel::new("admin").resource::<UserResource>()
```

A list-only resource names `NoForm`, the framework's empty record form, and
omits `form`, whose default is `None`:

```rust
impl Resource for AuditResource {
    type Model = Audit;
    type Form = NoForm<Self::Model>;

    fn table(cx: &Cx) -> Table<Audit> { /* unchanged */ }
}

Panel::new("admin").resource::<AuditResource>()
```

Before and after:

- `Panel::form_resource::<R>()` becomes `Panel::resource::<R>()`.
- `impl FormResource for R` merges into `impl Resource for R`: `type Form = F`
  moves, `fn form(cx) -> Schema` becomes `fn form(cx) -> Option<Schema>`
  wrapped in `Some`, and the record fns move unchanged.
- A list-only resource adds `type Form = NoForm<Self::Model>;` and changes no
  call site.
- `FormResource` and `Panel::form_resource` leave the public API; `NoForm`
  joins it. The implementation PR moves 45 `impl FormResource` blocks, 29
  `form_resource` call sites, and the 16 markdown files that name either.

## Behavior

**Routing.** `Panel::resource` registers the common routes, then the create,
edit, and options routes only when `<R::Form as RecordForm>::HAS_FORM` is true.
`RecordForm` gains `const HAS_FORM: bool = true;` and `NoForm` overrides it to
`false`, so registration needs no request and no database. A list-only resource
has no form URL at all.

**Schema.** Form handlers read `R::form(cx)` per request, as they do today
through `FormResource::form` (`panel/forms/render.rs:35`,
`panel/forms/submit.rs:59`, `panel/actions/options.rs:38`). A `None` answer on a
registered form route is a 404: the resource declares no form for that request.
The detail page projects `Resource::view_values` plus
`<R::Form as RecordForm>::hydrate` for a resource with a form, and
`view_values` alone for a list-only one (`panel/detail.rs:36-52`); registration
picks the projection from the same const.

**Chrome.** The list renders its create link when `HAS_FORM` and
`R::can_create(cx)` both hold, and attaches the row Edit prefix from
`HAS_FORM`. A resource whose `can_create` answers true without a form fails
`Panel::build`: the list would offer a create page that does not exist.
`policy-derived-chrome.md` (#383) deletes `Resource::editable` and attaches the
edit prefix where the registration serves forms; that flag is `HAS_FORM` here.

**Declaration checks.** `Panel::build` runs one check per resource with the
Db-only context (`panel/build.rs:612-616`):

- `Resource::Form` and `Resource::form` agree. A record form whose `form()`
  answers `None`, or a `NoForm` resource whose `form()` answers `Some`, is
  refused by name.
- A resource whose `can_create` answers true while `HAS_FORM` is false is
  refused. This replaces the `Panel::resource` guard
  (`panel/build.rs:411-425`), whose edit half the chrome no longer needs.
- For a resource with a form, every check `check_form_inner` runs today
  (`:435-547`): key agreement, blank agreement, tenant ownership,
  create-column coverage (`:508-512`, gated on `can_create(cx)`), and the
  `unique()` marker's backing index (`:513-545`).

Policy answers are read under the Db-only context, so a request-scoped
`can_create` answers as it would for an anonymous request
(`resource/mod.rs:110-115`). Request-time policy is unchanged: create and edit
handlers keep their `can_*` checks and answer 403, and the options endpoint
answers 400 for an unknown field, 200 on success, or 403 from the related
resource's policy.

Tenancy scoping, export, pagination, and transaction handling do not change.

## Edge cases

- **Request-dependent schema.** The declaration is read at build and on every
  request; the key set must be the same on every request. A resource that
  answers `Some` at build and `None` later serves a 404 on its form routes
  rather than panicking.
- **`NoForm` write items.** Nothing reaches them: a resource whose form type is
  `NoForm` has no create or edit route and runs no form check. `into_create`
  returns `M::Create::default()`, the empty builder; `exec_update` panics with a
  message naming the type.
- **Policy-gated create.** A resource whose `can_create` denies for a request
  still has its form routes; the page and POST handlers answer 403.
- **Tenant-owned resources.** The form must not claim the tenant column the
  framework stamps; the build check is unchanged.

## Alternatives

- **Detect `FormResource` inside `resource::<R>()`.** Rejected: the encoding
  does not work. A probe compiled with `rustc 1.98.0` shows why. With a helper
  trait implemented for `R: FormResource` and a fallback reached through
  `Deref`, the body `fn resource<R: Resource>()` compiles and always takes the
  fallback — for a resource that implements `FormResource` and for one that
  does not. Trait selection happens while the generic body is type-checked
  against its where-clauses, not per monomorphization, and choosing the
  fallback produces no diagnostic. Calling the handler directly is
  `error[E0277]: the trait bound R: FormResource is not satisfied`. The single
  method cannot branch on an impl that is not in its bounds.
- **Default associated type**,
  `type Form: RecordForm<Model = Self::Model> = NoForm<Self::Model>`.
  Associated-type defaults are unstable on the pinned toolchain (`E0658`),
  which ADR-0022 records. `NoForm` without the default is the answer; the cost
  is one line per list-only resource.
- **Two entry points**, `resource::<R>()` plus `with_form::<R>()`. It needs no
  new type work, but it keeps two names for one decision, which is the issue.
- **Mode type parameter**, `resource::<R, WithForm>()`. One method name, but
  every call site names the mode and the chrome still needs the form-type flag.

## Open questions

- Blocking-acceptance: accept `type Form = NoForm<Self::Model>;` in every
  list-only resource, and the ADR-0022 section 2 amendment that comes with it.
  ADR-0022 chose two traits to avoid that line.
- Blocking-implementation: the `NoForm::exec_update` shim must compile against
  the real `toasty::Result` and `Executor` types; the probe uses mocks. Fix the
  wording of the three new build errors.
- Deferrable: `NoForm` as a name; whether `ViewValues` and the `FORMS` const
  parameter survive once the flag comes from the form type.

## Out of scope

`RecordForm` parsing, the derive, tenant stamping, and the `CREATE_COLUMNS`
rule are unchanged. No column, filter, or `Schema` API changes ride along. The
guide chapters and the ADR-0022 amendment are part of the implementation PR.
