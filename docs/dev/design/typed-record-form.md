# A derived form that writes what the schema declares

## Problem

A record fn reads submitted values through string literals nothing checks, and
writes them through `toasty::create!` / `toasty::update!`, which are not
exhaustive. Three hand-maintained key sets must agree — the `Schema`'s lenses,
the record fn's `values.get("…")` calls, and the `create!` / `update!` field
lists — and no check runs in either direction.

`PostResource` is the clearest case: `form` binds 9 model fields, and it has a
`view`, a `hydrate_form_values`, a `create_record`, and an `update_record` that
each name those same fields again.

`Schema::unknown_keys` (`schema/mod.rs:229`, called at
`panel/forms/common.rs:64`) refuses *extra* posted keys. It cannot catch a
record fn reading a key the schema does not declare, because nothing connects the
two lists. `assert_hydrate_keys_are_form_fields`
(`examples/showcase/tests/common/mod.rs:461`) covers the hydrate direction for
3 of the 4 showcase resources, and only that direction.

The present-keys rule an app must obey lives on the validator
(`schema/mod.rs:308-313`) and not on `Resource::update_record`
(`resource/mod.rs:469-485`). A key the submission omits validates as `""`
(`schema/tree.rs:120-123`), and a record fn that resolves an omitted optional
field from the stored value (`app.rs:418-422`) makes the edit a discard while
the flash still reads "Updated" (`submit.rs:145`).

### What the guards already cover

| Guard | Catches | Does not catch |
| --- | --- | --- |
| `unknown_keys` | a key posted that no control declared | a record fn reading an undeclared key |
| `validate` requiredness | an omitted **required** key, inline with a 200 | an omitted **optional** key |
| `absent_fields` (`schema/mod.rs:349-361`) | a required key inside a group the user cannot see | presence, as such |

The framework also **manufactures** absent keys: `drop_client_typed_uploads`
(`common.rs:117-127`) deletes a declared `FileUpload`'s value when no file part
arrived, and `walk_absent_groups` (`schema/tree.rs:197-242`) skips an all-empty
`Repeater` and a hidden variant group whole. A submission that omits a declared
key is a normal state — `reject_unknown_form_keys`'s own test asserts "Absent
keys are fine (present-keys-only updates)" (`common.rs:266-272`).

The failure is not a form the framework mishandles. It is a record fn that
compiles against a key set nothing verifies, whose outcomes are a blank write
with a 303 and a success flash, or a 500 inside `submitted_parsed`
(`app.rs:956`). Both are silent to the app author.

## Decisions

### 1. Presence is two types

`Submitted<T>` is what a parse produces: `Absent` (the submission did not name
the field), `Blank` (it named the field, empty), `Value(T)`. `Field<T>` is what
a write consumes: `Keep` (leave the stored value) or `Set(T)`.

A parse resolves each `Submitted` into a `Field` or a `FieldError`, so the write
never sees `Blank`. `Field<Option<Uuid>>` gives `Keep` and `Set(None)` two
spellings for "leave it" and "clear it", in the type the setter consumes rather
than in a convention a record fn has to remember.

One type cannot do this. `Blank` carries no `T`, so a single enum leaves a
setter with no value to take and an accessor with no value to return.

### 2. A blank is a refusal unless the field names an answer

A `Blank` resolves to the field's blank answer:

- `None` for an `Option<T>` column.
- The `#[record_form(blank(..))]` literal where the app declares one.
- Otherwise a `FieldError`, rendered inline with a 200 like any field error.

One rule covers create and update, and it is what lets `create_record` take a
plain `Self::Form`: the form exists because every field resolved. There is no
zero value. `Uuid::default()` is `Uuid::nil()`
(`uuid-1.26.1/src/lib.rs:957-961`, the version `Cargo.lock` pins), so a
`T::default()` answer would write a nil FK into a `NOT NULL` column.

