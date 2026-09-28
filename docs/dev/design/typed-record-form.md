# A derived form that writes what the schema declares

## Problem

Three key sets must agree by hand: the `Schema`'s leaves, the record fn's
`values.get("…")` calls, and the `create!` / `update!` field lists.
`PostResource` binds 9 model fields, and `hydrate_form_values`,
`create_record`, and `update_record` each name all 9 again — four lists, and no
check runs on the agreement. (`view` is a fifth, and a partial one: it omits
`author_id` and `cover_id`.)

Two failures follow, both silent to the app author:

- An omitted key validates as `""` (`schema/tree.rs:120`), and a record fn that
  falls back to the stored value (`app.rs:418`) turns the edit into a discard
  behind a 303 and the success flash (`submit.rs:145`). The rule an app must
  obey — write only what the submission names — is documented on the validator
  (`schema/mod.rs:308`), not on `Resource::update_record`
  (`resource/mod.rs:469`), which is the signature that has to enforce it.
- A record fn reading an undeclared key never fails `unknown_keys`, and a
  `submitted_parsed` failure on it is a 500 (`app.rs:956`).

What the existing guards cover, and what they cannot:

| Guard | Catches | Does not catch |
| --- | --- | --- |
| `unknown_keys` (`schema/mod.rs:229`, called at `panel/forms/common.rs:64`) | a posted key no control declared | a record fn reading an undeclared key |
| `validate` requiredness | an omitted **required** key, inline with a 200 | an omitted **optional** key |
| `absent_fields` (`schema/mod.rs:357`) | a required key inside a group the user cannot see | presence, as such |

An absent key is a normal state, and the framework manufactures them:
`drop_client_typed_uploads` (`common.rs:117`) deletes a declared `FileUpload`'s
value when no file part arrived, and `walk_absent_groups` (`schema/tree.rs:197`)
drops an all-empty `Repeater` and a hidden variant group whole. An absent key is
never a bug on its own; what matters is whether the write cleared it.

## Types

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
    pub fn is_absent(&self) -> bool;
    pub fn is_named(&self) -> bool;
}

/// A field's write intent.
pub enum Field<T> {
    /// Leave the stored value.
    Keep,
    /// Write the value. `Set(None)` clears a nullable column.
    Set(T),
}

impl<T> Field<T> {
    pub fn is_keep(&self) -> bool;
    pub fn is_set(&self) -> bool;
    pub fn as_set(&self) -> Option<&T>;
}

/// Which schema key failed, and why. An app maps a kind to its own message;
/// `FieldError::message` is the framework's default sentence.
pub struct FieldError {
    pub kind: FieldErrorKind,
    /// The key the failure belongs to.
    pub key: String,
    /// The raw submission text that failed.
    pub value: String,
}

pub enum FieldErrorKind {
    Blank,
    Parse,
    NotAValue,
    UnknownKey,
}

/// One form field's claim on the schema, for the panel-build check.
pub struct ClaimedKeys {
    /// The form field these keys came from, for the refusal message.
    pub field: &'static str,
    /// A struct claims one key per leaf; an enum claims the union of every
    /// variant's keys.
    pub keys: Vec<String>,
    /// True only for an embedded enum, where one variant's controls render.
    pub may_exceed: bool,
}

pub trait RecordForm: Sized {
    /// Pinned, so `into_update` cannot be handed an unrelated record.
    type Model: toasty::schema::Model;

    /// Every schema key this form binds. The panel refuses a resource that
    /// leaves a `Schema::field_name()` unclaimed or claims a key the schema
    /// does not declare.
    fn claimed_keys(cx: &Cx) -> Vec<ClaimedKeys>;

    /// The stored row as the form spells it, for an edit or detail page.
    fn hydrate(cx: &Cx, record: &Self::Model) -> HashMap<String, String>;
}

/// One parse of one submission, in both shapes the record fns need. Resolving
/// twice could answer the presence question twice, and the two answers could
/// differ.
pub struct Parsed<F> {
    /// Every field resolved, so `create_record` has no presence to handle.
    pub form: F,
    /// The same values as write intent. `create_record` ignores this half.
    pub patch: Patch<F>,
}

/// `RecordForm` plus the per-field write intent. One per form, generated.
pub struct Patch<F> { /* one Field<T> per field of F */ }

