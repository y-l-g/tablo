# Policy, auth, tenancy

Three layers decide what a request may do. **Authentication** decides who is signed in.
**Tenancy** limits a signed-in user to their tenant's rows. **Policy** — the resource's
`policy()` — decides what that user may do with each resource and record.

## Policy

A policy answers one `Ability` at a time. The default policy is `Deny`, so a new resource exposes
nothing until you allow it:

```rust
use tablo::prelude::*;

impl Resource for UserResource {
    // …
    fn policy() -> impl Policy<User> {
        |cx: &Cx, ability: Ability<'_, User>| match ability {
            Ability::ViewAny | Ability::View(_) => true,
            Ability::Create | Ability::DeleteAny | Ability::Delete(_) => is_admin(cx),
            Ability::Update(user) => !user.sso_managed,
        }
    }
}
```

| Ability | Asked by |
| --- | --- |
| `ViewAny` | the list and its live refresh, the export, related tables, relationship options |
| `View(record)` | the detail page, the edit page and POST, deletes, each exported row, each relationship option, each row's actions |
| `Create` | the create page and POST, the Create button |
| `Update(record)` | the edit page and POST, the row's Edit action |
| `DeleteAny` | the delete and bulk-delete POSTs, the Delete action and the bulk column |
| `Delete(record)` | each record a delete removes, the row's Delete action and checkbox |

Handlers ask the same abilities that decide which buttons render, so a hidden action is also a
refused request. A denied request answers 403.

- **The list asks `ViewAny` only.** A policy is Rust code that cannot run in the database, and
  filtering rows after pagination would leave pages short. Rows a user must not see on the list
  belong out of `query()`; see [Resources](./resources.md#scoping-the-query).
- **A record is viewed before it is written.** The edit and delete handlers ask `View` together
  with `Update` or `Delete`, and a delete asks `DeleteAny` before any record loads.
- **Writes are checked against the stored row.** The update and delete handlers load the record
  inside the write's transaction and ask the policy about that row, not the submitted id. A bulk
  delete fails as a whole if any selected record is refused.
- **Relationship options** require `ViewAny` and `View` from the related resource's policy.

### Building a policy

`Allow`, `Deny`, `ReadOnly` (`ViewAny` and `View`) and `when(predicate)` are policies, and each
combines with another through `and` and `or`. `when` allows every ability while a predicate on the
request holds, which suits a rule about the user or the tenant that several resources share:

```rust
fn editors_only(cx: &Cx) -> bool {
    tablo::auth::current_user(cx).is_some_and(|user| user.login.ends_with("@example.com"))
}

impl Resource for PostResource {
    fn policy() -> impl Policy<Post> {
        ReadOnly.or(when(editors_only))
    }
}

impl Resource for AuthorResource {
    fn policy() -> impl Policy<Author> {
        when(editors_only)
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
Toasty and create a user with a hashed password:

```rust
toasty::models!(crate::Book, tablo_core::auth::AdminUser, tablo_core::auth::AuthSession)
```

```rust
let password_hash = tablo_core::auth::hash_password("secret")?; // Argon2id
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
  and every failure shows the same message.
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
  panel's gate treats the request as anonymous, and `current_user(cx)` answers `None` there. One
  browser holds one session, so signing in to a second panel ends the first.
- **Rate limiting** is not built in. Limit login attempts at your proxy or firewall.

Read the signed-in user in your own code with `auth::current_user(cx)`, which returns a
`CurrentUser { id, login, display_name, tenant_id, can_access_panel }`, or with
`auth::require_authenticated(cx)?`, which answers the request as the table above when there is
none.

### Your own user table

Implement `Authenticator` for your user model and install it:

```rust
Panel::new("admin").auth(Auth::custom(MyAuth))
```

`verify(cx, login, password)` checks credentials and returns the `CurrentUser`, and
`find_by_id(cx, id)` reloads that user on every request, so deactivating a user takes effect
immediately. Return `Ok(None)` for every credential failure; do comparable work for unknown and
known accounts so response times do not reveal which exist. Sessions stay in `AuthSession`.

### Turning it off

```rust
Panel::new("admin").auth(Auth::disabled())
```

Every panel route is then public and the login routes are not registered. Use it for public
demos and tests only.

## Tenancy

A tenant-owned resource shows each user only their tenant's rows. The request's tenant is the
signed-in user's `tenant_id`, read with `tenant_id(cx)`. No request header can set it; server code
such as a middleware or a test can, by inserting a `Tenant` request extension.

Declare the resource's tenancy with the column its rows carry their tenant in:

```rust
impl Resource for PostResource {
    type Model = Post; // has `tenant_id: uuid::Uuid`
    // …

    fn tenancy() -> Tenancy<Post> {
        Tenancy::column(Post::fields().tenant_id())
    }
}
```

That one declaration does two things:

- **The gate.** Every handler of the resource answers 403 when the request has no tenant.
- **The scope.** Every loader — list, export, detail, edit, delete, bulk delete, relationship
  options, related tables — adds `tenant_id = <request tenant>` to the resource's `query()`. A
  create sets the tenant column itself, so the record form leaves it out.

The column is a `Uuid` or `Option<Uuid>` field of the model, named by its lens, and it can have any
name. Do not repeat the filter in `query()`. Mounting the panel refuses a `Tenancy::column` lens
that is not one field of the model.

**A row that inherits its tenant.** A comment has no tenant column of its own; it belongs to a post
that does. `Tenancy::via` names the tenant through the relation, and the framework gates and
filters exactly as it does for a column:

```rust
fn tenancy() -> Tenancy<Comment> {
    Tenancy::via(Comment::fields().post().tenant_id())
}
```

Nothing is stamped on create: a comment's tenant is its post's. Declare the foreign key as a
[relationship field](./forms.md#relationships) over the post's resource, so a submitted post must
be one of the request tenant's posts.

**A deliberately cross-tenant resource** — a super-admin view — declares no tenancy and filters in
`query()` by hand. That gives up both the gate and the scope.

**In your own code**, load rows with `scoped_query::<PostResource>(cx)?`. It applies the tenant
scope and answers 403 when the request has no tenant. `PostResource::query(cx)` does not apply the
tenant scope.

**Foreign keys are re-checked inside the write.** A relationship field's key must be one of the
related resource's records for the request: in its tenant-scoped query and allowed by its `View`.
The create and edit handlers check it when they validate the form, and again inside the write's
transaction, so a related record deleted, moved to another tenant or hidden in between refuses the
write with the same field error. A record function needs no check of its own.
