# 0022 Forms write through a derived typed struct

A form resource declares a `#[derive(RecordForm)]` struct with one field per written column,
named and typed like the model, so a renamed or retyped column fails to compile. The derive also
provides the default form schema. An edit fills every key the submission omits from the
stored record, so `update_record` receives a whole form and an unposted key keeps its value.

The struct owns writes only. A resource's default table and detail page read the model, through
`#[derive(ModelColumns)]` on it: a column per field a person reads, whoever writes it, each picked
by the field's type. A form leaves out what Toasty writes, such as a `#[default]` timestamp, and
a list-only resource has no form at all; the model still has both. Every resource's model implements
`ModelColumns`, so a missing derive fails to compile instead of mounting an empty list. The orphan
rule leaves the impl to the model's crate: Tablo's `AdminUser` derives it, its hash left out.

A model whose form is its own fields derives `ModelForm`, which writes that struct and expands it
as `RecordForm` does. The struct takes the fields a person fills: it leaves out a key, a field
Toasty writes, a relation and the tenant column, and takes a defaulted field or a foreign key only
when a form key asks for it. A form whose fields differ from the model's stays its own struct.

The struct owns presence: a posted-empty field stores its blank answer, and a field with none is
required, whatever the layout says. Its parse is the only presence and type check; control
rules, the unique probe and `validate_record` add to one list of errors rendered inline. Mounting
checks that a create sets every non-nullable column.

The struct also owns the controls. The derive gives each field one, typed by the form, and a
resource's form is a `Schema<Form>` that only those controls fill, so every control posts a
struct key by construction. The form arranges controls rather than declares them: a field it
does not place renders after the ones it does, in declaration order.

An embedded value derives `EmbeddedForm` and binds as one field (`#[form(embed)]`). For an enum,
the variant is the discriminant column, chosen with a `Select`; a named discriminant always
wins. A `#[document]`, a relation, an `Option` of a nested value and an enum inside a variant are
not supported inside an embedded value.

A `#[document]` list of embedded structs binds as one field too (`#[form(repeat)]`), its item
deriving `RepeaterItem`. Each row posts its item's keys under `{key}.{row}.`, and the repeater's key
lists the rows in the order they show. Reading the submission through the form's schema gathers
them into that key's `FormValue::Rows` before anything else reads it, so completing an edit,
hiding a field and parsing see one key like any other. A row keeps its number when it moves, so
the browser renumbers nothing.

A many-to-many field writes no column of its own. It is named like the model's
`#[has_many(via = joins.target)]` field and typed as a `Vec` of the related keys, so the derive
checks both against the model and the related resource. Its control posts its key once per
checked box, which reading the submission gathers into one `FormValue::List` as it does a
repeater's rows. The write links and unlinks records as join rows built from the app schema, since
Toasty writes through no multi-step `via` and the join model's type is not the field's.

## Rejected

- Folding a repeater's rows and a multiple choice's values into their key as a JSON string: every
  reader decoded it again, and each handled a decoding failure no browser causes.
- Deriving the default table and detail page from the struct: each resource added its read-only
  columns by hand, and a list-only resource had none to derive from.
- Building the default columns at mount from the app schema: Toasty loads a record into its struct
  and reads no field back by name, so a column needs the typed reader a derive writes.
- A `ModelColumns` bound only where a default is used: Rust picks no impl by whether a bound
  holds, so an optional derive needs a second opt-in on each resource.
- `ModelForm` implying `ModelColumns`: a model with a hand-written record form needs the columns
  too, and an impl that `ModelForm` wrote could not be replaced by hand.
- Leaving out the fields a form skips: it couples the read views to the writes again, and a
  field the app writes is as often one a person reads. A secret is marked `#[columns(skip)]`.
- Presence on the control (`.required()`): a second declaration of what the blank answers say.
- A schema-side parse: the struct parses every key anyway.
- An untyped form schema checked at mount: a control no field binds and a field with no control
  were startup errors for what the type and the struct already know.
- Refusing a field the form does not place: the struct already says the field is written, so
  rendering its control beats refusing to start. The cost is that a field added to a sectioned
  layout renders after its last block until it is placed.
- Adding and removing a repeater's rows by posting the form and rendering it again: a round trip
  per row, and the page loses its scroll and focus.
- Renumbering a repeater's row keys after each move or removal: more script for what the order key
  already says.
- Naming the join model in a many-to-many field's attribute, so the write could use its typed
  create builder and its `#[default]`s: a second declaration of what the `via` field says. A join
  model with a column a link cannot fill is refused at mount instead.
- Replacing every link of an edited record: it rewrites the rows it keeps, and drops any column
  they hold.
- A `<select multiple>`: a click without a modifier key drops every other choice.
- One derive for a model and a form struct, told apart by `#[form(model = ..)]`: a form struct
  without it would become the form of a model it is not.
- Inferring a foreign key's source from its `#[belongs_to]`: the source is a resource, and several
  resources can share a model.
- Leaving out every `#[default]` field: a repeater writes a defaulted `#[document]` list.
