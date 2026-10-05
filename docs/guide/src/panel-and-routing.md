# Panel and routing

A `Panel` is an admin panel: resources and pages under one prefix, one shell, one login. You
configure it with builder calls and mount it on your app's Topcoat router, which serves every
registered resource and page, the login page, and the shell that frames them, beside your own
routes.

```rust
use tablo::prelude::*;
use topcoat::{asset::RouterBuilderAssetExt, router::{Router, RouterBuilderDiscoverExt}};

{{#include ../../../examples/guide/src/panel_routing.rs:panel-admin-router-body}}
```

## Routes

A resource with the slug `users`, on a panel mounted at `/admin`, serves:

| Method | Path | Serves |
| --- | --- | --- |
| `GET` | `/admin/users` | the list |
| `GET` | `/admin/users/{id}` | the [detail page](./detail-pages.md); 404 when the resource declares no `view()` |
| `POST` | `/admin/users/{id}/delete` | delete one record |
| `POST` | `/admin/users/bulk-delete` | delete the selected records |
| `GET` | `/admin/users/export` | the list as CSV |
| `GET`, `POST` | `/admin/users/create` | the create form ¹ |
| `GET`, `POST` | `/admin/users/{id}/edit` | the edit form ¹ |
| `GET` | `/admin/users/options` | option search for a searchable relationship select ¹ |

¹ Only for a resource whose `Form` is a record form; a list-only resource names `NoForm` and gets
none of these. See [Forms](./forms.md).

`{id}` is the record's primary key. Each route checks the resource's policy, so a registered
route is not an open one: see [Policy, auth, tenancy](./policy-auth-tenancy.md).

The panel also serves:

