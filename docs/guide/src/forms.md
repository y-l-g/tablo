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
**blank answer** is what it stores when its control is submitted empty, such as `None` for an
`Option`. A field with no blank answer is required: the panel renders its control required, and an
empty submission is refused inline. [`RecordForm`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.RecordForm.html) lists each
type's blank answer and every `#[form(..)]` key.

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

A resource with a record form serves a create page, whoever its policy lets in, so mounting the
panel refuses it unless every non-nullable column is filled by the form, by Toasty, by the tenant
stamp, or by an overridden `create_record` whose def names it with `create_column`. A resource
whose records arrive another way declares `.without_page(ResourcePage::Create)`: it serves the edit
page, no create page, and skips the check. A resource whose create page should also take a CSV file declares
[`.importable()`](./tables.md#import).

## A form from the model

When the record form is the model's fields a person fills, derive `tablo::ModelForm` on the model
and it writes the struct, named after the model:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-model-form}}
```

`NoteForm` holds `title`, `body` and `author_id`, with `NoteFormField` and `NoteFormControls`
beside it. The derive leaves out what no person fills, such as the key and the tenant column, and
`author_id` is in only because `#[form(relationship = AuthorResource)]` opts the foreign key in:
without it, a create would leave the column unset, which mounting refuses.
[`ModelForm`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.ModelForm.html) lists what it leaves out and how to opt a field in or
out.

Declare a `#[derive(RecordForm)]` struct instead when a form field is not a model column, such as
a [many-to-many field](#many-to-many-fields)'s list of keys, or when the model should not name a
resource. Move the field keys onto it and drop `ModelForm` from the model, or give one of the two
another name: both are `NoteForm` by default.

## Controls

The derive picks each field's control from its type and its `#[form(..)]` key, as
[`RecordForm`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.RecordForm.html) lists: a `bool` is a toggle, `#[form(options)]` a
choice, and a field with no key a text field. `controls()` hands each one over ready for its
modifiers, so an override arranges rather than rebinds:

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

Each variant posts its `snake_case` name and reads as that name in sentence case, unless a
catalog [translates](./translations.md#labels) it;
[`Options`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.Options.html) has the attributes that override either. A column, the
detail page and a group header read a variant's label, and a query compares the variant itself: `User::fields().role().eq(Role::Admin)`. An `Option<Role>`
field works the same way, reading an empty submission as `None`. An enum field without
`#[form(options)]` gets a text field, which accepts only an option's value. An enum leaf of an
[embedded value](#embedded-values) is such a text field, and its column shows the value.

Toasty stores the variant itself, under its `snake_case` name unless `#[column(variant = "..")]`
renames it; the option's value only spells it in forms and URLs. Moving a `String` column to an
enum is therefore a schema change: rows whose text is not Toasty's name for a variant no longer
load.

A `String` field takes the same list with `#[form(options = T)]`. `.options` takes
`Vec<(String, String)>` (an `Options` enum's list), `Vec<String>`, or `[&str; N]`
(`["admin", "member"]`).

A `Vec` of an `Options` enum with `#[form(options)]`, or a `Vec<String>` with
`#[form(options = T)]`, is a **multiple choice**: a checkbox per option, and Toasty stores the
chosen ones in one list column.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-multiple-choice}}
```

No option checked stores the empty list, so `.min_items(1)` is how the field requires a choice;
`.max_items(n)` caps it, and `.buttons()` renders a compact list of toggles.
[`RecordForm`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.RecordForm.html) has the rules.

A `Vec<String>` marked `#[form(tags)]` takes what the user types instead: a checkbox per value
it holds and per suggestion, then a text input adding values separated by commas.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-tags}}
```

[`ChoiceField::tags`](https://y-l.fr/tablo/nightly/api/tablo/struct.ChoiceField.html#method.tags)
has the rules.

**Layout blocks** arrange fields: `Section::new(title)` is a titled card, `Group::new()` an untitled
container, and `Grid::new(cols)` a grid of 1 to 12 columns. A schema or block takes a tuple of at
most twelve children; nest a `Group` for more.

A section can say what its fields are for and where they sit. `.description` adds a line under the
title and `.icon` an icon before it; `.aside()` puts both beside the fields on a wide screen, so a
long form reads as a list of topics. `.collapsible()` lets the title fold the fields away, and
`.collapsed()` opens the page with them folded, for fields the user rarely changes:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-sections}}
```

