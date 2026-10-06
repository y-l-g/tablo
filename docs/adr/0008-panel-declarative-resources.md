# Panel declares Resources, Shell is implicit

Date: 2026-08-28 — Status: accepted

## Decision

`Panel` is the declarative seam, mounted on an app-owned router:
`Router::builder().discover().app_context(db).panel(Panel::new("admin").resource::<UserResource>())?`
registers each resource's routes and redirects the panel root to the first resource.
`Panel::page::<P>()` registers a non-resource page at `{prefix}/{slug}`, and `Panel::home::<P>()`
one at the prefix, replacing that redirect.

One router mounts several panels at distinct prefixes. Navigation, brand, auth, and uploader
registries are per-panel state on every request under the prefix, never an
app-context singleton. Overlapping prefixes fail at mount. The first panel mounted installs the
shared layers: cookies, sessions, the runtime gate, and Topcoat's runtime layer. `Db` and the
asset bundle belong to the router, installed before mounting.

The panel registers its own layout at its prefix: `Panel::layout_shell` owns the document, so
applications carry no document HTML. `Panel::layout` replaces it with an app layout that keeps the
shell by calling `Panel::layout_shell`. `shell_assets` supplies the call-site assets the Tailwind
scan needs; `Panel::render_shell` stays the low-level primitive. `tablo::url` helpers answer a
resource or page URL in the request's panel.

The panel resolves every sidebar entry from the resource's `ResourceDef` (its plural label,
`icon`, `navigation_order`, or a whole `navigation` item) and from `Page::navigation()`: the
declaration owns label, order, and icon; the panel owns the URL. The default entry is
`NavTarget::Derived`, resolved to `{prefix}/{slug}` (or the prefix for home).
`NavigationItem::at` builds an explicit `NavTarget::Url` kept verbatim; `for_page` is a page's
derived constructor. `is_current_path` matches exact or slash-boundary
prefixes; the longest match in sidebar order renders active. A page always has a sidebar entry.
