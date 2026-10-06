# Resource::query is the resource's own row-scoping seam

Date: 2026-08-19 — Status: accepted

## Decision

`Resource::query(cx)` scopes a resource's own rows: soft deletes and row visibility. Every list,
form, and record loader reads through it. Tenancy enters as `cx.with(Tenant(id))`, not as
a global scope.

`ResourceDef::tenancy` declares ownership: `Tenancy::none()` (default), `Tenancy::column(lens)`,
or `Tenancy::via(lens)`. The lens names the column. `scoped_query` applies `<lens> = <request
tenant>` after the `query` override, so no override drops the tenant predicate and no override
removes it. `view_query` receives the same predicate. A scoped resource answers 403 with no
tenant; mounting refuses a `Tenancy::column` lens outside the model with an error naming the
resource and `Tenancy::via`. A column tenancy stamps the tenant on create, so the form omits
that field. A `via` tenancy stamps nothing, so mounting requires the form to write the foreign key
of the `belongs_to` its lens starts at through a relationship field over a tenant-scoped resource,
whose key the write re-checks.

A resource serving more than one tenant declares no tenancy and scopes in `query`, giving up the
403 gate with the filter. App code loading rows outside framework loaders calls `scoped_query`:
`query` on a scoped resource is tenant-unscoped.

`schema::OptionSource::scoped_query` is required; `Resource` provides the blanket impl. A direct
`OptionSource` implementor upholds the tenant predicate by convention.
