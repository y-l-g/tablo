# 0013 Authentication is part of the panel

Authentication is on by default for every panel, behind one seam: the app's user type implements
`PanelUser` and an `Authenticator` loads it from credentials or a session id. `Panel` gains no
type parameter; the user travels erased and `auth::user::<U>` reads it back. `Auth::disabled()`
is the explicit opt-out.

- Sessions are server-side rows keyed by token hash, rotated on login, revocable per user, and
  bound to the panel that issued them. A shard or procedure serves outside every prefix, so a
  panel declares the ones its pages use, and a request to one runs as that panel.
- The gate fails closed, and a user without access is refused like a bad password. Passwords use
  Argon2id, and login verifies even for an unknown user.
- A user has tenant memberships; the session selects one. Core auth never requires a tenant; a
  panel that registers tenants does.
- Every other feature is opt-in, through a seam for the same user type when it writes the app's
  own rows: sign-up (`Registrar`), the profile page (`ProfileEditor`), password reset
  (`PasswordResetter`), email verification (`EmailVerifier`), tenant registration
  (`TenantRegistrar`) and tenant settings (`TenantEditor`). Each validates and writes in one
  transaction. A credential change ends the user's sessions and links on every panel that loads
  the same user type, in the transaction that saves it.
- A mailed link never acts on a `GET` a mail scanner would follow when acting signs someone out;
  one that only proves an address does, since a click less is worth more.
- Two-factor sign-in needs no seam: Tablo stores each account's factor as `AuthFactor` rows, per
  panel, with the authenticator secret sealed under a key derived from the router's cookie `Key`
  and mailed codes hashed under another, so a database read alone yields neither. A correct
  password issues a short-lived `AuthToken` instead of a session until the second factor passes.
- Every mailed link and pending sign-in is one `AuthToken` row: the token's hash, its purpose, the
  account, the issuing panel and the expiry. One `AuthMail` names the sender and the configured
  base URL every link starts with.

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
- A verification link opened by `POST` behind a confirm button, which a mail scanner would not
  press: the scanner reads the mailbox the link proves, and a click less is worth more. A
  verification link that needs the account's session: the link is often opened on another device.
- Tablo writing the new email itself after `ProfileEditor::save`: it cannot tell which field of
  an app's own input is the address. A new `ProfileUpdate` variant carrying the address: the
  editor would still have to skip writing it, and the variant would not say so.
- A tenant model Tablo ships, or a `Tenant` trait the app's tenant type implements: tenants are
  the app's own rows in its own shape, and a membership is all a request needs of one. Tenant
  registration as a resource create page: it would not seat the user in the tenant it creates,
  nor make the session act for it.
- Two-factor columns on the app's user table behind a seam, as Fortify adds them to `users`:
  every app would migrate its table and implement a seam for state Tablo alone reads. A secret
  stored in the clear: a database read would yield every account's second factor. A key of its
  own: the cookie `Key` is already the secret an app keeps. A session row in a half-signed-in
  state: every gate would have to check it, where a challenge token opens nothing.
- A table per mailed link, and a sender and base URL per feature: each feature repeated the
  hashing, expiry and revocation, and an app repeated its sender. An address method on each
  seam: every seam that mails asks the same question of the same user type.
