# Forms

A resource with create and edit pages declares a **record form**, the typed struct a submission
parses into. The record form also gives each field its control, and `ResourceDef::form` arranges
them. A resource that wants one control per field, in declaration order, declares no form at all.

## The record form

```rust
{{#include ../../../examples/guide/src/models.rs:role-options}}

{{#include ../../../examples/guide/src/resources.rs:user-record-form}}
```

Each field names a model field and has that field's type, so renaming or retyping a column breaks
the build. A field is either a **scalar** — `String`, a [typed value](#typed-values), an
[`Options` enum](#controls), or an `Option` of one — bound to the one key its control posts, an
[embedded value](#embedded-values) marked `#[form(embed)]`, or a [repeater](#repeaters) marked
`#[form(repeat)]`.

Leave out the columns the form does not write: the tenant column of a tenant-owned resource, which
the framework sets on create, and columns with a Toasty `#[default(..)]` or `#[auto]`.

**Required and blank values.** The record form alone decides which fields are required. A field's
**blank answer** is what it stores when its control is submitted empty: `#[form(blank = <expr>)]`,
`None` for an `Option<T>`, `false` for a `bool` (an unchecked toggle posts `false`), and `""` for
a `String` marked `#[form(optional)]`. A field with no blank answer is required: the panel renders
its control required, and an empty submission is refused inline. `optional` applies to a `String`
only; another type declares `blank` or becomes an `Option`.

The resource names the struct as its `Form` and, to arrange the controls, places them from the
derive's `controls()` in a `Schema<UserForm>`:

```rust
impl Resource for UserResource {
    type Model = User;
    type Form = UserForm;

    fn declare() -> ResourceDef<Self> {
        let c = UserForm::controls();
        ResourceDef::new()
{{#include ../../../examples/guide/src/resources.rs:user-form}}
            // .table(..), .policy(..) …
    }

{{#include ../../../examples/guide/src/resources.rs:user-validate}}
}
```

A `Schema<UserForm>` takes only `UserForm`'s controls: a control of another form, or a field built
with `Field::text`, does not compile there. A control the schema does not place follows the ones
it does, in declaration order, so a form that adjusts one control places only that one:
`.form(Schema::new(c.email.email()))`. An unplaced control renders on its own after the schema's
last block, outside every section; a form arranged in sections places every control. Any control
takes an app's own input with `.custom(control)`: see [Custom controls](#custom-controls).

When the policy allows `Create`, mounting the panel refuses the resource unless every
non-nullable column is filled by the form, by Toasty, by the tenant stamp, or by an overridden
`create_record` whose def names it with `create_column`.

## Controls

The derive picks each field's control from the field: a `bool` is a toggle, `#[form(options)]` a
choice over the field type's options, `#[form(options = T)]` a choice over `T`'s options,
`#[form(relationship = R)]` a choice over `R`'s records (a multiple choice on a
[many-to-many field](#many-to-many-fields)), `#[form(file)]` a file field, `#[form(embed)]` the embedded
value's schema, `#[form(repeat)]` a repeater, and any other field a text field. `controls()`
hands each one over ready for its modifiers, so an override arranges rather than rebinds:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-controls-layout}}
```

The `role` control already offers `Role`'s options: `#[form(options)]` chose a choice over the
list of the field's type. A closed set of values is a unit enum deriving `toasty::Embed`, which
Toasty stores in one column, and `tablo::Options`, which gives the form, the filter and the column
one list of options:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-role-options}}
```

Each variant posts its `snake_case` name and reads as that name in sentence case;
`#[option(value = "..", label = "..")]` overrides either, and two variants sharing a value or a
label fail to compile. A column, the detail page and a group header read a variant's label, and a
query compares the variant itself: `User::fields().role().eq(Role::Admin)`. An `Option<Role>`
field works the same way, reading an empty submission as `None`. An enum field without
`#[form(options)]` gets a text field, which accepts only an option's value. An enum leaf of an
[embedded value](#embedded-values) is such a text field, and its column shows the value.

Toasty stores the variant itself, under its `snake_case` name unless `#[column(variant = "..")]`
renames it; the option's value only spells it in forms and URLs. Moving a `String` column to an
enum is therefore a schema change: rows whose text is not Toasty's name for a variant no longer
load.

The derive also gives the enum `value()`, `label()`, `from_value()` and, through the `Options`
trait, `label_of()`. A `String` field takes the same list with `#[form(options = T)]`. `.options` takes `Vec<(String, String)>` (an `Options` enum's list),
`Vec<String>`, or `[&str; N]` (`["admin", "member"]`).

**Layout blocks** arrange fields: `Section::new(title)` is a titled card, `Group::new()` an untitled
container, and `Grid::new(cols)` a grid of 1 to 12 columns. A schema or block takes a tuple of at
most twelve children; nest a `Group` for more.

**Fields** are what `controls()` hands over, and what a page's or an action's schema builds from
a Toasty field lens:

| Constructor | Column | Control | Builder |
| --- | --- | --- | --- |
| `Field::text(lens)` | `String`, a typed value, or an `Option` of one | `<input>`, or `<textarea>` with `.multiline(rows)` | `TextField` |
| `Field::choice(lens)` | any | `<select>` over static options or a relationship | `ChoiceField` |
| `Field::file(lens)` | `String` holding the file's path | file input: see [File uploads](#file-uploads) | `FileField` |
| `Field::toggle(lens)` | `bool` | checkbox | `CustomField` |
| `Field::custom(lens, control)` | `String`, a typed value, or an `Option` of one | your own `Control`: see [Custom controls](#custom-controls) | `CustomField` |

Every field takes `.label(..)`, which defaults to the column name in sentence case. Whether a
control is required is not the control's to say: on a create or edit page the panel renders it
required when its record-form field has no blank answer (a file input only while nothing is
stored). Outside a panel no record form applies, and every control renders optional. Each constructor returns its
control's builder, which offers only that control's modifiers, so a modifier on the wrong control
does not compile:

- text (`TextField`): `.email()`, `.unique()`, `.placeholder(..)`, `.multiline(rows)`;
- choice (`ChoiceField`): `.options(..)`, `.relationship::<R>()`, `.searchable()`, `.depends_on(..)`,
  `.multiple()`.

A choice needs something to offer: mounting refuses a resource form's or an action input's choice
with neither options nor a relationship, whose `<select>` would be empty and whose validation
would have nothing to check against. A page's own schema may build its options from data that is
empty for now, so rendering one does not refuse it.

### Custom controls

A `Control`, from `tablo::extend`, renders the input of a custom field: a record form's text control
turned custom with `.custom(control)`, or a `Field::custom` field. `.custom` (like `.choice()`) replaces the control
with its modifiers, so `email`, `unique` and `options` set before it no longer apply. The field keeps everything fields share —
the key, the label, the required marker, the error slot and the chrome around the input — and the
control renders only the input, from a `ControlInput` carrying the key, the current value and the
validation state:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-color-control}}
```

A control renders only the form; the detail page shows the value through a
[column](./detail-pages.md#what-the-page-shows). The submission is read like any other field's:
the value posted under the field's key, the last one when it is posted twice, parsed by the record
form. `Field::toggle` is built this way: `Toggle` renders a hidden `false` before the checkbox
under the same name, so an unchecked box submits `false` rather than nothing, and a `bool`
record-form field reads an empty submission as `false`.

### Typed values

A text control binds more than strings. Over an integer, float, `bool`, `Uuid` or
`jiff::Timestamp` column it renders the stored value, and the record form parses the submission
back through the type. A value the type refuses is an inline error naming it: `` `twelve` is not a
valid whole number ``. A `jiff::Timestamp` renders a `datetime-local` input, which carries no time zone, so
values display and parse as UTC.

Implement `tablo::extend::TypedValue` to bind your own type: `NOUN` names it in errors, `INPUT_TYPE` sets the
input's `type`, and `parse_input` reads a submission (by default through `FromStr`).

### Email and uniqueness

`.email()` renders `type="email"` and refuses what is not an address: a domain needs two labels
(`a@b` is refused), a display name (`Ada <ada@example.com>`) is refused, and the address is capped
at 254 bytes.

A text field over a column with a unique index checks uniqueness before the write, and reports a
taken value inline. `.unique()` states it explicitly; mounting the panel refuses `.unique()` on a
column without a unique index, single-column or composite. The check is a query before the write,
so two concurrent submissions can both pass it; the database index stays the final guard.

A unique field over a non-nullable column must be required: mounting refuses one whose record-form
field has a blank answer, since every empty submission would store the same value and the index
admits only one. An `Option` column stores `NULL` for an empty value, so a unique `Option` field
may stay optional.

### Relationships

A choice over a foreign key loads its options from the related resource. Mark the record-form field
`#[form(relationship = AuthorResource)]`; the form arranges and labels its control like any other.
The attribute also takes the field out of the derived table and shows its key on the detail page;
to keep them, leave the field unmarked and call `.choice().relationship::<AuthorResource>()` on its
control instead:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-relationship-field}}
```

Each option's value is the related record's primary key, and its text the record's title: the
column the related resource's `record_title` names, else its label and the key. Typing into a
searchable choice matches that column and the related table's searchable columns. A source that is
not a resource implements `OptionSource`, including its `label`. `.relationship_labelled::<R>(|a| ..)`
labels one field's options otherwise, to tell apart records that share a title.

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

### Many-to-many fields

A many-to-many relation stores one row per pair in a **join model** with a `belongs_to` to each
side. The model reaches the other side through it with a `#[has_many(via = ..)]` field:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-many-to-many-models}}
```

The record form names that field, typed as a `Vec` of the related records' keys, and marks it
with the related resource:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-many-to-many-field}}
```