A blank required field reaches a record fn in two designed cases.
`walk_absent_groups` suppresses requiredness inside an absent `Repeater` and a
hidden variant group — "a value the user cannot see must not fail the submit"
(`schema/tree.rs:191-196`, `schema/mod.rs:315-318`) — and an embedded leaf binds
`nullable: true` by policy (`schema/lenses.rs:40-44`), even where the flattened
column is `NOT NULL`.

`age: i64` in the showcase is neither nullable nor given a `blank(..)`, so an
emptied `age` refuses rather than storing a zero.

### 3. An empty posted control clears; an unposted control keeps

A browser posts `""` for a control the user emptied, and the stored value for
one the user did not touch — the edit form renders the stored value, so an
untouched save posts it back. So `Blank` means clear, uniformly.

That changes what six showcase fields do. `kept_one_of` / `kept_bool`
(`app.rs:68-88`) keep the stored value on an empty submit for `role`, `active`,
`status`, and `featured`, and an all-empty `tags` `Repeater` is classified
*absent* by `walk_absent_groups`, so it keeps too. All six start clearing, and
the migration names each one.

It rests on a framework invariant, which becomes a tested contract:

> Every rendered control posts its key, empty when cleared.

Every field kind renders a `name`
(`schema/fields/{text_input,select,textarea,file_upload}.rs`), and `variant.js`
hides a group with CSS, so a hidden control's inputs still post. A control that
clears by *not* posting would read as `Keep`, so a `Checkbox` field kind must
emit a hidden companion input. `FileUpload` already satisfies the invariant:
`clear_<field>` posts an empty file part (`tests/uploads.rs:700-704`) and
`store_uploads` writes the uploader's answer (`src/upload.rs:198-206`).

### 4. A field binds by ident; a column resolves at run time

The derive emits `M::fields().<name>()`, and `M::fields()` has one method per
model field ident (`toasty-macros/src/model/expand/fields.rs:55-70`), so a
model field rename is a rustc error.

What the derive cannot see is the compiled app schema, so two things stay
runtime:

- An embedded leaf's key is its **flattened storage column**
  (`schema/lenses.rs:155-160`, which reads `Db::schema().mapping`), not the
  ident. `Seo { title, description }` becomes `seo_title` / `seo_description`
  (`models.rs:60-70`), and only `cx` knows that — the key comes from
  `request_schema` (`schema/lenses.rs:57-59`). A top-level leaf takes its name
  from the field itself (`schema/lenses.rs:142`).
- `#[column(name = "…")]` renames a physical column with no ident change
  (`toasty-macros/src/model/schema/field.rs:159`, `model/schema/column.rs:30`).

So the ident half is a compile error and the key half is the panel-build check
(decision 9), and only that check sees an embedded leaf or a renamed column.

### 5. The write is generated, for update and for create

`toasty::update!` and `toasty::create!` both expand to one builder call per
*named* field, with no completeness check
(`toasty-macros/src/update/expand.rs:70-101`, `create/expand.rs:11-18`). A field
the block omits is dropped silently, and a `fields()` against
`Schema::field_names()` cannot catch it: both halves contain the field.

So both paths are generated:

- `Patch::into_update` calls `set_<field>` for exactly the fields that are
  `Set`. `set_<field>(&mut self, ..) -> &mut Self` is generated per field,
  ident-keyed (`toasty-macros/src/model/expand/update.rs:60-73`,
  `model/schema/field.rs:309`).
- `Form::into_create` calls `set_<field>` for every field, and the app names the
  columns the form does not own on the returned builder.

The two builders are asymmetric, and the design keeps them so: the create
builder's `exec` resolves to the model
(`toasty-macros/src/model/expand/create.rs:38`) and the update builder's to `()`
(`toasty-macros/src/model/expand/update.rs:195`), because an instance update
reloads the record in place through `UpdateTarget::apply_result`. So
`create_record` returns the builder's value and `update_record` returns the
record it was handed.

The instance builder is also the right target over a hand-built
`stmt::Update`: `record.update()` keeps `apply_update_defaults()` and the
`#[version]` compare-and-set condition that a rebuilt statement would drop.

### 6. `into_update` returns `Option`

A submission that names no form field yields a zero-assignment `stmt::Update`,
and toasty's verifier asserts on that:

