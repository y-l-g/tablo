# Relations: no new Relation trait, includes declared where they are read

Date: 2026-08-31 — Status: accepted

## Decision

Relations ride the resource query seam; there is no top-level `Relation` trait. A page that
reads a relation declares a typed `include` where it reads it (ADR-0018): a list column with
`ComputedColumn::include`, the detail page on `Resource::view_query`. `via` many-to-many stays
SQL-only, out of scope for v1 tables.

Relationship selects, option policy and tenancy, and option search behave as their rustdoc and the
`forms.md` guide chapter describe. Row identity needs no declaration: each row is keyed by its
record's primary key.
