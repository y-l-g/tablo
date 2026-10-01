# Policy, auth, tenancy

Three layers decide what a request may do. **Authentication** decides who is signed in.
**Tenancy** limits a signed-in user to their tenant's rows. **Policy** — the resource's `can_*`
predicates — decides what that user may do with each resource and record.

## Policy

Every predicate defaults to `false`, so a new resource exposes nothing until you allow it:

```rust
fn can_view_any(_cx: &Cx) -> bool { true }
fn can_view(_cx: &Cx, _user: &User) -> bool { true }
fn can_create(cx: &Cx) -> bool { is_admin(cx) }
fn can_update(_cx: &Cx, user: &User) -> bool { !user.sso_managed }
fn can_delete_any(cx: &Cx) -> bool { is_admin(cx) }
```

| Predicate | Default | Checked by |
| --- | --- | --- |
| `can_view_any(cx)` | `false` | the list and its live refresh, the export, related tables, relationship options |
| `can_view(cx, record)` | `false` | the detail page, the edit page and POST, deletes, each exported row, each relationship option, each row's actions |
| `can_create(cx)` | `false` | the create page and POST, the Create button |
| `can_update(cx, record)` | `false` | the edit page and POST, the row's Edit action |
| `can_delete_any(cx)` | `false` | the delete and bulk-delete POSTs, the Delete action and the bulk column |
| `can_delete(cx, record)` | `can_delete_any(cx)` | each record a delete removes, the row's Delete action and checkbox |

Handlers check the same predicates that decide which buttons render, so a hidden action is also a
refused request. A denied request answers 403.

- **The list checks `can_view_any` only.** `can_view` is Rust code that cannot run in the
  database, and filtering rows after pagination would leave pages short. Rows a user must not see
  on the list belong out of `query()`; see [Resources](./resources.md#scoping-the-query).
- **Writes are checked against the stored row.** The update and delete handlers load the record
  inside the write's transaction and check the predicates on that row, not on the submitted id.
  A bulk delete fails as a whole if any selected record is refused.
- **Relationship options** require both `can_view_any` and `can_view` on the related resource;
  overriding one does not imply the other.

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

Mark the resource with `requires_tenant()`:

```rust
impl Resource for PostResource {
    type Model = Post; // has `tenant_id: uuid::Uuid`
    // …

    fn requires_tenant() -> bool {
        true
    }
}
```

That one declaration does two things:

- **The gate.** Every handler of the resource answers 403 when the request has no tenant.
- **The scope.** Every loader — list, export, detail, edit, delete, bulk delete, relationship
  options, related tables — adds `tenant_id = <request tenant>` to the resource's `query()`. A
  create sets the tenant column itself, so the record form leaves it out.

The framework finds the column by name and type: a UUID field named `tenant_id`. Do not repeat the
filter in `query()`.

**A row that inherits its tenant.** A comment has no `tenant_id` of its own; it belongs to a post
that does. Override `tenant_scope` with the predicate through the relation, and the framework
applies it wherever it would apply the derived filter:

```rust
fn requires_tenant() -> bool {
    true
}

fn tenant_scope(tenant: uuid::Uuid) -> Option<toasty::stmt::Expr<bool>> {
    Some(Comment::fields().post().tenant_id().eq(tenant))
}
```

Keep `requires_tenant()` `true` here: writing the same filter into `query()` without the gate
would serve every tenant's rows to a user who has no tenant.

**A deliberately cross-tenant resource** — a super-admin view — declares `requires_tenant()`
`false` and filters in `query()` by hand. That gives up both the gate and the scope.

Mounting the panel refuses a resource with `requires_tenant()` `true` whose model has no `tenant_id`
UUID field and which does not override `tenant_scope`.

**In your own code**, load rows with `scoped_query::<PostResource>(cx)?`. It applies the tenant
scope and answers 403 when the request has no tenant. `PostResource::query(cx)` does not apply the
tenant scope. A record function that writes a foreign key should re-check the target through
`scoped_query` inside its transaction, since the tenant of the related row may have changed since
the form was validated.
