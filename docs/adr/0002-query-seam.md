# 0002 `ResourceDef::query` scopes rows per panel; the framework owns tenancy

`ResourceDef::query` is a resource's own row scoping (row visibility), and every list, form and
record loader reads through it. It is a setting of the def rather than a method of the
`Resource` trait, so each panel narrows the rows it serves (ADR-0023): a portal panel lists an
author's own posts where an admin panel lists every post, from one resource type. Each call
receives the query the calls before it return, so `Panel::resource_with` narrows what
`Resource::declare` narrows. Soft deletes are declared on the `ResourceDef` instead
(ADR-0029), and `scoped_query` leaves the trashed records out after `query` as it ANDs the tenant
filter. Tenancy is declared on the `ResourceDef`
(`Tenancy::column` or `Tenancy::via`), and `scoped_query` ANDs the tenant predicate after
the narrowings, so none can drop it. App code loading rows outside the framework's loaders calls
`scoped_query`, not `Query::all()`. A resource serving several tenants declares `Tenancy::none()`
and scopes in `ResourceDef::query`, giving up the 403 when a request has no tenant.

A resource that declares no tenancy over tenant-owned rows does not mount: it would serve every
tenant's rows, and forgetting `.tenancy(..)` is the likelier cause than intent. Rows are
tenant-owned when the model has a `TenantId` field, or a `belongs_to` to a model the panel mounts a
resource over, when that resource is tenant-scoped or the model has a `TenantId` field, as a comment
belongs to a post. The `ModelColumns`
derive lists the model's `TenantId` fields in `TENANT_COLUMNS`, which has no default so that an impl
by hand states it too; the check needs no declaration of its own, and `Tenancy::none()` states the
intent. The derive recognizes the field by its type's name, `TenantId`, so an alias of it goes
unseen, as it does for `ModelForm`.

On a panel that acts for tenants (it offers a tenant registration or settings page, or mounts a
resource over tenant-owned rows) every resource declares its tenancy, or the panel does not mount.
There the column check misses what matters most: a table whose tenant key is a plain `Uuid`, or
the panel's own users, which serve every tenant's rows to each tenant's members. A resource that
means it says `Tenancy::none()`; on a panel without tenants none has to.

The tenant column is typed `TenantId` (or `Option<TenantId>`), and a tenancy lens must name a
field implementing `TenantColumn`. The type is the only thing that tells a tenant column apart
from any other UUID column, so a lens to an unmarked column does not compile.

## Rejected

- `Resource::query(cx)`, a static method of the trait: one resource type scopes its rows the same
  way on every panel, so a panel that serves fewer rows needs a second resource type.
- A `#[tenant]` field attribute: `toasty::Model` derives only its own attributes.
- Refusing a lens by shape (the key, a relation endpoint, a name): it guesses at intent.
- Declaring the tenant column on the resource: a second declaration can disagree with the model.
- An opt-in `TenantOwned` marker trait on the model to trigger the undeclared-tenancy refusal: an
  app that forgets `.tenancy(..)` forgets the marker as easily, while the derive already sees the
  field's type.
- Refusing every resource that declares no tenancy, on every panel: most panels act for no
  tenant, and each of their resources would restate `Tenancy::none()`. A panel that acts for
  tenants refuses them, as above.
