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

A model deriving `PanelModel` also gets `columns()`, the read-side twin of a record form's
`controls()`: a struct with one `FieldColumn` per field its default table or detail page shows,
named like the field. A `FieldColumn` is neither a bare lens nor one fixed column type. Placed as
it is, in a table or on a detail page, it is the column the derive picks for the field's type
there, with the field's `#[columns(..)]` keys applied, so `Product::columns().qty` in a table is
the same column the default table lists. Asked for a kind, `.text()`, `.date()`, `.toggle()`,
`.select()` and the rest, it is that column type's plain builder over the field, labelled by its
`label` key; each kind compiles only for the field types its column reads, so a toggle over a
`String` fails to compile at the call. `lens()` hands the lens over for a filter, a tab or a field
access rule. `lens!` stays the low-level tool: a path into an embedded value, a model without the
derive, a custom column.

The derive picks a field's column by autoref specialization over its type, which works only in
an expression, so no struct field can name the picked type. The handle keeps the pick as a
function per place instead, and a table and a detail page each call their own: a timestamp is a
sortable day in a table and the stored instant on a detail page. A table's tuple therefore takes
anything `IntoColumns`, as a detail page's takes anything `IntoDetail`.

A form's field enum (`ProductFormField`) is transport: the key a value posts and an error
renders under, which an app's code never spells. A column never reads it, and an error is keyed
by its form, `FieldErrors<ProductForm>` and `WriteError<ProductForm>`, through the form's
`FormFields`: the enum, and a keys struct the derive writes beside it with one field per form
field. An error picks its field off those keys by name, `errors.add(|f| f.qty, ..)`, as
`posted.named(|f| f.qty)` and `WriteError::<ProductForm>::field(|f| f.qty, ..)` do, so a renamed
field fails to compile and no signature names the enum. The closure's argument is inferred from
the form type, so a `FieldErrors::new()` or a `WriteError::field` whose form only the return type
gives names it, `FieldErrors::<ProductForm>::new()`. An action with no input is keyed by `()`. The panel's
own errors stay keyed by the form key, `FieldErrors<String>`, and a test client posts by the
enum, the transport it drives.

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
  value's form needed the request's `Db` to hydrate and parse.
- Re-deriving Toasty's column naming in Tablo: the compiled mapping names every shape (flattened
  structs, enum payloads, shared columns, documents) in one place, and a copy of the rule drifts.
- A `String` column beside an `Options` list: a query or a comparison can name a value no option
  has, and every column and filter restates the list.
- `columns().qty` as a lens: every call site still names the column type and repeats what the
  derive already picked, `#[columns(..)]` keys included, so the default table and a declared one
  drift.
- `columns().qty` as the picked column type itself: the pick is autoref specialization, which a
  struct field cannot name, and a type-level pick needs an impl per value type that the orphan
  rule refuses for an app's `Option<MyEnum>` and `Vec<MyItem>`. It would also hand a table and a
  detail page the same column, where they pick differently.
- Picking by the field's spelled type in the derive: an `Options` enum and an embedded struct are
  both a bare path, and an alias hides a date.
- Keying a refusal by the field enum (`FieldErrors<ProductFormField>`,
  `errors.add(ProductFormField::Qty, ..)`): every signature and call site spells a transport type
  the derive names, not the form the app declared.
- A key accessor beside the enum (`ProductForm::keys().qty`): it still names the form at every
  call, where the closure takes it from the type the error is already keyed by.
