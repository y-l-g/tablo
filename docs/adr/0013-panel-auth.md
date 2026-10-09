# 0013 Authentication is part of the panel

Authentication is on by default for every panel, behind one seam: the app's user type implements
`PanelUser` and an `Authenticator` loads it from credentials or a session id. `Panel` gains no
type parameter; the user travels erased and `auth::user::<U>` reads it back. `Auth::disabled()`
is the explicit opt-out.

- Sessions are server-side rows keyed by token hash, rotated on login, revocable per user, and
  bound to the panel that issued them.
- The gate fails closed: pages redirect to login, runtime endpoints answer 401, and a user without
  access is refused like a bad password.
- Passwords use Argon2id, and login verifies even for an unknown user.
- A user has tenant memberships; the session selects one. Core auth never requires a tenant.
- Sign-up is opt-in through a second seam, a `Registrar` beside the `Authenticator` and for the
  same user type: it takes an action input, validates and writes in one transaction, and signs
  the new account in only when it has panel access.

## Rejected

- Stateless signed-cookie sessions, a `Panel` type parameter, a fixed erased identity struct, a
  tenant cookie, and app-built auth on generic Topcoat sessions.
- A `register` method on `Authenticator` with a refusing default: mounting could not tell an
  authenticator that registers from one that does not. A fixed sign-up form: an app that asks
  for more than a name, an email and a password would rebuild the page.
