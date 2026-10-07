# 0018 Relations are includes declared where they are read

There is no `Relation` trait. A column declares the relations its projection reads as typed
Toasty includes (`ComputedColumn::include`), and the list, the export and the detail page load
exactly what their columns declare; the detail page's `Detail` holds the same `Column` types a
`Table` lists. `Resource::query` stays row scoping only; a relation belongs there only when every
loader reads it, such as one the policy checks. A column reading an undeclared relation renders
`"(unloaded)"` and fails a debug assertion.

A related table on a detail page is the related resource's own table, run through its own loader.

## Rejected

- A `Relation` trait or a vocabulary of relation names between columns and resources.
- Loading every relation in `query`: each page pays for what another reads.
- A detail-page query the resource writes by hand: what the page reads and what it loads are
  declared in two places, and drift apart.
