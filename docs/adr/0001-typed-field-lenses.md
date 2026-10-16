# 0001 Typed field lenses, not string state paths

Every Schema field and every column, in a Table or on a detail page, binds through a typed Toasty
field lens, never a string path, so a renamed or retyped field fails to compile. `lens!(User.name)`
writes the query path and the value reader from one field name, so the two cannot name different
fields. A row's key is its record's primary key; no table, relation or select declares a key
closure.

A lens binds when its declaration is built. A path into an embedded value binds the flattened
column Toasty names for it, read off the schema Toasty compiles for the path's model and every
model it reaches, without a connection: a column's name depends on the models alone, never on the
driver or the table-name prefix, so it is the column the app's own `Db` stores the value in. A
table, a schema or a detail page built anywhere renders as declared, with no step binding it to a
database first.

A closed set of values is a unit enum field deriving `toasty::Embed` and `Options`, so a choice
field, a select filter, a column and a group header read its options and labels from the type, and
a query compares a variant rather than its spelling.

A resource's form schema carries its record form's type (ADR-0022), as a table and a detail page
carry their model's, so a control from another form fails to compile.

Form transport stays string-keyed; a record form (ADR-0022) parses it into a typed struct. A
detail page reads the typed record, never a map of display strings; only an embedded value's
leaves are spelled through its form schema, as its form spells them.

## Rejected

- String state paths: a typo or a rename fails at run time, and the path carries no type.
- Declaring first and binding to the app's `Db` schema at mount: every declaration carried an
  unbound state until then, a public `bind` step and a resolver threaded through record forms and
  embedded values, a custom page that forgot the step rendered declaration errors, and an embedded
  value's form rebuilt its keys from the request's `Db` on every hydrate and parse.
- Re-deriving Toasty's column naming in Tablo: the compiled mapping names every shape (flattened
  structs, enum payloads, shared columns, documents) in one place, and a copy of the rule drifts.
- A `String` column beside an `Options` list: a query or a comparison can name a value no option
  has, and every column and filter restates the list.