impl<F: RecordForm> Patch<F> {
    /// Generated per field, on `&self` so a record fn can read a resolved
    /// value before handing the patch to `into_update`. `Keep` returns `keep`,
    /// `Set` returns the resolved value.
    pub fn author_id(&self, keep: uuid::Uuid) -> uuid::Uuid;
    pub fn cover_id(&self, keep: Option<uuid::Uuid>) -> Option<uuid::Uuid>;
    pub fn seo_title(&self, keep: String) -> String;

    /// `None` when no field is `Set` (rule 6).
    pub fn into_update(
        self,
        record: &mut F::Model,
    ) -> Option<<F::Model as toasty::schema::Model>::Update<'_>>;
}

impl<F: RecordForm> F {
    /// The framework's only entry point from a submission to presence. `cx`
    /// resolves embedded leaf keys against the compiled app schema; the absent
    /// walk needs no context.
    pub fn from_submission(
        cx: &Cx,
        schema: &Schema,
        values: &HashMap<String, String>,
    ) -> Result<Parsed<Self>, Vec<FieldError>>;

    /// The write `create!` would spell by hand. Every field is `Set`. The app
    /// names the columns the form does not own on the returned builder.
    pub fn into_create(self) -> <F::Model as toasty::schema::Model>::Create;
}

pub trait FormResource: Resource {
    type Form: RecordForm<Model = Self::Model>;
    // … form, validate_record, create_record, update_record
}
```

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
impl FormResource for AuthorResource {
    type Model = Author;
    type Form = AuthorForm;

    fn form(cx: &Cx) -> Schema { /* hand-written, unchanged */ }

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
            // `Resource::requires_tenant` makes the handler answer 403 before
            // this runs, so `require_tenant` never panics on a tenantless
            // submit.
            q.set_tenant_id(require_tenant(&cx)?);
            q.exec(&mut *ex).await.map_err(|e| -> topcoat::Error { e.into() })
        }
    }

    async fn update_record(
        _cx: &Cx,
        mut record: Author,
        patch: Patch<AuthorForm>,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Author> {
        // Read resolved values first: `into_update` borrows the record for the
        // builder's life, and the accessors take `&self`.
        let name = patch.name(record.name.clone());
        let email = patch.email(record.email.clone());

        if let Some(mut q) = patch.into_update(&mut record) {
            q.set_name(name);
            q.set_email(email);
            // `exec` reloads `record` in place, so the returned row is the
            // written one. `after_commit` subscribers depend on that.
            q.exec(&mut *ex).await.map_err(|e| -> topcoat::Error { e.into() })?;
        }
        Ok(record)
    }
}
```

`Resource::hydrate_form_values` stays on `Resource`: the detail page reads it
too (`panel/detail.rs:46`) and ADR-0016 rests on the page and the form agreeing
about what a field holds. `FormResource` gives it a default delegating to
`<Self::Form as RecordForm>::hydrate`.

## Rules

1. **Presence is two types.** A parse produces `Submitted<T>`; a write consumes
   `Field<T>`. A parse resolves each `Submitted` into a `Field` or a
   `FieldError`, so a write never sees `Blank`. One enum cannot do this: `Blank`
   carries no `T`, so a setter has no value to take and an accessor has none to
   return.

2. **A blank resolves to the field's blank answer, or refuses.** `None` for an
   `Option<T>` column; the `#[record_form(blank(..))]` literal where the app
   declares one; otherwise a `FieldError`, rendered inline with a 200. One rule
   covers create and update, and it is what lets `create_record` take a plain
   `Self::Form` — the form exists because every field resolved. There is no
   `T::default()` answer: `Uuid::default()` is `Uuid::nil()`.

   A blank reaches a record fn in two designed cases, and the rule answers both
   identically: `walk_absent_groups` suppresses requiredness inside an absent
   `Repeater` and a hidden variant group (`schema/tree.rs:191`,
   `schema/mod.rs:315`), and an embedded leaf binds `nullable: true` by policy
   (`schema/lenses.rs:37`).

