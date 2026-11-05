# Adding a panel to an existing app

A panel mounts on the app's own Topcoat router and reads the app's `toasty::Db`, so an app with
its own users and sign-in adds one without giving either up. This chapter covers what the panel
needs from the app and the two ways to connect their sign-ins.

## What the panel needs

- **A Topcoat router and a Toasty `Db`.** `Panel` mounts on a `RouterBuilder` and its resources
  read Toasty models. An app on another framework runs the panel as a separate binary over the
  same database, or puts a Topcoat router in front of its existing service with
  [`TowerRoute`](https://docs.rs/topcoat/latest/topcoat/router/tower/struct.TowerRoute.html).
- **Models over the app's tables.** A resource needs a `toasty::Model`. Name a table Toasty would
  not derive with `#[table = "…"]`.
- **Tablo's tables.** Add `AuthSession` to the `Db`'s models, plus the
  [others its features name](./policy-auth-tenancy.md#authentication), and generate a migration
  for them as [Deployment](./deployment.md) shows. An app with a model named as one of Tablo's,
  such as its own `AdminUser`, lists its models with `toasty::models!`.
- **A prefix of its own.** The gate covers the panel's prefix, and a `#[layout("/")]` would wrap
  the panel too: give the app's layout a narrower path.

## Mounting beside the app's sessions

The panel keeps its sessions in its own `__Host-tablo_session` cookie. The router's
`SessionConfig` stays the app's, so a sign-in, a sign-out or a session lifetime on one side never
touches the other:

```rust
{{#include ../../../examples/guide/src/existing_app.rs:existing-app-router-body}}
```

Install the app's own pathless layers before `.panel(..)`, which installs Topcoat's runtime layer.

## Choosing who signs users in

| The app has | Use | The user sees |
| --- | --- | --- |
| a user table with passwords | [`Auth::custom`](./policy-auth-tenancy.md#your-own-user-table) | the panel's login form, a second sign-in |
| its own login page, SSO or magic links | `Auth::external` | the app's login page, once |

## Signing in through the app

Implement [`ExternalAuth`](https://y-l.fr/tablo/nightly/api/tablo/trait.ExternalAuth.html) over the
app's session and pass it to `Auth::external`:

```rust
{{#include ../../../examples/guide/src/existing_app.rs:existing-app-external}}
```

The user type implements `PanelUser` as with any authenticator, and its `can_access_panel` decides
who among the app's users enters: a user it refuses gets a 403 page and no panel session.

A visitor with no app session follows this path:

1. A page under the prefix redirects to `/admin/login?next=…`, as on any panel.
2. That route finds no app user and redirects to `login_url`, whose `next` is the route's own URL.
3. The app signs the user in and redirects to `next`.
4. The route finds the app's user, starts a panel session and lands on the page from step 1.

`next` reaches the app through the browser, so check it before following it:

```rust
{{#include ../../../examples/guide/src/existing_app.rs:existing-app-return}}
```

The gate asks `current_user` on every request, and serves a session only while it names the
session's user. Signing out of the app, or into another account, shuts the panel at its next
request, with no call to `auth::revoke_sessions_for_user`. For that to reach a live connection,
which keeps the cookies of its handshake, `current_user` must consult a session the server holds.
A browser that returns to the same app user finds its panel session, and its tenant, as it left
them. The app's session cookie must reach a page opened from another site, as `SameSite=Lax` does;
behind `SameSite=Strict` a followed link finds no user.

The shell's Sign out ends the panel's session alone, then sends the browser to `logout_url` by GET,
on any origin. Point that at a route that ends the app's session, an SSO provider's end-session URL
included, or at a page of the app's that offers to. While the app's session lives, the next visit
to the panel signs the user in again.

A browser holds one panel session across every panel on the router, so entering this panel ends
a session held on another.

The app owns the account, so mounting refuses `Auth::registration`, `Auth::password_reset`,
`Auth::email_verification` and `Auth::two_factor` beside `Auth::external`. The panel's tenant menu,
profile page and notifications work as on any panel.

## Keep the gate on

`Auth::disabled()` with a layer of the app's over the prefix leaves the panel open: its search and
other endpoints serve at `/_topcoat/runtime`, outside the prefix, and answer anyone. `Auth::external`
puts the same check in front of both.
