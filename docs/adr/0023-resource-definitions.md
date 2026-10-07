# 0023 A resource declares one value; each panel owns what it mounts

`Resource::declare` returns a `ResourceDef<Self>` holding every setting that takes no request,
including what a page shows of a record: the table, the detail page, the record label and the
public link. The `Resource` trait keeps the associated types and the methods that take one:
`query`, `validate_record`, the record fns and `after_commit`. Mounting builds each def once and
keeps it in the panel's state; every handler and cross-reference reads the request panel's copy,
so one resource can mount read-only in one panel and writable in another. A background job gets a
context from `Panel::context`, which mounts with the same checks.

## Rejected

- Methods on a resource value: data and behavior stay mixed across 26 items.
- A value of closures: it loses typed references, and async record fns are painful as closures.
- A registry keyed by resource type across panels: one configuration per type, rebuilt silently
  for an unmounted resource.
