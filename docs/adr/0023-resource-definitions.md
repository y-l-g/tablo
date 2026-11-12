# 0023 A resource or page declares one value; each panel owns what it mounts

`Resource::declare` returns a `ResourceDef<Self>` holding every setting that takes no request, and
what a page shows of a record: the table, the detail page, the record title as a lens of one of
the model's columns, and the public link as a closure over the record. The `Resource` trait keeps
the associated types and the methods that validate and write: `validate_record`, the record fns,
`before_commit` and `after_commit`. Row scoping is a setting of the def, `ResourceDef::query`, so
each panel narrows the rows it serves.
Mounting builds each def once and
keeps it in the panel's state; every handler and cross-reference reads the request panel's copy,
so one resource can mount read-only in one panel and writable in another. A background job gets a
context from the mounted panel's `PanelHandle`; `Panel::handle` builds one with the same checks
for a panel no router mounts.

A page follows the same shape: `Page::declare` returns a `PageDef` holding its slug, sidebar entry,
access check and header actions, and the trait keeps `render`. Access sits in the def, as a
resource's policy does, so `Panel::page_with` can narrow a page in one panel and leave it open in
another.

An action follows the same shape: `Action::declare` returns an `ActionDef<R>` holding where its
button shows, how it reads on each record, which records it runs on and its success message, and
`HeaderAction::declare` a `HeaderActionDef`. The trait keeps what names and types the action, as
`Resource` keeps its associated types: `Input`, and `NAME`, a constant checked at compile time as a
URL segment, which `Ability::is_action`, the policies and the URL builders name before any def is
built. It also keeps the methods that validate and write, `validate_input` and `run`. A panel
revokes a resource's action through the policy it mounts, not by editing
the action. A page has no policy: `page_with` adds header actions or narrows the page's access,
and does not remove an action the page declares.

## Rejected

- Methods on a resource value: data and behavior stay mixed across 26 items.
- A value of closures: it loses typed references, and async record fns are painful as closures.
- A registry keyed by resource type across panels: one configuration per type, rebuilt silently
  for an unmounted resource.
- A page's access as a trait method: one answer per type, which a second panel cannot narrow.
- An action's settings as constants and functions of its trait: a second way to declare beside
  `declare`, and two spellings of one setting, a constant and a function over the record (`ICON`
  and `icon_for`, `label` and `label_for`). This ADR first chose them, so that `NAME` and the
  rest would not be split between a constant and a value; `Resource` splits the same way, between
  its associated types and its def.
- `NAME` in the def: a value is checked at mount rather than at compile time, and `is_action` and
  the URL builders would have to build the def to read it.
