# 0031 A CSV import is the create form, once per row, in one transaction

A resource opts in with `ResourceDef::importable`, and only one that serves a create page may:
mounting refuses an importable resource that is `edit_only` or has no record form. Its list then
links **Import CSV** to `{list}/-/import`, offered to a request that may create, and the page takes
one CSV file.

Each row is a create submission. The header names the form keys the row posts, matched by key or
by field label, and each cell posts its key's value; a column the header leaves out takes the
field's `default`, as the create page's untouched control posts it. The row then runs the create
form's own pipeline: the field-access mask, disabled and hidden controls, the controls' rules, the
record form's parse, `validate_record`, the unique probe, the relationship re-check, and
`create_record`, which stamps the tenant column. Nothing about a row is checked that a create
form's submission is not, and nothing a create form's submission is checked for is skipped.

Every row first runs the steps a create runs before its transaction opens, so that no option
loader waits on the connection the transaction holds. One transaction then runs each row's unique
probe, relationship re-check and write in file order, so a row's unique value is checked against
the rows before it. Once a row is refused nothing more is written and the transaction rolls back,
but every later row is still checked: the page lists each refusal by row and column, and the file
is corrected and posted again. Each written row is a `Change::Create`: `before_commit` receives it
inside the transaction, where an error rolls back the whole file, and `after_commit` once the rows
commit together, in file order. A file holds at most `MAX_IMPORT_ROWS` rows, so the transaction
stays one request long.

A header that names a key the form does not take, a key the request may not change, a control
disabled on the create page, a file field or a repeater refuses the whole file before any row is
read: no cell can carry a file's bytes or a list of rows. A multiple choice's column repeats once
per value, as its control posts its key once per checked box; no other column repeats.

## Rejected

- An importer schema of its own, as Filament's `Importer` declares: columns, casts and rules the
  form already states, declared again, and drifting from it.
- Writing the rows that pass and reporting the rest: a file half imported is re-posted whole, and
  the rows it wrote the first time are created twice. A downloadable file of refused rows, its
  stored state and its access rule exist only to undo that.
- Background jobs over chunks of rows: Tablo has no queue, and a chunk's write outlives the
  request that checked its policy and tenant.
- Updating the records a key column matches: it needs a declared match key, and an `Update` and
  a `View` check on each matched record, which no create form performs.
- Reading the export's spelling back: the export writes what a reader sees, such as `$1,234.50`,
  `Yes` and an option's label; the import reads what the form posts.
- An import on every resource that creates: a bulk write path, and a button on every list, that
  the app never asked for.
- A column-mapping step: the header already names the keys, and the template link writes them.
- A `Change` of its own holding every imported row: a hook that audits creates would need a second
  arm for the same write, and one that misses it would skip the rows an import creates.