- The control is a multiple choice: a checkbox per record of the related resource, offered as a
  [relationship](#relationships)'s options are, with the records the record links checked.
  `.searchable()` filters the boxes as the user types; without JavaScript every box shows.
- The form posts the field's key once per checked box, after a hidden blank, so a submission
  with no box checked links no record. Each key the record does not link yet must name a record
  the choice offers, and the write checks each again inside its transaction.
- A create links the new record to each chosen record, one join row each. An edit adds the rows
  of the records it chose and deletes those of the ones it dropped; one that does not post the
  field keeps the links. An overridden `create_record` or `update_record` links only by
  delegating to `write_create` or `write_update`.
- An edit unlinks only the records its form could offer: a linked record the related resource's
  policy hides from the user, or one in another tenant, stays linked.
- The choice offers at most 200 records. Past that it offers none, shows the ones the record
  links, and an edit keeps or drops those but links no other; attach records from a
  [related table](./detail-pages.md#many-to-many-relations) instead.
- The derived detail page lists the linked records by their titles, with
  `RelationColumn::list::<TagResource>(relation!(Article.tags))`, which a table or a declared view
  can show too.
- A new link sets the two keys and nothing else, so mounting refuses a join model with another
  column that is neither nullable nor `#[auto]`, or a key spanning several columns. A join
  model's `#[default]` and `#[update]` expressions are not applied: a nullable column with one
  stays `NULL`.
- Making the two keys the join model's primary key, `#[key(article_id, tag_id)]` as above, has
  the database store each pair once, whatever two concurrent writes do. It also
  refuses a multiple choice in a resource's form that names no many-to-many field. In an
  action's input, `form::parse_list` reads a multiple choice.

### Dependent choices

`.depends_on(&field, column)` narrows a relationship choice to the related records whose `column`
equals the value another field of the same form posts: the cities of the chosen country.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-dependent-choice}}
```

- The form renders the options of the parent's current value, and none while it is blank. When the
  parent changes, the browser fetches the new options from `GET {list_url}/options` and keeps the
  current choice only if they still offer it. A choice they drop changes too, so a choice
  depending on it, or a condition watching it, follows.
- A submission must name a record of the parent value it posts, or, on an edit that does not post
  the parent, of its stored value. The write checks the key again inside its transaction, as for
  any relationship.
- The 200-option cap counts the parent's records. Past it, a `.searchable()` dependent choice
  searches among them, and any other offers no option. A failed fetch offers none either.
- Without JavaScript the options stay those of the value the page rendered with, and a submission
  naming another value's record is refused.
- `column` must belong to the relationship's model, and the parent must be placed in the same
  schema: mounting refuses either mistake. Only a resource's form serves the options, so mounting
  also refuses a dependent choice in an action's input.
- A multiple choice neither depends on another field nor narrows one: mounting refuses both.

### Conditional fields

`.visible_when(&field, values)` shows a field only while another field of the same form posts one
of `values`: an option's value, or `true` or `false` for a toggle. A `Section`, `Group` or `Grid`
takes it too, for every field it holds:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-conditions}}
```

