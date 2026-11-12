# 0040 The audit log is built on the public API, in a crate of its own

The audit log was the first built-in wired through `tablo-core`'s private state: its resource read
the panel's registry, its hook read the session, the mount checked its model by name, an area
group placed its list through a variant of its own, and auth wrote impersonation into it. Whatever
it reached privately is what an app building the same kind of feature cannot reach. `tablo-audit`
now builds it from the public API alone, and the core gains each piece it needed, as API any app
or crate can call:

| The audit log needed | The core now offers |
| --- | --- |
| who did it, the impersonator over the user they act as | `auth::actor(cx)` |
| the resources a reader may list, their labels and hidden fields | `mounted_resources(cx)` |
| a read-only resource whatever policy the app sets | `ResourceDef::restrict` |
| a list that refuses a panel without its trail | `ResourceDef::requires_commit_hook` |
| the impersonation events auth records | `AuthHook`, `AuthEvent`, `Panel::auth_hook` |
| a mount that refuses a `Db` without its model | `Panel::require_model` |
| its words, in the locales a panel offers | `Panel::default_translations` |
| its markup's classes in the app's stylesheet | `tablo_build` scans each `DEP_*_TABLO_SRC` a crate publishes |

- No plugin trait. `tablo-audit` is an extension trait on `Panel` that registers a commit hook, an
  auth hook, a required model, its catalogs and a resource, each through a method an app could
  call itself. A `Plugin` trait would add a second way to register each of those; it waits until
  a built-in needs something the builder methods cannot express.
- The list is an ordinary resource. An area group places it as it places any resource, with
  `resource_with::<AuditLog>`, so the area's special entry and its two declaration faults go.
- `restrict` caps a resource's abilities whatever `policy` sets later, so the log's own `declare`
  keeps it read-only through the def an app's customization returns. It is a policy, not a flag:
  the same rule a page, an action and an export already check.
- A crate's catalog sits under Tablo's and the app's, so it rewords none of Tablo's messages, and
  a locale the panel does not offer is skipped rather than refused: the crate ships every
  language it has, the app offers the ones it wants.
- Registering a hook, a model or a catalog twice changes nothing, so calling `audit_trail` again,
  or beside `audit_log`, records each write once.
- `ResourceDef::requires_commit_hook` lets the log refuse a panel that does not run its trail:
  placed alone, it would mount and list nothing.
- A stop of impersonation goes through when a hook fails to record it, logged: a session that
  cannot stop acting as someone else, or sign out, is worse than a missing entry.
- The facade re-exports the crate as `tablo::audit` behind the `audit` feature, so an app still
  names one Tablo dependency.

Notifications, import and impersonation stay in the core. Impersonation is part of the session and
its gate; moving it out would widen what a crate may do to a session for no gain. Import and
notifications move only when one of them needs a seam this decision does not give.

## Rejected

- Keeping the audit log in the core and listing the private calls it makes: nothing would keep
  the list current, and nothing would prove an app can rebuild it.
- A `Plugin` trait with a `register(panel)` method: every built-in would register through it and
  through the builder, two paths for one thing.
- Making `PanelState` and the session public: they hold what the core must change freely, such as
  the relation children and the realm, and a crate reading them would break on each change.
