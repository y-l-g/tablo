# Relation includes: columns declare them, each loader loads what it reads

Date: 2026-09-22 — Status: accepted

## Decision

**1. A column declares the relations its projection reads, as typed paths.**
`ComputedColumn::include(Post::fields().author())` takes a Toasty include over the table model, so a
missing relation rejects at compile time. Repeat calls accumulate; a column reading no relation
declares nothing.

**2. List and export load exactly what their columns declare.** `TablePage::load` and the export
apply the table declarations to the tenant-scoped `scoped_query` themselves, once each. No name
vocabulary sits between column and resource.

**3. `Resource::query` is row scoping only.** Soft deletes and row visibility live there; every
loader starts from it. A relation every loader reads belongs in `query`: one the policy's `View`
reads, or one a `group_by` or row key reads.

**4. The detail page loads `Resource::view_query`.** Relation tables run the related resource's
own loader, so `view_query`, defaulting to `query`, carries only what `view_values`
and `view_content` read off the record. The framework ANDs the tenant scope onto it as onto
`query` (ADR-0002).

**5. The unloaded guard checks a missing declaration.** A column reading an undeclared relation
renders against an unloaded `Deferred`; `is_unloaded` renders `"(unloaded)"` with a
`debug_assert!`.
