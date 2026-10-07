# 0018 Relations are includes declared where they are read

There is no `Relation` trait. A column declares the relations its projection reads as typed
Toasty includes (`Column::includes`), and the list, the export and the detail page load exactly
what their columns declare; the detail page's `Detail` holds the same `Column` types a `Table`
lists. `RelationColumn` and `CountColumn` take a `relation!` lens, which pairs a relation field's
include with its reader, so the include they declare is the relation they read; a
`ComputedColumn` declares what its closure reads with `include`. `Resource::query` stays row
scoping only; a relation belongs there only when every loader reads it, such as one the policy
checks. A relation column whose row was loaded without its include renders `"(unloaded)"` and
fails a debug assertion.

A related table on a detail page is the related resource's own table, run through its own loader.

## Rejected

- A `Relation` trait or a vocabulary of relation names between columns and resources.
- Loading every relation in `query`: each page pays for what another reads.
- A detail-page query the resource writes by hand: what the page reads and what it loads are
  declared in two places, and drift apart.
- A relation column taking the bare relation path: a path loads the relation but cannot read it
  off the row, so the column would need a reader naming the field a second time.
