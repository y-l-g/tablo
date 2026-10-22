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
newest first, with their links. Once the page loads, it keeps updating over Topcoat's runtime
connection, so a notification sent while the page is open shows at once, as a toast. The panel
needs auth and the `StoredNotification` model, which `tablo::models!` registers.
[`Panel::notifications`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.notifications) has the rules.

## Sending

In a request, `Notifications::of(cx)` sends on the request's panel, for the tenant the request
acts for. `send` takes the recipient's `PanelUser::user_id`:

```rust
{{#include ../../../examples/guide/src/notifications.rs:send}}
```

`send` writes the row on its own connection and commits it at once. Inside a write's
transaction, in a record fn, an action's `run` or `Resource::before_commit`, send through the
transaction instead, so the notification commits with the write or not at all;
[`Notifications::send_in`](https://y-l.fr/tablo/nightly/api/tablo/struct.Notifications.html#method.send_in)
has the rules:

```rust
{{#include ../../../examples/guide/src/notifications.rs:send_in}}
```

A job sends through the panel's handle, for no tenant until `tenant` names one:

```rust
{{#include ../../../examples/guide/src/notifications.rs:job}}
```

## Who sees what

The bell lists a user's notifications on its own panel: those sent for the tenant they act for,
and those sent for no tenant. A notification for one tenant therefore never shows to a member of
another, and one sent for no tenant, such as a job's, shows whichever tenant the user acts for.

## Keeping them

Nothing deletes stored notifications on its own. A job prunes the ones older than the app keeps:

```rust
{{#include ../../../examples/guide/src/notifications.rs:prune}}
```

## One process

A send wakes only the bells open on the process that sent it: behind a load balancer, a bell on
another process shows the notification on its next page load.
