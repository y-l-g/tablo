# Record forms: a derived typed value, completed from the stored record

Date: 2026-09-28 — Status: accepted — Amended: 2026-09-29, 2026-09-30

## Decision

**1. A form writes through one derived struct.** A resource with a create or edit form declares
`#[derive(RecordForm)]` over a struct with one field per model column the form writes, named and
typed like the model's field (GH #369). The derive binds each field through
`M::fields().<ident>()` and asserts the field's type against the model's, so a renamed or retyped
column is a compile error. Keys come from the framework at run time (`leaf_key`, `value_keys`),
because an embedded leaf's key is its flattened storage column. A scalar (`String`, a `TypedValue`
type, or an `Option` of one) binds one key; `#[form(embed)]` binds every key of an
`EmbeddedForm` value and writes it whole.

**2. The form lives on `Resource`, registered once (GH #382).** `type Form`, `CREATE_COLUMNS`,
`form`, `validate_record`, `create_record`, and `update_record` live on `Resource`, and every
resource registers with `Panel::resource`. A list-only resource names
`type Form = NoForm<Self::Model>;`: an associated type cannot carry a default on stable (E0658), and
the one line removes a misregistration a second trait allowed, a form resource registered without
its form routes. `RecordForm::HAS_FORM`, `false` on `NoForm`, decides at registration whether the
create, edit, and options routes exist and whether the list links to create.

**3. An edit completes the submission from the stored record.** Every declared key the submission
does not post is filled from `RecordForm::hydrate` of the stored record — the advisory load for
validation, the in-transaction load for the parse — so `update_record` receives a whole form. The
**named** keys are the ones posted, minus an untouched file input; a field with a named key is
named, and the write assigns only named fields, plus what the model's own `#[update(..)]` defaults
and `#[version]` column assign on every instance update. An unposted key therefore keeps its value,
and a concurrent write to it survives.

**4. A blank resolves to the field's blank answer.** An emptied control is posted, so it is named
and stores the field's blank answer: the `#[form(blank = ..)]` expression, else `""` for `String` or
`None` for `Option<T>`. With no answer the parse refuses the key inline. There is
no `T::default()` answer for a scalar: `Uuid::default()` is the nil UUID. An embedded value's leaves
take the same rule, the declared answer included (GH #371), so an emptied leaf of a type with no
answer is refused on its key rather than storing the type's `Default`.

**5. The record fns default to the derived write.** `write_create` stamps a gated resource's
tenant column (found by `tenant_field_index`) with `Insert::set` and executes the create builder;
`write_update` executes `Posted::into_update`, which returns `None` when no field is named because
toasty asserts on an update with no assignment. An override that checks something inside the
transaction delegates to them.

**6. `Panel::build` checks the struct against the schema.** Every control is bound by exactly one
field and every field's key is a declared control; an optional control, or one inside a `Repeater`,
binds a field that answers blank, an embedded value answering when each of its leaves does; a
gated resource's form does not claim its tenant column; a resource with a record form overrides
`form()`; and a `NoForm` resource declares no schema and allows neither create nor edit. A control a
submission can skip is exempt: an embedded enum's discriminant, whose empty answer reaches the read's
fallback (a submitted payload, else the first variant) rather than a leaf's rule, and a variant
group's payload, which `variant.js` hides. A payload inside a `Repeater` is asked like any other
control: an all-empty repeater group skips its requiredness while the parse still reads it.

**7. A create sets every non-nullable column.** Where `can_create` allows it, each non-nullable,
non-relation column must be a form field, filled by toasty, the stamped tenant column, or named in
`Resource::CREATE_COLUMNS` by an override that sets it. Toasty keeps `#[default(..)]` in
generated code only, not in the app schema, so the check reads the defaults off
`M::Create::default()`: a column its insert leaves `NULL` is one nothing fills.

**8. One round of errors.** Schema rules, the unique probe, the parse, and `validate_record` merge
into one error map and render inline with a 200. `validate_record` returns errors keyed by the
derive's field enum and runs once every field parses.

## Consequences

- The showcase's four resources declare no hydration and no presence helpers; `AuthorResource` and
  `UserResource` declare no record fns, and `PostResource` and `CommentResource` override them only
  to re-check the parent row inside the transaction.
- An edit that omits a required key keeps the stored value instead of failing validation, so an API
  client can post one field.
- Emptying an optional select stores the field's blank answer rather than the stored value.
- `Resource::hydrate_form_values` is `Resource::view_values`: the detail projection of a list-only
  resource, and the keys only the view shows for a form resource.
- Residuals: a `set_*` call an override adds to the builder `into_update` returns is unchecked, and
  `CREATE_COLUMNS` is the app's word that an override sets a column. The create-column check and
  the list-only refusal read `can_create` with a Db-only context, so a request-scoped predicate
  that denies there skips them; the list page still links to create only for a form resource.
- A form's key set is read once, at build: `form(cx)` must declare the same controls on every
  request.
- A `validate_record` error on a field the form binds to no key fails the submit closed.

## Amendment — 2026-09-30

**One attribute, one classifier (GH #392).** Both form derives read `#[form(..)]`: the struct names
its model with `#[form(model = User)]`, a field `#[form(embed)]` or `#[form(blank = ..)]`. A field is
embedded only when marked `#[form(embed)]`; every other field is a scalar whose `FormScalar` bound
is asserted spanned on the field, so a `Vec<String>` field fails there. A scalar's key is its
resolved lens's name, and an embedded value's keys are its schema node's.

**One keyed error map (GH #392).** Point 8's three vocabularies — the parse's `FieldError`, the
typed `FieldErrors<F>`, and the schema's `HashMap<String, Vec<String>>` — are one `FieldErrors`: a
keyed list of `FieldError`, each carrying its key, its kind, and its message. `validate_record`
returns it and names each error's key: a control's own name, or a repeater group's label. A key this
submission renders nowhere — a leaf of a variant group its discriminant hides (a submission naming
no discriminant hides nothing: the payload may name the variant) — fails the submit as a declaration
error, where point 8's last consequence left that leaf failing closed silently. A rejected upload
replaces the errors under its field's key, so its reason stands where "this field is required" would
restate the symptom.

**An embedded leaf takes the blank rule (GH #371).** Point 4's rule is the leaf's, wherever the leaf
sits: `EmbeddedForm::answers_blank` reports whether every leaf has an answer, and
`#[derive(EmbeddedForm)]` reads `#[form(blank = ..)]` on a leaf the way the record form reads it on a
scalar. The derive's parse refuses a blank leaf with no answer on the leaf's own key, and point 6's
check refuses the declaration where the control can be posted empty. ADR-0019's `Default` rule for a
blank leaf goes; an `Option<T>` leaf is the answer for a column that stores no value. A leaf of a
variant group the discriminant hides is not read at all, so it is asked for no answer; a discriminant
the submission posts empty is named, and the read's fallback is what answers it.