```rust
// toasty @ 6a1f5d9, crates/toasty/src/engine/verify.rs:283
assert!(!i.assignments.is_empty(), "stmt = {i:#?}");
```

A plain `assert!`, not a `debug_assert!`, reached unconditionally from
`Engine::exec` (`toasty/src/engine.rs:88`). It fires in release.

`into_update` returns `None` when no field is `Set`, and the app skips `exec`.
`Some` implies at least one assignment, so an empty statement is unreachable
from app code.

### 7. One validation hook, keyed by a field enum

`validate_record(&Cx, &Self::Form) -> HashMap<AuthorFormField, Vec<String>>`.

The 200-versus-500 split is load-bearing: field errors render inline with a 200
(`submit.rs:111-113`, `submit.rs:196-208`) and a record-fn error is a 500
(`resource/mod.rs:413-415`). The key type is required because a typo'd key is a
silently dropped rule — `errors.entry(field).or_default().extend(..)` never
notices a key no control produced. The derive already owns the ident-to-key map.

### 8. A parse failure is a `FieldError` with a kind

```rust
pub struct FieldError {
    /// Which schema key failed, and why. An app maps a kind to its own
    /// message; `FieldError::message` is the framework's default sentence.
    pub kind: FieldErrorKind,
    /// The key the failure belongs to.
    pub key: String,
    /// The raw submission text that failed, for the default message.
    pub value: String,
}

pub enum FieldErrorKind { Blank, Parse, NotAValue, UnknownKey }
```

A kind rather than a bare `String` because the repo's rule is that the framework
owns the sentence (`schema/validation.rs`). There is no i18n layer today, so a
kind costs one enum now and a translation table later; a `String` message costs
a signature change later.

### 9. Presence is classified once, by the framework

One framework function produces the absent set, and `validate`, `check_unique`,
and the parse step all read it, so they cannot disagree.

`Schema::absent_fields` stays `pub(crate)` and is **not** that function. It
answers a group question: the inner field names of an all-empty `Repeater` and
of a variant group the discriminant does not name
(`schema/mod.rs:349-361`, `schema/tree.rs:197-242`). Its consumers are the
requiredness skip (`schema/mod.rs:341`), the unique probe
(`panel/forms/unique.rs:43`), and the relationship-existence skip
(`schema/mod.rs:374`). It cannot distinguish a field the submission did not name
from one it named empty.

Key presence is the separate question `values.contains_key` answers, and the
framework answers it for the derive.

### 10. The form is a trait; the derive is the ergonomic form

```rust
pub trait FormResource: Resource {
    type Form: RecordForm<Model = Self::Model>;
    // … form, validate_record, create_record, update_record
}
```

39 of the 120 `impl Resource` sites in the workspace declare `fn form`; the rest
are mostly `tablo-core`'s own list and test resources. Giving `Resource` a
`type Form` with no default puts a placeholder type in all 120, permanently, for
a guarantee four showcase resources need, and an associated type cannot carry a
default on stable (E0658). A second trait removes the placeholders:
form-bearing resources implement both, list-only resources implement `Resource`.
`Panel::build` gates the create and edit routes on `R: FormResource`.

The derive is not load-bearing. Binding by ident and generating the write is a
trait; the derive writes the one-line-per-field parts. A hand-written
`impl RecordForm` must stay possible — it is how the framework's own tests
exercise the write without a macro.

## API

```rust
#[derive(tablo_core::RecordForm)]
#[record_form(model = User, blank(role = "member", active = true))]
pub struct UserForm {
    name: String,
    email: String,
    role: String,
    active: bool,
    /// No `blank`, so an emptied `age` is a `FieldError` with a 200.
    age: i64,
}
```

