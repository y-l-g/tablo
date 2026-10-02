# Relations: no new Relation trait, includes declared where they are read

Date: 2026-08-31 — Status: accepted

## Decision

Relations ride the resource query seam; there is no top-level `Relation` trait. `Table` and
`Schema` write no `#[shard]` or relation query. `via` many-to-many stays SQL-only, out of scope
for v1 tables.

**Loader.** A page needing a relation declares a typed `include` where it reads it: a list column
with `TextColumn::include(Post::fields().author())`, the detail page on `Resource::view_query`
(ADR-0018). A relation-cell list is one round trip (`NestedMerge` plus correlated subquery). A
cell reading an unloaded `Deferred` renders `"(unloaded)"` with a `debug_assert!`.

**Table.** Relation cells are `TextColumn::computed("Author", |p: &Post| ...)`; `searchable` and
`sortable` apply only to local columns.

**Schema.** `Field::choice(Post::fields().author_id()).relationship(..)` takes a typed
primary-key projection whose `Display` becomes `<option value>`; a wrong projection rejects at
compile time where the type differs from the PK. The related PK is a single primitive `Display`
type. An edit form hydrates the FK with the projection's canonical string.

**Policy.** Option loads respect the related policy: refused `ViewAny` denies the load and the
field reports `{label} is not available`, while `View` filters rows before labels render. The cap
counts the raw bounded fetch before filtering.

**Tenancy.** Option loaders take `schema::OptionSource::scoped_query`; `option_query` runs
`R::scoped_query(cx)`, carrying the framework tenant predicate. `relationship::<R>(value, label)`
names the source by type parameter alone.

**Option search.** Above the cap the failure splits into `Overflow`: a searchable select degrades
to type-to-search, a non-searchable one keeps the retry error. Search reuses the related
`Table`'s `searchable()` columns through `search_expr(q)`. The endpoint is `GET
{parent_list_url}/options?field=&q=`, allow-listed to a declared searchable relationship choice,
bounded at `limit(201)`, never a whole-table load. Validation for an overflowed searchable select
is a targeted `pk_eq_expr` plus `scoped_query` plus `View` check; bounded sets keep membership
validation. The UI is a native `<select>` plus `selects.js`; without JavaScript the select works.

**Row identity.** `Table::new(key, columns)` declares row and record key together from the typed
PK projection; `Table::new_split(display, record, columns)` splits them for non-PK display. Single
and bulk deletes require `View` plus `Delete`.