`visible_when` borrows the watched field, so build the condition before the schema takes it.

- The browser shows and hides the field as the watched one changes, with no request. A hidden
  field's control is disabled, so the browser neither validates nor posts it. Without JavaScript,
  the fields start as the stored values show them and stay that way.
- The panel reads a submission the same way and drops a hidden field's posted key, and a file
  field's carried upload: an edit keeps the stored value, and a create stores the blank answer. A
  conditional field therefore needs a blank answer, and mounting refuses one without.
- Mounting also refuses a value the watched field never posts (a choice's option or a toggle's
  `true` and `false` are checked), and a condition watching a field that another condition hides
  while the guarded field still shows: place the guarded field inside the block that hides the
  watched one. A schema also refuses a condition watching a field it does not place, which only a
  page's or an action's schema can do: a resource's form places every record-form field.
- A toggle is followed through its checkbox. An app's own checkbox `Control` posts the same
  `value` checked or not, so a condition cannot follow it.
- A multiple choice's key holds a list, which no condition value spells: mounting refuses a
  condition watching one.

## Submitting

- **Validation runs in one round.** The record form's parse (required fields and typed values),
  email, relationships and uniqueness are checked together, and the form re-renders with every
  error inline, with status 200 and nothing written. `validate_record` needs the parsed form, so
  it runs only when every field parses. It returns `FieldErrors<UserFormField>`, keyed by the
  record form's field enum (`errors.add(UserFormField::Age, "Age must be zero or more")`), so
  every error renders under its field's control, or an embedded value's first control.
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

