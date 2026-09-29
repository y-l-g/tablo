# Record forms: a derived typed value, completed from the stored record

Date: 2026-09-28 — Status: accepted — Amended: 2026-09-29

## Decision

**1. A form writes through one derived struct.** A resource with a create or edit form declares
`#[derive(RecordForm)]` over a struct with one field per model column the form writes, named and
typed like the model's field (GH #369). The derive binds each field through
`M::fields().<ident>()` and asserts the field's type against the model's, so a renamed or retyped
column is a compile error. Keys come from the framework at run time (`leaf_key`, `value_keys`),
because an embedded leaf's key is its flattened storage column. A scalar (`String`, a `TypedValue`
type, or an `Option` of one) binds one key; `#[record_form(embed)]` binds every key of an
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
and stores the field's blank answer: `""` for `String`, `None` for `Option<T>`, otherwise the
`#[record_form(blank = ..)]` expression. With no answer the parse refuses the key inline. There is
no `T::default()` answer for a scalar: `Uuid::default()` is the nil UUID. An embedded value keeps
ADR-0019's leaf rule, so an emptied leaf stores its type's `Default`.

**5. The record fns default to the derived write.** `write_create` stamps a gated resource's
tenant column (found by `tenant_field_index`) with `Insert::set` and executes the create builder;
`write_update` executes `Posted::into_update`, which returns `None` when no field is named because
toasty asserts on an update with no assignment. An override that checks something inside the
transaction delegates to them.

**6. `Panel::build` checks the struct against the schema.** Every control is bound by exactly one
field and every field's key is a declared control; an optional control, or one inside a `Repeater`
or a variant group (whose requiredness a submission can skip), binds a field that answers blank; a
gated resource's form does not claim its tenant column; a resource with a record form overrides
`form()`; and a `NoForm` resource declares no schema and allows neither create nor edit.

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
