# Typed field lenses, not string state paths

Date: 2026-08-19 — Status: accepted — Amended: 2026-09-10, 2026-09-28

## Decision

Every Schema field and Table column binds through a typed Toasty field lens
(`User::fields().email()`), never a string `statePath`. The lens carries nullability, uniqueness,
column renames, and type, so hydration (Model → Schema) and dehydration (Schema → Create/Update)
are compile-time checked. Filament's `"data.author.name"` string paths, and its `data_set`/`data_get`
runtime, have no place in Rust.

`required` defaults from lens nullability (GH #100, GH #147), and a single-segment `String` lens
carries uniqueness too, read from the model's index list rather than from the field
(`lens_field_unique`; composite indexes included, which is what `#[unique(tenant_id, email)]` needs
— GH #183, GH #189). Metadata for the other field kinds, storage names, and instance→field
extraction remain upstream gaps — form values stay string-keyed (`HashMap<String, String>`) at the
value level, so the lens proves field existence, not typed data flow. See upstream issues #115
(metadata) and #119 (instance→field extraction).

## Amendment — 2026-09-28

**Record fns receive typed values.** The form transport stays string-keyed, and a record form
(ADR-0022) parses it into a struct whose fields are bound by ident to the model's fields, so the
write is typed at compile time. Upstream issues #115 and #119 stand: an embedded leaf's key is still
resolved at run time.
