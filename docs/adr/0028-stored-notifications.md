# 0028 Stored notifications ride the runtime connection

A stored notification is a `Notification`, the type a write flashes, kept as a
`StoredNotification` row for one panel user on one panel, and for one tenant or none. Its content
is one JSON column, so a field `Notification` gains needs no migration. `Notifications` sends it:
`Notifications::of(cx)` in a request, for the request's tenant, and
`PanelHandle::notifications()` in a job. `Panel::notifications()` shows the bell.

- The bell is one shard whose two live regions ask for a connection. Once the page loads, they
  re-render over Topcoat's runtime socket, which every runtime render already shares: one run per
  tab, the auth gate and the shard's guard on each connect, reconnection, and CSWSH checks are
  Topcoat's. Without a socket the bell shows what the page loaded with.
- A send wakes the recipient's bells through an in-process `NotificationBus` that carries a panel
  and a user id, never content. A woken bell checks its session again, then reads back through
  the `Inbox` filter: the signed-in user, the request's panel, the acting tenant or none. Every
  read and write of the bell filters through it, never through an id alone.
- `revoke_sessions_for_user` wakes the user's bells, so an open tab stops at once; any other end
  of a session stops it at its next wake, before it shows anything new.
- Marking read, removing, marking all read and clearing are procedures, applied in place: the
  procedure wakes the bells, and each connected bell re-renders itself. This departs from
  [0026](0026-topcoat-runtime-only.md), whose writes are form posts that redirect back: a
  redirect would reload the page to change one row of a sheet. The procedures are covered by
  Topcoat's origin check and the session cookie's `SameSite=Lax`, not by Tablo's CSRF token.
- An action's URL renders only as a same-origin path or an `http` or `https` URL.

## Rejected

- A WebSocket route of Tablo's own: a second socket per tab, and a client script, a protocol,
  reconnection and origin checks the runtime already has.
- Polling, as Filament does: a request per tab per interval, and minutes of delay.
- Sending content over the bus: a woken bell would show what its own query never read.
- A bus shared across processes now: the in-process bus ships first; one process wakes only its
  own bells until a bus behind the same seam, such as Postgres `LISTEN/NOTIFY`, lands.
- A row per field of `Notification`: every new field would be a migration.
