# Panel and routing

How a panel is mounted, the routes a resource adds, and the panel options that shape the shell.

`Panel` owns the router, the `Db` in app context, and the shell layout. Registering a resource or a
[page](#pages) adds its routes and its sidebar item.

Routes for a resource with slug `users` under prefix `admin`:

- `GET /admin/users` : list
- `GET + POST /admin/users/create` : create
- `GET /admin/users/{id}` : detail page, read-only (GH #187) — 404 when the resource declares no
  `view` schema,
  see [Detail pages](./detail-pages.md)
- `GET + POST /admin/users/{id}/edit` : edit
- `POST /admin/users/{id}/delete` : delete with confirm step
- `POST /admin/users/bulk-delete` : bulk delete
- `GET /admin/users/export` : CSV export
- `GET /admin/users/options` : relation option search for a searchable select (GH #150),
  see [Forms](./forms.md)
- `GET /admin` serves the [home page](#pages), or redirects to the first resource when the panel
  has none

Useful panel options:

```rust
Panel::new("admin")
    .brand(Brand::new("Acme"))
    .dark_mode(true)
    .login_hint("Demo: admin@example.com / password")
```

`brand` sets the name in the sidebar header and on the login card, and in the topbar below the `md`
breakpoint,
where the sidebar folds into a sheet. A brand without a logo shows its initial as the mark.
`dark_mode` sets the theme a visitor sees **before they
have chosen one** — the toggle is always rendered, and a stored choice wins in both directions
(GH #184): picking light persists, and the next page stays light instead of falling back to this
default. Omit `dark_mode` and the panel starts light.

The panel owns the URL of each resource's list page and resolves a resource's sidebar entry to
`{prefix}/{slug}`; the resource owns the label and the ordering. See
[Resources](./resources.md) for the `navigation()` override.

## Pages

A page that is not record CRUD — a dashboard, a report, a settings screen — implements `Page` and
registers on the panel, which mounts it and lists it in the sidebar:

```rust
use tablo_core::{Page, Panel};
use topcoat::{
    Result,
    context::Cx,
    view::{View, view},
};

struct Dashboard;

impl Page for Dashboard {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => tablo_ui::page(tablo_ui::page_header(tablo_ui::page_title("Dashboard"))) })
    }
}

// `ReportsPage` implements `Page` the same way.
Panel::new("admin")
    .home::<Dashboard>()        // GET /admin
    .resource::<UserResource>()
    .page::<ReportsPage>()      // GET /admin/reports
```

The slug and the sidebar label default to the type name without a `Page` suffix: `ReportsPage`
mounts at `reports` with the label `Reports`, and `MediaLibraryPage` at `media-library` with
`Media library`. Override `slug()`, `navigation_label()`, or `navigation()` — the last sets the
`order` and the icon, as a resource's does. Every page has a sidebar entry. Pages and resources share one slug
namespace, and `Panel::build` rejects a duplicate, a slug the panel routes itself (`login`,
`logout`), and a slug that is not one URL segment.

`Panel::home` mounts its page at the prefix itself, in place of the redirect to the first resource,
and a second `home` fails the build. Its sidebar entry leads the entries of the same `order` and
points at the prefix, which every path under it matches, so the sidebar marks one entry active: the
most specific match.

The panel checks for a resolved user before `render` runs, and the app's `#[layout]` at the prefix
frames the page in the shell. A page serves one `GET`; a form it renders posts to an app `#[route]`
under the prefix, which the auth gate covers. `examples/showcase` registers a dashboard, the media
library and the live feed this way.

A page lays itself out with the composites the panel's own pages use, so it reads as one of them:
`tablo_ui::page` owns the width and the padding, and `page_header` takes a `page_title`, an optional
`page_description` under it and optional `page_actions` at the end of the title's line. Content goes
in `page_content`: `tablo_ui::card` for a panel, the same surface as a form section and a table, and
`tablo_ui::empty_state` for a region with nothing to list. The surfaces draw from the tokens in the
app's `styles.css` (`--card`, `--border`, `--shadow-sm`, `--primary`...), so a theme restyles the
panel's pages and the app's alike.

## Public pages

A public page is an app-level `#[page]` outside the panel prefix. The auth gate installs two layers
— the panel prefix and `/_topcoat/runtime` — and a layer wraps only the routes under its path
prefix, so no route under another prefix is gated. Route discovery is link-time over the binary, so
the `discover()` call inside `Panel::build` collects these pages with no router change:

```rust
use tablo_core::Panel;
use topcoat::{
    Result,
    context::Cx,
    router::{Slot, layout, page},
    view::{View, view},
};

// The layout path is a prefix: `/blog` wraps `/blog` and `/blog/{id}`, and
// nothing else. A layout at `/` would wrap `/admin` too.
#[layout("/blog")]
async fn blog_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    Panel::document(
        cx,
        "Blog",
        view! {
            <div class="mx-auto max-w-3xl px-6 py-10">(slot)</div>
        },
    )
    .await
}

#[page("/blog")]
async fn blog() -> Result<impl View> {
    Ok(view! { "Posts" })
}
```

`Panel::document` renders the document the admin shell renders — `topcoat::dev::script()`, the theme
script, and, where the panel registered `Panel::shell_assets`, the runtime script, the font, the
stylesheet and the shell scripts. The panel renders the `<body>` element and the dark-mode `<html>`
class; the page owns its chrome inside, classes included.

The runtime script, the font, the stylesheet and the shell scripts are `Asset` URLs, and an `Asset`
panics where no asset config is registered: a layout that rendered them under a router built without
`.assets(..)` — a markup test, say — fails instead of rendering. `Panel::document` leaves them out
there, as the panel's own shell does.

The panel's resource loaders are panel-scoped (auth, tenancy, chrome), so a public page queries the
model directly: `Post::filter(Post::fields().status().eq("published".to_string()))`, with an explicit
`.include(..)` for every relation the page reads. See [Data access](./data-access.md). The framework
scopes a `Resource`'s loaders, not a page's own query, so app code that loads rows outside those
loaders calls `scoped_query` — a public page in a multi-tenant app states its tenant predicate in the
query it writes (ADR-0002).
