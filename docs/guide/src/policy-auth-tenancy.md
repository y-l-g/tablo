# Policy, auth, tenancy

Three layers decide what a request may do. **Authentication** decides who is signed in.
**Tenancy** limits a signed-in user to the rows of the tenant they act for. **Policy** — the
resource's `ResourceDef::policy` — decides what that user may do with each resource and record.

## Policy

A policy answers one `Ability` at a time. The default policy is `Deny`, so a new resource exposes
nothing until you allow it:

```rust
use tablo::prelude::*;

impl Resource for UserResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:user-policy}}
    }
}
```

| Ability | Asked by |
| --- | --- |
| `ViewAny` | the list and its live refresh, the sidebar entry, the export, related tables, relationship options |
| `View(record)` | the detail page, the edit page and POST, deletes, each exported row, each relationship option, each row's actions |
| `Create` | the create page and POST, the Create button |
| `Update(record)` | the edit page and POST, the row's Edit action |
| `DeleteAny` | the delete and bulk-delete POSTs, the Delete action and the bulk column |
| `Delete(record)` | each record a delete removes, the row's Delete action and checkbox |
| `RunAny { action }` | the [action](./actions.md)'s POSTs and every button for it: a record action's on rows, record pages and the bulk bar, and the bulk column for a bulk action; a header action's in the list header |
| `Run { action, record }` | each record a record action runs on, its button on the record's row and pages and, for a bulk action, the record's checkbox |

`action` is the action's `NAME`, so one policy tells a resource's actions apart.
`ability.is_action::<Publish, _>()` asks whether the ability is `RunAny` or `Run` for the action
type `Publish`, so renaming the action's `NAME` cannot leave a policy matching the old string:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-action}}
```

The `_` is the resource, inferred when the action belongs to one. A header action, which runs on
no record, is asked only `RunAny`; `ability.is_header_action::<A>()` matches it. Handlers ask the same abilities
that decide which buttons render, so a hidden action is also a refused request. A denied request
answers 403.

- **The list asks `ViewAny` only.** A policy is Rust code that cannot run in the database, and
  filtering rows after pagination would leave pages short. Rows a user must not see on the list
  belong out of `query()`; see [Resources](./resources.md#scoping-the-query).
- **A record is viewed before it is written.** The edit, delete and action handlers ask `View`
  together with `Update`, `Delete` or `Run`. A delete asks `DeleteAny`, and an action `RunAny`,
  before any record loads.
- **Writes are checked against the stored row.** The update, delete and action handlers load the
  record inside the write's transaction and ask the policy about that row, not the submitted id. A
  bulk delete or bulk action fails as a whole if any selected record is refused `View`, and skips
  the records refused `Delete` or `Run`.
- **An action's `can_run` is not authorization.** It reads the record's state and sees no policy;
  the policy's `RunAny` and `Run` decide who runs the action. A panel that mounts the resource
  with `ReadOnly` through `Panel::resource_with` therefore offers and runs none of its actions.
- **Relationship options** require `ViewAny` and `View` from the related resource's policy.

### Building a policy

`Allow`, `Deny`, `ReadOnly` (`ViewAny` and `View`) and `when(predicate)` are policies, and each
combines with another through `and` and `or`. `when` allows every ability while a predicate on the
request holds, which suits a rule about the user or the tenant that several resources share. The
rule reads the app's own user type, `Staff` here (see [Your own user table](#your-own-user-table)):

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-editors-only}}

impl Resource for PostResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:post-policy-editors}}
    }
}

impl Resource for AuthorResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:author-policy-editors}}
    }
}
```

A closure `|cx: &Cx, ability: Ability<'_, M>| -> bool` is a policy too, as in the first example,
and so is any type that implements `Policy<M>`.

### In your own code

`can::<R>(cx, ability)` asks `R`'s policy, for a page or route that renders or writes `R`'s
records itself. `can_list::<R>(cx)` answers whether the request may open `R`'s list: the panel's
sign-in, `R`'s tenant, and `ViewAny`, the checks the list handler makes. A dashboard that links
to lists checks it before rendering each link.

A page, route or shard the app serves under a panel calls `auth::guard(cx)?` to require the
panel's signed-in user exactly as the panel's own pages do. A shard needs it: Topcoat serves
shards at its runtime path, where no page guard runs. A form route verifies its CSRF token with
`csrf::verify`.

## Authentication

Authentication is on by default. The built-in login checks an email and password against the
`AdminUser` table and keeps sessions in the `AuthSession` table; register both models with
Toasty and create a user with `auth::create_admin`, which stores the password's hash:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-models}}
```

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-hash-password}}
```

[Your first panel](./first-panel.md) shows the complete setup.

### What a request gets

| Request | Signed in with panel access | Signed in without panel access | Not signed in |
| --- | --- | --- | --- |
| `GET` of a panel page | the page | 403 | redirect to `/admin/login?next=…` |
| any other panel request | handled | 403 | 401 |
| `/_topcoat/runtime` | handled | 403 | 401 |

After login, the user returns to `next`, which must be a same-origin path. A user has panel access
when their `AdminUser.active` is `true`. The gate covers only the panel prefix and
`/_topcoat/runtime`; routes elsewhere, including your app's own routes and directories served with
`Panel::serve_dir`, are public. On a router with several panels, `/_topcoat/runtime` answers 401
without a session only when every panel is gated.

