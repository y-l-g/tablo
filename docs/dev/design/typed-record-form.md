# A record form that writes what the schema declares

Closes #369.

Line citations refer to the tree this design was written against, `72632705`.

## Summary

A form-bearing resource declares one struct, `#[derive(RecordForm)]`, whose
fields are the model columns its form writes. The framework parses every
submission into that struct, hydrates edit and detail pages from it, and writes
it back through toasty's generated builders. `create_record` and
`update_record` have default implementations, so a resource whose write is
"store what the form says" writes neither. On edit, a key the submission does
not post keeps its stored value; the framework fills it from the stored record
before parsing, so both record fns receive one fully typed value. At
`Panel::build`, the framework refuses a form whose struct and `Schema`
disagree on a key, on whether a control may be left blank, or on who owns the
tenant column.

## Motivation

Three key sets must agree by hand: the `Schema`'s controls, the record fn's
`values.get("…")` calls, and the `toasty::create!` / `toasty::update!` field
lists. `PostResource` binds 9 model fields (`title`, `body`, `status`,
`featured`, `author_id`, `cover_id`, `tags`, `seo`, `publication`). `form`,
`hydrate_form_values`, `create_record`, and `update_record` each name all 9, so
the same list is written four times and nothing checks that the four copies
agree. `view` names a subset: it omits `author_id` and `cover_id`.

Two failures follow, and the app author sees neither:

- An omitted key validates as `""` (`schema/tree.rs:120`). A record fn that
  reads it without a fallback blanks the stored value, and one that falls back
  (`kept`, `examples/showcase/src/app.rs:418`) must be written per field. The
  rule an app must obey — write only what the submission names — is documented
  on `Schema::validate` (`schema/mod.rs:308`), not on `Resource::update_record`
  (`resource/mod.rs:486`), the signature that has to enforce it.
- A record fn reading a key no control declares never trips `unknown_keys`, and
  a parse failure inside a record fn is a 500: `submitted_parsed` for
  `author_id` (`app.rs:956`), and the `parse_leaf` panic
  (`schema/embedded.rs:341`) for an embedded leaf.

The existing guards and their reach:

| Guard | Catches | Does not catch |
| --- | --- | --- |
| `unknown_keys` (`schema/mod.rs:229`, called at `panel/forms/common.rs:64`) | a posted key no control declares | a record fn reading an undeclared key |
| `Schema::validate` requiredness | an empty or omitted **required** key, inline with a 200 | an omitted **optional** key |
| `absent_fields` (`schema/mod.rs:356`) | a required key inside an all-empty `Repeater` or a hidden variant group | anything about the write |

The showcase carries ten helpers whose only job is to encode presence rules
per field and per type (`submitted_trimmed`, `submitted_parsed`, `kept`,
`kept_one_of`, `kept_bool`, `kept_parsed`, `kept_age`, `kept_embedded`,
`submitted_cover`, `kept_cover`, `app.rs:35-186`). They disagree with each
other: `kept_one_of` and `kept_bool` keep the stored value when a `Select` is
emptied, while `kept` and `kept_cover` clear it.

## User-facing API

### Declaring a form

A record form is a struct with one field per model column the form writes. The
field's name is the model field's name and its type is the model field's type:

```rust
#[derive(tablo_core::RecordForm)]
#[record_form(model = User)]
pub struct UserForm {
    pub name: String,
    pub email: String,
    #[record_form(blank = "member")]
    pub role: String,
    #[record_form(blank = true)]
    pub active: bool,
    #[record_form(blank = 0)]
    pub age: i64,
}
```

A field is one of two kinds:

- **A scalar**: `String`, any `TypedValue` type (`bool`, the integer and float
  types, `uuid::Uuid`, `jiff::Timestamp`), or an `Option` of one of those. It
  binds one form key, the key its control posts.
- **An embedded value**, marked `#[record_form(embed)]`. Its type implements
  `EmbeddedForm` (ADR-0019), and it binds every key the value occupies: each
  leaf column, plus the discriminant for an enum.

