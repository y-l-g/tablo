# 0022 Forms write through a derived typed struct

A form resource declares a `#[derive(RecordForm)]` struct with one field per written column,
named and typed like the model, so a renamed or retyped column fails to compile. The derive also
provides the default form schema, table and detail page. An edit fills every key the submission omits from the
stored record, so `update_record` receives a whole form and an unposted key keeps its value.

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
lists the rows in the order they show. The submission folds them into that key, as a JSON array of
each row's keys, before anything reads it, so completing an edit, hiding a field and parsing see
one key like any other. A row keeps its number when it moves, so the browser renumbers nothing.

A many-to-many field writes no column of its own. It is named like the model's
`#[has_many(via = joins.target)]` field and typed as a `Vec` of the related keys, so the derive
checks both against the model and the related resource. Its control posts its key once per
checked box, which the submission folds into a JSON array as it does a repeater's rows. The
write links and unlinks records as join rows built from the app schema, since Toasty writes
through no multi-step `via` and the join model's type is not the field's.

## Rejected

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
