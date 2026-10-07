# 0002 `Resource::query` scopes rows; the framework owns tenancy

`Resource::query(cx)` is a resource's own row scoping (soft deletes, row visibility), and every
list, form and record loader reads through it. Tenancy is declared on the `ResourceDef`
(`Tenancy::column` or `Tenancy::via`), and `scoped_query` ANDs the tenant predicate after
`query`, so no override can drop it. App code loading rows outside the framework's loaders calls
`scoped_query`, not `query`. A resource serving several tenants declares no tenancy and scopes in
`query`, giving up the 403 gate.

The tenant column is typed `TenantId` (or `Option<TenantId>`), and a tenancy lens must name a
field implementing `TenantColumn`. The type is the only thing that tells a tenant column apart
from any other UUID column, so a lens to an unmarked column does not compile.

## Rejected

- A `#[tenant]` field attribute: `toasty::Model` derives only its own attributes.
- Refusing a lens by shape (the key, a relation endpoint, a name): it guesses at intent.
- Declaring the tenant column on the resource: a second declaration can disagree with the model.
