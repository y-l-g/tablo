# Forms

A resource with create and edit pages declares two things: a **record form**, the typed struct a
submission parses into, and a **schema** in `form()`, the controls the page renders. `form`
defaults to the record form's derived schema, so a resource that wants one control per field in
declaration order declares no `form` at all. Mounting the panel calls the declarations once and checks
that the two agree.

## The record form

```rust
{{#include ../../../examples/guide/src/models.rs:role-options}}

{{#include ../../../examples/guide/src/resources.rs:user-record-form}}
```

Each field names a model field and has that field's type, so renaming or retyping a column breaks
the build. A field is either a **scalar** — `String`, a [typed value](#typed-values), or an
`Option` of one — bound to the one key its control posts, or an [embedded value](#embedded-values)
marked `#[form(embed)]`.

Leave out the columns the form does not write: the tenant column of a tenant-owned resource, which
the framework sets on create, and columns with a Toasty `#[default(..)]` or `#[auto]`.

**Blank values.** `#[form(blank = <expr>)]` is what a field stores when its control is submitted
empty. `String` answers `""` and `Option<T>` answers `None` through the type's own blank, and a
`bool` answers `false` through the derive's default, since an unchecked toggle posts `false`. Any
other type needs `blank` when its control is optional.

The resource names the struct as its `Form` and, to arrange the controls, declares them from the
derive's `controls()`:

```rust
impl Resource for UserResource {
    type Model = User;
    type Form = UserForm;

{{#include ../../../examples/guide/src/resources.rs:user-form}}
    // table(), policy …
}
```

Mounting the panel refuses the resource unless every control is bound by exactly one form field,
every form field has a control, every optional control's field has a blank value, and — when
the policy allows `Create` — every non-nullable column is filled by the form, by Toasty, by the tenant
stamp, or by an overridden `create_record` that lists it in `Resource::CREATE_COLUMNS`.

## Controls

The derive picks each field's control from the field: a `bool` is a toggle, `#[form(options = T)]`
a choice over `T`'s options, `#[form(choice)]` a bare choice, `#[form(file)]` a file field,
`#[form(embed)]` the embedded value's schema, and any other field a text field. `controls()`
hands each one over ready for its modifiers, so an override arranges rather than rebinds:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-controls-layout}}
```

The `role` control already offers `Role`'s options: `#[form(options = Role)]` chose a choice over
that list. A closed set of values is a `#[derive(Options)]` enum, shared by the form, the filter
and the column:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-role-options}}
```

Each variant stores its `snake_case` name and reads as that name in sentence case;
`#[option(value = "..", label = "..")]` overrides either. The derive also gives the enum
`value()`, `label()`, `from_value()` and, through the `Options` trait, `label_of()`. `.options`
takes `Vec<(String, String)>` (an `Options` enum's list), `Vec<String>`, or `[&str; N]` (`["admin",
"member"]`).

**Layout blocks** arrange fields: `Section::new(title)` is a titled card, `Group::new()` an untitled
container, and `Grid::new(cols)` a grid of 1 to 12 columns. `Repeater::new(label)` is a titled group
that may be left entirely empty: its fields' rules apply once one of them is filled, and
`.required()` refuses an all-empty group. A schema or block takes a tuple of at most eight children;
nest a `Group` for more.

**Fields** are built from a Toasty field lens:

| Constructor | Column | Control | Builder |
| --- | --- | --- | --- |
| `Field::text(lens)` | `String`, a typed value, or an `Option` of one | `<input>`, or `<textarea>` with `.multiline(rows)` | `TextField` |
| `Field::choice(lens)` | any | `<select>` over static options or a relationship | `ChoiceField` |
| `Field::file(lens)` | `String` holding the file's path | file input: see [File uploads](#file-uploads) | `FileField` |
| `Field::toggle(lens)` | `bool` | checkbox | `CustomField` |
| `Field::custom(lens, control)` | `String`, a typed value, or an `Option` of one | your own `Control`: see [Custom controls](#custom-controls) | `CustomField` |

Every field takes `.label(..)`, `.required()` and `.optional()`. The label defaults to the column
name in sentence case, and `required` defaults to whether the column is non-nullable. Each
constructor returns its control's builder, which offers only that control's modifiers, so a
modifier on the wrong control does not compile:

- text (`TextField`): `.email()`, `.unique()`, `.placeholder(..)`, `.multiline(rows)`;
- choice (`ChoiceField`): `.options(..)`, `.relationship(..)`, `.searchable()`.

### Custom controls

A `Control` renders the input of a `Field::custom` field. The field keeps everything fields share —
the key, the label, the required rule, the type's parse rule, the error slot and the chrome around
the input — and the control renders only the input, from a `ControlInput` carrying the key, the
current value and the validation state:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-color-control}}
```

