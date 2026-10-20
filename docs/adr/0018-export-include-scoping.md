# 0018 Relations are includes declared where they are read

There is no `Relation` trait. A column declares the relations its projection reads as typed
Toasty includes (`Column::includes`), and the list, the export and the detail page load exactly
what their columns declare; the detail page's `Detail` holds the same `Column` types a `Table`
lists. `RelationColumn` and `CountColumn` take a `relation!` lens, which pairs a relation field's
include with its reader, so the include they declare is the relation they read; a
`ComputedColumn` declares what its closure reads with `include`. The sealed `ToOneRelation` trait
tells a required `Deferred` field from a nullable one and names the related model: it names no
relation and links no column to a resource, so it is not the `Relation` trait rejected below.

A relation column names the resource `R` whose rules govern the related records:
`RelationColumn::of::<R>`, `new::<R>`, `list::<R>` and `CountColumn::of::<R>`. The include still
loads the relation, and the loader that applied it then runs `Column::restrict`: one query through
`R`'s tenant-scoped query, keyed by the loaded records, and `R`'s `View` policy on each, which
records for the request whether `R` shows each record. A cell renders only the records recorded as
shown, so a hidden to-one record renders empty, and so does a record no loader checked. The rows
keep what the include loaded: a policy, a title or a computed column reading the same relation
sees it whole, and a required relation never reads as unloaded. The include itself stays
unfiltered: Toasty 0.11 panics (`todo!`) on a filtered include over a `via` relation, and a
required `belongs_to` has no shape for a filtered-out record. `unscoped` columns, for a model no resource
administers, show what the include loads. `Resource::query` stays row scoping only; a relation
belongs there only when every loader reads it, such as one the policy checks. An unscoped relation
column whose row was loaded without its include renders `"(unloaded)"` and fails a debug
assertion.

A related table on a detail page is the related resource's own table, run through its own loader.

## Rejected

- A `Relation` trait or a vocabulary of relation names between columns and resources.
- Loading every relation in `query`: each page pays for what another reads.
- A detail-page query the resource writes by hand: what the page reads and what it loads are
  declared in two places, and drift apart.
- Relation columns that show what the include loads, under no resource's rules: a soft-deleted,
  hidden or other tenant's record showed wherever another resource pointed at it.
- Dropping the hidden records from the loaded rows: the parent's policy, its title and its row
  actions then read a relation the column rewrote, so a policy over a nullable relation allowed a
  record it would refuse, and a required one read as unloaded and panicked.
- A relation column over a bare relation path: a path loads the relation but cannot read it
  off the row, so the column would need a reader naming the field a second time.