```rust
/// A field's presence in a submission.
pub enum Submitted<T> {
    /// The submission did not name the field.
    Absent,
    /// The submission named the field and it was empty.
    Blank,
    /// The submission named the field and carried a value.
    Value(T),
}

impl<T> Submitted<T> {
    /// `Absent` is the only unnamed state.
    pub fn is_absent(&self) -> bool;
    pub fn is_named(&self) -> bool;
}

/// A field's write intent. `Keep` leaves the stored value alone; `Set(T)`
/// writes `T`, so `Set(None)` is how a nullable column is cleared.
pub enum Field<T> {
    Keep,
    Set(T),
}

impl<T> Field<T> {
    pub fn is_keep(&self) -> bool;
    pub fn is_set(&self) -> bool;
    pub fn as_set(&self) -> Option<&T>;
}
```

```rust
/// One form field's claim on the schema, for the panel-build check.
pub struct ClaimedKeys {
    /// The form field these keys came from, for the refusal message.
    pub field: &'static str,
    /// The keys. A struct claims one per leaf; an enum claims the union of
    /// every variant's keys.
    pub keys: Vec<String>,
    /// True only for an embedded enum: only one variant's controls render, so
    /// the claim legitimately exceeds the schema.
    pub may_exceed: bool,
}

pub trait RecordForm: Sized {
    /// The model this form writes. Pinned, so `into_update` cannot be handed
    /// an unrelated record.
    type Model: toasty::schema::Model;

    /// Every schema key this form binds. The panel refuses a resource that
    /// leaves a `Schema::field_name()` unclaimed or claims a key the schema
    /// does not declare.
    fn claimed_keys(cx: &Cx) -> Vec<ClaimedKeys>;

    /// The stored row as the form spells it, for an edit or detail page.
    fn hydrate(cx: &Cx, record: &Self::Model) -> HashMap<String, String>;
}

/// One parse of one submission, in both shapes the record fns need. Resolving
/// twice would answer the presence question twice, and the two answers could
/// differ.
pub struct Parsed<F> {
    /// Every field resolved, so a create record fn has no presence to handle.
    pub form: F,
    /// The same values as write intent. A create record fn ignores this half.
    pub patch: Patch<F>,
}

/// `RecordForm` plus the per-field write intent. One per form, generated.
pub struct Patch<F> { /* one Field<T> per field of F */ }

impl<F: RecordForm> Patch<F> {
    /// Generated per field, on `&self` so an app can read a resolved value
    /// before it hands the patch to `into_update`.
    ///
    /// `Keep` returns `keep`; `Set` returns the submission's resolved value.
    /// For a nullable column `Set(None)` is the clear.
    pub fn author_id(&self, keep: uuid::Uuid) -> uuid::Uuid;
    pub fn cover_id(&self, keep: Option<uuid::Uuid>) -> Option<uuid::Uuid>;
    pub fn seo_title(&self, keep: String) -> String;

    /// The write `update!` would spell by hand, for exactly the fields the
    /// submission named. `None` when it named none, so no zero-assignment
    /// `stmt::Update` reaches toasty.
    pub fn into_update(
        self,
        record: &mut F::Model,
    ) -> Option<<F::Model as toasty::schema::Model>::Update<'_>>;
}

impl<F: RecordForm> F {
    /// The framework's only entry point from a submission to presence. `cx` is
    /// here for key resolution (`leaf_key` needs the compiled app schema), not
    /// for the absent walk, which needs no context.
    pub fn from_submission(
        cx: &Cx,
        schema: &Schema,
        values: &HashMap<String, String>,
    ) -> Result<Parsed<Self>, Vec<FieldError>>;

    /// The write `create!` would spell by hand. Every field is `Set`, so the
    /// builder always carries at least one assignment. The app names the
    /// columns the form does not own — tenant, timestamps, a rewritten FK — on
    /// the returned builder.
    pub fn into_create(self) -> <F::Model as toasty::schema::Model>::Create;
}
```