A file field (`#[form(file)]` on a record-form `String`, or `Field::file` on a page) binds a
`String` column that stores the file's path or URL, never its bytes. A form
with a file field is sent as `multipart/form-data`, with a 10 MiB body limit (larger answers 413).
Filenames are reduced to a safe basename before anything sees them.

Where the bytes go is your app's decision. Install an `Uploader` on the panel; a panel whose
resource form declares a file field refuses to mount without one:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-uploader}}
```

- `store` returns the value to save. An `Err(reason)` is shown to the user inline as
  `<Label> could not be uploaded: <reason>`, so keep the reason free of paths and driver messages.
- **Keeping a file across a failed submit.** When a form re-renders with errors, the file input is
  empty again. The form carries the path `store` just returned, and the panel reuses it on the next
  submit only when `Uploader::holds(path)` answers `true`. The default answers `false`, so the user
  uploads again. Answer `true` only for a path your store produced and still holds inside its own
  root; checking whether an arbitrary path exists would let a client pick any file.
- **Editing.** The edit form shows the stored file as a link and an empty file input. Leaving it
  empty keeps the file; the "Remove the current file" checkbox (`clear_<field>`) empties the
  field. The input is required only while nothing is stored, and clearing a required field fails
  validation, so declare the record-form field `#[form(optional)]` when a record may lose its
  file.
