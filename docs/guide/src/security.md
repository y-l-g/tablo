# Security

What Tablo does by default, and what your deployment must provide for those defaults to hold.

## Your deployment must provide

- **HTTPS.** The session, CSRF and notification cookies are `__Host-` cookies marked `Secure`.
  Browsers accept them over plain HTTP only on `localhost`; anywhere else, without HTTPS, the
  browser drops them and every form POST fails its CSRF check with 403.
- **The client's address behind a proxy.** Tablo throttles sign-ins per login and client address
  (see [Authentication](#authentication)). Behind a reverse proxy, trust it with the router's
  `TrustedProxies` so the address is the client's; otherwise every request carries the proxy's,
  and a guesser who knows a login can lock its owner out.
- **Per-IP and multi-replica rate limiting.** Limit attempts per IP, and across replicas, at your
  proxy or firewall: the panel's throttle does not limit one address across logins, one login
  across addresses, or the spellings of a login that your database matches as one, such as
  accented variants under MySQL's default collation.
- **Session revocation.** Call `auth::revoke_sessions_for_user` when a password changes or an
  account is deactivated; see [Authentication](./policy-auth-tenancy.md#authentication).

## Requests and writes

- Every panel POST, login and logout included, verifies a double-submit CSRF token before touching
  the database; a missing or wrong token answers 403.
- Delete and bulk-delete POSTs also require the `confirm=1` marker the confirmation dialog sends.
  It prevents accidental deletes; it is not a security boundary.
- A form POST with a key the form does not declare answers 400, so a client cannot write a
  field the form does not show, such as `role` or `tenant_id`.
- Update, delete and custom action handlers load the target through the resource's scoped query
  inside the write's transaction and check policy on that row. See
  [Policy](./policy-auth-tenancy.md#policy).
- Table search escapes `%` and `_` and binds the term as a parameter; no user input is
  interpolated into SQL.

## Authentication

- Passwords are hashed with Argon2id. A login for an unknown email verifies against a dummy hash,
  so it takes as long as a wrong password, and every failure shows the same message.
- Each login may make five sign-in attempts a minute from one client address. A sixth answers the
  same 403 and message as a wrong password without checking the password, and logs a warning; a
  successful sign-in clears that address's count. A guesser elsewhere keeps their own count, so
  they cannot lock the owner out.
  `Auth::password().throttle(LoginThrottle::new(10, Duration::from_secs(300)))` changes the limit
  and `LoginThrottle::off()` removes it. Counts live in the process and the panel: each replica
  and each panel counts its own, and a restart clears them. Past 10,000 counts the oldest is
  dropped.
- Sessions are stored server-side, keyed by a hash of the cookie's token, and replaced on login.
- A redirect after login follows `next` only to a same-origin path.

## Responses

- **Framing.** Every response to a request under the panel prefix, including its 404 and 405
  pages, carries `Content-Security-Policy: frame-ancestors 'self'`, so another site cannot frame
  the admin. The app's own routes outside the prefix carry whatever policy the app sets. `Panel::frame_ancestors("'self' https://intranet.example")` changes the directive, and
  `Panel::without_frame_ancestors()` omits it for a proxy that sets its own policy. A
  `Content-Security-Policy` your own handler sets is kept. The router answers three errors before
  any layer runs, so they carry no header: the 403 for a cross-site request, the 400 for a
  malformed `x-topcoat-identity` header, and the 500 for a panic.
- **Redirects.** Return `Err(redirect(..))` (307) from a `GET` and `Err(see_other(..))` (303) after
  a write, so a reload does not repeat it. A redirect raised after streaming started becomes a
  `window.location.replace`. Wrap a layout's `Slot` in `error_boundary` for a branded error page.
- **Notifications** are one-time flash cookies set on the redirect after a write and consumed by
  the next page, so a reload does not show them again.

## Uploaded files

- A file field stores only a value that came from a file part: the uploader's answer, the stored
  value on an untouched edit, or nothing when cleared. A path typed into the form is never stored.
  See [File uploads](./forms.md#file-uploads).
- A stored value renders as a link only when it is a root-relative path (`/…`, not `//host`) or an
  `http(s)` URL; anything else, such as `javascript:`, renders as text.
- Files served with `Panel::serve_dir` share the panel's origin, so every file response carries
  `X-Content-Type-Options: nosniff`, a sandboxing `Content-Security-Policy`, and
  `Content-Disposition: attachment` unless the file is a common image, audio or video type or
  plain text. An uploaded HTML or SVG file therefore downloads instead of running script. The
  policy is fixed; serve active documents from another origin.
- Served directories are **public**: the auth gate does not cover them.