A folded section's fields still post, and a section holding a field error renders open.
[`Section`](https://y-l.fr/tablo/nightly/api/tablo/struct.Section.html) has the full rules.

`Tabs::new` shows one `Tab` at a time. Each tab's fields still render and post while another
tab shows, so splitting a long form into tabs changes what the user sees, not what the form
submits:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-tabs}}
```

A refused submission reopens on the first tab holding an error, and the browser opens the tab of a
control it refuses itself. [`Tabs`](https://y-l.fr/tablo/nightly/api/tablo/struct.Tabs.html) has the
full rules.

**Fields** are what `controls()` hands over, and what a page's or an action's schema builds from
a Toasty field lens:

| Constructor | Column | Control | Builder |
| --- | --- | --- | --- |
| `Field::text(lens)` | `String`, a typed value, or an `Option` of one | `<input>`, or `<textarea>` with `.multiline(rows)` | `TextField` |
| `Field::choice(lens)` | any | `<select>` over static options or a relationship | `ChoiceField` |
| `Field::rich_text(lens)` | `String` holding HTML | an editor with a toolbar: see [Rich text](#rich-text) | `RichTextField` |
| `Field::file(lens)` | `String` holding the file's path | file input: see [File uploads](#file-uploads) | `FileField` |
| `Field::files(lens)` | `Vec<String>` holding the files' paths | file list: see [Several files per field](#several-files-per-field) | `FileField` |
| `Field::toggle(lens)` | `bool` | checkbox | `CustomField` |
| `Field::custom(lens, control)` | `String`, a typed value, or an `Option` of one | your own `Control`: see [Custom controls](#custom-controls) | `CustomField` |

Every field takes `.label(..)`, which defaults to the column name in sentence case. Whether a
control is required is not the control's to say: on a create or edit page the panel renders it
required when its record-form field has no blank answer (a file input only while nothing is
stored). Outside a panel no record form applies, and every control renders optional. Each constructor returns its
control's builder, which offers only that control's modifiers, so a modifier on the wrong control
does not compile:

[`TextField`](https://y-l.fr/tablo/nightly/api/tablo/struct.TextField.html), [`ChoiceField`](https://y-l.fr/tablo/nightly/api/tablo/struct.ChoiceField.html),
[`RichTextField`](https://y-l.fr/tablo/nightly/api/tablo/struct.RichTextField.html) and [`CustomField`](https://y-l.fr/tablo/nightly/api/tablo/struct.CustomField.html)
list each builder's modifiers, such as `.url()`, `.relationship::<R>()`, `.switch()` or
`.accepted()`. A multiple choice is required through
[`ChoiceField::min_items`](https://y-l.fr/tablo/nightly/api/tablo/struct.ChoiceField.html#method.min_items),
since its blank answer is the empty list. A choice needs something to offer, so mounting refuses a
resource form's or an action input's choice with neither options nor a relationship.

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

A text control binds more than strings. Over a typed column it renders the stored value in the
input its type names, and the record form parses the submission back through the type. A value the
type refuses is an inline error naming it: `` `twelve` is not a valid whole number ``.

Integers and floats get a number input, the `jiff` civil types and `Timestamp` a date or time
input; [`TypedValue`](https://y-l.fr/tablo/nightly/api/tablo/extend/trait.TypedValue.html) lists each type's input.

An amount that must stay exact, such as a price, is a decimal column. The `tablo` features
`rust_decimal` and `bigdecimal` bind `rust_decimal::Decimal` and `bigdecimal::BigDecimal`, and turn
on Toasty's feature of the same name, which stores them:

```toml
[dependencies]
tablo = { version = "0.9.0", features = ["sqlite", "rust_decimal"] }
rust_decimal = "1"
```

A decimal gets a number input and keeps the scale it was typed with, so `12.50` is stored as
`12.50`. A spelling the type cannot hold exactly, such as an exponent or more decimals than
`Decimal` carries, is an inline error rather than a rounded value.

A decimal column is not sortable by default: SQLite, MySQL and Turso store a `Decimal` as text, and
every driver stores a `BigDecimal` as text, so the database would order `10.00` before `9.50`. The
`ModelColumns` derive leaves it unsortable; declare `.sortable()` only on PostgreSQL with
`Decimal`.

Implement [`tablo::extend::TypedValue`](https://y-l.fr/tablo/nightly/api/tablo/extend/trait.TypedValue.html) to bind your own
type: it names the type in errors, picks the input, and parses and spells the value, with a default
for each item but the name.

### Time zones

A `datetime-local` input posts a wall-clock time without a zone, so the panel places it in the
request's zone: `tablo::time_zone(cx)` answers the signed-in user's `PanelUser::time_zone`, else
the panel's `Panel::time_zone`, else UTC.

```rust
{{#include ../../../examples/guide/src/panel_routing.rs:panel-time-zone}}
```

A timestamp's control therefore shows the stored instant as the user's own wall-clock time, and a
`DateColumn` or `DateFilter` reads the user's own day: `14:00` entered in Paris on 20 October
stores `12:00:00Z`. The stored value stays the instant, which the export spells in UTC.
[`time_zone`](https://y-l.fr/tablo/nightly/api/tablo/fn.time_zone.html) has the rules for skipped and repeated times, offsets, and
custom controls.

### Length, bounds and patterns

A text field declares the rules a value keeps. Each renders as the HTML attribute of the same name,
so the browser checks it before it submits, and the server checks it again; a refused value is an
inline error, and nothing is written. A value the record already stores is not checked again, so
a rule added over existing rows does not lock them.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-rules}}
```

[`TextField`](https://y-l.fr/tablo/nightly/api/tablo/struct.TextField.html) has each rule's details: what a length counts, which
types take a bound, and the regular-expression syntax a `pattern` shares between the server and
the browser. A rule that cannot work, such as a minimum above its maximum, fails the mount.

A text field also compares with another field of its form: `.gt(&other)`, `.gte`, `.lt` and `.lte`
order two numbers, dates or times, and `.different(&other)` refuses an equal value, as
`ends_on.gte(&starts_on)` keeps a stay from ending before it starts. Only the server checks them,
in the same round as the other rules; [`TextField::gt`](https://y-l.fr/tablo/nightly/api/tablo/struct.TextField.html#method.gt)
has the rules. A rule these do not express belongs in `Resource::validate_record`.

### Email, URL and uniqueness

`.email()` refuses what is not an address, and `.url()` what is not an `http` or `https` URL: the
browser's own check accepts a `javascript:` URL, which a link to the stored value would run.

A text field over a column with a unique index checks uniqueness before the write, and reports a
taken value inline; `.unique()` states it explicitly. The database index stays the final check,
so a unique field over a non-nullable column must be required, or every empty submission would
store the same value. [`TextField::unique`](https://y-l.fr/tablo/nightly/api/tablo/struct.TextField.html#method.unique) says what a
concurrent write answers on each database.

### Relationships

A choice over a foreign key loads its options from the related resource. Mark the record-form field
`#[form(relationship = AuthorResource)]`; the form arranges and labels its control like any other:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-relationship-field}}
```

Each option's value is the related record's primary key, and its text the record's title: the
column the related resource's `record_title` names, else its label and the key. Typing into a
searchable choice matches that column and the related table's searchable columns. A source that is
not a resource implements `OptionSource`, including its `label`. `.relationship_labelled::<R>(|a| ..)`
labels one field's options otherwise, to tell apart records that share a title.

Options follow the related resource's tenant scope and policy, and a submitted key must be one of
them, checked again inside the write's transaction, so a hand-crafted POST cannot point at another
tenant's row. Past 200 options a `.searchable()` choice turns into type-to-search.
[`ChoiceField::relationship`](https://y-l.fr/tablo/nightly/api/tablo/struct.ChoiceField.html#method.relationship) has the rules.

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

The control is a checkbox per record the related resource offers. A create links the new record
to each chosen one, one join row each, and an edit adds and deletes rows to match, unlinking only
the records its form could offer. An overridden `create_record` or `update_record` links only by
delegating to `write_create` or `write_update`. Making the two keys the join model's primary key,
`#[key(article_id, tag_id)]` as above, has the database store each pair once.
[`RecordForm`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.RecordForm.html#many-to-many-fields) has the rules, the option cap and
what the mount refuses.

### Dependent choices

`.depends_on(&field, column)` narrows a relationship choice to the related records whose `column`
equals the value another field of the same form posts: the cities of the chosen country.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-dependent-choice}}
```

When the parent changes, the browser fetches the new options, and a submission must name a record
of the parent value it posts. [`ChoiceField::depends_on`](https://y-l.fr/tablo/nightly/api/tablo/struct.ChoiceField.html#method.depends_on)
has the rules and what the mount refuses.

### Creating a related record

`.creatable()` lets the user create the related record without leaving the form: a link beside
the select opens the related resource's create page in a dialog, and the record it saves becomes
the choice. The form keeps what was typed in it.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-creatable-choice}}
```

The dialog runs the related resource's own create page; [`ChoiceField::creatable`](https://y-l.fr/tablo/nightly/api/tablo/struct.ChoiceField.html#method.creatable)
has the rules and what the mount refuses.

### Help, disabled controls and defaults

Every control but a repeater takes `.help(text)`, rendered under it, and `.disabled()` or
`.disabled_on_edit()`. A text, choice or custom control also takes `.default(value)`, the value the
create form starts with: any form scalar, or a `&str` in the form spelling. A multiple choice's
default is an array or `Vec` of them: `.multiple().default(["email", "sms"])`.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-field-basics}}
```

A disabled control posts nothing, so an edit keeps the stored value and a create stores the
default, else the blank answer; `.disabled()` on a field with neither cannot mount, since no create
could fill it. A default seeds the create form, where the blank answer is what an empty submission
stores. Each modifier's page has its rules, such as
[`default`](https://y-l.fr/tablo/nightly/api/tablo/struct.TextField.html#method.default).

A record form's field also takes `.hidden_on_create()` or `.hidden_on_edit()`, which renders nothing
for it on that form. Here an invite sets the address once and the role only once the account exists:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-hidden-on}}
```