- **Links.** The stored value renders as a link, on the edit form and in a `FileColumn` on the
  detail page, only when it is a root-relative path (`/uploads/a.png`, not `//host`) or an
  `http(s)` URL; anything else renders as text.
- **Serving.** `Panel::serve_dir(path, dir)` serves a directory, and the served files are
  **public**: the auth gate does not cover them. An app that needs protected files serves them
  from its own route. See [Security](./security.md) for the headers served files carry.

## Embedded values

A Toasty `#[derive(Embed)]` struct or enum is stored in its parent's row as flattened columns
(`seo_title`, `seo_description`). Derive `EmbeddedForm` on it and the form binds the whole value:

```rust
{{#include ../../../examples/guide/src/models.rs:seo-struct}}

// In the def's form: one call renders a control per field.
{{#include ../../../examples/guide/src/forms.rs:forms-embedded-schema}}

// In the record form: the value is one field.
{{#include ../../../examples/guide/src/forms.rs:forms-embedded-record-form}}
```

- Every field of the value is a scalar, or a nested value marked `#[form(embed)]`.
  `#[form(label = "…")]`, `#[form(multiline = N)]`, `#[form(blank = ..)]` and
  `#[form(optional)]` customize a field; an unknown attribute is a compile error.
- An embedded field is required, like a record-form field, unless it has a blank answer. An
  emptied field stores its blank answer, and a field with none refuses an empty submission
  inline.
- **Enums** render a choice of variant plus one group of fields per variant; the page shows only
  the chosen variant's group, and the variant can change on edit. The submission's variant decides
  which fields are read and validated, so a stale value in a hidden group never blocks a submit.
  A hidden group's controls are disabled, so the browser neither validates nor submits them.
  Without JavaScript only the stored variant's group shows, so a create page offers no variant's
  fields, and the variant choice still decides.
- On the detail page, `EmbeddedColumn::new(lens!(Post.seo))` shows each field of the value under
  its own label; an enum shows its variant's name and that variant's fields only.
- Not supported inside a value: a `#[document]` field, a relation, an enum nested inside an enum
  variant, and tuple structs.

A resource's form places an embedded value whole, as its record form's control. A page's schema
can bind a single embedded field on its own by its path: `Field::text(lens!(Post.seo.title))`
binds the flattened `seo_title` column. Bind such a schema with `.bind(&db)` before rendering it;
rendering one whose embedded paths are unbound fails rather than post the wrong key.

## Repeaters

Toasty stores a `#[document]` list of embedded structs in one column. Derive `RepeaterItem`
on the struct and mark the record-form field `#[form(repeat)]`: the form renders a row of the
item's controls per item, which the user adds, removes and moves.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-repeater}}

{{#include ../../../examples/guide/src/forms.rs:forms-repeater-layout}}
```

- An item's fields are scalars, declared like an [action input](./actions.md)'s: `label`,
  `multiline`, `placeholder`, `blank`, `optional` and `options` customize a field, and a field
  with no blank answer is required in every row.
- Each row posts its item's fields under its own prefix (`steps.0.minutes`), and the repeater's
  own key lists the rows in the order they show. A refused row renders its error under its own
  control, and the rows come back in the order the user left them.
- A repeater takes at most `MAX_ROWS` (100) rows; a submission listing more is refused.
- No rows is the repeater's blank answer, so a repeater is never required. An edit that does not
  post the repeater, such as one hidden by a [condition](#conditional-fields), keeps the stored
  rows; one that posts no rows stores an empty list.
- Adding, removing and moving rows needs JavaScript. Without it the stored rows still edit.
- An action's input can place a repeater too, with a hand-written `ActionInput` whose `parse`
  reads the rows with `schema::parse_items`. A page handling its own post folds the rows into
  the repeater's key first, with `Schema::fold_repeaters`.
- On the detail page, `RepeaterColumn` shows each item's fields under their labels. The record
  form's derived detail page includes one; its table does not.
- Not supported inside an item: an embedded value, another repeater, a relationship, a file field
  and a condition.