```rust
#[derive(tablo_core::RecordForm)]
#[record_form(model = Post)]
pub struct PostForm {
    pub title: String,
    pub body: String,
    #[record_form(blank = "draft")]
    pub status: String,
    #[record_form(blank = false)]
    pub featured: bool,
    pub author_id: uuid::Uuid,
    pub cover_id: Option<uuid::Uuid>,
    pub tags: String,
    #[record_form(embed)]
    pub seo: Seo,
    #[record_form(embed)]
    pub publication: Publication,
}
```

A column the form does not write stays off the struct. `tenant_id` is the
framework's to stamp (see [Tenant stamping](#tenant-stamping)), and
`created_at` takes a toasty default on the model:

```rust
#[derive(Debug, Clone, toasty::Model)]
pub struct Post {
    // …
    #[default(jiff::Timestamp::now())]
    pub created_at: Timestamp,
}
```

`#[record_form(blank = <expr>)]` is the value a scalar takes when its control
is posted empty. `String` answers `""` and `Option<T>` answers `None` without
one; any other type needs `blank` wherever its control can be left empty (see
[Resolving a value](#resolving-a-value)). The expression converts with
`Into<T>`, so `blank = "draft"` works for a `String`.

The derive also emits a field enum, `UserFormField { Name, Email, Role, Active,
Age }`, named `<Struct>Field`, which is `<UserForm as RecordForm>::Field`.

### Registering the resource

A resource with a form implements `FormResource` next to `Resource` and
registers with `Panel::form_resource`:

```rust
impl FormResource for AuthorResource {
    type Form = AuthorForm;

    fn form(_cx: &Cx) -> Schema {
        Schema::new((
            TextInput::r#for(Author::fields().name()),
            TextInput::r#for(Author::fields().email()).email().unique(),
        ))
    }
}

Panel::new("admin")
    .form_resource::<UserResource>()
    .form_resource::<AuthorResource>()
    .form_resource::<PostResource>()
    .form_resource::<CommentResource>();
```

`Panel::resource::<R: Resource>` serves the list, the detail page, delete,
bulk delete, and export, and its list links to no create page.
`Panel::form_resource::<R: FormResource>` serves those plus the create page,
the edit page, and the relationship-options endpoint. `Schema` stays
hand-written because a Rust type does not pick its control: `role` is a
`String` on a `Select`, `active` a `bool` on a `Select`, `body` a `String` on a
`Textarea`.

### Record fns

`create_record` receives the parsed form. `update_record` receives the record
and a `Posted<F>`: the parsed form plus the set of fields the submission named.
Both default to the derived write, so `AuthorResource` and `UserResource`
declare neither.

A resource that checks something inside the transaction overrides the record
fn and delegates the write to `write_create` / `write_update`:

```rust
impl FormResource for CommentResource {
    type Form = CommentForm;

    fn form(_cx: &Cx) -> Schema { /* unchanged */ }

    async fn create_record(
        cx: &Cx,
        form: CommentForm,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Comment> {
        ensure_post_in_tenant(cx, form.post_id, ex).await?;
        tablo_core::write_create::<Self>(cx, form, ex).await
    }

    async fn update_record(
        cx: &Cx,
        record: Comment,
        posted: Posted<CommentForm>,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Comment> {
        // `Posted` derefs to the form: an unposted `post_id` reads as the
        // stored one.
        ensure_post_in_tenant(cx, posted.post_id, ex).await?;
        tablo_core::write_update::<Self>(cx, record, posted, ex).await
    }
}
```

A record fn that writes more than the form owns uses the builders directly.
`Posted::into_update` returns `None` when the submission named no form field,
and otherwise the instance update builder with one `set_<field>` per named
field:

```rust
async fn update_record(
    _cx: &Cx,
    mut record: Post,
    posted: Posted<PostForm>,
    ex: &mut dyn toasty::Executor,
) -> Result<Post> {
    let slug = slugify(&posted.title);
    if let Some(mut q) = posted.into_update(&mut record) {
        q.set_slug(slug);
        q.exec(&mut *ex).await.map_err(|e| -> topcoat::Error { e.into() })?;
    }
    // The instance update reloads `record`, so this is the written row.
    Ok(record)
}
```

`F::into_create` returns the model's create builder with every form field set.
`write_create` stamps the tenant; a record fn that executes the create builder
itself sets the tenant with `require_tenant(cx)?`, and names every column it
sets beyond the form in `FormResource::CREATE_COLUMNS`.

### Validation rules

`validate_record` receives the parsed form and returns errors keyed by the
field enum. The errors render inline with a 200 and nothing is written:

```rust
fn validate_record(_cx: &Cx, form: &UserForm) -> FieldErrors<UserForm> {
    let mut errors = FieldErrors::new();
    if form.age < 0 {
        errors.add(UserFormField::Age, "Age must be zero or more");
    }
    errors
}
```

A misspelled field is a compile error. A rule that fails in a record fn is a
500, so a range or cross-field rule belongs here.

### Before and after

`UserResource` before: `hydrate_form_values`, `validate`, `create_record`, and
`update_record`, 112 lines (`app.rs:327-438`), plus the `kept*` helpers they
call. After: `UserForm` above, `form` unchanged, and a 7-line
`validate_record`. `PostResource` keeps two 4-line record fns for its
author check. The ten presence helpers are deleted.

## Behavior

### The submit pipeline

Create (`resource_create_post`):

1. Parse the body, verify CSRF, and reject unknown keys with a 400.
2. Store uploads, restore carried uploads, and strip transport keys.
3. Run `Schema::validate_async`. An absent key validates as `""`.
4. Open the transaction and run `check_unique`.
5. Normalize a copy of the values, then `F::parse`, then, if every field
   parsed, `validate_record`.
6. On any error, re-render with a 200. Otherwise call
   `create_record(cx, form, tx)` and commit.

Edit (`resource_edit_post`):

1. Parse, verify CSRF, run the advisory load, and check `can_view` and
   `can_update`.
2. Reject unknown keys, store and restore uploads, and strip transport keys.
3. Record the **named keys** (see [Completion and naming](#completion-and-naming)).
4. **Complete** every declared key the submission did not name from
   `F::hydrate(advisory)`, then run `Schema::validate_async`.
5. Open the transaction, run the authoritative load, and re-check policy.
6. Re-complete the unnamed keys from `F::hydrate(record)`, the in-transaction
   record. Then run `check_unique` against that projection, normalize,
   `F::parse`, and `validate_record`.
7. On any error, re-render with a 200. Otherwise call
   `update_record(cx, record, Posted { form, named }, tx)` and commit.

Errors from `validate_async`, upload storage, `check_unique`, `F::parse`, and
`validate_record` merge into one map and render together. `F::parse` adds
an error only to a key that has no error yet, so a blank required control shows
the schema's message once. `validate_record` runs only when every field parses:
a submission with a field that fails to parse shows those errors first. The
re-render shows the completed, un-normalized values.

### Completion and naming

A **named key** is a key present in the submission after transport keys are
stripped, with one exception: an untouched `FileUpload` is not named. That is a
file input with an empty part and no `clear_<field>`, whose stored path is
non-empty; this replaces the untouched-file backfill at
`panel/forms/submit.rs:85-102`. A cleared upload is named with `""`, and an
uploaded or carried file is named with its stored path.

A **named field** is a form field with at least one named key. A scalar has
one key. An embedded value has all of its keys, so posting `seo_title` alone
names `seo`.

**Completion** fills every declared key that is not named. On edit it takes
the key from `F::hydrate` of the stored record: the advisory record for
validation, then the in-transaction record for the parse. On create there is no
record, so the key stays absent and parses as blank. After completion the parse
sees one value per declared key, which is why `create_record` receives a plain
`F` and `Posted<F>` derefs to one.

The write assigns only named fields, plus the model's own `#[update(..)]`
defaults and `#[version]` bump, which `record.update()` adds to every update.
A field the submission did not post is never assigned, so a concurrent write
to it between the page load and the commit survives. A named field is assigned even when its value equals the
stored one.

### Resolving a value

`F::parse` reads each field from the completed, normalized values:

- **A scalar** trims its key's value. A non-empty value parses with `FromStr`,
  and `String` keeps the trimmed text. An empty value takes the field's blank
  answer: `""` for `String`, `None` for `Option<T>`, the `blank` expression
  where one is declared. With no answer, it is a `FieldErrorKind::Required`
  error. A parse failure is `FieldErrorKind::Invalid`, worded with the type's
  `TypedValue::NOUN`, as the schema's typed rule words it
  (`schema/validation.rs`).
- **An embedded value** calls `EmbeddedForm::read_form`, which now returns
  `Result<Self, Vec<FieldError>>`. A leaf keeps ADR-0019's rule: empty reads
  as the leaf type's `Default`. A parse failure or an undeclared discriminant
  is an `Invalid` error rather than a panic. An emptied leaf therefore stores
  its type's `Default`: an emptied `jiff::Timestamp` leaf stores the Unix
  epoch.

A blank reaches the parse without a schema error in two designed cases:
`walk_absent_groups` suppresses requiredness inside an all-empty `Repeater`
and inside a variant group the discriminant does not name
(`schema/tree.rs:197`). The panel-build check below requires such a field to
answer blank, so a `Required` error from the parse always accompanies the
schema's own error.

### Writing

`F::into_create` builds `<M as Model>::Create::default()` (which applies the
model's `#[default(..)]`s) and chains the builder's by-value `<field>(v)`
setter once per form field. `F::into_update(record, named)` calls
`record.update()` and then one `set_<field>(&mut self, v: impl Assign<T>)`
(`toasty-macros/src/model/expand/update.rs:70` at toasty `6a1f5d9`) per named
field; it returns `None` when no field is named. An embedded value is assigned
whole.

`into_update` returns `Option` because toasty asserts on an update with no
assignments:

```rust
// toasty @ 6a1f5d9, crates/toasty/src/engine/verify.rs:283
assert!(!i.assignments.is_empty(), "stmt = {i:#?}");
```

This is a plain `assert!`, so it fires in release builds. A model carrying an
`#[update(..)]` default never reaches it, because `apply_update_defaults` leaves
an assignment behind. With `Option`, app code cannot build an empty statement.

The builder comes from `record.update()`, so the model's `#[update(..)]`
defaults and its `#[version]` condition apply to every write `into_update`
returns. An edit that names no field writes nothing. It still commits the empty
transaction and runs `after_commit` with the unchanged record, then answers 303
with the success flash.

`write_update::<R>` is `into_update` followed by `F::exec_update`. The trait
needs that method because toasty's instance update builder implements no trait
that carries `exec` (the `IntoStatement` impl at `update.rs:271` covers only
the query-rooted builder). The builder's `exec` reloads the record in place, so
`write_update` returns the written row, which `after_commit` receives.

### Tenant stamping

`write_create::<R>` turns the builder into its `Insert<M>` (`IntoInsert`).
When `R::requires_tenant()` holds and `tenant_field_index::<M>()`
(`tenancy.rs:79`) finds a tenant column, it sets that column to
`require_tenant(cx)?` with `Insert::set` (`toasty/src/stmt/insert.rs:115`)
before executing. A gated resource whose tenant is inherited, like
`CommentResource` (`tenant_scope` through the parent post), has no tenant
column, and nothing is stamped. An update never assigns the tenant column,
because no form may claim it (check 3 below).

### Panel-build checks

`Panel::form_resource::<R>` adds these checks to `check_resource_inner`
(`panel/build.rs:374`). Each runs against `R::form` and `F::fields` built with
the build-time `validation_cx`, which carries the `Db` and therefore the
compiled schema an embedded key needs (`panel/build.rs:457`):

1. **Key agreement.** Every `Schema::field_names()` key is claimed by exactly
   one form field, and every claimed key is a declared field name. A control
   no field binds is reported first: that is the direction that silently drops
   data.
2. **Blank agreement.** A scalar field whose control is optional, or whose
   control sits inside a `Repeater` or a variant group, answers blank.
   Embedded fields are exempt because their leaves answer with `Default`.
3. **Tenant ownership.** On a gated resource, no form field claims the column
   `tenant_field_index` finds.
4. **Create columns.** Where `can_create` allows it, every non-nullable,
   non-relation column is a form field, filled by toasty (`#[auto]`,
   `#[default(..)]`), the stamped tenant column, or named in
   `FormResource::CREATE_COLUMNS`. Toasty keeps `#[default(..)]` in generated
   code only, so the check reads it off `M::Create::default()`: a column that
   insert leaves `NULL` is one nothing fills.

`Panel::resource::<R>` refuses a resource whose `can_create(cx)` or
`editable()` is true: that resource declares create or edit and has no form to
serve it. The checks call `can_create` with the Db-only build context, so a
request-scoped predicate that denies there skips checks 4 and the refusal; the
list page links to create only for a `form_resource` registration, whatever
`can_create` answers per request.

The checks read the schema once, at build. `form(cx)` must therefore declare
the same key set on every request. No `Schema` feature varies the key set by
request: a hidden variant group keeps its controls in the schema, and
`variant.js` hides them with the `hidden` attribute
(`crates/tablo-ui/assets/variant.js:33`).

### What the derive refuses

The derive refuses these syntactically:

- A generic or lifetime-carrying struct, a tuple struct, or a struct with no
  fields.
- A field typed `Deferred<_>`. A form binds the foreign key (`author_id`), not
  the relation; toasty does generate `set_author` for a `BelongsTo`
  (`toasty-macros/src/model/expand.rs:400`), but its argument is a relation
  expression that no control posts.
- `blank` on an `Option<T>` field (its blank answer is `None`) or on an `embed`
  field.
- An unknown `record_form` key.

rustc refuses the rest at the use site the derive emits:

- A field the model lacks: `M::fields().<ident>()` does not exist.
- A field whose type differs from the model's: the path assertion
  `Path<M, T>` fails.
- A scalar outside the supported set, which covers a `#[document]` column: the
  `FormScalar` bound fails. `FormScalar` is implemented for `String`, every
  `TypedValue` type, and an `Option` of either, so an app extends it by
  implementing `TypedValue`.
- `embed` on a type without `EmbeddedForm`.

### Errors and status codes

| Situation | Answer |
| --- | --- |
| A posted key no control declares | 400 (unchanged) |
| A schema rule, a parse error, or a `validate_record` error | 200, re-rendered inline, nothing written |
| A `validate_record` error on a field `RecordForm::fields` binds to no key | 500, nothing written |
| A record fn error | `hook_failure` (`panel/forms/submit.rs:160`, unchanged) |
| `PostResource`: author missing from the tenant | 500 from the record fn (unchanged) |
| `CommentResource`: post outside the tenant | 404 from the record fn (unchanged) |
| A declaration the build checks refuse | `Panel::build` returns `Err` naming the resource and the field |

### Showcase behavior changes

1. On edit, emptying `role`, `active`, `status`, or `featured` stores the blank
   answer (`member`, `true`, `draft`, `false`). Today `kept_one_of` and
   `kept_bool` keep the stored value. Create is unchanged: the blank answers
   are today's create defaults.
2. An edit that omits a required key keeps the stored value and passes
   validation. Today the key validates as `""` and fails as required. This
   applies to every resource.
3. A value that fails to parse after validation renders inline with a 200.
   Today it is a 500 (`submitted_parsed`) or a panic (`parse_leaf`).

Unchanged: a blank `age` stores 0, an empty `tags` group stores `""` on create
and edit, an empty cover clears `cover_id`, and both FK error classes stay as
they are.

## Edge cases

- **Partial API edits.** An edit posting only `email` changes only `email`; one
  posting only `seo_title` writes `seo` with the stored `seo_description`,
  because completion filled it from the in-transaction record.
- **Embedded enum switch.** A named discriminant selects the variant. Only that
  variant's payload is read, and a hidden group's inputs still post, since the
  `hidden` attribute does not disable an input.
- **Checkbox controls.** `Field::toggle` posts a hidden `false` before the
  checkbox under the same name, because an unchecked box posts nothing and
  would read as unnamed, which keeps.
- **Whitespace.** Every scalar trims, matching today's record fns.
  Whitespace-only is blank.
- **`validate_record` on edit** sees the completed form, so a cross-field rule
  reads stored values for the fields the submission did not post.
- **A no-op edit** does not fire `#[update(..)]` defaults or bump a
  `#[version]`, because no statement runs.
- **Tenancy.** A tenantless request on a gated resource is a 403 before any
  record fn runs, so `require_tenant` inside `write_create` never fails on a
  panel request.

## Alternatives

- **Presence types (`Submitted<T>` → `Field<T>` → `Patch<F>`).** The app would
  read per-field `Keep | Set`. On edit, the typed form has no value for an
  absent field, so `validate_record` cannot take a whole `F`. Every accessor
  needs the stored value passed back in (`patch.name(record.name.clone())`),
  and three public types encode a distinction the framework settles by
  completing the submission.
- **A diff-based write** (assign only fields whose value differs from the
  stored row). The diff runs against the in-transaction row, not the value the
  user saw, so it does not prevent overwriting a concurrent change. It also
  needs `PartialEq` on every embedded type, and `Seo` derives none. Writing the
  named fields matches today's contract.
- **The `Schema` as the typed form**, with each lens-bound control carrying a
  typed writer and no struct. Form values stay string-keyed, so a control has no
  typed value to write (ADR-0001). The generated update builder sets a field
  only by its ident (`update.rs:70`), so an assignment keyed at runtime means
  building a `stmt::Update` through `UpdateTarget`, which drops
  `apply_update_defaults` and the `#[version]` assignment and condition the
  generated `update()` adds (`model.rs:90`).
- **`type Form` on `Resource`.** An associated type cannot carry a default on
  stable (E0658), so all 121 `impl Resource` sites would name a placeholder.
- **One registration method with a `NoForm<M>` form.** Every list-only
  resource, about 80 of them and mostly `tablo-core` test resources, would
  write an empty `impl FormResource`.
- **`T::default()` as the blank answer.** `Uuid::default()` is the nil UUID
  and `bool::default()` is `false`, so a blank would store a value no one
  chose. `blank = …` names it instead.
- **A pre-write hook for in-transaction checks.** It would save three lines per
  override and add a concept. An override that delegates keeps the check and
  the write in one function.
- **An interim `Resource::form_field_names` step.** It adds public API that the
  next step deletes, and key agreement alone does not fix keep versus clear.
- **Generating the `Schema` from the form.** A field's type does not pick its
  control (see [Registering the resource](#registering-the-resource)).

## Implementation plan

One implementation PR, because the trait split, the pipeline, and the derive
are unusable without one another. Two tests are written first, against toasty
alone, and gate the API:

- **The empty-assignment test.** `record.update().exec(..)` with no setter
  panics, which confirms `into_update`'s `Option`. The assert runs on toasty's
  worker task, so the caller panics unwrapping a dropped channel
  (`db/connection.rs:65`) and the test expects `RecvError`.
- **The borrow and return test.** `record.update()` borrows the record mutably
  for the builder's life, `exec(mut self, ..)` consumes the builder
  (`update.rs:195`), and the record then holds the written row.

Then, in order:

1. Add `extern crate self as tablo_core;` to `crates/tablo-core/src/lib.rs`,
   so the derive's `tablo_core::` paths resolve inside `tablo-core`'s own unit
   tests. `proc_macro_crate` already answers `FoundCrate::Itself` with
   `tablo_core` (`crates/tablo-macros/src/embedded.rs:67-71`).
2. In `tablo-core`: `RecordForm`, `FormScalar`, `FormField`,
   `FieldError` and `FieldErrorKind`, `FieldErrors`, `Posted`,
   `write_create`, and `write_update`. Make `form_keys` public as
   `schema::value_keys`, with the discriminant first so an embedded field's
   errors render under its variant control.
3. `EmbeddedForm::read_form` and `parse_leaf` return `Result`. Remove
   `submitted`: outside its tests (`crates/tablo-core/tests/embedded_value.rs`),
   its caller is `kept_embedded`, which step 8 deletes. `EmbeddedForm::any_present`
   stays: an enum's payload fallback asks a nested value whether its own keys
   were posted (`crates/tablo-macros/src/embedded.rs:317`).
4. Add `FormResource` and `Panel::form_resource`. Move `form`, the create and
   edit routes, and the options route (`panel/mod.rs:324`) behind it. Remove
   `form`, `validate`, `create_record`, and `update_record` from `Resource`, and
   rename `hydrate_form_values` to `view_values`. Add the build checks.
5. Rewrite the submit pipeline (`panel/forms/submit.rs`) and move the upload
   backfill into completion.
6. Add the `RecordForm` derive in `crates/tablo-macros/src/record_form.rs`,
   re-exported beside `EmbeddedForm` (`crates/tablo-core/src/lib.rs:54`).
7. Migrate `tablo-core`'s test resources: 21 `create_record` and 10
   `update_record` overrides, 39 `form` overrides, and 9
   `hydrate_form_values`, each count excluding the trait's own default.
8. Migrate the showcase: `User` and `Author` first (no record fns), then
   `Comment` and `Post`. Add `#[default(jiff::Timestamp::now())]` to
   `User.created_at` and `Post.created_at`. Delete the ten presence helpers.
9. Migrate `benchmarks/tablo/src/main.rs`. Its `PostResource` is `editable()`,
   so it gains a derived form and registers with `form_resource`. Its
   `AuthorResource` drops its unused `form`, since its `can_create` and
   `editable` stay false.

`Resource` keeps `view_values(cx, record)`, defaulting to empty. It is the
detail projection for a resource registered with `Panel::resource`. For a
`form_resource`, the detail page merges `view_values` with `F::hydrate`, and
the form's keys win. The edit page reads `F::hydrate` alone. ADR-0016's rule
that the detail page and the form agree on a field holds by construction.

## Tests

- The two gating tests above.
- **Completion.** An edit posting only `email` keeps `name`; this rewrites
  `update_record_keeps_absent_fields`
  (`examples/showcase/tests/edit_check.rs:155`) as an HTTP test. An edit
  posting only `seo_title` keeps `seo_description`. An edit omitting a required
  key passes.
- **Completion reads the authoritative record.** A unit test on the completion
  step: with an advisory and an in-transaction record that differ on an unnamed
  key, the parsed form carries the in-transaction value and the write assigns
  nothing to that key.
- **Blank answers.** `String` yields `""`, `Option<T>` yields `None`, a
  declared `blank` applies on create and on edit, and a hand-written
  `RecordForm` with no answer reaches `Required`, inline with a 200.
- **The four showcase selects.** Emptying each on edit stores its blank answer.
- **No-op edit.** A submission naming no form field runs no statement and
  answers 303.
- **Uploads.** An untouched upload is unnamed and unassigned; a cleared
  optional upload is empty. `clearing_an_optional_upload_empties_the_stored_path`
  and `a_refused_edit_upload_keeps_showing_the_stored_file`
  (`crates/tablo-core/tests/uploads.rs:691`, `:765`) keep passing.
- **The posting contract.** One test per field kind asserting the rendered
  control carries a `name`, plus a hidden variant group whose inputs post.
- **Embedded enum.** A variant switch published → scheduled → published writes
  the chosen variant each time.
- **Normalize then parse.** A typed value is stored in the type's spelling,
  following `a_valid_submission_is_stored_in_the_types_spelling`
  (`crates/tablo-core/tests/typed_leaves.rs:99`).
- **One round of errors.** A schema error and a `validate_record` error in one
  submission render together; an error on an embedded field renders under its
  first key.
- **A parse error after validation**, via a control bound to a field of another
  type, renders inline with a 200.
- **Build checks.** One refusal each for: a control no field binds; a claimed
  key no control declares; an optional control on a field with no blank answer;
  a `Repeater` control on a field with no blank answer; a gated form claiming
  the tenant column; and `Panel::resource` with `can_create` or `editable()`.
- **Tenant stamping.** `AuthorResource`'s default create stores the request
  tenant, and `CommentResource` stamps nothing.
- **Registration.** A list-only resource builds with `Panel::resource`, and a
  form resource gets create, edit, and options routes.
- **FK classes.** The existing Post (500) and Comment (404) tests keep passing.
- **Derive refusals.** Syntactic refusals go in the macro unit tests beside
  `crates/tablo-macros/src/embedded.rs:805`. Type-level refusals (unknown
  field, type mismatch, unsupported scalar, `embed` without `EmbeddedForm`) go
  in `compile_fail` doctests on the derive's documentation in `tablo-core`. The
  workspace has no `trybuild` harness, and `cargo test --workspace` runs
  doctests.
- Delete `assert_hydrate_keys_are_form_fields`
  (`examples/showcase/tests/common/mod.rs:461`) and its callers: key agreement
  is checked at build.

## Docs

- ADR-0022, "Record forms", in ADR-0019's shape: the decisions above, with a
  `## Consequences` section recording the residuals. An app's own `set_*`
  calls on the builder `into_update` returns are unchecked, and a model column
  neither the form nor the app sets fails at the driver: toasty exposes no way
  to enumerate a model's settable fields.
- Amend ADR-0001 (`docs/adr/0001-typed-field-lenses.md:17-18`), which records
  that form values stay string-keyed. The transport stays string-keyed, and
  the record fns now receive typed values. Keep the reference to upstream
  issues #115 and #119.
- Amend ADR-0019 for `read_form` returning `Result` and the removal of
  `submitted`.
- `docs/adr/README.md`: the index row for ADR-0022.
- The two toasty gaps the design works around live where they bite: the
  `RecordForm::exec_update` rustdoc records that the instance update builder
  implements no trait carrying `exec`, and the Alternatives above record that it
  sets a field only by ident.
- `CONTEXT.md`: add `Record form`, `Posted`, and `Completion` entries (the
  last defines named keys and fields), with an `_Avoid_` list for each (`Patch`, `Draft`, `Presence`,
  `Input`). Amend the `Resource` entry (`CONTEXT.md:62`), which names
  `hydrate_form_values`.
- `docs/guide/src/resources.md`: a chapter adapted from the User-facing API
  section above. Reword `:88`: there is still no `Resource` derive (GH #222),
  and the macros crate now ships `derive(EmbeddedForm)` and
  `derive(RecordForm)`.
- `docs/dev/architecture.md`: the `tablo-macros` line at `:13`, and the "A
  write request" ordering at `:62-74`.
- Replace the present-keys paragraph on `Schema::validate`
  (`schema/mod.rs:308-311`) with the completion rule.

## Open questions

None. Four decisions were settled before this document was written: completion
over presence types, default record fns with delegation, two registration
methods, and blank answers equal to the showcase's create defaults.

## Out of scope

- **An in-transaction relationship re-check in the framework.** `Post` and
  `Comment` keep their own, with their distinct 500 and 404 classes.
- **Request-dependent key sets.** Nothing in the `Schema` produces them today.
- **Form-level errors** not tied to a field.
- **A blank answer for embedded leaves.** They keep ADR-0019's `Default` rule.
- **`#[document]` columns in a record form.**
- **A `Resource` derive** (GH #222).
