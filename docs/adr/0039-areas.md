# 0039 An area is a path segment with a sidebar of its own

A panel's settings, an account's and a tenant's, are many pages a user opens rarely, and listed in
the sidebar they crowd the pages the panel exists for. `Panel::area(Area::new("settings"))`
registers a part of the panel under `{prefix}/settings`: its groups hold resources and pages as
the panel does, and a page under the area's URL shows the area's sidebar, with a link back to the
panel's home, in place of the panel's. The panel's sidebar lists the area as one entry.

- An area is a path segment, not a list of links. Whether a request is inside an area is a prefix
  test on its path, so a record page, a create page or an error page deep under an area entry
  shows the area's sidebar without asking which entry it belongs to, and the trail starts at the
  area. Its resources and pages mount at `{prefix}/{area}/{slug}` from a slug namespace of their
  own: an area may name an entry `members` beside the panel's own `members`.
- An area's entries are the panel's sidebar entries with a slot, not a second registry. Relation
  tabs, global search order, badges and visibility read one list, and the shell picks the entries
  of the area the path is under, or of none.
- The pages auth serves in the shell, the profile, two-factor and tenant settings pages, can move
  into an area. Their routes and every link to them, the user menu's and the tenant menu's, read
  their path from the panel's realm, so a moved page has one URL. Their def takes a slug, a label,
  an icon and an order; mounting refuses what the page does not render rather than drop it.
- A tenant group is headed by the name of the tenant the request acts for, and its entries show
  only while the request acts for one. The tenant a settings page edits is then in the sidebar,
  beside the switch that changes it.
- An area's root redirects to the first entry the user may open, as the panel's root redirects to
  its first resource, so a link to the area never answers 403 for an entry the user cannot see.

## Rejected

- Filament's clusters, which render a cluster's pages as tabs or a sub-navigation inside the
  content: a second navigation component beside the sidebar, which collapses differently on a
  phone, and a page that shows two menus at once.
- An area as a set of links matched against the request path, with resources kept at
  `{prefix}/{slug}`: deciding where a deep page belongs then needs every entry's URL rules, and
  two areas could not reuse a slug.
- Placing a resource or page in an area from its def, beside `navigation_group`: the def would
  name a part of one panel, and a resource registered by two panels belongs to each panel's own
  structure.