`display` renders the stored value on the detail page, as text by default. The submission is read
like any other field's: the value posted under the field's key, the last one when it is posted
twice. `Field::toggle` is built this way: `Toggle` renders a hidden `false` before the checkbox
under the same name, so an unchecked box submits `false` rather than nothing. A toggle is
optional, since it is never empty, and a `bool` record-form field reads an empty submission as
`false` without a declared blank.

### Typed values

`Field::text` binds more than strings. Over an integer, float, `bool`, `Uuid` or
`jiff::Timestamp` column it renders the stored value and parses the submission back through the
type. A value the type refuses is an inline error naming it: `` `twelve` is not a valid whole
number ``. A `jiff::Timestamp` renders a `datetime-local` input, which carries no time zone, so
values display and parse as UTC.

Implement `TypedValue` to bind your own type: `NOUN` names it in errors, `INPUT_TYPE` sets the
input's `type`, and `parse_input` reads a submission (by default through `FromStr`).

### Email and uniqueness

`.email()` renders `type="email"` and refuses what is not an address: a domain needs two labels
(`a@b` is refused), a display name (`Ada <ada@example.com>`) is refused, and the address is capped
at 254 bytes.

A text field over a column with a unique index checks uniqueness before the write, and reports a
taken value inline. `.unique()` states it explicitly; mounting the panel refuses `.unique()` on a
column without a unique index, single-column or composite. The check is a query before the write,
so two concurrent submissions can both pass it; the database index stays the final guard.

On a non-nullable column, uniqueness also makes the field required, even with `.optional()`:
an empty `String` is stored as `""`, and the index admits only one. An `Option` column stores
`NULL` for an empty value, so a unique `Option` field may stay optional.

### Relationships

A choice over a foreign key loads its options from the related resource:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-relationship-field}}
```

Each option's value is the related record's primary key.

- Options come from the related resource's tenant-scoped query and follow its policy: the list is
  empty and the field shows "not available" unless the related resource's policy allows
  `ViewAny`, and each record must pass its `View`.
- A submitted key must be one of those records, so a hand-crafted POST cannot point at another
  tenant's row. The write checks the key again inside its transaction, so a record deleted, moved
  to another tenant or hidden since the form validated refuses the write with the same field
  error.
- At most 200 options load. Past that, a `.searchable()` choice turns into type-to-search against
  `GET {list_url}/options`, which searches the related table's `searchable()` columns; a
  non-searchable choice shows an error instead. `.searchable()` also filters a short list as
  you type. Without JavaScript the plain select remains.

## Submitting

- **Validation runs in one round.** Required fields, email, relationships, uniqueness and typed
  parsing are checked together, and the form re-renders with every error inline, with status 200
  and nothing written. `validate_record` needs the parsed form, so it runs only when every field
  parses. Each error names the key it renders under: the control's key, an embedded value's
  flattened key, or a repeater's label. An error keyed to something the form does not render
  fails the request instead of being dropped.
- **Unknown keys are refused.** A POST carrying a key the form does not declare answers 400, so a
  client cannot write `role` or `tenant_id`. The CSRF token and the file fields' `clear_` and
  `keep_` keys are the exceptions.
- **An edit writes only what was posted.** On an edit, a declared key missing from the submission
  keeps its stored value, and the update assigns only the fields the submission named, plus the
  model's own `#[update(..)]` defaults and `#[version]` bump. An API client can post a single
  field. A control submitted empty is posted, and stores its field's blank value.