- `GET /admin`: the [home page](#pages), or a redirect to the first registered resource's list
  when there is none.
- `GET`, `POST /admin/login` and `POST /admin/logout`, unless authentication is disabled.

## Builder reference

| Call | Effect | See |
| --- | --- | --- |
| `Panel::new(prefix)` | the panel at `/{prefix}` (`""` mounts at `/admin`) | |
| `resource::<R>()` | registers a resource's routes and sidebar entry | [Resources](./resources.md) |
| `page::<P>()` | registers a page at `/{prefix}/{slug}` | [Pages](#pages) |
| `home::<P>()` | registers a page at `/{prefix}` | [Pages](#pages) |
| `brand(Brand)` | sets the name and optional logo in the sidebar, the topbar and the login card | [Branding](#branding-and-theme) |
| `dark_mode(bool)` | sets the theme a first-time visitor sees | [Branding](#branding-and-theme) |
| `login_hint(text)` | adds a line under the login form | [Branding](#branding-and-theme) |
| `auth(Auth)` | replaces or disables the built-in password login | [Policy, auth, tenancy](./policy-auth-tenancy.md#authentication) |
| `uploads(uploader)` | sets where file fields store their bytes | [Forms](./forms.md#file-uploads) |
| `serve_dir(path, dir)` | serves a directory of files, publicly | [Forms](./forms.md#file-uploads) |
| `shell_assets(css, font)` | adds the stylesheet, font and scripts | [Assets](#assets) |
| `layout(render)` | frames the panel's pages with your layout instead of the shell | [The shell layout](#the-shell-layout) |
| `frame_ancestors(..)`, `without_frame_ancestors()` | changes the anti-framing header | [Security](./security.md) |

## Mounting

`Router::builder().panel(panel)` mounts a panel; the `RouterBuilderPanelExt` trait, in the
prelude, provides it. The router is yours: discover your own pages and routes, install the `Db`
with `.app_context(db)` and the asset bundle with `.assets(..)`, then mount. The panel reads both
from the router, so they come first.

The first panel mounted also installs what every panel shares: cookies, sessions unless the
router already configures them, the gate over Topcoat's runtime endpoints with the shard
dispatch, and Topcoat's runtime layer with link prefetching off unless the router already set
those up. The runtime layer has no
path, so mount panels after your own pathless layers: a page re-run must reach those layers
already rewritten to a `GET`.

The builder calls never fail; `.panel(..)` returns an error naming what is wrong instead. It
refuses a router with no `Db`, a `Db` missing the shipped auth models, two resources or pages
with one slug, a slug the panel routes itself
(`login`, `logout`), a slug that is not a single URL segment, a second home page, `shell_assets`
on a router with no asset bundle, a prefix that overlaps another panel's or Topcoat's
`/_topcoat/runtime`, and every resource declaration check described in
[Resources](./resources.md#startup-checks).

## Several panels

One router mounts any number of panels at distinct prefixes, each with its own resources, pages,
sidebar, brand and auth:

```rust
{{#include ../../../examples/guide/src/panel_routing.rs:panel-two-panels-body}}
```

A resource registered by two panels is declared once: both serve the same table, form and policy,
each under its own prefix. A panel's prefix must not overlap another's: `/admin` and `/admin/reports`
are refused, `/admin` and `/administration` are not.

Each panel has its own login page, and a session belongs to the panel that signed the user in. A
user signed in to `/admin` is anonymous on `/portal`, and signing in to `/portal` ends the `/admin`
session. See [Policy, auth, tenancy](./policy-auth-tenancy.md#authentication).

## URLs

The `tablo::url` helpers answer for the request's panel, so app code never spells a prefix:

```rust
{{#include ../../../examples/guide/src/panel_routing.rs:panel-urls-body}}
```

The request's panel is the one whose prefix the request is under; on a router with a single panel
it is that panel for every request. A resource or page that panel does not register has no URL
there, and the helper returns `None`.

## The shell layout

The panel frames every page under its prefix in the shell — sidebar, topbar, theme toggle and
notification toasts — with a layout it registers itself. Declare no `#[layout]` of your own at the
prefix: a second layout there would nest a second document inside the first.

To change the frame, pass your own layout function to `Panel::layout`. `Panel::layout_shell` is
the shipped one, so a layout that keeps the shell calls it around its own markup; one that does not
call it replaces the shell entirely:

```rust
{{#include ../../../examples/guide/src/panel_routing.rs:panel-custom-layout}}
```

## Sidebar

Every resource and page gets one sidebar entry, labelled with its `navigation_label()` and linked
to its list or page URL. Entries sort by `order` (lower first, default `0`); entries with the same
`order` keep registration order, and the home page leads its `order`.

Override `navigation()` on the resource or page to change the order or add an icon. Start from the
default entry so the panel still resolves the URL:

```rust
impl Resource for UserResource {
    // …
{{#include ../../../examples/guide/src/resources.rs:user-navigation}}
}
```

`NavigationItem::for_page::<Self>()` is the equivalent for a page. `NavigationItem::at(label, url)`
links elsewhere; the panel keeps that URL as written. The entry whose URL is the longest match
for the current path is marked active.

## Pages

A page that is not a record list — a dashboard, a report, a settings screen — implements `Page`:

```rust
use tablo_core::{Page, Panel};
use topcoat::{Result, context::Cx, view::{View, view}};

{{#include ../../../examples/guide/src/panel_routing.rs:panel-reports-page}}
```

- **Slug and label** default to the type name without its `Page` suffix: `ReportsPage` mounts at
  `reports` and is labelled "Reports"; `MediaLibraryPage` mounts at `media-library` and is
  labelled "Media library". Override `slug()` and `navigation_label()` to change them. Pages and
  resources share one slug namespace.
- **The home page.** `Panel::home::<P>()` mounts a page at the prefix itself instead of the
  redirect to the first resource. A panel has at most one; a second fails `.panel(..)`.
- **Access.** With authentication on, the page renders only for a signed-in user with panel access,
  inside the shell layout.
- **Forms.** A page serves one `GET`. A form it renders posts to an app `#[route]`; put that route
  under the panel prefix so the auth gate covers it.

The `tablo_ui` composites give a page the same frame as the panel's own pages: `page` sets the
width and padding; `page_header` holds a `page_title`, an optional `page_description` and optional
`page_actions`; `page_content` holds the body. Inside it, use `card` for a panel of content and
`empty_state` for a region with nothing to show. They are styled by the design tokens in your
`styles.css` (`--card`, `--border`, `--primary`, …), so one theme restyles the panel's pages and
yours.

## Public pages

A page outside the panel prefix is an ordinary Topcoat `#[page]` on your router. The auth gate
covers only the panel prefix and `/_topcoat/runtime`, so such a page is public.

`Panel::document` renders the same HTML document as the admin shell — head, assets, theme — around
your own markup. Outside any prefix it takes the panel's shell settings when the router mounts one
panel; with several, it renders the document without the panel's stylesheet and font.

```rust
{{#include ../../../examples/guide/src/panel_routing.rs:panel-blog}}
```

A public page loads its rows with a plain Toasty query, not through a resource. The resource's
tenant scope applies only to the panel's loaders, so in a multi-tenant app the page states its own
tenant predicate. See [Data access](./data-access.md).

## Branding and theme

```rust
{{#include ../../../examples/guide/src/panel_routing.rs:panel-brand-body}}
```

- `brand` names the panel in the sidebar header, on the login card, and in the topbar on narrow
  screens. A brand without a logo shows its first letter.
- `dark_mode(true)` makes the panel start dark for a visitor who has not chosen a theme. The theme
  toggle always renders, and a visitor's choice, light or dark, is remembered and wins over this
  default. Without the call the panel starts light.
- `login_hint` renders a muted line under the login form, such as demo credentials.

## Assets

Without assets the panel renders unstyled HTML without the shell's scripts, and every page and
form still works. To style it, install the app's Topcoat asset bundle on the router, then give the
panel the Tailwind stylesheet and the font the shell links:

```rust
{{#include ../../../examples/guide/src/panel_routing.rs:panel-geist}}

{{#include ../../../examples/guide/src/panel_routing.rs:panel-assets-body}}
```

`shell_assets` also makes the shell load `tablo-ui`'s scripts: live search, confirmation dialogs,
searchable selects, toasts and the sidebar and theme toggles. Mounting refuses `shell_assets` on a
router with no asset bundle. The stylesheet comes from `tablo_build::tailwind()` in the app's `build.rs`
([Your first panel](./first-panel.md#the-stylesheet)).
