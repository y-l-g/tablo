# A resource declares one value, and each panel owns what it mounts

Date: 2026-10-05 — Status: accepted

## Decision

`Resource::declare` returns a `ResourceDef<Self>`: every setting that takes no request — slug,
labels, navigation, policy, tenancy, table, form, view, relations, actions, create columns — as
one builder value with a default for each. The `Resource` trait keeps the associated types and the
methods that take a request: `query`, `view_query`, the display hooks, `validate_record`, and the
record fns.

A panel registers resources lazily. Mounting builds each def once, binds its declarations to the app schema,
fills its defaults into a `Mounted<R>`, and keeps it in the panel's state; every handler and every
cross-reference (a relation, a relationship field's options, `can`, `scoped_query`) reads the
request panel's copy. `Panel::resource_with` adjusts the def for one panel, so one resource type
can mount read-only in one panel and writable in another. A resource the request's panel does not
mount answers as not mounted, and a relation or a relationship field naming one fails the mount.
A context with no panel at all, such as a test's or a background job's, answers from the type's
own `declare`.

## Rejected

- Methods taking `&self` on a resource value: the 26 items stay, data and behaviour stay mixed.
- A trait-free value of closures: it loses the typed references (`relationship::<AuthorResource>`,
  `url::resource::<R>`), and async record fns borrowing `cx` and the executor are painful as
  closures.
- A registry keyed by resource type across panels: it rebuilt a def silently for an unmounted
  resource and could hold one configuration per type.