3. **An empty posted control clears; an unposted control keeps.** A browser
   posts `""` for a control the user emptied and the stored value for one they
   did not touch, so `Blank` means clear. This changes six showcase fields:
   `kept_one_of` / `kept_bool` (`app.rs:68-88`) keep the stored value on an
   empty submit for `role`, `active`, `status`, and `featured`, and an
   all-empty `tags` `Repeater` is classified absent, so it keeps too. All six
   start clearing.

   The rule rests on a contract the framework must hold and test: **every
   rendered control posts its key, empty when cleared.** Every field kind emits
   a `name` (`schema/fields/{text_input,select,textarea,file_upload}.rs`),
   `variant.js` hides a group with CSS so its inputs still post, and
   `FileUpload` satisfies it twice over: `clear_<field>` posts an empty file
   part (`tests/uploads.rs:700`) and `store_uploads` writes the uploader's
   answer (`src/upload.rs:198`). A `Checkbox` field kind must emit a hidden
   companion input, or clearing it by not posting would read as `Keep`.

4. **A field binds by ident; a column resolves at run time.** The derive emits
   `M::fields().<name>()`, and `M::fields()` has one method per model field
   ident (`toasty-macros/src/model/expand/fields.rs:51`), so a model rename is a
   rustc error. Two things stay runtime, both invisible to the derive: an
   embedded leaf's key is its **flattened storage column**, not the ident
   (`Seo { title, description }` becomes `seo_title` / `seo_description`,
   `models.rs:60`) — `leaf_key` (`schema/embedded.rs:110`) reads
   `Db::schema().mapping` through `request_schema` (`schema/lenses.rs:57`),
   while a top-level leaf takes its name from the field itself
   (`schema/lenses.rs:142`); and `#[column(name = "…")]` renames a physical
   column with no ident change
   (`toasty-macros/src/model/schema/field.rs:159`). So the ident half is a
   compile error and the key half is the `claimed_keys` cross-check, the only
   check that sees an embedded leaf or a renamed column.

5. **The write is generated, for update and for create.** `toasty::update!` and
   `toasty::create!` expand to one builder call per *named* field with no
   completeness check, and a `fields()` cross-check against
   `Schema::field_names()` cannot catch an omission because both halves contain
   the field. So `Patch::into_update` calls `set_<field>` for exactly the `Set`
   fields and `Form::into_create` for every field, both through the generated
   `set_<field>(&mut self, v: impl Assign<T>) -> &mut Self`
   (`toasty-macros/src/model/expand/update.rs:70`) — which is why the embedded
   case below can pass a `stmt::apply(..)` where a scalar passes the value.

   The two builders stay asymmetric: the create builder's `exec` resolves to the
   model and the update builder's to `()`, because an instance update reloads
   the record in place through `UpdateTarget::apply_result`. So
   `create_record` returns the builder's value and `update_record` returns the
   record it was handed. The instance builder is also the target over a
   hand-built `stmt::Update`: `record.update()` keeps `apply_update_defaults()`
   and the `#[version]` compare-and-set condition.

6. **`into_update` returns `Option`.** A submission that names no form field
   would produce a zero-assignment `stmt::Update`, and toasty asserts on it:

   ```rust
   // toasty @ 6a1f5d9, crates/toasty/src/engine/verify.rs:283
   assert!(!i.assignments.is_empty(), "stmt = {i:#?}");
   ```

   A plain `assert!`, reached unconditionally from `Engine::exec`, so it fires in
   release. `into_update` returns `None` when no field is `Set` and the app
   skips `exec`; `Some` implies at least one assignment, so an empty statement
   is unreachable from app code.

7. **One validation hook, keyed by a field enum.**
   `validate_record(&Cx, &Self::Form) -> HashMap<AuthorFormField, Vec<String>>`.
   The 200-versus-500 split is load-bearing: field errors render inline with a
   200 (`submit.rs:111`) and a record-fn error maps to `hook_failure`
   (`submit.rs:158`). The key type is required because a typo'd key is a
   silently dropped rule — `errors.entry(field).or_default().extend(..)` never
   notices a key no control produced, and the derive already owns the
   ident-to-key map.

8. **A parse failure is a `FieldError` with a kind.** A kind, not a bare
   `String`: the framework owns the sentence (`schema/validation.rs`) and an app
   maps a kind to its own.

