# Relation includes: columns declare them, each loader loads what it reads

Date: 2026-09-22 — Status: accepted — Amended: 2026-09-25, 2026-09-30

## Decision

**1. A column declares the relations its projection reads, as typed paths.**
`TextColumn::include(Post::fields().author())` takes a Toasty include over the table's model, so a
relation the model does not have is a compile error. The declaration sits on the column because the
closure that reads the relation is the thing that needs it: it travels with the projection when a
column is moved or copied. Repeat calls accumulate, and a column that reads no relation declares
nothing. This is Filament's model, where a column's relationship is eager-loaded for the table.

**2. The list and the export load exactly the relations their columns declare.** The loader
(`TablePage::load`) and the export apply the table's declarations to the tenant-scoped
`scoped_query` themselves, once each. The resource maps nothing: there is no name vocabulary between
the column and the resource.

**3. `Resource::query` is row scoping only.** Soft deletes and row-level visibility live there, and
every loader starts from it: the edit page, delete, bulk delete, the unique-value probe, the
relationship option lists and their targeted existence check, and the pagination probes read only the
record's own columns. A relation `can_view` reads is the one include that belongs in `query`, because
every loader runs that predicate.

**4. The detail page loads `Resource::view_query`.** `view_relations` is an opaque hook the framework
cannot inspect, so the resource states the detail page's includes on `view_query`, which defaults to
`query`. The framework ANDs the tenant scope onto it as it does onto `query` (ADR-0002).

**5. The unloaded-relation guard is the check for a missing declaration.** A column that reads a
relation it did not declare renders against an unloaded `Deferred`, and its `is_unloaded` guard
(ADR-0011: `"(unloaded)"` plus a `debug_assert!`) fails loudly in test builds.

## Consequences

- A wrong relation name cannot compile, and a resource no longer writes a helper that matches names
  back to includes.
- The export's visibility scan renders no cell, so it loads none of the columns' relations; only the
  streaming pass does. `export_visibility_scan_loads_no_includes` and
  `export_loads_the_relations_its_columns_include` pin both halves with a real relation.
- An option load renders a value and a label per row through opaque closures, so it loads no relation:
  an option label projects the related record's own columns, and one that reads a relation panics in
  `Deferred::get`.
- An export column subset stays out of scope: this decides which relations the rendered columns load,
  not which columns render.
