# 0031 A CSV import is the create form, once per row, in one transaction

A resource that serves a create page opts in with `ResourceDef::importable`. Each row of the file
is a create submission: the header names the form keys, and the row runs the create form's own
pipeline, so nothing a create is checked for is skipped and the import adds only refusals of the
file's own shape. A header that names a column no create-form control can take, such as a file
field or a repeater, refuses the whole file before any row is checked.

Every row first runs the steps a create runs before its transaction opens, so no option loader
waits on the connection the transaction holds. One transaction then writes the rows in file
order, so a row's unique value is checked against the rows before it, and the whole file commits
or none of it. A refused row does not stop the check: every later row is still checked and
written into the doomed transaction, so the page lists every refusal at once. Each row's
`create_record` runs in a savepoint, so a row it refuses leaves no partial write for the rows
after it. A file holds at most `Limits::import_rows` rows, so the transaction stays one request long.

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