9. **Presence is classified once, by the framework.** One framework function
   produces the absent set, and `validate`, `check_unique`, and the parse step
   all read it, so they cannot disagree.

   `Schema::absent_fields` stays `pub(crate)` and is not that function. It
   answers a group question — the inner field names of an all-empty `Repeater`
   and of a variant group the discriminant does not name
   (`schema/mod.rs:357`, `schema/tree.rs:197`) — and its consumers stay the
   requiredness skip, the unique probe (`panel/forms/unique.rs:43`), and the
   relationship-existence skip (`schema/mod.rs:374`). It cannot distinguish a
   field the submission did not name from one it named empty. Key presence is
   the separate question `values.contains_key` answers, and the framework
   answers it for the derive.

10. **`FormResource` is a second trait; the derive is a convenience.** 39 of the
    121 workspace `impl Resource` sites declare `fn form`; the rest are mostly
    `tablo-core`'s own list and test resources. A `type Form` on `Resource` with
    no default puts a placeholder in all 121, permanently, for a guarantee four
    showcase resources need, and an associated type cannot carry a default on
    stable (E0658). So form-bearing resources implement both traits,
    list-only ones implement `Resource`, and `Panel::build` gates the create and
    edit routes on `R: FormResource`.

    A hand-written `impl RecordForm` must stay possible: it is how the
    framework's own tests exercise the write without a macro.

## Derive output

`#[derive(RecordForm)] #[record_form(model = Post, blank(..))]`:

- `hydrate` (model → flat key/value map) and `from_submission`, its inverse,
  both keyed through `leaf_key`. For an embedded value the derive calls the
  existing `EmbeddedForm::write_form` / `read_form` rather than
  reimplementing them.
- `from_submission`: one `Submitted` per field from the key's presence, then
  the value, then one `Field` — in one pass over the keys. A `String` field
  stays the untyped identity path (`Schema::normalize_values`,
  `schema/validation.rs:246`); a typed field parses through `TypedValue`
  (`schema/validation.rs:22`) **after** `normalize_values`, so the parse sees
  the spelling the write stores. `Submitted` is public because the tests assert
  its three states, and the derive is its only constructor.
- `into_update` (one `set_<field>` per `Set` field) and `into_create` (one per
  field).
- `validate_record`'s field enum, one variant per field.
- One per-field accessor on `&self`.
- `claimed_keys`, and the `check_resource_inner` cross-check in the existing
  slot where `R::form(cx)` is already bound (`panel/build.rs:411`). A function,
  not a `const`: an embedded leaf's key exists only once the app schema is
  compiled (`schema/lenses.rs:57`).

### Embedded values are per leaf; an enum is the exception

`EmbeddedForm::read_form` calls `parse_leaf` for every leaf unconditionally
(`tablo-macros/src/embedded.rs:405`), and `parse_leaf` collapses absent-and-empty
to `T::default()` (`schema/embedded.rs:341`). A submission naming `seo_title`
but not `seo_description` would store `""` into a `NOT NULL` flattened column.
The `EmbeddedForm` derive already emits the per-leaf presence as
`leaf_present` (`tablo-macros/src/embedded.rs:314`); the derive reuses it.

