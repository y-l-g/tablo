# Record forms: a derived typed value, completed from the stored record

Date: 2026-09-28 — Status: accepted

## Decision

**1. A form writes through one derived struct.** A form resource declares `#[derive(RecordForm)]`
with `#[form(model = User)]`: one field per written model column, named and typed like the model,
so a renamed or retyped column rejects at compile time. Keys resolve at run time. A scalar
(`String`, a `TypedValue` type, or an `Option` of one) binds one key; `#[form(embed)]` binds every
key of an `EmbeddedForm` value and writes it whole. A `bool` is a toggle reading empty as
`false`; `#[form(options = T)]` is a choice over `T`'s `Options` list, `#[form(choice)]` a bare
choice, `#[form(file)]` a file field, any other field a text field. The derive emits
`controls(dx)` plus the default `schema` arranging one per field; `Resource::form` defaults to
that schema and an override arranges the controls into a layout. Each constructor returns its
control's builder, so a modifier on the wrong control does not compile.

**2. The form lives on `Resource`, registered once.** `type Form`, `CREATE_COLUMNS`, `form`,
`validate_record`, `create_record`, and `update_record` live on `Resource` with `Panel::resource`.
A list-only resource names `type Form = NoForm<Self::Model>`; `RecordForm::HAS_FORM` decides
whether create, edit, and options routes exist and whether the list links to create.

**3. An edit completes from the stored record.** Every declared key the submission omits fills
from `RecordForm::hydrate` of the stored record, so `update_record` receives a whole form. Named
keys are the posted ones minus an untouched file input; the write assigns only named fields plus
model `#[update(..)]` defaults and `#[version]`. An unposted key keeps its value.

**4. A blank resolves to the field's blank answer.** A posted-empty control stores
`#[form(blank = ..)]`, else `""` for `String` or `None` for `Option<T>`; with no answer the parse
refuses the key inline. No `T::default()` answer exists for scalars. An emptied embedded leaf
takes the same rule; a leaf of a hidden variant group is not read.

**5. Record fns default to the derived write.** `write_create` stamps a gated resource's tenant
column and executes the create builder; `write_update` executes `Posted::into_update`, returning
`None` when no field is named. An override checking inside the transaction delegates to them.

**6. Mounting checks struct against schema.** Every control binds exactly one field and every
field owns a control; an optional or `Repeater` control binds a field answering blank; a gated
form omits its tenant column; a form resource overrides `form()`; `NoForm` declares no schema
and allows no create or edit. A skippable control is exempt: an embedded enum discriminant and a
hidden variant payload. A payload in a `Repeater` is asked like any control.

**7. A create sets every non-nullable column.** Where policy allows `Create`, each non-nullable,
non-relation column is a form field, a Toasty fill, the tenant stamp, or a `CREATE_COLUMNS` entry
for an override. The check reads defaults off `M::Create::default()`.

**8. One round of errors.** Schema rules, unique probe, parse, and `validate_record` merge into one
`FieldErrors` keyed list rendered inline with a 200. `validate_record` runs once every field
parses and names each error's key; a key the submission renders nowhere fails the submit as a
declaration error. A rejected upload replaces errors under its field key.
