# 0033 Global search reads what each resource already declares

A panel's topbar holds a search box over every resource it mounts. A record matches when each
word of the term is in one of its table's searchable columns or in its `record_title` column, and
each result is titled by that column. `ResourceDef::global_search(GlobalSearch)` adds the details
under each title; `ResourceDef::without_global_search` and `Panel::without_global_search` opt out.

- Search is on by default and declares no columns of its own: the table's searchable columns and
  the title already say what a user searches a resource by, and a relationship option over the
  resource searches the same ones.
- A resource searches through its list's loader: `scoped_query`, so the tenant and the trash
  apply, the policy's `ViewAny`, and the table as the request's field access masks it. Each record
  opens only a page the policy lets the request open, so the search shows no record the list and
  its pages would not.
- The groups follow the sidebar's order, and the limits are fixed: 5 of each resource in the box,
  25 on the results page. The sidebar already orders the resources for the user.
- The input lives in the shell; the results under it are a shard taking the panel's prefix and the
  term as arguments, re-rendered as the user pauses. Each panel with a global search declares the
  shard as its endpoint, so a panel without sign-in answers it too, and the shard lists results
  only for the panel whose page rendered it.
- `{prefix}/-/search?q=` lists more of each resource, and answers the box's form without
  JavaScript.

## Rejected

- Opting resources in, or Filament's searchable attributes: a second list of columns beside the
  table's, which drifts from what the list searches.
- A pluggable search provider: a second loader, which must restate the tenant, trash, policy and
  field access checks the list's loader already makes.
- A JSON endpoint and a client script for the dropdown: Topcoat's runtime is the browser layer
  ([0026](0026-topcoat-runtime-only.md)).
- A signal the shell reads on the server: each keystroke would re-run the whole page under the
  box.
- A per-resource limit and sort: knobs for an order the sidebar already gives.
