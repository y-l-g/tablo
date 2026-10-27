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

Each [`Ability`](https://y-l.fr/tablo/nightly/api/tablo/enum.Ability.html) variant names one thing a user may do: list, view,
create, update, delete, restore, run an action. Its page says which pages and POSTs ask it.

`action` is the action's `NAME`, so one policy tells a resource's actions apart.
`ability.is_action::<Publish, _>()` asks whether the ability is `RunAny` or `Run` for the action
type `Publish`, so renaming the action's `NAME` cannot leave a policy matching the old string:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-action}}
```

The `_` is the resource, inferred when the action belongs to one. A header action, which runs on
no record, is asked `RunHeader`, which `ability.is_header_action::<A>()` matches.

Three rules shape every policy; [`Policy`](https://y-l.fr/tablo/nightly/api/tablo/trait.Policy.html) lists the rest:

- **Handlers ask what buttons ask.** A hidden button is also a refused request, answered 403.
- **The list asks `ViewAny` only.** A policy is Rust code that cannot run in the database, and
  filtering rows after pagination would leave pages short. Rows a user must not see on the list
  belong out of `query()`; see [Resources](./resources.md#scoping-the-query).
- **Writes are checked against the stored row**, loaded inside the write's transaction, never
  against the submitted key.

The policy sees the row as stored, never what the submission writes over it. Who may change a
field at all is [field access](./resources.md#field-access): its `edit` rule disables the control
and keeps the stored value. A rule about the values a write stores, such as who may set a post to
published, belongs in `Resource::before_commit`, which receives the `Change` inside the write's
transaction: a `Change::Create` with the new row, a `Change::Update` with `before` and `after`. A
custom action's `Change::Action` carries only the rows before it ran, so an action that publishes
checks its own rule. `before_commit`'s `Err` rolls the write back:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-stored-value}}
```

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

A closure ending in `_ => true` allows every ability it does not name: the restore a soft delete
adds, an action declared later, any ability a later Tablo release adds. Name every variant, as
the user policy above does, so that a new one fails to compile, or build the policy with
`Abilities`, which refuses every ability it does not declare:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-abilities}}
```

A write's record check also allows the write at all, and `view` does not allow `ViewAny`:
[`Abilities`](https://y-l.fr/tablo/nightly/api/tablo/struct.Abilities.html) has the rules for each method. A header action is
declared with `run_header::<A>`.

A policy that allows everything but some writes reads `ability.written_record()`, the record an
update, delete, restore, force delete or record action's run writes, so a record action the
resource gains later is refused too. `Create` and `RunHeader` write no record they are handed and
answer `None`; a policy that must refuse them matches them by name:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-written-record}}
```

`policy_fn` takes a closure `|cx, ability| -> bool`, as in the first example, and `policy` takes
any type that implements `Policy<M>`. Within `declare`, the def learns its resource from the return
type only after the closure is checked, so a closure that reads the record names the ability's
type, `ability: Ability<'_, User>`, or the chain starts from `ResourceDef::<Self>::new()`.

### In your own code

`can::<R>(cx, ability)` asks `R`'s policy, for a page or route that renders or writes `R`'s
records itself. `can_list::<R>(cx)` answers whether the request may open `R`'s list: the panel's
sign-in, `R`'s tenant, and `ViewAny`, the checks the list handler makes. A dashboard that links
to lists checks it before rendering each link.

A page, route or shard the app serves under a panel calls `auth::guard(cx)?` to require the
panel's signed-in user exactly as the panel's own pages do. A shard needs it: Topcoat serves
shards at its runtime path, where the page's checks do not run. The panel declares the shards and
procedures its pages use with [`Panel::endpoint`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.endpoint), so they
run as that panel.

A form route verifies the CSRF token it posted under `csrf::FIELD_NAME` with
`csrf::verify(cx, token)`.

## Authentication

