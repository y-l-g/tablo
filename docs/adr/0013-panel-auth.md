# Authentication is part of the panel — server-side sessions, one override seam

Date: 2026-09-10 — Status: accepted

## Decision

Authentication is a default-on `Panel` concern in `tablo-core`:

- **One override seam.** An app user type implements `PanelUser` (`user_id`, `display_name`,
  `can_access_panel`, `tenants`); an `Authenticator` maps credentials to it and session IDs back
  to it. `Auth::custom(...)` stores the erased authenticator per `Panel`; the user travels in `Cx`
  as `Arc<dyn PanelUser>`. The default `PasswordAuth` checks a shipped `AdminUser` model.
  `Auth::disabled()` is the explicit fail-open opt-out; `Panel` gains no type parameter.
- **Read back as the app type.** `auth::user::<U>(cx)` downcasts and answers `None` for anonymous
  requests and foreign types; `auth::require_user::<U>(cx)` answers as the gate does when
  anonymous, 403 for a foreign type. Code needing no user field asks `auth::signed_in(cx)`.
- **Server-side sessions.** Topcoat token transport plus an `AuthSession` table keyed by token
  hash: seven-day fixed lifetime, rotated on login, deleted on logout, revocable per user. Login
  sweeps up to 500 expired rows; the sweep is best-effort and never fails login. `AuthSession`
  indexes `expires_at`.
- **Fail closed.** Unauthenticated pages redirect to `{prefix}/login?next=` (same-origin relative
  only); runtime endpoints answer 401; valid credentials without access answer 403 like a bad
  password. One auth layer covers panel and runtime prefixes; handlers and shards also call
  `auth::guard`.
- **A session belongs to its panel.** The row records the issuing prefix; a gate resolves only its
  own sessions, and `auth::user` answers only there. One browser holds one Topcoat session, so a
  second sign-in ends the first.
- **Argon2id** with PHC storage is the password default; login verifies even for unknown users.
  `auth::verify_password` is public for custom authenticators.
- **Memberships, not a tenant.** `PanelUser::tenants` lists `Membership { tenant, name }`;
  `find_by_id` loads them per request. The request acts for the session-selected tenant while it
  remains a membership, else the first. Selection lives on the row, set by `POST {prefix}/tenant`
  (CSRF-checked, 403 for non-members); the top-bar switcher posts there with two or more
  memberships. The shipped `AdminUser` belongs to no tenant. Core auth never requires a tenant.
- **Scope.** Password reset stays out of v1; brute-force limiting stays a deployment concern.

Logout stays reachable for a de-permitted user: the gate answers `{prefix}/logout` for any
resolved user, and `logout_post` demands identity rather than permission. `GET {prefix}/login` is
gate-bypassed and re-issues CSRF. With `PasswordAuth`, deactivation resolves no user at all.

Rejected: stateless signed-cookie sessions, a `Panel` type parameter, a fixed erased identity
struct, a tenant cookie, and app-built auth on generic Topcoat sessions.