- **Login** verifies Argon2id hashes. An unknown email costs the same work as a wrong password,
  and every failure shows the same message. Each login may make five attempts a minute from
  one address; set another limit with `Auth::throttle`, as
  [Security](./security.md#authentication) shows.
- **Sessions** are rows in `AuthSession`, identified by a hash of the cookie's token. A session
  lasts seven days from login, is replaced on each login and deleted on logout. Each successful
  login also deletes up to 500 expired sessions. The row records the prefix of the panel that
  signed the user in, so an existing `auth_session` table needs a migration: add a non-null
  `panel` column and backfill it to the panel's prefix (for example `ALTER TABLE auth_session
  ADD COLUMN panel TEXT NOT NULL DEFAULT '/admin'`), then drop the default when every row names
  its panel. Renaming a panel's prefix invalidates the sessions it issued.
- **Revoking.** Call `auth::revoke_sessions_for_user(cx, user_id)` when a user's password changes
  or their account is deactivated; otherwise existing sessions stay valid until they expire. On a
  panel's request it revokes the sessions that panel issued; outside any panel, the user's
  sessions on every panel, so a reset flow for a user with two panels revokes from outside a panel
  or once per panel.
- **Several panels.** Each panel has its own `Auth` and its own login page, and a session belongs
  to the panel that signed the user in: the session row records the panel's prefix. Another
  panel's gate treats the request as anonymous, and `auth::user` answers `None` there. One
  browser holds one session, so signing in to a second panel ends the first.

Read the signed-in user in your own code as your user type: `auth::user::<AdminUser>(cx)` returns
`Option<&AdminUser>`, and `auth::require_user::<AdminUser>(cx)?` answers the request as the table
above when nobody is signed in. `auth::signed_in(cx)` answers whether anyone is.

### Your own user table

Implement `PanelUser` for the type the panel signs in, and `Authenticator` to load it:

```rust
use tablo::{Membership, PanelUser, auth::{Authenticator, verify_password}};

{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-staff-auth}}
```

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-custom-auth-body}}
```

- **The user type is yours.** It need not be a model: the showcase's `SignedStaff` is a row plus
  the workspaces it holds a seat in. `auth::user::<SignedStaff>(cx)` reads it back, fields and
  all, and answers `None` on a panel whose authenticator loads another type.
- **`verify`** checks credentials. Return `Ok(None)` for every credential failure.
  `verify_password` checks an Argon2id hash and, given `None` for an unknown account, does the
  same work, so response times do not reveal which accounts exist.
- **`find_by_id`** reloads the user on every request, so deactivating a user or removing a
  membership takes effect immediately.

Sessions stay in `AuthSession`, so register it; the provided `AdminUser` is needed only by
`PasswordAuth`.

### Turning it off

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-auth-disabled-body}}
```

Every panel route is then public and the login routes are not registered. Use it for public
demos and tests only.

## Tenancy

A tenant-owned resource shows each user only the rows of the tenant they act for. A user may act
for each tenant `PanelUser::tenants` lists, as a `Membership { tenant, name }`; the provided
`AdminUser` belongs to none, so a tenanted app signs in [its own user table](#your-own-user-table).

- **The request's tenant** is the membership the user selected, else their first, read with
  `tenant_id(cx)`; `membership(cx)` returns the whole `Membership`. A user with no membership has
  no tenant.
- **Switching.** With two or more memberships, the top bar shows a tenant switcher. It posts to
  `{prefix}/tenant`, which stores the choice on the session and returns to the panel's home
  page.
  A tenant the user is not a member of answers 403, and a stored choice applies only while the
  membership lasts. The session table records the choice, so an existing `auth_session` table
  needs a nullable `tenant` column, typed as Toasty stores a `Uuid` on your database. One
  browser acts for one tenant at a time, in every tab.
- **Overrides.** No request header sets the tenant. Server code such as a middleware or a test
  can, by inserting a `Tenant` request extension or putting `Tenant(id)` on the `Cx`.

Declare the resource's tenancy with the column its rows carry their tenant in:

```rust
impl Resource for PostResource {
    type Model = Post; // has `tenant_id: TenantId`
    // …

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:post-tenancy}}
    }
}
```

That one declaration does two things:

- **The gate.** Every handler of the resource answers 403 when the request has no tenant.
- **The scope.** Every loader — list, export, detail, edit, delete, bulk delete, relationship
  options, related tables — adds `tenant_id = <request tenant>` to the resource's `query`. A
  create sets the tenant column itself, so the record form leaves it out.

The column is a `TenantId` field of the model, or `Option<TenantId>` for a nullable one, named by
its lens; the name itself is the app's choice. Do not repeat the filter in `query`. Mounting the
panel refuses a `Tenancy::column` lens that is not one field of the model.

**A row that inherits its tenant.** A comment has no tenant column of its own; it belongs to a post
that does. `Tenancy::via` names the tenant through the relation, and the framework gates and
filters exactly as it does for a column:

```rust
impl Resource for CommentResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:comment-tenancy-via}}
    }
}
```

Nothing is stamped on create: a comment's tenant is its post's. The lens starts at a `belongs_to`
relation, and the form declares that relation's foreign key (`post_id`) as a
[relationship field](./forms.md#relationships) over a tenant-scoped resource, so a submitted post
must be one of the request tenant's posts. Mounting the panel refuses a form that writes the key any
other way, or a lens that does not start at a `belongs_to`.

**A deliberately cross-tenant resource** — a super-admin view — declares no tenancy and filters in
`query()`. That gives up both the gate and the scope.

**In your own code**, load rows with `scoped_query::<PostResource>(cx)?`. It applies the tenant
scope and answers 403 when the request has no tenant. `PostResource::query(cx)` does not apply the
tenant scope.

**Foreign keys are re-checked inside the write.** A relationship field's key must be one of the
related resource's records for the request: in its tenant-scoped query and allowed by its `View`.
The create and edit handlers check it when they validate the form, and again inside the write's
transaction, so a related record deleted, moved to another tenant or hidden in between refuses the
write with the same field error. A `create_record` or `update_record` the panel's create and edit
POSTs call needs no check of its own.