```rust
impl FormResource for AuthorResource {
    type Model = Author;
    type Form = AuthorForm;

    fn form(cx: &Cx) -> Schema { /* unchanged, hand-written */ }

    fn validate_record(
        _cx: &Cx, _form: &Self::Form,
    ) -> HashMap<AuthorFormField, Vec<String>> {
        HashMap::new()
    }

    fn create_record(
        cx: &Cx,
        form: Self::Form,
        ex: &mut dyn toasty::Executor,
    ) -> impl std::future::Future<Output = Result<Author>> + Send
    where
        Self: Sized,
    {
        let cx = cx.clone();
        async move {
            let mut q = form.into_create();
            // `require_tenant` makes the handler answer 403 before this runs,
            // so it never panics on a tenantless submit.
            q.set_tenant_id(require_tenant(&cx)?);
            // The create builder's `exec` resolves to the model.
            q.exec(&mut *ex).await.map_err(|e| -> topcoat::Error { e.into() })
        }
    }

    async fn update_record(
        _cx: &Cx,
        mut record: Author,
        patch: Patch<AuthorForm>,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Author> {
        // App checks first: `into_update` borrows the record for the builder's
        // life. The per-field accessors take `&self`, so reading a resolved
        // value does not consume the patch.
        let name = patch.name(record.name.clone());
        let email = patch.email(record.email.clone());

        if let Some(mut q) = patch.into_update(&mut record) {
            q.set_name(name);
            q.set_email(email);
            // `exec` consumes the builder and reloads `record` in place, so
            // the record handed back is the written one. `after_commit`
            // subscribers depend on that.
            q.exec(&mut *ex).await.map_err(|e| -> topcoat::Error { e.into() })?;
        }
        Ok(record)
    }
}
```

`Resource::hydrate_form_values` stays on `Resource`, because the detail page
reads it too (`panel/detail.rs:46`) and ADR-0016 rests on the page and the form
agreeing about what a field holds. `FormResource` gives it a default that
delegates to `<Self::Form as RecordForm>::hydrate`, so a form-bearing resource
writes no hydration code and a formless one keeps its own.

The `if let Some(..)` is the whole of the empty-assignment fix, and the type
makes it impossible to forget: `Some` implies at least one assignment.

## What the derive generates

- `hydrate` (`Model` → the flat key/value map) and `from_submission`, its
  inverse, both keyed through `leaf_key` (`schema/embedded.rs:106-115`). For an
  embedded enum the existing `EmbeddedForm::write_form` already does the
  variant-aware write (`schema/embedded.rs:74-80`) and `read_form` the
  variant-aware read (`schema/embedded.rs:81-92`), so the derive calls them
  rather than reimplementing them.
- `from_submission`, one `Submitted` per field from the key's presence, then the
  field's value from the answer, and one `Field` per field from that, in one pass
  over the keys. A `String` field stays the untyped identity path
  (`schema/validation.rs:249-251`); a typed field parses through `TypedValue`
  (`schema/validation.rs:22`) **after** `normalize_values`, so the parse sees
  the spelling the write stores. `Submitted` is public because the tests assert
  its three states, and the derive is its only constructor.
- `into_update` and `into_create`, one `set_<field>` per `Set` / per field.
- `validate_record`'s field enum, one variant per field.
- One per-field accessor on `&self`.
- `claimed_keys` and the `check_resource_inner` cross-check, in the existing slot
  where `R::form(cx)` is already bound (`panel/build.rs:411`). It is a function,
  not a `const`: an embedded leaf's key exists only once the app schema is
  compiled (`schema/lenses.rs:57-59`).

### Embedded values are per leaf, and an enum is the one exception

`EmbeddedForm::read_form` calls `parse_leaf` for every leaf unconditionally
(`tablo-macros/src/embedded.rs:405-407`), and `parse_leaf` collapses
absent-and-empty to `T::default()` (`schema/embedded.rs:341-357`). A submission
naming `seo_title` but not `seo_description` would store `""` into a `NOT NULL`
flattened column. The `EmbeddedForm` derive already emits the per-leaf presence
this needs, as `leaf_present` (`tablo-macros/src/embedded.rs:404`); reuse it.

So a form declares the leaves:

```rust
#[derive(tablo_core::RecordForm)]
#[record_form(model = Post)]
pub struct PostForm {
    title: String,
    body: String,
    status: String,
    featured: bool,
    cover_id: Option<uuid::Uuid>,
    tags: String,
    // `Seo`'s leaves, flattened. `into_update` writes them as one patch.
    seo_title: String,
    seo_description: String,
    // `Publication`'s discriminant decides the variant, so the enum stays one
    // field whose presence is the discriminant.
    publication: Publication,
    author_id: uuid::Uuid,
}
```