An embedded struct declares its leaves flat, and `into_update` groups the
present ones into one assignment:

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
    // `Seo`'s leaves, flattened.
    seo_title: String,
    seo_description: String,
    // `Publication`'s discriminant decides the variant, so the enum stays one
    // field whose presence is the discriminant.
    publication: Publication,
    author_id: uuid::Uuid,
}
```

```rust
// The shape toasty's own `update!` emits for an embedded partial
// (`toasty-macros/src/update/expand.rs:140`), and which the instance builder
// accepts: `__macro_fields_root` is a `#[doc(hidden)] pub` method on that
// builder (`toasty-macros/src/model/expand/update.rs:179`) and
// `impl<T> Assign<T> for Assignment<T>` exists
// (`toasty/src/stmt/assignment.rs:95`).
let fields = q.__macro_fields_root();
q.set_seo(toasty::stmt::apply([
    toasty::stmt::patch(fields.seo().title(), patch.seo_title.as_set()?),
    toasty::stmt::patch(fields.seo().description(), patch.seo_description.as_set()?),
]));
```

An **embedded enum is one field, not a set of leaves**: naming the discriminant
replaces the whole value and the variant owns its columns, so the derive gives
the field whole-value `Keep | Set(Publication)` and keeps `Publication`'s
`read_form` variant rule (`schema/embedded.rs:89`) — the variant from the
discriminant, an undeclared discriminant refused, a missing one falling back by
which payload was submitted. Its `ClaimedKeys` carries `may_exceed: true`: the
schema renders one variant's controls, and the claim is the union across all of
them.

### Refusals

The derive refuses, by type error, a form field that cannot be written:

- A `Deferred<_>` field (`Post.author` has no `set_author`; a form declares the
  FK, `author_id`).
- A field with no `set_` setter: `expand_setter_target` returns `None` for a
  `via` relation and every other non-primitive
  (`toasty-macros/src/model/expand.rs:394`).
- A generic or lifetime-carrying form. `RecordForm` cannot name a `Model` per
  instantiation; `tablo-macros/src/embedded.rs:95` is the `unsupported()` helper
  that reports this shape of refusal.
- A field the model does not have, through `M::fields().<name>()`.
- A `#[document]` column: one column and N fields, and neither `Submitted` nor
  `Field` has a representation for it, so it refuses through the same
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
  (`prepare_submission`, `submit.rs:54`; ADR-0017). The backfill is what makes
  an untouched file input a `Value` rather than a `Blank`; it stays.
- **`check_unique` and relationship existence** (`panel/forms/unique.rs:43`,
  `schema/mod.rs:374`).
- **Tenant injection and the FK error classes**: 500 for a missing author
  (`app.rs:961-972`), 404 for a cross-tenant parent (`app.rs:1113`). These are
  `set_*` calls on the builder `into_update` returns.
- **`EmbeddedForm` and its derive.** The flatten rule calls `write_form` /
  `read_form`; it does not replace them.
- **A model column neither the form nor the app names.** `toasty` offers no way
  to enumerate a model's fields — `__macro_fields_root` returns a struct of
  accessor methods, not a list — so this stays a runtime driver error. ADR-0022
  records it under `## Consequences`.

## Tests

- Behavioural tests beside `crates/tablo-core/tests/embedded_value.rs`, driven
  from the single `[[test]]` target (`crates/tablo-core/Cargo.toml:13`). A
  macro unit test carries no consumer manifest, so `proc_macro_crate` cannot
  resolve `tablo-core` and can only assert refusals — beside the existing ones in
  the `mod tests` at `crates/tablo-macros/src/embedded.rs:805`.
- **The empty-assignment regression.** A submission naming no field produces no
  `stmt::Update` and answers 303.
- **`into_update`'s borrow and return shape.** `exec(mut self, ..)` consumes the
  builder (`toasty-macros/src/model/expand/update.rs:195`) and reloads the
  record in place, so `Ok(record)` hands back the written row. A record fn
  returning a pre-write snapshot notifies `after_commit` subscribers with stale
  values.
- **Three states per field type**, and each per-field accessor on `Keep`, on
  `Set(Some)`, and on `Set(None)`.
- **A blank with no answer refuses**, inline with a 200: `age: i64` on create
  and on update.
- **The posting invariant.** One test per field kind asserting the rendered
  control carries a `name`, plus a hidden variant group whose inputs still post.
- **`Blank` clears.** `role`, `active`, `status`, `featured`, and clearing
  `tags` on edit. `posts_create_with_empty_optional_tags_group_submits`
  (`file_repeater_check.rs:145`) covers create only.
- **The panel-build check**, four refusals: a form key the schema does not
  declare; a `Schema::field_name()` no field claims; an embedded enum whose
  union does not cover the active variant; a relation-named form field.
- **Embedded struct, both directions.** Naming one leaf leaves the other `Keep`,
  and a blank leaf with no answer refuses. Whole-value presence expresses
  neither.
- **Embedded enum.** A variant switch across published → scheduled → published,
  asserting the un-chosen variant's payload is not read as absent.
- **A parse after `normalize_values`**, following
  `a_valid_submission_is_stored_in_the_types_spelling`
  (`tests/typed_leaves.rs:99`), which asserts the re-read fixpoint.
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
- `update_record_keeps_absent_fields`
  (`examples/showcase/tests/edit_check.rs:155`),
  `clearing_an_optional_upload_empties_the_stored_path` and
  `a_refused_edit_upload_keeps_showing_the_stored_file`
  (`tests/uploads.rs:691`, `:765`) keep passing.