The server reads a hidden field as it reads a disabled one: a create stores its default, else its
blank answer, and an edit keeps the stored value.

### Conditional fields

`.visible_when(&field, values)` shows a field only while another field of the same form posts one
of `values`: an option's value, or `true` or `false` for a toggle. A `Section`, `Group` or `Grid`
takes it too, for every field it holds:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-conditions}}
```

`visible_when` borrows the watched field, so build the condition before the schema takes it.

The browser shows and hides the field as the watched one changes, with no request, and the panel
reads a submission the same way, dropping a hidden field's key. A conditional field therefore needs
a blank answer. [`visible_when`](https://y-l.fr/tablo/nightly/api/tablo/struct.TextField.html#method.visible_when) has the rules and
what the mount refuses.

`.disabled_when(&field, values)` keeps the control on the page but disables it while the watched
field posts one of `values`: a member's account cannot be SSO-managed.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-disabled-when}}
```

While the condition holds, the server reads the field as it reads a `.disabled()` one, so a create
stores its default and an edit keeps the stored value. The condition reads what the submission
posts, so it is no access check: [field access](./resources.md#field-access) is.
[`disabled_when`](https://y-l.fr/tablo/nightly/api/tablo/struct.TextField.html#method.disabled_when) has the rules and what the mount
refuses.

## Submitting

Every error renders inline in one round, with nothing written: the parse, the controls' rules,
relationships and uniqueness together, then `validate_record` once every field parses, keyed by
the record form's field enum (`errors.add(UserFormField::Age, "Age must be zero or more")`). A POST
carrying a key the form does not declare answers 400, so a client cannot write `role` or
`tenant_id`; an edit writes only the fields it posted; and a model with a `#[version]` field
refuses a save another save overtook. [`RecordForm`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.RecordForm.html#submitting) has
the rules.

The create page has a second button, **Create & create another**, for entering several records in
a row: it runs every check above, and a save lands back on the create form with the success flash,
its fields at their defaults. From a relation table's create link, the next form keeps the owner
preselected.

The form page's script saves on `Ctrl+S` (`Cmd+S` on macOS), and asks before a followed link, a
reload or a closed tab drops what the user typed; a refused save asks from the start, since its
values are the user's. The browser's back and forward buttons leave without asking. Without
JavaScript the form posts as it renders and the page leaves without asking.

To write more than the form holds, override `create_record` or `update_record`
([Resources](./resources.md#writes)) and use the builders: `form.into_create()` is the model's
create builder with every form field set, and `posted.into_update(&mut record)` is the update
builder with one assignment per posted field, or `None` when nothing was posted.

## Rich text

A rich text field edits HTML through a toolbar and stores it in a `String` or `Option<String>`
column. A record form's text control becomes one with `.rich_text()`, and a panel whose field of
another type does refuses to mount; a page or an action input builds one with
`Field::rich_text(lens)` or `Field::rich_text_input(name)`:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-rich-text}}
```

The toolbar is the schema: the editor registers only the elements its tools produce, and the
server cleans every submission down to them, so a `<script>` or a `javascript:` link never reaches
the column. A text column moving to rich text needs its values rewritten through
`tablo::rich_text::from_text` first. [`RichTextField`](https://y-l.fr/tablo/nightly/api/tablo/struct.RichTextField.html) has the
rules.

`RichTextColumn::new(lens)` shows the value on a [detail page](./detail-pages.md) as formatted HTML,
and in a table and an export as its text. It cleans the value again as it renders, keeping what any
`Tool` produces, since a seed or another app can write the column. A page of your own renders a
stored value through `tablo::rich_text::clean`, unescaped, and reads its text through
`tablo::rich_text::text`.

## File uploads

A file field (`#[form(file)]` on a record-form `String`, or `Field::file` on a page) binds a
`String` column that stores the file's path or URL, never its bytes; a file list binds a
`Vec<String>` of them ([Several files per field](#several-files-per-field)). A form
with a file field is sent as `multipart/form-data`, with a 10 MiB body limit (larger answers 413)
that `Panel::limits` raises with `Limits::body_bytes`.
Filenames are reduced to a safe basename before anything sees them.

Where the bytes go is your app's decision; a panel whose resource form declares a file field
refuses to mount without an `Uploader`. `Panel::uploads_dir` installs the stock one, a
`DirUploader`, and serves its directory:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-uploads-dir}}
```

It writes each file into the directory as `{uuid}-{basename}`, within the filesystem's 255-byte
name limit, and stores its URL: `/uploads/` and the name as one percent-encoded path segment. The
directory answers that URL only with a signature that expires, which the panel adds wherever it
links or shows a stored file; a page of your own adds it with `url::file(cx, &path)`. Once a write
commits, it deletes the files the write let go: those of a deleted record, and the one an edit
replaced or cleared; a soft delete keeps them until the force delete. The upload of a form that
never saved, such as one refused for another field that the user then left, is deleted after
24 hours.

A file field takes any file until it declares `.extensions(["png", "jpg"])` or `.max_bytes(n)`.
The server refuses a file either rule refuses before the uploader sees it, and the extensions also
reach the browser's file picker as `accept`. The extension is what is checked, not the content: a
served file's type comes from its extension, so the extension decides how a browser treats it.
[`FileField::extensions`](https://y-l.fr/tablo/nightly/api/tablo/struct.FileField.html#method.extensions)
has the rules.

Any other store implements `Uploader`. This one refuses a file whose first bytes are not a PNG's
or a JPEG's signature, which no field rule checks, then stores through the `DirUploader` that
`Panel::uploads_dir_with` hands it, so its files still answer only at signed URLs. It forwards
`keep` and `delete` too: the inner uploader deletes a file nothing kept after a day. A signature
is no proof the rest is an image: decode the file when that matters.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-uploader}}
```

`store` returns the value to save, and its error is shown to the user, so keep it free of paths.
The edit form keeps a stored file until the user removes it, and a form that re-renders with errors
reuses an upload only under the tag the panel signed for its field, where `Uploader::holds` vouches
for the path.
[`Uploader`](https://y-l.fr/tablo/nightly/api/tablo/trait.Uploader.html) has the rules, and [Security](./security.md) the headers
served files carry.

### Several files per field

`#[form(file)]` on a `Vec<String>`, or `Field::files` on a page, makes a file list: the column
stores one path per file, in the order the form lists them. Its input takes several files at once,
which a save adds after the stored ones, and each stored file has its own remove checkbox, so the
list works without JavaScript. With it, each row also has Move up and Move down buttons, and the
list saves in the order its rows show. Only the remove checkbox drops a stored file: a file another
save added while the form was open stays. The extensions and size apply to each file, and
`max_files` bounds how many the list holds once a save adds its files; choosing more than that
stores none:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-file-list}}
```

A refused save keeps the files it stored, as a single file field does, so the user drops one and
saves again. Every file still shares the one form body limit, so a list of large files needs a
raised `Limits::body_bytes`.
[`Field::files`](https://y-l.fr/tablo/nightly/api/tablo/struct.Field.html#method.files) has the
rules.

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

Every field of the value is a scalar, or a nested value marked `#[form(embed)]`; an enum renders
a choice of variant and the chosen variant's fields. On the detail page,
`EmbeddedColumn::new(lens!(Post.seo))` shows each field under its own label.
[`EmbeddedForm`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.EmbeddedForm.html) has the rules: which fields are required,
which variant a submission reads, the attributes, and what a value cannot hold.

A resource's form places an embedded value whole, as its record form's control. A page's schema
can bind a single embedded field on its own by its path: `Field::text(lens!(Post.seo.title))`
binds the flattened `seo_title` column, as soon as the field is built.

## Repeaters

Toasty stores a `#[document]` list of embedded structs in one column. Derive `FormInput`
on the struct and mark the record-form field `#[form(repeat)]`: the form renders a row of the
item's controls per item, which the user adds, removes and moves.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-repeater}}

{{#include ../../../examples/guide/src/forms.rs:forms-repeater-layout}}
```

An item's fields are scalars, declared like an [action input](./actions.md)'s with the
[`FormInput`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.FormInput.html) attributes, and a field with no blank answer is
required in every row. Each row posts its item's fields under its own prefix, such as `steps.0.minutes`, and an edit that
does not post the repeater keeps the stored rows. Adding, removing and moving rows needs
JavaScript. [`Field::repeater`](https://y-l.fr/tablo/nightly/api/tablo/struct.Field.html#method.repeater) has the rules and the limits.
