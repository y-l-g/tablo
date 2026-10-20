# Notifications

A `Notification` is a status, a title, an optional description and optional links. An action
[flashes one](./actions.md) as a toast after its write. Sent with `Notifications`, the same value
is stored for one panel user and listed in the bell of a panel that shows one.

## The bell

`Panel::notifications` shows a bell at the end of the topbar:

```rust
{{#include ../../../examples/guide/src/notifications.rs:enable}}
```

The bell counts the signed-in user's unread notifications and opens a sheet that lists them,
newest first, with their links. Each can be marked read or unread and removed, and the sheet marks
them all read or clears them. Once the page loads, the bell keeps updating over Topcoat's runtime
connection: a notification sent while the page is open joins the list, raises the count and shows
as a toast, and a change made in one tab shows in the user's others.

The panel needs auth, and its `Db` the `StoredNotification` model, which `tablo::models!`
registers; the mount refuses it otherwise.

## Sending

In a request, `Notifications::of(cx)` sends on the request's panel, for the tenant the request
acts for. `send` takes the recipient's `PanelUser::user_id`:

```rust
{{#include ../../../examples/guide/src/notifications.rs:send}}
```

`send` writes the row and commits on its own, so send from `Resource::after_commit` rather than
from inside an action's `run`: a notification sent from `run` stays when the write rolls back.

A job sends through the panel's handle, for no tenant until `tenant` names one:

```rust
{{#include ../../../examples/guide/src/notifications.rs:job}}
```

## Who sees what

The bell lists a user's notifications on its own panel: those sent for the tenant they act for,
and those sent for no tenant. Its procedures mark and remove only those: an id the user's bell
does not list changes nothing.

A link renders only when its URL is a same-origin path such as `/admin/orders/42` or an `http` or
`https` URL.

A bell that is open over the runtime connection checks the session again before it shows
anything new, and `auth::revoke_sessions_for_user` stops the user's open bells at once.

## Keeping them

Nothing deletes stored notifications on its own. A job prunes the ones older than the app keeps:

```rust
{{#include ../../../examples/guide/src/notifications.rs:prune}}
```

## One process

A send wakes the bells open on the process that sent it. Behind a load balancer, a bell on
another process shows the notification on its next page load.
