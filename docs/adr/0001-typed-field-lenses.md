# 0001 Typed field lenses, not string state paths

Every Schema field and every column, in a Table or on a detail page, binds through a typed Toasty
field lens, never a string path, so a renamed or retyped field fails to compile. `lens!(User.name)`
writes the query path and the value reader from one field name, so the two cannot name different
fields. A row's key is its record's primary key; no table, relation or select declares a key
closure.

Form transport stays string-keyed; a record form (ADR-0022) parses it into a typed struct. A
detail page reads the typed record, never a map of display strings.

## Rejected

- String state paths: a typo or a rename fails at run time, and the path carries no type.