and `into_update` groups the present leaves into one assignment:

```rust
// The shape toasty's own `update!` emits for an embedded partial
// (`toasty-macros/src/update/expand.rs:139-146`), and which the instance
// builder accepts: `__macro_fields_root` is `#[doc(hidden)] pub`
// (`toasty-macros/src/model/expand/update.rs:178-181`) and
// `impl<T> Assign<T> for Assignment<T>` exists
// (`toasty/src/stmt/assignment.rs:95`).
q.set_seo(toasty::stmt::apply([
    toasty::stmt::patch(fields.seo().title(), patch.seo_title.as_set()?),
    toasty::stmt::patch(fields.seo().description(), patch.seo_description.as_set()?),
]));
```

An **embedded enum is one field, not a set of leaves**, because a variant switch
is a state transition rather than a value: naming the discriminant replaces the
whole value, and the variant owns its columns. `Publication`'s `read_form`
implements that rule — the variant from the discriminant, an undeclared
discriminant refused loudly, a missing one falling back by which payload was
submitted (`schema/embedded.rs:81-92`) — so the derive keeps it and gives the
field whole-value `Keep | Set(Publication)`.

Its `ClaimedKeys` carries `may_exceed: true`, because the schema renders one
variant's controls and the claim is the union across all of them.

### Compile-time refusals

The derive refuses, by type error, a form field that cannot be written:

- A field whose type is `Deferred<_>` (`Post.author` has no `set_author`; a
  form declares the FK, `author_id`).
- A field with no `set_` setter. `expand_setter_target` returns `None` for a
  `via` relation and for every other non-primitive
  (`toasty-macros/src/model/expand.rs:394-413`).
- A generic or lifetime-carrying form. `RecordForm` cannot name a `Model` per
  instantiation; `tablo-macros/src/embedded.rs:95-104` is the `unsupported()`
  helper that reports this shape of refusal.
- A field the model does not have, through `M::fields().<name>()`.

A `#[document]` column is one column and N fields, and neither `Submitted` nor
`Field` has a representation for it, so the derive refuses it through the same
`unsupported()` path.

## Stays hand-written

- **`Schema`.** A field's Rust type does not pick its control: `role` is a
  `String` on a `Select`, `active` a `bool` on a `Select`, `author_id` a `Uuid`
  on a relationship `Select`, `tags` a `String` under a `Repeater`, `body` a
  `String` on a `Textarea` (`app.rs:287-321`, `app.rs:856-914`).
- **The unknown-key allow-list** (`common.rs:64`, `schema/mod.rs:229`). A form
  field must never widen it.
- **Transport keys and the upload pipeline**: `strip_transport_keys`,
  `drop_client_typed_uploads`, `store_uploads` (`src/upload.rs:185`),
  `restore_pending_uploads`, the untouched-file backfill, and `clear_<field>`
  (`submit.rs:54-121`, ADR-0017). The backfill is what makes an untouched file
  input a `Value` rather than a `Blank`; it stays.
- **`check_unique` and relationship existence** (`schema/mod.rs:375-390`).
- **Tenant injection and the FK error classes**: 500 for a missing author
  (`app.rs:961-972`), 404 for a cross-tenant parent (`app.rs:1113-1116`). These
  are `set_*` calls on the builder `into_update` returns.
- **`EmbeddedForm` and its derive.** The flatten rule calls `write_form` /
  `read_form`; it does not replace them.

### Out of reach of any build check

A model column that neither the form nor the app names is a runtime driver
error, and no build check can see it: `toasty` offers no way to enumerate a
model's fields, since `__macro_fields_root` returns a struct of accessor methods
rather than a list. ADR-0022 records it under `## Consequences`.

## Testing