To write more than the form holds, override `create_record` or `update_record`
([Resources](./resources.md#writes)) and use the builders: `form.into_create()` is the model's
create builder with every form field set, and `posted.into_update(&mut record)` is the update
builder with one assignment per posted field, or `None` when nothing was posted.

## File uploads

`Field::file` binds a `String` column that stores the file's path or URL, never its bytes. A form
with a file field is sent as `multipart/form-data`, with a 10 MiB body limit (larger answers 413).
Filenames are reduced to a safe basename before anything sees them.

Where the bytes go is your app's decision. Install an `Uploader` on the panel:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-uploader}}
```

- `store` returns the value to save. An `Err(reason)` is shown to the user inline as
  `<Label> could not be uploaded: <reason>`, so keep the reason free of paths and driver messages.
  Without an uploader the panel stores the sanitized filename and discards the bytes.
- **Keeping a file across a failed submit.** When a form re-renders with errors, the file input is
  empty again. The form carries the path `store` just returned, and the panel reuses it on the next
  submit only when `Uploader::holds(path)` answers `true`. The default answers `false`, so the user
  uploads again. Answer `true` only for a path your store produced and still holds inside its own
  root; checking whether an arbitrary path exists would let a client pick any file.
- **Editing.** The edit form shows the stored file as a link and an empty file input. Leaving it
  empty keeps the file; the "Remove the current file" checkbox (`clear_<field>`) empties the
  field. The input is required only while nothing is stored, and clearing a required field fails
  validation, so declare `.optional()` when a record may lose its file.
- **Links.** The stored value renders as a link, on the edit form and the detail page, only when
  it is a root-relative path (`/uploads/a.png`, not `//host`) or an `http(s)` URL; anything else
  renders as text.
- **Serving.** `Panel::serve_dir(path, dir)` serves a directory, and the served files are
  **public**: the auth gate does not cover them. An app that needs protected files serves them
  from its own route. See [Security](./security.md) for the headers served files carry.

## Embedded values

A Toasty `#[derive(Embed)]` struct or enum is stored in its parent's row as flattened columns
(`seo_title`, `seo_description`). Derive `EmbeddedForm` on it and the form binds the whole value:

```rust
{{#include ../../../examples/guide/src/models.rs:seo-struct}}

// In `form(dx)`: one call renders a control per field.
{{#include ../../../examples/guide/src/forms.rs:forms-embedded-schema}}

// In the record form: the value is one field.
{{#include ../../../examples/guide/src/forms.rs:forms-embedded-record-form}}
```

- Every field of the value is a scalar, or a nested value marked `#[form(embed)]`.
  `#[form(label = "…")]`, `#[form(multiline = N)]` and `#[form(blank = ..)]` customize a field;
  an unknown attribute is a compile error.
- Embedded fields are optional by default. An emptied field stores its blank value, and a field
  with no blank value refuses an empty submission inline.
- **Enums** render a choice of variant plus one group of fields per variant; the page shows only
  the chosen variant's group, and the variant can change on edit. The submission's variant decides
  which fields are read and validated, so a stale value in a hidden group never blocks a submit.
  Without JavaScript every group renders, and the variant choice still decides.
- Not supported inside a value: a `#[document]` field, a relation, an enum nested inside an enum
  variant, and tuple structs.

To bind a single embedded field on its own, pass its path: `Field::text(Post::fields().seo().title())`
binds the flattened `seo_title` column, which the panel resolves through the database schema when
it mounts. Such a field is optional unless you call `.required()`. A schema built outside a panel,
such as one a custom page renders, resolves the same way inside `tablo::declare(&db, || ..)`.
