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
| `RunAny { action }` | the [record action](./actions.md#record-actions)'s POSTs and its buttons on rows, record pages and the bulk bar, and the bulk column for a bulk action |
| `Run { action, record }` | each record a record action runs on, its button on the record's row and pages and, for a bulk action, the record's checkbox |
| `RunHeader { action }` | the [header action](./actions.md#header-actions)'s POST and its button in the list header, with `ViewAny` |

`action` is the action's `NAME`, so one policy tells a resource's actions apart.
`ability.is_action::<Publish, _>()` asks whether the ability is `RunAny` or `Run` for the action
type `Publish`, so renaming the action's `NAME` cannot leave a policy matching the old string:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-action}}
```

The `_` is the resource, inferred when the action belongs to one. A header action, which runs on
no record, is asked `RunHeader`, which `ability.is_header_action::<A>()` matches. Handlers ask the
same abilities that decide which buttons render, so a hidden action is also a refused request. A
denied request answers 403.

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
- **A record action's `can_run` is not authorization.** It reads the record's state and sees no
  policy; the policy's `RunAny` and `Run` decide who runs the action. A header action's
  `can_run(cx)` is a check of its own, beside the policy on a resource; on a page, which has no
  policy, it is where the action asks one. A panel that mounts the resource
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
shards at its runtime path, where the page's checks do not run. On a router with several panels,
the panel declares the shards and procedures its pages use with `Panel::endpoint(shard)`: a
request to one then runs as that panel, and a session another panel signed in answers 401. A
shard no panel declares runs as the panel whose session sent the request, whichever it is, and
as no panel when no session did, as on a panel with `Auth::disabled()`.

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
the gate covers the prefix. On a router with several panels, `/_topcoat/runtime` answers 401
without a session only when every panel is gated.

- **Login** verifies Argon2id hashes. An unknown email costs the same work as a wrong password,
  and every failure shows the same message. `PasswordAuth` stores an email trimmed and
  lowercased, and looks it up the same way, so `Ada@Example.com` and `ada@example.com` are one
  account. A resource that edits an admin's email must store it the same way. A row an earlier
  version stored with capitals cannot sign in or reset its password: list the accounts that
  differ only in case with `SELECT LOWER(TRIM(email)) FROM admin_users GROUP BY 1 HAVING
  COUNT(*) > 1`, merge or delete them, then run `UPDATE admin_users SET email =
  LOWER(TRIM(email))`. SQLite's `LOWER` folds ASCII letters only, so lowercase any other capital
  by hand. Each login may make five attempts a minute from
  one address; set another limit with `Auth::throttle`, as
  [Security](./security.md#authentication) shows.
- **Sessions** are rows in `AuthSession`, identified by a hash of the cookie's token. A session
  lasts seven days from login, is replaced on each login and deleted on logout. Each successful
  login also deletes up to 500 expired sessions. The row records the prefix of the panel that
  signed the user in, so an existing `auth_sessions` table needs a migration: add a non-null
  `panel` column and backfill it to the panel's prefix (for example `ALTER TABLE auth_sessions
  ADD COLUMN panel TEXT NOT NULL DEFAULT '/admin'`), then drop the default when every row names
  its panel. Renaming a panel's prefix invalidates the sessions it issued.
- **Revoking.** Call `auth::revoke_sessions_for_user(cx, user_id)` when a user's password changes
  or their account is deactivated; otherwise existing sessions stay valid until they expire. On a
  panel's request it revokes the sessions and reset links of every panel whose authenticator loads
  the same user type, as the profile's own password change does; outside any panel, the user's
  sessions and links on every panel.
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
  same work, so response times do not reveal which accounts exist. It and `hash_password` run on
  Tokio's blocking pool, so await them rather than hashing on the request's task.
- **`find_by_id`** reloads the user on every request, so deactivating a user or removing a
  membership takes effect immediately.
- **`time_zone`** names the zone the user reads and enters times in, such as one they pick on
  their profile page and the app stores; it defaults to the panel's. See
  [Time zones](./forms.md#time-zones).

Sessions stay in `AuthSession`, which `tablo::models!` registers. It registers the shipped
`AdminUser` too, which only `PasswordAuth` reads: with another authenticator its table stays
empty.

### Sign-up

`Auth::registration` lets visitors create their own account at `{prefix}/register`, which the
login page links to. `PasswordAuth` registers an `AdminUser` from a name, an email address and a
password typed twice:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-sign-up-password}}
```

Every account `PasswordAuth` registers is active, and an active `AdminUser` may open the whole
panel, so open it only where any visitor may hold an account.

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

- **The form** is any `FormInput`, or the shipped `SignUp`. `#[form(password)]` renders a
  masked control, with a button that shows what was typed, that a refused form renders empty, and reads the password as typed, outer
  spaces included, as the login compares it. `#[form(email)]` refuses a malformed address before
  `validate` runs. A missing field and a malformed value answer 422 with the error
  under the field, as `validate`'s refusals do; nothing is written.
- **`new_password_errors`** refuses a password under eight characters and a confirmation that
  differs, under the `password` and `password_confirmation` keys.
- **After the commit**, an account that may open the panel is signed in, its session rotated as a
  login rotates it, and lands on `next` or the panel's home page. One that may not, such as an
  account awaiting approval, lands on the login page with a notification instead. A signed-in
  user who opens the sign-up page lands on the panel's home page.
- **Mounting refuses** a registrar whose `User` is not the type the authenticator loads, a
  registration on `Auth::disabled()`, a form field named `csrf_token` or `next`, a file field and
  a relationship choice, whose options would list records to a visitor nobody signed in.
- **Throttling.** Each client address may submit five sign-ups a minute, counted whether or not
  they create an account; `Auth::sign_up_throttle` changes the limit. Past it the page answers
  429.

The sign-up page tells a visitor whether an email is taken, which the login page never does: an
app that must keep its accounts private verifies the address by email before saying so, which
Tablo does not ship.

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

- **The form** is any `FormInput`, rendered in the panel's shell. The page shows what the input's
  `write` writes, so `Profile` never shows a password. A disabled control keeps what `fill` wrote,
  whatever the POST carries. A refused save renders the form again with each error under its
  field; nothing is written.
- **`save` answers** `ProfileUpdate::Details`, which keeps every session, or
  `ProfileUpdate::Credentials` for a change to what the user signs in with. The user's sessions then
  end on every panel whose authenticator loads the same user type, in the transaction that saves,
  and the browser that saved gets a new session under the tenant it acted for. `PasswordAuth`
  answers `Credentials` for a new email or password.
- **An email another account holds** is reported only once the current password verifies, so a
  stolen session cannot probe which addresses have accounts.
- **After the commit** the page reloads with a notification.
- **Mounting refuses** an editor whose `User` is not the type the authenticator loads, a profile on
  `Auth::disabled()`, a form field named `csrf_token`, a file field and a relationship choice,
  whose options no route serves the page.
- **Throttling.** Each user may submit five saves a minute, from any address, counted whether or
  not they save; `Auth::profile_throttle` changes the limit. Past it the page answers 429, which
  keeps a stolen session from guessing the current password. A user past the limit can still sign
  in.

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

- **The request page** answers the same whether an account uses the address or not: it redirects
  to the login page with a notification, and writes and mails the link after answering, so the
  database and the mail server add no time when an account exists. An account that may not enter
  the panel gets no link, and a failed send is logged, not shown.
- **The link goes to the stored address**, never the typed one. A lookup that matches loosely,
  such as a collation that folds accents, would otherwise mail one account's link to another
  address.
- **A link** works once, for one hour, on the panel that mailed it, and only while the account may
  still enter the panel. A new request replaces the account's earlier link, and a password change
  on the profile page ends it, as it ends every link the account holds. The token is stored only
  as a hash, and the reset page sends no referrer and may not be cached.
- **A reset** checks the new password with `new_password_errors`, then sets it, ends the account's
  sessions and reset links on every panel whose authenticator loads the same user type, and
  redirects to the login page. All of this commits in one transaction.
- **Throttling.** Each address may request three links in fifteen minutes, from any client,
  counted whether or not an account uses it; `PasswordReset::throttle` changes the limit. Past it
  the page answers 429. A limit per client belongs at the proxy.
- **Mounting refuses** a resetter whose `User` is not the type the authenticator loads, a reset on
  `Auth::disabled()`, a reset without `Auth::mail`, a base URL that is not an absolute `http` or
  `https` URL or that carries a query or fragment, and a router without a `MailConfig`.

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

- **Until the user verifies**, a page redirects to `{prefix}/verify-email` and any other request
  answers 403. The page offers to mail a link, and to sign out. A user whose `PanelUser::email` is
  `None` has no address to prove and is not held.
- **Accounts that predate verification** are unverified: turning it on sends each of them,
  seeded admins included, to `{prefix}/verify-email` at their next request. Backfill
  `email_verified_at` (from `created_at`, say) for the addresses you already trust.
- **A sign-up** mails the link at once, after the response.
- **A link** works once, for one day, on the panel that mailed it, and only while the account may
  enter the panel and still stores the address it was mailed to. It verifies on `GET` in any
  browser, signed in or not, then lands on the panel or on the login page. A mail scanner that
  opens the link verifies the address too: it fetched the link from the mailbox the link proves.
- **A new email** waits for its own link. A profile editor calls `auth::request_email_change`
  from `save` in place of writing the address: on a panel with email verification it records the
  change and answers `EmailChange::Pending`, and once the save commits the panel mails the new
  address a link that stores it and the old address a link that keeps it. Elsewhere it answers
  `EmailChange::Immediate` and the editor stores the address itself. `PasswordAuth` does this.
  The link proves only that whoever asked reads the new mailbox, so ask for the current password
  before calling it, as `PasswordAuth` does: a stolen session could name its thief's mailbox.
- **Either link** opens a page whose button does the work, so a mail scanner that fetches it
  changes nothing.
- **The new address's link** stores it, verified, once, for one day. It is a credential change:
  the account's sessions and sign-in links end on every panel signing in its user type, and the
  browser that confirmed keeps a new session when it was signed in as the account.
- **The old address's link** works for a week, before or after the change goes through: it keeps
  the old address, restoring it if the change went through, and ends every session, sign-in link
  and pending change the account holds, the cancelling browser's included: whoever asked for the
  change may hold one.
- **Throttling.** Each account may ask for three links in fifteen minutes;
  `EmailVerification::throttle` changes the limit. Past it the page answers 429.
- **Mounting refuses** a verifier whose `User` is not the type the authenticator loads, a
  verification on `Auth::disabled()`, and one without `Auth::mail`.

### Two-factor sign-in

`Auth::two_factor` lets each signed-in user add an authenticator app, and asks an account that
has one for a code from it after its password. The panel stores the factors itself, as
`AuthFactor` rows, so it works with any `Authenticator`. Each authenticator secret is sealed under
the router's cookie `Key`, which the router must hold:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-two-factor}}
```

Add `AuthFactor` and `AuthToken` to the `Db`'s models.

- **Setting up** happens at `{prefix}/two-factor`, which the user menu links to. It asks for the
  current password first, through `Authenticator::confirm_password`, so a stolen session cannot
  add a factor its thief holds. That method verifies with the user's `PanelUser::email` as the
  login; override it when users sign in with something else. The page then shows a QR code and
  the key, and turns two-factor sign-in on once a code from the app matches. It then shows eight
  recovery codes, once: the panel stores only their hashes, keyed like the secrets. The pages
  answer `Cache-Control: no-store`.
- **Each panel enrols apart.** A factor signs in to the panel that set it up only, even when other
  panels load the same user type: enrol on each.
- **Signing in**, a correct password leads to `{prefix}/two-factor-challenge` instead of a
  session. The challenge lasts five minutes in its own cookie and takes a code from the app or a
  recovery code. Each code works once: an app code does not sign in twice, and a recovery code is
  used up.
- **Turning it on or off** is a credential change: the account's other sessions and links end.
  Turning it off, and showing new recovery codes, take a code.
- **Codes by email.** `TwoFactor::new().email_codes()` lets a user with an address choose codes
  mailed to it in place of an app, through `Auth::mail`. Each sign-in mails a six-digit code that
  works once, for ten minutes; the challenge page mails a new one on request, in place of the
  last. Each account may have three mailed in fifteen minutes; `TwoFactor::mail_throttle` changes
  the limit. An account holds one factor at a time: it turns one off before setting up the other.
  A panel that drops `email_codes()` mails no more codes: an account set up for them signs in
  with a recovery code. A mail server that fails leaves the challenge up, with its button to send
  another code.
- **Throttling.** Each account may try five codes or passwords a minute, at sign-in and on the
  settings page together; `TwoFactor::throttle` changes the limit. Past it the page answers 429.
  The count lives in each process: behind several, the limit holds per process. Someone who knows
  the password can use up the owner's attempts, and each new sign-in replaces the last challenge.
- **The cookie `Key` seals the secrets.** Rotating it makes every app code fail until its owner
  signs in with a recovery code and sets the app up again; a leaked key opens every secret.
- **Requiring it.** `TwoFactor::new().required()` sends a signed-in user without a second factor
  to `{prefix}/two-factor` before anything else, as an unverified address does: a page redirects
  there and any other request answers 403.
- **Mounting refuses** two-factor sign-in on `Auth::disabled()`, a router without a cookie
  `Key`, and codes by email without `Auth::mail`.

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

- **The request's tenant** is the membership the user selected, else the one
  `PanelUser::default_tenant` names while the user holds it, such as a default the user picks on
  their profile, else their first. Read it with `tenant_id(cx)`; `membership(cx)` returns the
  whole `Membership`. A user with no membership has no tenant.
- **Switching.** With two or more memberships, a tenant menu sits atop the sidebar. It posts to
  `{prefix}/tenant`, which stores the choice on the session and returns to the panel's home
  page.
  A tenant the user is not a member of answers 403, and a stored choice applies only while the
  membership lasts. The session table records the choice, so an existing `auth_sessions` table
  needs a nullable `tenant` column, typed as Toasty stores a `Uuid` on your database. One
  browser acts for one tenant at a time, in every tab.
- **Registering a tenant.** `Auth::tenant_registration` adds a page at `{prefix}/register-tenant`
  where the signed-in user creates a tenant, through a `TenantRegistrar` for the type the
  authenticator loads. Its `Input` is the page's form; `validate` and `register` run in one
  transaction, and `register` writes the tenant and the user's membership of it, answering that
  `Membership`. The session then acts for the new tenant, and the next request's `find_by_id`
  must list it. The tenant menu links to the page, so it shows from one membership on.
- **A user with no membership** on a panel with tenant registration reaches nothing else: a page
  redirects to the registration page and any other request answers 403, as with an unverified
  address, which comes first. A sign-up that creates no tenant lands there.
- **Overrides.** No request header sets the tenant. Server code such as a middleware or a test
  can, by inserting a `Tenant` request extension or putting `Tenant(id)` on the `Cx`.

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-tenant-registration}}
```

`Auth::tenant_profile` adds a settings page at `{prefix}/tenant/profile` for the tenant the
request acts for, through a `TenantEditor`. `may_edit` says which members may open it, by default
all of them; the tenant menu links to it for them, and the page answers 403 to anyone else. `fill`
loads the form from the tenant, and `validate` and `save` run in one transaction. The form names
the tenant it was filled from, so one posted after another tab switched tenants saves nothing:

```rust
{{#include ../../../examples/guide/src/policy_tenancy.rs:policy-tenant-profile}}
```

Mounting refuses a registrar or an editor whose `User` is not the type the authenticator loads,
either page on `Auth::disabled()`, a form field named `csrf_token` (or `editing_tenant` on the
settings page), a file field and a relationship choice.

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
its lens; the name itself is the app's choice. Do not repeat the filter in `query`. Mounting the
panel refuses a `Tenancy::column` lens that is not one field of the model.

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

**A deliberately cross-tenant resource** — a super-admin view — declares no tenancy and filters in
`query()`. That gives up both the tenant check and the scope.

**In your own code**, load rows with `scoped_query::<PostResource>(cx)?`. It applies the tenant
scope and answers 403 when the request has no tenant. `PostResource::query(cx)` does not apply the
tenant scope.

**Foreign keys are re-checked inside the write.** A relationship field's key must be one of the
related resource's records for the request: in its tenant-scoped query and allowed by its `View`.
The create and edit handlers check it when they validate the form, and again inside the write's
transaction, so a related record deleted, moved to another tenant or hidden in between refuses the
write with the same field error. A `create_record` or `update_record` the panel's create and edit
POSTs call needs no check of its own.
