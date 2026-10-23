# Security

What Tablo does by default, and what your deployment must provide for those defaults to hold.

## Your deployment must provide

- **HTTPS.** The session, CSRF and notification cookies are `__Host-` cookies marked `Secure`.
  Browsers accept them over plain HTTP only on `localhost`; anywhere else, without HTTPS, the
  browser drops them and every form POST fails its CSRF check with 403.
- **The client's address behind a proxy.** Tablo throttles sign-ins per login and client address.
  Behind a reverse proxy, trust it with the router's `TrustedProxies` so the address is the
  client's; otherwise a guesser who knows a login can lock its owner out.
- **Per-IP and multi-replica rate limiting** at your proxy or firewall, for what the panel's
  throttle does not count: [`LoginThrottle`](https://y-l.fr/tablo/nightly/api/tablo/struct.LoginThrottle.html) lists it.
- **Session revocation.** Call
  [`auth::revoke_sessions_for_user`](https://y-l.fr/tablo/nightly/api/tablo/auth/fn.revoke_sessions_for_user.html) when a password
  changes outside the panel, or a custom user's credentials change. A panel write that changes
  an `AdminUser`'s email or password hash ends that admin's sessions itself, and deactivating one
  ends them at its next request.

## Requests and writes

- Every panel POST, login and logout included, verifies a double-submit
  [CSRF](https://y-l.fr/tablo/nightly/api/tablo/csrf/index.html) token before any write it asks for; a missing or wrong token answers
  403.
- Delete and bulk-delete POSTs also require the `confirm=1` marker the confirmation dialog sends.
  It prevents accidental deletes; it is not a security boundary.
- A form POST with a key the form does not declare answers 400, so a client cannot write a
  field the form does not show, such as `role` or `tenant_id`
  ([`RecordForm`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.RecordForm.html)).
- Update, delete and custom action handlers load the target through the resource's scoped query
  inside the write's transaction and check policy on that row. See
  [Policy](./policy-auth-tenancy.md#policy).
- Table search binds the term as a parameter; no user input is interpolated into SQL
  ([`Table`](https://y-l.fr/tablo/nightly/api/tablo/struct.Table.html)).

## Authentication

- Passwords are hashed with Argon2id, and a login for an unknown email takes as long as a wrong
  password: [`PasswordAuth`](https://y-l.fr/tablo/nightly/api/tablo/struct.PasswordAuth.html).
- Sign-ins are throttled per login and client address, five a minute unless
  [`Auth::throttle`](https://y-l.fr/tablo/nightly/api/tablo/struct.Auth.html#method.throttle) says otherwise, and sign-ups per
  address ([`Auth::sign_up_throttle`](https://y-l.fr/tablo/nightly/api/tablo/struct.Auth.html#method.sign_up_throttle)).
- Sessions are stored server-side as a hash of the cookie's token, and replaced on login and on
  sign-up: [`AuthSession`](https://y-l.fr/tablo/nightly/api/tablo/auth/struct.AuthSession.html).
- A redirect after login follows `next` only to a same-origin path.

## Responses

- **Headers.** Every response under the panel prefix carries a `Content-Security-Policy` that loads
  script files from the panel's origin only and lets no other site frame the panel or receive its
  forms, along with
  `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin` and `Cache-Control: no-store`,
  so the back button after sign-out shows no record. Topcoat's live regions need inline scripts,
  so the policy does not stop an injected script; escaping does.
  [`Panel::content_security_policy`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.content_security_policy)
  replaces the policy, for assets on another origin or a panel framed by an intranet, and lists
  the responses the headers miss. Behind a proxy that serves HTTPS only, turn on HSTS with
  [`Panel::strict_transport_security`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.strict_transport_security).
- **Redirects.** Return `Err(redirect(..))` (307) from a `GET` and `Err(see_other(..))` (303) after
  a write, so a reload does not repeat it. A redirect raised after streaming started becomes a
  `window.location.replace`. A failed page load shows the panel's
  [error page](./panel-and-routing.md#the-shell-layout); wrap part of a page in `error_boundary`
  to show a fallback in its place instead.
- **Notifications** are one-time flash cookies set on the redirect after a write and consumed by
  the next page, so a reload does not show them again.
- **Rich text** is the one stored value Tablo renders as HTML, cleaned to the elements its
  toolbar's tools produce. A page of your own renders one through
  [`rich_text::clean`](https://y-l.fr/tablo/nightly/api/tablo/rich_text/fn.clean.html), never unescaped as stored.

## Uploaded files

- A file field stores only a value that came from a file part, never a path typed into the form,
  and renders a stored value as a link only when it is a root-relative path or an `http(s)` URL:
  [`Uploader`](https://y-l.fr/tablo/nightly/api/tablo/trait.Uploader.html).
- `Panel::uploads_dir` serves a file only at a signed URL that expires within two hours, which a
  page renders after checking the user may see the record, so a link copied into a chat or a log
  stops working. The signature covers a path, not a record: a column showing a value users type
  signs whatever path they type, so show stored uploads only from file fields:
  [`Panel::uploads_dir`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.uploads_dir).
- Files served with `Panel::serve_dir` or `uploads_dir` share the panel's origin, so they carry
  headers that make an uploaded HTML or SVG file download instead of running script. A
  `serve_dir` directory is **public** unless it sits under the panel prefix:
  [`Panel::serve_dir`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.serve_dir).
