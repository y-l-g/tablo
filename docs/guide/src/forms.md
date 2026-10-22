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
empty submission is refused inline. [`RecordForm`](https://y-l.fr/tablo/nightly/api/tablo/derive.RecordForm.html) lists each
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
whose records arrive another way declares `.edit_only()`: it serves the edit page, no create page,
and skips the check. A resource whose create page should also take a CSV file declares
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
[`ModelForm`](https://y-l.fr/tablo/nightly/api/tablo/derive.ModelForm.html) lists what it leaves out and how to opt a field in or
out.

Declare a `#[derive(RecordForm)]` struct instead when a form field is not a model column, such as
a [many-to-many field](#many-to-many-fields)'s list of keys, or when the model should not name a
resource. Move the field keys onto it and drop `ModelForm` from the model, or give one of the two
another name: both are `NoteForm` by default.

## Controls

The derive picks each field's control from its type and its `#[form(..)]` key, as
[`RecordForm`](https://y-l.fr/tablo/nightly/api/tablo/derive.RecordForm.html) lists: a `bool` is a toggle, `#[form(options)]` a
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

Each variant posts its `snake_case` name and reads as that name in sentence case;
[`Options`](https://y-l.fr/tablo/nightly/api/tablo/derive.Options.html) has the attributes that override either. A column, the
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

**Layout blocks** arrange fields: `Section::new(title)` is a titled card, `Group::new()` an untitled
container, and `Grid::new(cols)` a grid of 1 to 12 columns. A schema or block takes a tuple of at
most twelve children; nest a `Group` for more.

**Fields** are what `controls()` hands over, and what a page's or an action's schema builds from
a Toasty field lens:

| Constructor | Column | Control | Builder |
| --- | --- | --- | --- |
| `Field::text(lens)` | `String`, a typed value, or an `Option` of one | `<input>`, or `<textarea>` with `.multiline(rows)` | `TextField` |
| `Field::choice(lens)` | any | `<select>` over static options or a relationship | `ChoiceField` |
| `Field::rich_text(lens)` | `String` holding HTML | an editor with a toolbar: see [Rich text](#rich-text) | `RichTextField` |
| `Field::file(lens)` | `String` holding the file's path | file input: see [File uploads](#file-uploads) | `FileField` |
| `Field::toggle(lens)` | `bool` | checkbox | `CustomField` |
| `Field::custom(lens, control)` | `String`, a typed value, or an `Option` of one | your own `Control`: see [Custom controls](#custom-controls) | `CustomField` |

Every field takes `.label(..)`, which defaults to the column name in sentence case. Whether a
control is required is not the control's to say: on a create or edit page the panel renders it
required when its record-form field has no blank answer (a file input only while nothing is
stored). Outside a panel no record form applies, and every control renders optional. Each constructor returns its
control's builder, which offers only that control's modifiers, so a modifier on the wrong control
does not compile:

- text (`TextField`): `.email()`, `.url()`, `.tel()`, `.password()`, `.unique()`, `.placeholder(..)`,
  `.multiline(rows)`,
  and the [rules](#length-bounds-and-patterns) `.min_length(n)`, `.max_length(n)`, `.min(..)`,
  `.max(..)`, `.pattern(..)`;
- choice (`ChoiceField`): `.options(..)`, `.relationship::<R>()`, `.searchable()`, `.depends_on(..)`,
  `.multiple()`, and `.radio()` or `.buttons()` to list the options rather than a `<select>`;
- toggle (`CustomField` over a `bool`): `.switch()` draws it as an on/off switch;
- rich text (`RichTextField`): `.toolbar(..)`, `.placeholder(..)`, `.min_length(n)`, `.max_length(n)`.

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

A text control binds more than strings. Over a typed column it renders the stored value in the
input its type names, and the record form parses the submission back through the type. A value the
type refuses is an inline error naming it: `` `twelve` is not a valid whole number ``.

| Column type | Input |
| --- | --- |
| an integer | `type="number"` |
| `f32`, `f64` | `type="number" step="any"` |
| `jiff::civil::Date` | `type="date"` |
| `jiff::civil::Time` | `type="time"`, to the minute |
| `jiff::civil::DateTime` | `type="datetime-local"`, to the minute |
| `jiff::Timestamp` | `type="datetime-local"`, to the minute, in the request's [time zone](#time-zones) |
| `bool` (on a text control), `Uuid`, `TenantId` | `type="text"` |

A `jiff::Zoned` column has no control: no browser input carries a zone.

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

- **A timestamp's control** shows the stored instant as the wall-clock time in that zone, and the
  time it posts back stores the instant it names there: `14:00` entered in Paris on 20 October
  stores `12:00:00Z`. A time the zone skips moves forward by the gap, and a time it repeats takes
  the earlier instant; a value posted back untouched keeps its stored instant, in a repeater's
  rows too. A value posted with an offset, such as `2026-10-20T14:00:00-05:00`, names that
  instant and stores it in UTC. A `min` or `max` refusal spells the bound as the control shows it.
- **A `DateColumn` and a `DateFilter`** read a timestamp's calendar day in the same zone, so a
  user sees and filters by their own day. A civil date or date-time names its day whatever the
  zone. A filter's link therefore selects by the viewer's day: two users in different zones who
  open the same `?f.starts_at=2026-10-20` see different rows.
- **The stored value is the instant.** An export and a `TextColumn`, on the list or the detail
  page, spell it in RFC 3339, in UTC; a `DateColumn` shows the zone's day there too.
- **A custom control** receives the stored RFC 3339 value and posts a value the panel does not
  convert: it posts an instant with an offset, since a wall-clock time is refused.
- **`Schema::read(cx, pairs)`**, which a page handling its own post calls, reads a timestamp's
  wall-clock time in the request's zone too; `parse_input` alone refuses one, since it names no
  instant.

### Length, bounds and patterns

A text field declares the rules a value keeps. Each renders as the HTML attribute of the same name,
so the browser checks it before it submits, and the server checks it again on the trimmed value,
after the field's type parses it; a refused value is an inline error, and nothing is written. A
blank value is the field's blank answer, which no rule refuses, and a value the record already
stores is not checked again, so a rule added over existing rows does not lock them. A default
that breaks a rule, or that its type does not parse, fails the mount.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-rules}}
```

- `.min_length(n)` and `.max_length(n)` count the characters of the trimmed value, as a database's
  `VARCHAR(n)` does. A browser counts UTF-16 units of the untrimmed value, so it may let through
  a value the server then refuses, never the reverse for `max_length`. A `<textarea>` takes them
  too.
- `.min(bound)` and `.max(bound)` are inclusive, and compare values of the field's type: a number
  as a number, a date, time or timestamp in time, so `.min(date(2026, 1, 1))` bounds a
  `jiff::civil::Date` field. The bound is a value of that type; one it does not parse, or a type
  without an order such as `String`, fails the mount. A time's control edits it to the minute, so
  give a time bound in whole minutes.
- `.pattern(expr)` must match the whole value. The server matches it with the `regex` crate and the
  browser as a JavaScript expression in its `v` mode, so write it in the syntax both read:
  classes, groups, alternation and repetition. Spell digits `[0-9]`, since the server's `\d` also
  matches other scripts' digits; escape a `-` inside a class (`[a-z\-]`), which the browser
  otherwise refuses, dropping the whole pattern; and leave out lookaround, backreferences and
  inline flags such as `(?i)`. One that does not compile on the server fails the mount.
- A browser checks `pattern` and the lengths only on text-like inputs, and `min` and `max` only on
  numbers, dates and times; a `.multiline` textarea renders no `pattern`. The server checks every
  rule either way.
- A minimum above its maximum fails the mount, since no value would pass.

A rule across fields, or one these do not express, belongs in `Resource::validate_record`.

### Email, URL and uniqueness

`.email()` renders `type="email"` and refuses what is not an address: a domain needs two labels
(`a@b` is refused), a display name (`Ada <ada@example.com>`) is refused, and the address is capped
at 254 bytes. `.url()` renders `type="url"` and refuses what [`TextField::url`](https://y-l.fr/tablo/nightly/api/tablo/struct.TextField.html#method.url) does not take: the
browser's own check accepts a `javascript:` URL, which a link to the stored value would run.

A text field over a column with a unique index checks uniqueness before the write, and reports a
taken value inline. `.unique()` states it explicitly; mounting the panel refuses `.unique()` on a
column without a unique index, single-column or composite. The check is a query before the write,
so two concurrent submissions can both pass it; the database index stays the final check. When a
create fails on a database error, or an edit fails on a database error rather than its version
condition, the form runs the query again once the write's transaction is rolled back: the
submission that lost the race re-renders with the same inline error, and a failure the query does
not explain still answers 500. That needs the database to refuse the losing write once the
winning one has committed, as PostgreSQL and MySQL do; SQLite can refuse it as busy before then,
and Turso's concurrent transactions conflict at commit, so either can still answer 500. A
resource whose scoped query cannot see the row the index compares against, such as an index
broader than its tenancy, still answers 500 too.

A unique field over a non-nullable column must be required: mounting refuses one whose record-form
field has a blank answer, since every empty submission would store the same value and the index
admits only one. An `Option` column stores `NULL` for an empty value, so a unique `Option` field
may stay optional.

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

- Options come from the related resource's tenant-scoped query and follow its policy: the list is
  empty and the field shows "not available" unless the related resource's policy allows
  `ViewAny`, and each record must pass its `View`.
- A submitted key must be one of those records, so a hand-crafted POST cannot point at another
  tenant's row. The write checks the key again inside its transaction, so a record deleted, moved
  to another tenant or hidden since the form validated refuses the write with the same field
  error.
- At most 200 options load. Past that, a `.searchable()` choice turns into type-to-search against
  `GET {list_url}/-/options`, which searches the related table's `searchable()` columns; a
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
- A declared table or detail page lists the linked records by their titles with
  `RelationColumn::list::<TagResource>(relation!(Article.tags))`.
- A new link sets the two keys and nothing else, so mounting refuses a join model with another
  column that is neither nullable nor `#[auto]`, or a key spanning several columns. A join
  model's `#[default]` and `#[update]` expressions are not applied: a nullable column with one
  stays `NULL`.
- Making the two keys the join model's primary key, `#[key(article_id, tag_id)]` as above, has
  the database store each pair once, whatever two concurrent writes do. It also
  refuses a multiple choice in a resource's form that names no many-to-many field. In an
  action's input, `tablo::parse_list` reads a multiple choice.

### Dependent choices

`.depends_on(&field, column)` narrows a relationship choice to the related records whose `column`
equals the value another field of the same form posts: the cities of the chosen country.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-dependent-choice}}
```

- The form renders the options of the parent's current value, and none while it is blank. When the
  parent changes, the browser fetches the new options from `GET {list_url}/-/options` and keeps the
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

### Help, disabled controls and defaults

Every control but a repeater takes `.help(text)`, rendered under it, and `.disabled()` or
`.disabled_on_edit()`. A text, choice or custom control also takes `.default(value)`, the value the
create form starts with: any form scalar, or a `&str` in the form spelling. A multiple choice's
default is an array or `Vec` of them: `.multiple().default(["email", "sms"])`.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-field-basics}}
```

- The control names its help text in `aria-describedby`, beside its error.
- A disabled control renders `disabled`, so the browser posts nothing for it, and the panel drops
  a key posted anyway, with a file field's carried upload. An edit keeps the stored value. A create
  stores the field's default, else its blank answer: `.disabled()` on a field with neither refuses
  to mount, since no create could fill it.
- `.disabled_on_edit()` leaves the create form's control enabled: the create sets the value, and
  no edit changes it.
- A condition watching a disabled field reads what the browser shows: its default on a create,
  its stored value on an edit, never what the submission posts.
- A default seeds the create form and an action's input form; an edit form shows the stored
  value. The blank answer is different: it is what an empty submission stores. Mounting refuses a
  default the control never posts: one a typed field does not parse, or not one of a choice's
  options.
- A repeater's row controls take neither `disabled` nor `default`: mounting refuses both.

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
  every error renders under its field's control, or an embedded value's first control. A rule
  that reads the database refuses from `create_record` or `update_record` instead, which render
  their `WriteError::Refused` errors the same way ([Resources](./resources.md#writes)).
- **Unknown keys are refused.** A POST carrying a key the form does not declare answers 400, so a
  client cannot write `role` or `tenant_id`. The CSRF token, the edit form's `record_version` and
  the file fields' `clear_` and `keep_` keys are the exceptions.
- **An edit writes only what was posted.** On an edit, a declared key missing from the submission
  keeps its stored value, and the update assigns only the fields the submission named, plus the
  model's own `#[update(..)]` defaults and `#[version]` bump. An API client can post a single
  field. A control submitted empty is posted, and stores its field's blank value.
- **A save another save overtook is refused.** When the model has a `#[version]` field and
  derives `ModelColumns`, the edit form carries the version it opened on as `record_version`. A
  save whose version no longer matches the stored record writes nothing and re-renders the form
  with the submitted values, an alert above them, and the current version, so saving again
  overwrites the other save. A save that lands between the edit's load and its write fails
  Toasty's version condition and re-renders the same way where the database reports it so, as
  PostgreSQL and MySQL do; SQLite can refuse the transaction as busy instead, and Turso's
  concurrent transactions conflict at commit, which both answer 500. A POST without
  `record_version` skips the first comparison, not the second; one whose `record_version` is no
  number answers 400. Only the edit form carries the version: actions, relation managers and the
  profile pages write without it, and the default Delete removes the record whatever its version. Mounting
  refuses a form field named `record_version` or `csrf_token`.

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

- **The toolbar is the schema.** `.toolbar(..)` takes groups of `Tool`s, `Tool::DEFAULT` unless
  declared. Each tool names the elements it produces: `Tool::Bold` a `<strong>`, `Tool::Link` an
  `<a href>`, `Tool::BulletList` a `<ul>` of `<li>`. The editor registers only those, so a tool
  left off the toolbar cannot be typed and drops from a paste. The editor takes a link only to a
  URL the server keeps, and a code block's language, which the editor records as a class, is not
  stored.
- **The server cleans what it reads.** A submission keeps only the elements and attributes its
  field's tools produce, before the record form parses it: a `<script>`, a `style`, an `on…`
  handler and a link to a scheme other than `http`, `https` or `mailto` never reach the column,
  and every link gets `rel="noopener noreferrer nofollow"`. A document with no text reads as an
  empty submission, so the field's blank answer applies, and the empty paragraphs the editor
  leaves at a document's end are dropped.
- **Length rules count the text.** `.min_length(n)` and `.max_length(n)` count the characters a
  reader sees, not the markup.
- **Without JavaScript** the control is a `<textarea>` of the HTML, cleaned the same way on submit.
  The editor is TipTap, which the page loads only when it holds a rich text control.
- **A text column moving to rich text needs its values moved too.** The field reads what the
  column stores as HTML, so `a<b` loses its `<b`, and each edit stores what it read. Rewrite each
  stored value through `tablo::rich_text::from_text`, which escapes it and wraps its paragraphs,
  before the field edits it.

`RichTextColumn::new(lens)` shows the value on a [detail page](./detail-pages.md) as formatted HTML,
and in a table and an export as its text. It cleans the value again as it renders, keeping what any
`Tool` produces, since a seed or another app can write the column. A page of your own renders a
stored value through `tablo::rich_text::clean`, unescaped, and reads its text through
`tablo::rich_text::text`.

## File uploads

A file field (`#[form(file)]` on a record-form `String`, or `Field::file` on a page) binds a
`String` column that stores the file's path or URL, never its bytes. A form
with a file field is sent as `multipart/form-data`, with a 10 MiB body limit (larger answers 413).
Filenames are reduced to a safe basename before anything sees them.

Where the bytes go is your app's decision; a panel whose resource form declares a file field
refuses to mount without an `Uploader`. `Panel::uploads_dir` installs the stock one, a
`DirUploader`, and serves its directory:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-uploads-dir}}
```

It writes each file into the directory as `{uuid}-{basename}`, within the filesystem's 255-byte
name limit, and stores its URL: `/uploads/` and the name as one percent-encoded path segment. Any
other store implements `Uploader`; this one refuses all but images, then stores in a directory:

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-uploader}}
```

- `store` returns the value to save. An `Err(reason)` is shown to the user inline as
  `<Label> could not be uploaded: <reason>`, so keep the reason free of paths and driver messages.
- **Keeping a file across a failed submit.** When a form re-renders with errors, the file input is
  empty again. The form carries the path `store` just returned, and the panel reuses it on the next
  submit only when `Uploader::holds(path)` answers `true`. The default answers `false`, so the user
  uploads again. Answer `true` only for a path your store could have written and still holds inside
  its own root; checking whether an arbitrary path exists would let a client pick any file. A
  `DirUploader` answers `true` for a URL spelled as its `store` spells one, naming a file in its
  directory.
- **Editing.** The edit form shows the stored file as a link and an empty file input. Leaving it
  empty keeps the file; the "Remove the current file" checkbox (`clear_<field>`) empties the
  field. The input is required only while nothing is stored, and clearing a required field fails
  validation, so declare the record-form field `#[form(optional)]` when a record may lose its
  file.
- **Links.** The stored value renders as a link, on the edit form and in a `FileColumn` on the
  detail page, only when it is a root-relative path (`/uploads/a.png`, not `//host`) or an
  `http(s)` URL; anything else renders as text.
- **Serving.** `Panel::serve_dir(path, dir)` serves a directory (`uploads_dir` calls it), and the
  served files are **public** unless the path sits under the panel prefix, which the auth gate
  covers. See [Security](./security.md) for the headers served files carry.

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
  [`EmbeddedForm`](https://y-l.fr/tablo/nightly/api/tablo/derive.EmbeddedForm.html) lists the attributes that customize a field.
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
binds the flattened `seo_title` column, as soon as the field is built.

## Repeaters

Toasty stores a `#[document]` list of embedded structs in one column. Derive `FormInput`
on the struct and mark the record-form field `#[form(repeat)]`: the form renders a row of the
item's controls per item, which the user adds, removes and moves.

```rust
{{#include ../../../examples/guide/src/forms.rs:forms-repeater}}

{{#include ../../../examples/guide/src/forms.rs:forms-repeater-layout}}
```

- An item's fields are scalars, declared like an [action input](./actions.md)'s with the
  [`FormInput`](https://y-l.fr/tablo/nightly/api/tablo/derive.FormInput.html) attributes, and a field with no blank answer is
  required in every row.
- Each row posts its item's fields under its own prefix (`steps.0.minutes`), and the repeater's
  own key lists the rows in the order they show. A refused row renders its error under its own
  control, and the rows come back in the order the user left them.
- A repeater takes at most `MAX_REPEATER_ROWS` (100) rows; a submission listing more is refused.
- No rows is the repeater's blank answer, so a repeater is never required. An edit that does not
  post the repeater, such as one hidden by a [condition](#conditional-fields), keeps the stored
  rows; one that posts no rows stores an empty list.
- Adding, removing and moving rows needs JavaScript. Without it the stored rows still edit.
- An action's input can place a repeater too, with a hand-written `FormInput` whose `parse`
  reads the rows with `tablo::parse_items`. A page handling its own post reads it into
  `FormValues` first, with `Schema::read`.
- On the detail page, `RepeaterColumn` shows each item's fields under their labels. The model's
  default detail page includes one; its table does not.
- Not supported inside an item: an embedded value, another repeater, a relationship, a file field
  and a condition.
