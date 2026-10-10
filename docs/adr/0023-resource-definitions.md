# 0023 A resource or page declares one value; each panel owns what it mounts

`Resource::declare` returns a `ResourceDef<Self>` holding every setting that takes no request, and
what a page shows of a record: the table, the detail page, the record title as a lens of one of
the model's columns, and the public link as a closure over the record. The `Resource` trait keeps
the associated types and the methods that load and write: `query`, `validate_record`, the record
fns and `after_commit`.
Mounting builds each def once and
keeps it in the panel's state; every handler and cross-reference reads the request panel's copy,
so one resource can mount read-only in one panel and writable in another. A background job gets a
context from the mounted panel's `PanelHandle`; `Panel::handle` builds one with the same checks
for a panel no router mounts.

A page follows the same shape: `Page::declare` returns a `PageDef` holding its slug, sidebar entry,
access check and header actions, and the trait keeps `render`. Access sits in the def, as a
resource's policy does, so `Panel::page_with` can narrow a page in one panel and leave it open in
another.

An action keeps its settings on its trait: `NAME` is a constant, checked at compile time as a URL
segment and matched by `Ability::is_action`, so a def would split the action's settings between a
constant and a value. A panel revokes an action through the policy it mounts, not by editing the
action.

## Rejected

- Methods on a resource value: data and behavior stay mixed across 26 items.
- A value of closures: it loses typed references, and async record fns are painful as closures.
- A registry keyed by resource type across panels: one configuration per type, rebuilt silently
  for an unmounted resource.
- A page's access as a trait method: one answer per type, which a second panel cannot narrow.
- An `ActionDef` value: `NAME` must stay a constant, and the policy already decides per panel who
  runs the action.
