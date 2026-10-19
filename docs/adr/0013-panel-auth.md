# 0013 Authentication is part of the panel

Authentication is on by default for every panel, behind one seam: the app's user type implements
`PanelUser` and an `Authenticator` loads it from credentials or a session id. `Panel` gains no
type parameter; the user travels erased and `auth::user::<U>` reads it back. `Auth::disabled()`
is the explicit opt-out.

- Sessions are server-side rows keyed by token hash, rotated on login, revocable per user, and
  bound to the panel that issued them.
  A shard or procedure serves outside every prefix, so a panel declares the ones its pages use;
  a request to a declared endpoint runs as that panel and ignores other panels' sessions.
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
  Topcoat mail to the address `PanelUser::email` answers. The link is written and mailed after
  the response, so the answer takes as long for an unknown address. A reset ends the account's
  sessions and links like a credential change.
- Email verification is opt-in through a fifth seam, an `EmailVerifier` for the same user type:
  it says whether a user proved their address and records an address a link proved. The gate
  sends an unverified user to the page that mails the link, and answers 403 elsewhere. The link
  verifies on `GET`, signed in or not, and only the address the account still stores.
- With verification on, a new email waits for its link: the profile editor calls
  `auth::request_email_change` in place of writing it, the panel mails the new address a link
  that stores it as a credential change, and the old address a link that keeps it, for a week,
  restoring it if the change went through, and signs every browser out. Unlike the verification
  link, both links act only on the `POST` of the page they open: a scanner on the old mailbox
  would otherwise sign the user out at delivery.
- Tenant registration is opt-in through a sixth seam, a `TenantRegistrar` for the same user
  type: it takes a form input, validates and writes the tenant and the user's membership in one
  transaction, and answers the membership the session then selects. A panel with it sends a user
  with no membership to its page, as it sends an unverified user to theirs: core auth still never
  requires a tenant, a panel that registers them does.
- Every mailed link and pending sign-in is one `AuthToken` row: the token's hash, its purpose,
  the account, the issuing panel and the expiry. A credential change ends the account's sign-in
  tokens; a reset, a cancelled email change and a revocation also end a pending change. A link
  that proves an address or keeps the old one outlives both. One `AuthMail` names the sender
  and the configured base URL every link starts with.

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
- A table per mailed link, and a sender and base URL per feature: each feature repeated the
  hashing, expiry and revocation, and an app repeated its sender. An address method on each
  seam: every seam that mails asks the same question of the same user type.
