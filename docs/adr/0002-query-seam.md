# 0002 `Resource::query` scopes rows; the framework owns tenancy

`Resource::query(cx)` is a resource's own row scoping (soft deletes, row visibility), and every
list, form and record loader reads through it. Tenancy is declared on the `ResourceDef`
(`Tenancy::column` or `Tenancy::via`), and `scoped_query` ANDs the tenant predicate after
`query`, so no override can drop it. App code loading rows outside the framework's loaders calls
`scoped_query`, not `query`. A resource serving several tenants declares `Tenancy::none()` and
scopes in `query`, giving up the 403 when a request has no tenant.

A resource that declares no tenancy over a model with a `TenantId` field does not mount: it would
serve every tenant's rows, and forgetting `.tenancy(..)` is the likelier cause than intent. The
`ModelColumns` derive lists the model's `TenantId` fields, so the check needs no declaration of its
own; `Tenancy::none()` states the intent.

The tenant column is typed `TenantId` (or `Option<TenantId>`), and a tenancy lens must name a
field implementing `TenantColumn`. The type is the only thing that tells a tenant column apart
from any other UUID column, so a lens to an unmarked column does not compile.

## Rejected

- A `#[tenant]` field attribute: `toasty::Model` derives only its own attributes.
- Refusing a lens by shape (the key, a relation endpoint, a name): it guesses at intent.
- Declaring the tenant column on the resource: a second declaration can disagree with the model.
- An opt-in `TenantOwned` marker trait on the model to trigger the undeclared-tenancy refusal: an
  app that forgets `.tenancy(..)` forgets the marker as easily, while the derive already sees the
  field's type.
- Refusing every resource that declares no tenancy: most models have no tenant column, and each
  of their resources would restate `Tenancy::none()`.
