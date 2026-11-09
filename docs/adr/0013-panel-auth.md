# 0013 Authentication is part of the panel

Authentication is on by default for every panel, behind one seam: the app's user type implements
`PanelUser` and an `Authenticator` loads it from credentials or a session id. `Panel` gains no
type parameter; the user travels erased and `auth::user::<U>` reads it back. `Auth::disabled()`
is the explicit opt-out.

- Sessions are server-side rows keyed by token hash, rotated on login, revocable per user, and
  bound to the panel that issued them. Each panel has its own cookie, named from its prefix since
  `__Host-` fixes `Path=/`, so a browser stays signed in to several panels. A shard or procedure serves outside every prefix, so a
  panel declares the ones its pages use, and a request to one runs as that panel.
- A runtime request also runs only as the panel of the page that sent it. A fetch names its page
  in the `Referer`; a socket's runs carry a header stamped from the handshake's URL, which the
  runtime lets no run set. The name picks which of the request's cookies counts, so forging it
  grants nothing. A request naming no panel, such as one from a page outside every panel, runs on
  the one session it holds among the panels declaring the endpoint; holding two, it is anonymous,
  rather than Tablo guessing. Tablo's documents carry
  an inline script of their panel, or of no panel: Topcoat loads a page whose scripts this document
  has not run, so moving between them opens a new socket. A host's own document carries none, so a
  link from a panel to one keeps the socket, which still answers as that panel. Runs on a socket
  Tablo's stamp never saw, because the app installed the runtime layer after the panels, are
  refused.
- The panel of a page decides which session a request means; it is no boundary between panels.
  They share an origin, and cookies at `Path=/`, so a script on one panel's page can send a request
  as any other panel the browser is signed in to. Keeping the two sessions apart from a script
  takes a separate origin for each.
- A session's token rides in Tablo's own cookie, never the router's `SessionTokenStore`: a router has
  one of those, and a panel writing it would replace a host app's Topcoat session on every login.
- An app that signs its users in itself keeps doing so through `ExternalAuth`, set by
  `Auth::external`: Tablo asks it for the request's user. The panel still keeps its own session
  row, which holds the selected tenant and binds the browser to the panel, and it serves only
  while the app's session names the row's user. The gates read that session as any other, so
  external auth adds no path through them that could fail open. The row outlives a request whose
  app session names no one, because a live connection's cookies can be older than the browser's.
- The gate fails closed, and a user without access is refused like a bad password. Passwords use
  Argon2id, and login verifies even for an unknown user.
- A user has tenant memberships; the session selects one. Core auth never requires a tenant; a
  panel that registers tenants does.
- Every other feature is opt-in, through a seam for the same user type when it writes the app's
  own rows: sign-up (`Registrar`), the profile page (`ProfileEditor`), password reset
  (`PasswordResetter`), email verification (`EmailVerifier`), tenant registration
  (`TenantRegistrar`) and tenant settings (`TenantEditor`). `Auth<U>` carries the
  authenticator's user type until `Panel::auth` erases it, so a seam over another type does not
  compile. Each validates and writes in one transaction. A credential change ends the user's sessions and links on every panel that loads
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
- A panel session reading a router's `SessionTokenStore` and `SessionLifetime` when the app
  installs them: its lifetime and token would be the app's, and the two sign-ins would still share
  one cookie.
- `ExternalAuth` with no session row, the app's session alone signing a request in: the tenant
  switch would need a tenant cookie, and each gate a second path with no row behind it.
- One cookie for every panel: a browser held one session, so signing in to a panel ended the last.
- Telling panels apart by cookie path: `__Host-` requires `Path=/`, and the runtime serves outside
  every prefix, so a path-scoped cookie would not reach it.
- A `sign_in` function an app's route calls after its own login: every app would write the same
  route, and the panel would keep a session after the app's ended unless the app revoked it.
- `Auth::disabled()` with a layer of the app's over the prefix: the runtime path stays open.
- Answering a cross-panel navigation from the gate, by its `Referer`: a stripped header would keep
  the socket, where the document's own script needs nothing from the request.
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
