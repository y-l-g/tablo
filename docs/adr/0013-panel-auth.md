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
- The profile page is opt-in through a third seam, a `ProfileEditor` for the same user type: it
  fills a form input from the signed-in user, validates and writes in one transaction, and says
  whether a credential changed. A credential change ends the user's sessions and reset links on
  every panel that loads the same user type, in the transaction that saves, and starts a new
  session for the browser that saved.
- Password reset is opt-in through a fourth seam, a `PasswordResetter` for the same user type:
  it finds an account by email and sets its password. Tablo mails a single-use link through
  Topcoat mail, stores only the token's hash, and starts the link with a configured base URL. The
  link is written and mailed after the response, so the answer takes as long for an unknown
  address. A reset ends the account's sessions and links like a credential change.

## Rejected

- Stateless signed-cookie sessions, a `Panel` type parameter, a fixed erased identity struct, a
  tenant cookie, and app-built auth on generic Topcoat sessions.
- A `register` method on `Authenticator` with a refusing default: mounting could not tell an
  authenticator that registers from one that does not. A fixed sign-up form: an app that asks
  for more than a name, an email and a password would rebuild the page.
- Profile editing as a resource over the user table: its edit page would need the user's own
  record and a policy that admits only it, and nothing would ask for the current password or
  revoke sessions when a credential changes.
- Reset links built from the request's `Host` header: the client chooses it, so a forged request
  would mail the victim a link to another host. A reset behind a Tablo cargo feature: the mail
  builder adds little to the dependency tree, and SMTP stays behind Topcoat's own feature.
