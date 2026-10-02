# Detail pages: `Resource::view` and one Schema, read two ways

Date: 2026-09-21 — Status: accepted

## Decision

**One Schema, rendered read-only.** `Resource::view(dx: &DeclCx)` defaults to `Schema::empty()`;
a resource declaring a view links a `View` row action and serves the page. The declaration carries
the app schema alone; the panel calls it once at build and serves the cached schema. Rendering is
`Schema::render(cx, Source::view(values))` with `Mode::View` on `RenderSource`; each field binds
`Field::text`, `Field::choice`, or `Field::file` and renders label plus stored value, while
`Grid`, `Section`, and `Group` keep their structure. `RenderSource::errors_for` returns nothing in
`Mode::View`. An embedded enum renders its stored variant's group and the shared columns that
variant declares.

**Existence is derived.** Whether the page exists is the cached declarations' `viewed()`, a
non-empty view schema. `Panel::resource` registers the detail route unconditionally; the handler
404s a resource with no view, the same answer as an unknown ID.

**Routes keep precedence.** Topcoat routes through `matchit`: static segments outrank parameters
and longer paths outrank shorter prefixes, so `/create` and `/{id}/edit` still resolve.

**Loading uses the one query seam.** The GET loads `Resource::view_query` under the same tenant
scope and PK filter as the edit GET (ADR-0002, ADR-0018), so tenancy and the 404 for unknown or
out-of-scope IDs come from the seam. A refused `View` is a 403. A view field whose key the values
lack renders `(missing)` and fails a `debug_assert!`.

**Relations are the related list table.** `Resource::relations() -> Vec<Relation<Self::Model>>`
declares each `Relation::has_many` by related resource and foreign key. The panel renders each on
detail and edit pages as the related resource's own `Table` through its scoped query narrowed to
the owner. Detail relations keep only the View link; edit relations carry writes and a create link
seeding the foreign key. URL parameters prefix with the related slug; writes carry `?return=`
followed only under the panel prefix. Free-form record content renders through `view_content`.

Values come from `RecordForm::hydrate`, extended with `Resource::view_values` for view-only keys;
the form's keys win. The tuple limit on `IntoSchema`, `IntoColumns`, and `IntoFilters` is eight;
more top-level blocks wrap in a `Group`.