Authentication is on by default. The built-in login checks an email and password against the
`AdminUser` table and keeps sessions in the `AuthSession` table. `tablo::models!` registers both
beside the app's models; create a user with `auth::create_admin`, which stores the password's
hash:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-models}}
```

A panel whose lists declare [toggleable columns](tables.md#hiding-columns) also keeps each user's
choice in the `ColumnPreference` table. A feature that mails links, such as a password reset,
keeps its tokens in `AuthToken`, and two-factor sign-in its factors in `AuthFactor`;
`tablo::models!` registers all three. An app listing its models with `toasty::models!` lists these
too, and the mount names any it misses. Toasty names a table after its model, so an app with a
model named as one of Tablo's, such as its own `AdminUser`, lists its models that way.

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-hash-password}}
```

[Your first panel](./first-panel.md) shows the complete setup.

### What a request gets

| Request | Signed in with panel access | Signed in without panel access | Not signed in |
| --- | --- | --- | --- |
| `GET` of a panel page | the page, or its [error page](./panel-and-routing.md#the-shell-layout) | 403 page with Sign out | redirect to `/admin/login?next=…` |
| any other panel request | handled | 403 | 401 |
| `/_topcoat/runtime` | handled | 403 | 401 |

After login, the user returns to `next`, which must be a same-origin path. A user has panel access
when their `AdminUser.active` is `true`. The gate covers only the panel prefix and
`/_topcoat/runtime`; routes elsewhere, including your app's own routes and directories served with
`Panel::serve_dir`, are public — a directory served under the panel prefix is the exception, since
the gate covers the prefix. `Panel::uploads_dir` answers only at the signed URLs the panel's pages
render. On a router with several panels, `/_topcoat/runtime` answers 401
without a session only when every panel is gated.

- **Login** verifies Argon2id hashes and treats an email's case as insignificant; each login may
  make five attempts a minute from one address, which `Auth::throttle` changes; `Auth::account_throttle`
  adds an opt-in limit per login across addresses
  ([Security](./security.md#authentication)). [`PasswordAuth`](https://y-l.fr/tablo/nightly/api/tablo/struct.PasswordAuth.html) has
  the details, and what to do about rows stored with capitals.
- **Sessions** are rows in `AuthSession`, valid for seven days, and each belongs to the panel that
  signed the user in: a user signed in to `/admin` is anonymous on `/portal`.
  [`AuthSession`](https://y-l.fr/tablo/nightly/api/tablo/auth/struct.AuthSession.html) has the rules and the columns it needs.
- **Revoking.** Call `auth::revoke_sessions_for_user(cx, user_id)` when a user's password changes
  or their account is deactivated; otherwise existing sessions stay valid until they expire.

Read the signed-in user in your own code as your user type: `auth::user::<AdminUser>(cx)` returns
`Option<&AdminUser>`, and `auth::require_user::<AdminUser>(cx)?` answers the request as the table
above when nobody is signed in. `auth::signed_in(cx)` answers whether anyone is.

### Your own user table

Implement `PanelUser` for the type the panel signs in, and `Authenticator` to load it:

```rust
use tablo::{Authenticator, Membership, PanelUser, auth::verify_password};

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
  same work, so response times do not reveal which accounts exist. It and `hash_password` run on
  Tokio's blocking pool, so await them rather than hashing on the request's task.
- **`find_by_id`** reloads the user on every request, so deactivating a user or removing a
  membership takes effect immediately.
- **`time_zone`** names the zone the user reads and enters times in, such as one they pick on
  their profile page and the app stores; it defaults to the panel's. See
  [Time zones](./forms.md#time-zones).
- **`locale`** names the locale the user reads the panel in; it defaults to the browser's best
  match. See [Translations](./translations.md#the-users-locale).

Sessions stay in `AuthSession`, which `tablo::models!` registers. It registers the shipped
`AdminUser` too, which only `PasswordAuth` reads: with another authenticator its table stays
empty.

### Sign-up

`Auth::registration` lets visitors create their own account at `{prefix}/register`, which the
login page links to. `PasswordSignUp` registers an `AdminUser` from a name, an email address and a
password typed twice:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-sign-up-password}}
```

`PasswordSignUp::pending` stores each account inactive: the sign-up lands on the login page, and
the account signs in once an administrator sets `active`. `PasswordSignUp::open` stores it active
and signs it in at once, and an active `AdminUser` may open the whole panel, so choose it only
where any visitor may hold an account.

Your own user table registers through a `Registrar` beside its `Authenticator`, which works like
an [action with input](./actions.md#asking-for-input): `Input` is the form the page asks for,
`validate` refuses a submission field by field, and `register` writes the account. Both run in one
transaction the framework opens:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-staff-sign-up}}
```

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-staff-sign-up-panel}}
```

[`Auth::registration`](https://y-l.fr/tablo/nightly/api/tablo/struct.Auth.html#method.registration) lists how the form posts, where the
visitor lands after the commit, what the mount refuses and how sign-ups are throttled. The
sign-up page tells a visitor whether an email is taken, which the login page never does.

### Profile

`Auth::profile` adds a profile page at `{prefix}/profile`, which the user menu links to, where the
signed-in user edits their own account. `PasswordAuth` edits an `AdminUser`'s name, email address
and password:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-profile-password}}
```

It asks for the current password before the email or the password changes, refuses a new password
`new_password_errors` refuses and, once the current password verifies, an email another account
holds, and leaves the password as it is when the new-password field is blank.

Your own user table edits through a `ProfileEditor` beside its `Authenticator`, which works like a
`Registrar`: `fill` seeds the form from the signed-in user, `validate` refuses a submission field by
field, and `save` writes it. `validate` and `save` run in one transaction the framework opens. The
shipped `Profile` input fits any table with a name, an email and a password:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-staff-profile}}
```

[`Auth::profile`](https://y-l.fr/tablo/nightly/api/tablo/struct.Auth.html#method.profile) lists what the form renders, which saves end
the user's sessions, what the mount refuses and how saves are throttled.

### Password reset

`Auth::password_reset` adds a "Forgot your password?" link to the login page. It leads to
`{prefix}/forgot-password`, which mails a reset link, and the link opens `{prefix}/reset-password`,
where the visitor chooses a new password. `Auth::mail` names the sender and the base URL every
link the panel mails starts with. The mail goes through the router's `MailConfig`, so register one
before mounting the panel, and add `AuthToken` to the `Db`'s models:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-password-reset}}
```

The base URL is the scheme and host the app answers on. Reset links start with it rather than with
the request's `Host` header, which the client chooses and could point at another host.

Your own user table resets through a `PasswordResetter` beside its `Authenticator`:
`find_by_email` loads the account an address signs in as, and `reset_password` sets the new
password inside the reset's transaction. The link goes to the address `PanelUser::email` answers;
an account that answers none gets no link.

[`Auth::password_reset`](https://y-l.fr/tablo/nightly/api/tablo/struct.Auth.html#method.password_reset) lists how long a link works,
what a reset ends, what the mount refuses and how requests are throttled. The request page
answers the same whether an account uses the address or not, so it never reveals one.

### Email verification

`Auth::email_verification` makes every signed-in user prove they read the address
`PanelUser::email` answers before the panel serves them anything else. It mails links through
`Auth::mail`, like a password reset:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-email-verification}}
```

Your own user table verifies through an `EmailVerifier` beside its `Authenticator`: `is_verified`
says whether the user has proved their address, and `store_verified_email` stores the address a
link proved and marks it verified, inside the link's transaction. `PasswordAuth` keeps the proof in
`AdminUser::email_verified_at`; `create_admin` seeds a verified account.

Accounts that predate verification are unverified, seeded admins included: backfill
`email_verified_at` for the addresses you already trust before turning it on. A new email waits
for its own link: a profile editor calls `auth::request_email_change` from `save` instead of
writing the address, as `PasswordAuth` does.
[`Auth::email_verification`](https://y-l.fr/tablo/nightly/api/tablo/struct.Auth.html#method.email_verification) lists what each link
does and for how long, what the mount refuses and how links are throttled.

### Two-factor sign-in

`Auth::two_factor` lets each signed-in user add an authenticator app, and asks an account that
has one for a code from it after its password. The panel stores the factors itself, as
`AuthFactor` rows, so it works with any `Authenticator`. Each authenticator secret is sealed under
the router's cookie `Key`, which the router must hold:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-two-factor}}
```

Add `AuthFactor` and `AuthToken` to the `Db`'s models.

Each panel enrols apart, and the cookie `Key` seals the secrets, so rotating it makes every app
code fail until its owner signs in with a recovery code.
[`Auth::two_factor`](https://y-l.fr/tablo/nightly/api/tablo/struct.Auth.html#method.two_factor) lists the setup, the challenge, codes
by email, throttling and what the mount refuses.

### Turning it off

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-auth-disabled-body}}
```

Every panel route is then public and the login, sign-up, profile, password reset, two-factor and
tenant routes are not registered. Use it for public
demos and tests only.

## Tenancy

A tenant-owned resource shows each user only the rows of the tenant they act for. A user may act
for each tenant `PanelUser::tenants` lists, as a `Membership { tenant, name }`; the provided
`AdminUser` belongs to none, so a tenanted app signs in [its own user table](#your-own-user-table).

The request's tenant is the membership the user selected, else the one
`PanelUser::default_tenant` names, else their first; read it with `tenant_id(cx)`, a `TenantId`,
and the whole `Membership` with `auth::membership(cx)`. A user with two or more memberships switches between
them from a menu atop the sidebar ([`PanelUser::tenants`](https://y-l.fr/tablo/nightly/api/tablo/trait.PanelUser.html#method.tenants)).
A panel acts for tenants when it offers a tenant registration or settings page, or mounts a
resource over tenant-owned rows; any other acts for none, so `tenant_id(cx)` answers `None` there,
in its custom pages and widgets too. No request header sets the tenant; server code such as a middleware or a test can, through a
`Tenant` request extension or `Tenant(id)` on the `Cx`, where `id` is a `TenantId`: a bare `Uuid`
does not compile there.

`Auth::tenant_registration` lets a signed-in user create a tenant, through a `TenantRegistrar`
whose `register` writes the tenant and the user's membership of it; `required()` sends a user with
no membership there before anything else:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-tenant-registration}}
```

`Auth::tenant_profile` adds a settings page for the tenant the request acts for, through a
`TenantEditor` whose `may_edit` says which members may open it:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-tenant-profile}}
```

[`Auth::tenant_registration`](https://y-l.fr/tablo/nightly/api/tablo/struct.Auth.html#method.tenant_registration) and
[`Auth::tenant_profile`](https://y-l.fr/tablo/nightly/api/tablo/struct.Auth.html#method.tenant_profile) list what each page does and
what the mount refuses.

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

- **The tenant check.** Every handler of the resource answers 403 when the request has no tenant.
- **The scope.** Every loader — list, export, detail, edit, delete, bulk delete, relationship
  options, related tables — adds `tenant_id = <request tenant>` to the resource's `query`. A
  create sets the tenant column itself, so the record form leaves it out.

The column is a `TenantId` field of the model, or `Option<TenantId>` for a nullable one, named by
its lens. Do not repeat the filter in `query`. A resource over tenant-owned rows that declares no
tenancy at all fails to mount, since it would serve every tenant's rows; the
[mount errors](./mount-errors.md) say which declaration it needs.

**A row that inherits its tenant.** A comment has no tenant column of its own; it belongs to a post
that does. `Tenancy::via` names the tenant through the relation, and the framework checks and
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

**A deliberately cross-tenant resource** — a super-admin view — declares `Tenancy::none()` and
filters in `query()`. That gives up both the tenant check and the scope.

**In your own code**, load rows with `scoped_query::<PostResource>(cx)?`. It applies the tenant
scope and answers 403 when the request has no tenant. `PostResource::query(cx)` does not apply the
tenant scope.

**Foreign keys are re-checked inside the write.** A relationship field's key must be one of the
related resource's records for the request: in its tenant-scoped query and allowed by its `View`.
The create and edit handlers check it when they validate the form, and again inside the write's
transaction, so a related record deleted, moved to another tenant or hidden in between refuses the
write with the same field error. A `create_record` or `update_record` the panel's create and edit
POSTs call needs no check of its own.
