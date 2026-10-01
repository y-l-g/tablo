# Resource::query is the resource's own row-scoping seam

Date: 2026-08-19 — Status: accepted — Amended: 2026-09-22, 2026-10-01

## Decision

`Resource::query(cx) -> Query<List<Model>>` is the one overridable seam for a resource's **own** row
scoping: soft deletes, row-level visibility, and the includes a page loads. Every list, form, record
and shard loader reaches its rows through it, and tenancy enters as `cx.with(Tenant(id))` on the way
in, not as a global scope a caller must remember to remove. The tenant filter itself is not stated
there: the framework owns that half.

A resource declares its tenancy in `Resource::tenancy() -> Tenancy<Model>`: `Tenancy::none()` (the
default), `Tenancy::column(lens)` for a tenant UUID column of the model's own, or
`Tenancy::via(lens)` for a tenant reached through a relation — the showcase's comments, which belong
to a post that carries one, declare `Tenancy::via(Comment::fields().post().tenant_id())`. The column
is named by its lens, never found by its name. For a scoped resource every loader — list,
edit/delete load, bulk fetch, unique pre-check, export, and the three relationship option loaders —
runs `scoped_query::<R>(cx)`, which ANDs `<lens> = <request tenant>` onto whatever `query`
returned; the detail page's `view_query` gets the same predicate. `scoped_query` is a free function
applied *after* the resource's own override on purpose: no override can drop the tenant half by
accident, and there is deliberately no override that removes the predicate.

The gate is inseparable from the scope: a scoped resource answers 403 to a request with no tenant in
every handler, and `scoped_query` answers the same 403. A scoped tenancy always yields a predicate,
so the one declaration error left is a `Tenancy::column` lens that is not a single field of the
model; mounting the panel refuses it through `check_resource` with an error naming the resource and
`Tenancy::via`. A column tenancy also tells the create which field to stamp with the request's
tenant, so the record form must not claim it.

A resource that must serve more than one tenant declares no tenancy and scopes in `query` by hand —
the one explicit, visible way to be tenant-unscoped, stated in the resource body where a reviewer
sees it, and it gives up the 403 gate with the filter. App code that loads rows outside the
framework's loaders must call `scoped_query` too: `query` on a scoped resource is the
tenant-unscoped base by design, so the tenant-unscoped case is visible at the call site rather than
implied by an omission.

`schema` does not depend on `resource`: the option loaders are generic over `schema::OptionSource`,
whose `scoped_query` is a **required** method, and `resource` supplies the blanket impl every
`Resource` gets. For a direct `OptionSource` implementor the rule is convention rather than
structure — nothing stops that body writing `Ok(Query::all())` beside `requires_tenant() == true`,
and the compiler will not notice. `Resource` remains the only shape whose scope the framework
applies for you; an app that wants that guarantee implements `Resource`, not `OptionSource`.
