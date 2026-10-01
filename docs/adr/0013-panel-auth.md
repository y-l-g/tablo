# Authentication is part of the panel — server-side sessions, one override seam

Date: 2026-09-10 — Status: accepted — Amended: 2026-09-14, 2026-09-24, 2026-09-30, 2026-10-01

## Decision

Authentication is a default-on `Panel` concern in `tablo-core`, behind a feature that is enabled by
default:

- **One override seam, the app's own user type.** An app's user type implements `PanelUser`
  (`user_id`, `display_name`, `can_access_panel`, `tenants`), and an `Authenticator` with
  `type User: PanelUser` maps credentials to it and a session's stored id back to it.
  `Auth::custom(...)` erases the authenticator to a boxed trait object stored per `Panel`, and the
  resolved user travels in the request `Cx` as an `Arc<dyn PanelUser>`. The default `PasswordAuth`
  authenticates against a shipped `AdminUser` model. `Auth::disabled()` is the explicit fail-open
  opt-out; `Panel` gains no type parameter.
- **Read back as the app's type.** `auth::user::<U>(cx)` downcasts to `U` and answers `None` for
  nobody signed in and for a panel whose authenticator loads another type, which a helper shared
  by two panels meets legitimately — so it fails closed rather than panicking.
  `auth::require_user::<U>(cx)` answers as the gate does when nobody is signed in, and 403 for a
  user of another type. Code that needs no field of the user asks `auth::signed_in(cx)`.
- **Server-side sessions.** Topcoat's token transport plus an `AuthSession` Toasty table keyed by the
  token hash: seven-day fixed lifetime, rotated on login, deleted on logout, revocable per user so
  deactivation and a future password-reset spec have a correct revoke-all path. A successful login
  also sweeps up to `SESSION_SWEEP_BATCH` (500) expired rows, whoever owns them (GH #302):
  resolution purges the row it looks up, so without the sweep a row whose owner never returns stays
  forever. The sweep rides login — the write path a user takes once their session has lapsed — so
  cleanup costs one bounded delete per sign-in rather than work on every request, and it is
  best-effort: a failure is logged and does not fail the login. `AuthSession` indexes `expires_at`,
  so the sweep is a range scan and the batch bounds the delete; a migrated table needs that index
  added with the model's schema change.
- **Fail closed.** The panel is gated by default. Unauthenticated page requests redirect to
  `{prefix}/login?next=` (same-origin relative only); runtime endpoints answer 401; valid credentials
  without panel access get the same 403 as a bad password. An auth layer covers the panel prefix and
  the runtime prefix, and panel handlers and shards additionally call `auth::guard`
  (defense in depth, mirroring the shards-authorize-themselves invariant).
- **A session belongs to its panel.** A router mounts several panels, each with its own `Auth`, and
  the `AuthSession` row records the prefix of the panel that signed the user in. A panel's gate
  resolves only its own sessions; another panel's session reads as anonymous there, and
  `auth::user` answers only on the panel whose session resolved the user, so two user tables
  whose ids collide never stand in for each other. The runtime endpoints serve every panel at one
  path, so their gate resolves the session through the auth of the panel that issued it, and a
  live table's shard re-checks the gate of the panel its path names. One browser holds one Topcoat
  session, so signing in to a second panel ends the first: separate concurrent sign-ins would need
  per-panel session cookies, which Topcoat's single token transport does not offer.
- **Argon2id** with PHC-string storage is the password default; login verifies even for unknown users,
  so errors and timing do not enumerate accounts. `auth::verify_password(password, Option<&str>)`
  is public so a custom `Authenticator` gets the same unknown-account work for free.
- **Memberships, not a tenant.** A user lists the tenants they may act for in
  `PanelUser::tenants`, as `Membership { tenant, name }`; `find_by_id` loads them with the user on
  every request, so a removed membership applies at once. The request acts for the tenant the
  session selected while it is still one of them, else the first. The selection lives on the
  `AuthSession` row (`tenant`), set by `POST {prefix}/tenant` — CSRF-checked, refused with 403 for a
  tenant the user is not a member of — and the top bar's switcher posts there when the user has two
  or more memberships. The shipped `AdminUser` belongs to no tenant. Core auth never requires a
  tenant.
- **Scope.** Password reset is not in v1 (separate spec); brute-force limiting stays a deployment
  concern, since an in-process limiter is false safety across instances and account lockout is a DoS
  against the real admin.

Logout is the one route an authenticated-but-no-longer-permitted user may still reach (GH #146): the
gate answers `{prefix}/logout` for any resolved user, and `logout_post` demands a resolved identity
rather than panel permission — clearing the session must not require the access that was just
revoked. The bypass is scoped to the POST method at the exact logout path, and no other handler may
live there. `GET {prefix}/login` is gate-bypassed and re-issues a CSRF token, so a user whose pair
went stale still recovers; with the shipped `PasswordAuth`, deactivation resolves no user at all
(`find_by_id` filters on `active`), so the stranding case exists only for custom `Authenticator`s
whose `find_by_id` keeps resolving a de-permitted user.

## Consequences

A fresh app registers the shipped models, seeds an `AdminUser`, and gets a working login and a gated
panel; an existing app implements two traits and swaps its authenticator in. The showcase signs its
own staff in, each with seats in two blogs, and a core integration test covers both the shipped and
the custom path. Tenant-scoped pages become reachable by logging in — the tenant comes from the
user's memberships. The selected tenant lives on the session, so two tabs share it and a link does
not carry it; a tenant segment in the URL would give each tab its own, at the cost of making every
panel URL per-request, and `PanelUser::tenants` would not change for it. `tablo-core` grows its first production Toasty
models, and the Argon2 and session dependencies are unconditional: `Auth::disabled()` is the one
opt-out, so an ungated panel is always a line of app code. Password reset, registration, 2FA, and roles/RBAC remain open, each with
a seam that does not need reopening: per-user session revocation, per-`Panel` `Auth` values, and
`can_access_panel`.

Rejected: stateless signed-cookie sessions (no revocation), making `Panel` generic over the user type
(it would make every framework type generic; the erased user downcasts back to the app's type where
app code reads it), a fixed erased identity struct (app code could not read its own fields, such as a
role, without a second lookup), a cookie for the selected tenant (Topcoat serves shards at its
runtime path, where a cookie scoped to the panel's path does not arrive), and requiring apps to build
auth on generic Topcoat sessions themselves (it breaks the default-panel promise). Account lockout and
an in-process rate limiter are covered by the scope bullet above.
