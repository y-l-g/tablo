# 0035 A widget is a page's server-rendered block, and its page owns the grid

A dashboard is a `Page` whose `PageDef::widget::<W>()` calls list the `Widget`s of its grid. Its
default `render` shows the grid under the page's header; a `render` the page writes places
`widgets::<Self>(cx)`, as it places `header_action_buttons::<Self>(cx)`. A widget renders a view;
a row of stats is a widget that renders `stats(cx, …)` over `Stat`s.

- A widget is a view rendered with the page, not a component with its own route or state. It
  renders through the page's `GET`, behind the page's sign-in and `access`, so it adds no route
  for the auth gate, the CSRF check or the tenant matrices to cover. A widget that reruns on its
  own, as a table's sort and page do through their route, needs a later ADR.
- A resource's list page has a grid of its own, filled by `ResourceDef::list_widget::<W>()`, so a
  banner or a row of figures above a list needs no rewritten page. It renders with the list, behind
  the list's sign-in and policy, and adds no route either. A named render hook per spot of each
  page, taking a view function, was rejected: it renders synchronously, so it cannot read the
  database, and it repeats the access a widget already declares.
- A widget reads its own data, as a page does. What counts a resource's records goes through
  `scoped_query` and asks `can_list` before it shows or links anything, which
  `Stat::records::<R>` does for the common tile.
- Access is per widget, so a page shows each user only the widgets they may see without a page
  per audience.
- Stats are a function over values, not a trait. A trait per kind, each with a blanket
  `impl Widget`, would overlap once there are two kinds, and a type could not be both.
- Charts, a stat's sparkline included, are SVG the server writes. The browser layer stays
  Topcoat's runtime ([0026](0026-topcoat-runtime-only.md)), and a chart reads without JavaScript.

## Rejected

- Filament's widgets as Livewire components, with polling and lazy loading: a route and a state
  per widget, and a runtime timer Topcoat does not have.
- A chart library in the browser: a script Tablo ships, a second renderer of the data, and a page
  that draws nothing without JavaScript.
- A panel-wide widget registry, as Filament's dashboard reads: a page would show widgets it does
  not name. A resource's list names its own.
