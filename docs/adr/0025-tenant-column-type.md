# The tenant column is typed `TenantId`, so a lens must name one

Date: 2026-10-06 — Status: accepted

## Decision

A tenant-scoped model types its tenant column `TenantId`, or `Option<TenantId>` for a nullable
column. `Tenancy::column` and `Tenancy::via` accept only a lens whose field type implements
`TenantColumn`: `TenantId` and `Option<TenantId>` do, and an app may implement it for its own key
type. `TenantId` stores a UUID, so the database column is unchanged.

A field's type is the only thing that tells a tenant column apart from an author, a parent row, or
the key; a lens check cannot. An app cannot implement `TenantColumn` for `uuid::Uuid` — neither
the trait nor the type is the app's — so a lens to an unmarked UUID column does not compile. The
mount checks stay, and each names the fix: a lens that names no field of the model, a `via` lens
that is one field, a `via` lens earlier than a `belongs_to`, a record form claiming the stamped
column, and `create_columns` naming it.

## Rejected

- A `#[tenant]` field attribute: `toasty::Model` derives only the attributes it declares, so a
  Tablo field attribute would need an upstream change; the newtype marks the field today.
- Refusing a lens by shape — the key, a relation endpoint, a field name — guesses at intent, so it
  would refuse sound declarations while an unrelated UUID column still passed.
- Documenting the risk instead: the misdeclaration mounts, filters on the wrong column, and stamps
  the request tenant into it.
- Declaring the column in the resource rather than typing it on the model: a second declaration can
  disagree with the model, and only the type is what the compiler checks.
