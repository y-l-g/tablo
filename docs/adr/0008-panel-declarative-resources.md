# Panel declares Resources, Shell is implicit

Date: 2026-08-28 — Status: accepted — Amended: 2026-09-19, 2026-09-22, 2026-09-30

## Decision

`Panel` is the declarative seam:
`Panel::new("admin").resource::<UserResource>().build().expect("panel builds")` discovers
`#[page]`/`#[layout]` via `Router::builder().discover()`, registers each resource's routes (the list
page at `{prefix}/{slug}`), and redirects the panel root to the first resource.
`Panel::page::<P: Page>()` registers a non-resource page at `{prefix}/{slug}`, and
`Panel::home::<P>()` one at the prefix, replacing that redirect (GH #407). An app's layout
delegates to `Panel::layout_shell`, which owns the complete document, so applications carry no
document HTML or `AssetConfig` fallback; `shell_assets(tailwind::stylesheet!(), font)` supplies the
call-site assets the app's Tailwind scan needs, and `Panel::render_shell` stays the low-level
primitive for custom layouts.

The Panel resolves every sidebar entry, and for a resource it consumes `Resource::navigation()`:
the resource owns label, order and grouping, the Panel owns the URL. The default entry names no URL
at all — `NavTarget::Derived` — and the Panel that owns the item resolves that target to the URL it
registers the route at (`{prefix}/{slug}`). An explicit
target (`NavTarget::Url`, which `NavigationItem::at` builds) is kept verbatim, so resolution can
never rewrite a link its author wrote, not even one shaped like `/admin/{slug}`.
`NavigationItem::for_resource` and `for_page` are the derived constructors and `at` the explicit
one, so no public constructor can emit a wrong mount. A `Page` declares its entry through
`Page::navigation()` the same way, and the home page's entry resolves to the prefix itself.
`is_current_path` matches exact paths or slash-boundary prefixes; since the home entry matches every
path under the prefix, the sidebar marks one entry active: the first, in sidebar order, of the
matches with the longest URL. An override that wants a different order sets the `order` field. A
page always has a sidebar entry; there is no hidden page.

## Consequences

An app with one `Resource` gets a Filament-grade shell by delegating its layout to
`Panel::layout_shell`, with zero document or sidebar HTML in application code. `examples/showcase` is
the reference for the asset-loading contract and for typed navigation. `Panel` remains the single
owner of Router/Db/Shell per `CONTEXT.md`.
