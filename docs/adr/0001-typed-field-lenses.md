# 0001 Typed field lenses, not string state paths

Every Schema field and every column, in a Table or on a detail page, binds through a typed Toasty
field lens, never a string path, so a renamed or retyped field fails to compile. `lens!(User.name)`
writes the query path and the value reader from one field name, so the two cannot name different
fields. A row's key is its record's primary key; no table, relation or select declares a key
closure.

A closed set of values is a unit enum field deriving `toasty::Embed` and `Options`, so a choice
field, a select filter, a column and a group header read its options and labels from the type, and
a query compares a variant rather than its spelling.

Form transport stays string-keyed; a record form (ADR-0022) parses it into a typed struct. A
detail page reads the typed record, never a map of display strings; only an embedded value's
leaves are spelled through its form schema, as its form spells them.

## Rejected

- String state paths: a typo or a rename fails at run time, and the path carries no type.
- A `String` column beside an `Options` list: a query or a comparison can name a value no option
  has, and every column and filter restates the list.
