# 0018 Relations are includes declared where they are read

There is no `Relation` trait. A column declares the relations its projection reads as typed
Toasty includes (`Column::includes`), and the list, the export and the detail page load exactly
what their columns declare; the detail page's `Detail` holds the same `Column` types a `Table`
lists. `RelationColumn` takes a `relation!` lens, which pairs a relation field's include with its
reader, so the include it declares is the relation it reads; a
`ComputedColumn` declares what its closure reads with `include`. The sealed `ToOneRelation` trait
tells a required `Deferred` field from a nullable one and names the related model: it names no
relation and links no column to a resource, so it is not the `Relation` trait rejected below.

`CountColumn` declares no include: once the rows load, its `Column::restrict` counts each row's
related records with one count query through `R`'s scoped query, filtered to the row's records,
and records the counts for the request. The page loads none of the records it counts.

A relation column names the resource `R` whose rules govern the related records:
`RelationColumn::of::<R>`, `new::<R>`, `list::<R>` and `CountColumn::of::<R>`. A
`RelationColumn`'s include still loads the relation, and the loader that applied it then runs `Column::restrict`: one query through
`R`'s scoped query (its tenancy, its trash and `ResourceDef::query`), keyed by the loaded records,
which records for the request whether `R` shows each record. `R`'s `View` policy plays no part:
it answers whether the user may open a record's pages, never whether a row shows. A cell renders only the records recorded as
shown, so a hidden to-one record renders empty, and so does a record no loader checked. The rows
keep what the include loaded: a policy, a title or a computed column reading the same relation
sees it whole, and a required relation never reads as unloaded. The include itself stays
unfiltered: Toasty 0.11 panics (`todo!`) on a filtered include over a `via` relation, and a
required `belongs_to` has no shape for a filtered-out record. `unscoped` columns, for a model no resource
administers, show what the include loads. `ResourceDef::query` stays row scoping only; a relation
every loader reads, such as one the policy checks, is declared with
`ResourceDef::include_unscoped`.

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
- A `CountColumn` counting the included list: every page loaded every related record of its
  rows, then sent their keys back through `R`'s scoped query, to show a number.
- One grouped count per page for `CountColumn`: Toasty 0.11 builds no `GROUP BY`, so a count
  takes one query per row, bounded by the page size.
- A relation column over a bare relation path: a path loads the relation but cannot read it
  off the row, so the column would need a reader naming the field a second time.
