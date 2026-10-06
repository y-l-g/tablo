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
`controls()` plus the default `schema` arranging one per field, and the default `table` listing
each field a column can show; a `ResourceDef` without a `form` or `table` uses them, and one
without a `view` shows the form. A def's own form or table arranges or extends them. Each constructor returns its
control's builder, so a modifier on the wrong control does not compile.

**2. The form lives on `Resource`, registered once.** `type Form`, `validate_record`,
`create_record`, and `update_record` live on `Resource`, and the form schema and `create_columns`
on its `ResourceDef`, registered with `Panel::resource`.
A list-only resource names `type Form = NoForm<Self::Model>`; `RecordForm::HAS_FORM` decides
whether create, edit, and options routes exist and whether the list links to create.

**3. An edit completes from the stored record.** Every declared key the submission omits fills
from `RecordForm::hydrate` of the stored record, so `update_record` receives a whole form. Named
keys are the posted ones minus an untouched file input; the write assigns only named fields plus
model `#[update(..)]` defaults and `#[version]`. An unposted key keeps its value.

**4. The record form owns presence.** A posted-empty control stores its field's blank answer:
`#[form(blank = ..)]`, `None` for an `Option`, `false` for a `bool`, `""` for a
`#[form(optional)]` `String`. A field with none is required: the parse refuses the key inline,
and the panel renders its control required whatever the layout declared, so no control-side
presence setting exists to disagree with the struct. An embedded leaf takes the same rule, set by
its own derive; a leaf of a hidden variant group is not read. A layout has no optional group: a
group of controls that may all be left empty is a set of fields with blank answers.

**5. Record fns default to the derived write.** `write_create` stamps a gated resource's tenant
column and executes the create builder; `write_update` executes `Posted::into_update`, returning
`None` when no field is named. An override checking inside the transaction delegates to them.

**6. Mounting checks the layout against the struct.** Every control posts a key of the record
form and every key has a control; a gated form omits its tenant column; `NoForm`, which has no
fields, allows no create or edit. A unique field over a non-nullable column has no blank answer,
since every empty submission would store the same value.

**7. A create sets every non-nullable column.** Where policy allows `Create`, each non-nullable,
non-relation column is a form field, a Toasty fill, the tenant stamp, or a `create_columns` entry
for an override. The check reads defaults off `M::Create::default()`.

**8. One parse, one round of errors.** The record form's parse is the only presence and type
check; the controls add only their own rules (an email address, a choice among its options).
Control rules, the unique probe, the parse, and `validate_record` merge into one keyed list
rendered inline with a 200. `validate_record` runs once every field parses and keys each error by
the record form's field enum, so every error renders under a control: an embedded value's under
its first. A rejected upload replaces errors under its field key. A hand-written record form
refusing a key its `fields` do not bind fails the submit as a declaration error.

## Rejected

- **Presence on the control** (`.required()`, `.optional()`): a second declaration of what the
  struct's blank answers already say, which mounting had to reconcile and the submit had to
  re-word.
- **A schema-side parse**: the struct parses every key anyway; a second parser only produced
  errors the parse then had to defer to.