- Behavioural tests beside `crates/tablo-core/tests/embedded_value.rs`, driven
  from the single `[[test]]` target (`crates/tablo-core/Cargo.toml:13-17`). A
  macro unit test carries no consumer manifest, so `proc_macro_crate` cannot
  resolve `tablo-core` and can only assert refusals
  (`crates/tablo-macros/src/embedded.rs:809-819`).
- **The empty-assignment regression.** A submission naming no field produces no
  `stmt::Update` and answers 303.
- **`into_update`'s borrow and return shape.** `exec(mut self, ..)` consumes the
  builder (`toasty-macros/src/model/expand/update.rs:195-200`) and reloads the
  record in place, so `Ok(record)` hands back the written row. A record fn that
  returns a pre-write snapshot notifies `after_commit` subscribers with stale
  values.
- **Three states per field type**, and each per-field accessor on `Keep`, on
  `Set(Some)`, and on `Set(None)`.
- **A blank with no answer refuses**, inline with a 200: `age: i64` on create
  and on update.
- **The posting invariant.** One test per field kind asserting the rendered
  control carries a `name`, plus a hidden variant group whose inputs still post.
- **`Blank` clears.** Tests for `role`, `active`, `status`, `featured`, and for
  clearing `tags` on edit. `posts_create_with_empty_optional_tags_group_submits`
  (`file_repeater_check.rs:145-181`) covers create only.
- **The panel-build check**, four refusals: a form key the schema does not
  declare; a `Schema::field_name()` no field claims; an embedded enum whose
  union does not cover the active variant; a relation-named form field.
- **Embedded struct, both directions.** Naming one leaf leaves the other `Keep`,
  and a blank leaf with no answer refuses. Whole-value presence can express
  neither.
- **Embedded enum.** A variant switch across published → scheduled → published,
  asserting the un-chosen variant's payload is not read as absent.
- **A parse after `normalize_values`**, following the fixpoint test at
  `tests/typed_leaves.rs:312-339`.
- **An error keyed by the field enum**, asserting a rule for `age` renders
  inline with a 200 rather than becoming a 500.
- **An FK check that reads the resolved value**: `ensure_post_in_tenant` and
  `PostResource::author_exists` run against what the write stores.
- **A model column every form writes.** One test per showcase model asserting
  no column is left to the driver.
- **`FormResource` split**: a list-only resource still builds a panel, and a
  form-bearing one gets create and edit routes.
- `assert_hydrate_keys_are_form_fields` extended to `CommentResource`
  (`examples/showcase/tests/common/mod.rs:461`).
- `update_record_keeps_absent_fields` (`tests/edit_check.rs:155-184`) and the
  clear/keep upload cases (`tests/uploads.rs:690-806`) keep passing.

## Scope

Two steps, each a complete change with its own value. Splitting the typed form
further yields an interval with no consumer — the types exist before anything
calls them, and the signature change breaks every caller at once.

### 1. Key agreement at panel build

- `Resource::form_field_names(&Cx) -> Vec<String>`, defaulting to `Vec::new()`,
  documented as "unclaimed", with a `tracing::warn!` when a resource that
  declares a form leaves it unclaimed. A silently inert check is the failure
  mode to avoid.
- In `check_resource_inner`, after `let form = R::form(cx);`
  (`panel/build.rs:411`): require every `Schema::field_name()` to appear in
  `form_field_names()`, and every entry to be a `Schema::field_name()`.
  Bidirectional. The data-loss direction — a control the resource does not bind
  — is reported first. Set equality, because a subset check is the half
  `assert_hydrate_keys_are_form_fields` already covers.
- All four showcase resources declare their keys (2 + 3 + 6 + 9). The lists
  already exist in their `hydrate_form_values`; this copies them.
- `assert_hydrate_keys_are_form_fields` gains `CommentResource` and the reverse
  direction.
- One paragraph in `docs/guide/src/resources.md` replacing the present-keys
  rule, which sits only on `schema/mod.rs:308-313`.
- Step 2 removes `form_field_names` and reads `<R::Form as RecordForm>::claimed_keys`
  in the same slot.

### 2. The typed form

One step, because each half is unusable alone. Two tests are written first and
their answers gate the API:

- **The empty-assignment test.** A submission naming no field produces no
  statement. Confirms decision 6's `Option`.
- **The borrow and return test.** `into_update` returns
  `Option<<M as Model>::Update<'_>>`, `exec` consumes it, `Ok(record)` hands back
  the written row.

Then, in this order:

- `Submitted`, `Field`, `FieldError`, `FieldErrorKind`, `Parsed`,
  `ClaimedKeys`, and `RecordForm` in `tablo-core`.
- The `FormResource: Resource` split, and the create/edit routes gated on
  `R: FormResource` in `Panel::build`.
- The submit pipeline. `validate_record` needs a `Self::Form`, so the parse moves
  ahead of it. Today the order is `R::validate` (`submit.rs:111`) →
  `check_unique` (`submit.rs:192`, `:263`) → re-render on error
  (`submit.rs:196-208`, `:266-279`) → `normalize_values` (`submit.rs:210`,
  `:281`) → the record fn. The new order is `validate_async` → `check_unique` →
  re-render on error → `normalize_values` → `from_submission` →
  `validate_record` → re-render on error → the record fn, which takes
  `parsed.form` on create and `parsed.patch` on edit. A second re-render step,
  and the unique probe sees normalized values.
- The `RecordForm` derive in `tablo-macros`, next to `EmbeddedForm`.
- The four showcase resources: `Author`, `Comment`, `User`, `Post`. `Post` is
  the informative one — an embedded struct, an embedded enum, a `Repeater`, two
  relationship `Select`s, tenant injection, and both FK error classes — so it
  goes third, after `User` has proven a typed optional field and a `blank(..)`.
- `benchmarks/tablo` (`src/main.rs`) has 2 `impl Resource` sites in a detached
  workspace that gates 6 and 7 check; both implement `Resource` only.
- The tests above.

Surfaces the retyping touches, measured on the workspace: 120 `impl Resource`
sites plus 2 in `benchmarks/tablo`; 21 `create_record` overrides, 10
`update_record`, 9 `hydrate_form_values`, and 1 `Resource::validate`. Ten
intra-doc links name `hydrate_form_values` or `Resource::validate`
(`schema/mod.rs:90`, `resource/mod.rs`, `app.rs:111`, `docs/adr/0010:23`,
`0016:55`, `0019:50`, `docs/guide/src/detail-pages.md:40`,
`docs/guide/src/resources.md:14,124`, `docs/guide/src/forms.md:56`,
`CONTEXT.md:62`) and all run under `RUSTDOCFLAGS="-D warnings"`.

## Docs

- Amend ADR-0001 (`:17-18`), which records that form values stay string-keyed
  because "the lens proves field existence, not typed data flow". The ident half
  is compile-time; the key half is a panel-build check. Keep the trailing "See
  upstream issues #115 (metadata) and #119 (instance→field extraction)" — #119
  is not closed by this change, because an embedded leaf's column name is a
  runtime key.
- ADR-0022 for decisions 1–10, with a `## Consequences` section, in ADR-0019's
  shape. Record the residuals there: the unchecked `set_*` an app adds to the
  builder `into_update` returns (tenant, a rewritten FK), the model column
  neither the form nor the app names, and `create!`'s non-exhaustiveness on
  paths the form does not own.
- `docs/adr/README.md`: the index row. Numbers are permanent.
- `CONTEXT.md`: a `RecordForm` entry, and amend the `Resource` entry at `:62`,
  which names `hydrate_form_values` as the record's string projection. `Patch`,
  `Submitted`, and `Field` are new domain terms and want the `_Avoid_` list the
  file's own convention gives them — `Draft`, `Presence`, and `Input` are the
  traps.
- `docs/guide/src/resources.md:88`, which states there is no `Resource` derive
  (GH #222). There is still none; a form derive is a different thing. Reword
  rather than delete.
- `docs/dev/architecture.md:13`, where `tablo-macros` is described as "the
  `EmbeddedForm` derive", the module map, and the "A write request" ordering at
  `:62-74`, which decision 9 changes.