## Scope

Two steps. Splitting the typed form further yields types with no consumer, and
the signature change breaks every caller at once.

### 1. Key agreement at panel build

- `Resource::form_field_names(&Cx) -> Vec<String>`, defaulting to `Vec::new()`,
  documented as "unclaimed", with a `tracing::warn!` when a resource that
  declares a form leaves it unclaimed — a silently inert check is the failure
  mode to avoid.
- In `check_resource_inner`, after `let form = R::form(cx);`
  (`panel/build.rs:411`): require every `Schema::field_name()` to appear in
  `form_field_names()`, and every entry to be a `Schema::field_name()`.
  Bidirectional, and set equality: the data-loss direction (a control the
  resource does not bind) is reported first, and a subset check is the half
  `assert_hydrate_keys_are_form_fields` already covers.
- All four showcase resources declare their keys. The lists already exist in
  their `hydrate_form_values`; this copies them.
- `assert_hydrate_keys_are_form_fields` gains `CommentResource` and the reverse
  direction.
- One paragraph in `docs/guide/src/resources.md` replacing the present-keys
  rule, which sits only on `schema/mod.rs:308-313`.
- Step 2 removes `form_field_names` and reads
  `<R::Form as RecordForm>::claimed_keys` in the same slot.

### 2. The typed form

One step, because each half is unusable alone. Two tests are written first and
their answers gate the API:

- **The empty-assignment test.** A submission naming no field produces no
  statement. Confirms rule 6's `Option`.
- **The borrow and return test.** `into_update` returns
  `Option<<M as Model>::Update<'_>>`, `exec` consumes it, and `Ok(record)` hands
  back the written row.

Then, in this order:

- `Submitted`, `Field`, `FieldError`, `FieldErrorKind`, `Parsed`,
  `ClaimedKeys`, and `RecordForm` in `tablo-core`.
- The `FormResource: Resource` split, and the create/edit routes gated on
  `R: FormResource` in `Panel::build`.
- The submit pipeline. `validate_record` needs a `Self::Form`, so the parse
  moves ahead of it. Today: `prepare_submission` — `reject_unknown_form_keys`
  (`submit.rs:60`) → `validate_async` → `R::validate` (`submit.rs:111`) →
  `check_unique` (`submit.rs:192`, `:263`) → re-render on error →
  `normalize_values` (`submit.rs:210`, `:281`) → the record fn (`:215`, `:282`).
  New: `validate_async` → `check_unique` → re-render on error →
  `normalize_values` → `from_submission` → `validate_record` → re-render on
  error → the record fn, which takes `parsed.form` on create and `parsed.patch`
  on edit. A second re-render step, and the unique probe sees normalized
  values.
- The `RecordForm` derive in `tablo-macros`, next to `EmbeddedForm`.
- The four showcase resources: `Author`, `Comment`, `User`, `Post`. `Post` goes
  third, after `User` has proven a typed optional field and a `blank(..)`: it
  carries an embedded struct, an embedded enum, a `Repeater`, two relationship
  `Select`s, tenant injection, and both FK error classes.
- `benchmarks/tablo` (`src/main.rs`) has 2 `impl Resource` sites in a detached
  workspace that gate set steps 6 and 7 build; both implement `Resource` only.
- The tests above.

Surfaces the retyping touches, measured on the workspace: 121 `impl Resource`
sites in `crates/` and `examples/`, plus 2 in `benchmarks/tablo`; 21
`create_record` overrides, 10 `update_record`, 9 `hydrate_form_values`, and 1
`Resource::validate`, each excluding the `Resource` trait's own default.

## Docs

- Amend ADR-0001 (`:17-18`), which records that form values stay string-keyed
  because "the lens proves field existence, not typed data flow". The ident half
  is compile-time; the key half is a panel-build check. Keep the trailing
  reference to upstream issues #115 and #119 — #119 is not closed by this
  change, because an embedded leaf's column name is a runtime key.
- ADR-0022 for rules 1–10, with a `## Consequences` section, in ADR-0019's
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
  `:62-74`, which the submit-pipeline reorder changes.
