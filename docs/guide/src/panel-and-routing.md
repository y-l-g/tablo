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

A resource with the slug `users`, on a panel mounted at `/admin`, serves its list at
`/admin/users`, each record's pages under `/admin/users/{id}`, and its list-wide routes, such
as the create form and the CSV export, under `/admin/users/-/`. The `-` segment keeps those apart
from a record's, so a record whose key is `create` still has its detail page. A resource without a
record form serves no form routes, and an action or a relation adds routes of its own.
[`Panel::resource`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.resource) lists every route and when it is served.

A registered route is not an open one: each checks the resource's policy
([Policy, auth, tenancy](./policy-auth-tenancy.md)).

The panel also serves its home page at `/admin`, its sign-in, and the auth pages
[`Auth`](./policy-auth-tenancy.md#authentication) turns on. Every other path under `/admin`
answers 404 behind the [gate](./policy-auth-tenancy.md#what-a-request-gets), so a visitor signs
in before learning a path serves nothing. [`Panel`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html) lists those routes.

## Builder reference

| Call | Effect | See |
| --- | --- | --- |
| `Panel::new(prefix)` | the panel at `/{prefix}` (`""` mounts at `/admin`) | |
| `resource::<R>()` | registers a resource's routes and sidebar entry | [Resources](./resources.md) |
| `page::<P>()` | registers a page at `/{prefix}/{slug}` | [Pages](#pages) |
| `home::<P>()` | registers a page at `/{prefix}` | [Pages](#pages) |
| `brand(Brand)` | sets the name and optional logo in the sidebar, the topbar, and above the sign-in and sign-up cards, and the optional favicon | [Branding](#branding-and-theme) |
| `dark_mode(bool)` | sets the theme a first-time visitor sees | [Branding](#branding-and-theme) |
| `login_hint(text)` | adds a line under the sign-in card | [Branding](#branding-and-theme) |
| `time_zone(zone)` | sets the zone a user without their own reads and enters times in, UTC by default | [Forms](./forms.md#time-zones) |
| `user_menu_item(NavigationItem)` | adds an entry to the user menu, above Sign out | [The shell layout](#the-shell-layout) |
| `auth(Auth)` | replaces or disables the built-in password login | [Policy, auth, tenancy](./policy-auth-tenancy.md#authentication) |
| `uploads(uploader)` | sets where file fields store their bytes | [Forms](./forms.md#file-uploads) |
| `serve_dir(path, dir)` | serves a directory of files, publicly unless the path sits under the panel prefix | [Forms](./forms.md#file-uploads) |
| `uploads_dir(url_prefix, dir)` | stores file fields' uploads in a directory and serves it | [Forms](./forms.md#file-uploads) |
| `shell_assets(css, font)` | adds the stylesheet, font and scripts | [Assets](#assets) |
| `layout(render)` | frames the panel's pages with your layout instead of the shell | [The shell layout](#the-shell-layout) |
| `frame_ancestors(..)`, `without_frame_ancestors()` | changes the anti-framing header | [Security](./security.md) |

## Mounting

`Router::builder().panel(panel)` mounts a panel; the `RouterBuilderPanelExt` trait, in the
prelude, provides it. The router is yours: discover your own pages and routes, install the `Db`
with `.app_context(db)` and the asset bundle with `.assets(..)`, then mount. The panel reads both
from the router, so they come first, and it installs a layer with no path, so it comes after your
own pathless layers; [`panel`](https://y-l.fr/tablo/nightly/api/tablo/trait.RouterBuilderPanelExt.html#tymethod.panel) says why.

The builder calls never fail; `.panel(..)` returns a `MountError` listing every mistake
instead: see [Mount errors](./mount-errors.md).

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

A helper returns `None` where the panel serves no such page, such as for a resource it does not
register, so a link to a page that does not exist never renders. A request outside every prefix,
such as a public page, has no panel until it enters one with `PanelHandle::enter`; see
[Data access](./data-access.md#outside-a-panel). `tablo::navigation_item::<R>(cx)` answers the
same way with `R`'s whole sidebar entry, which suits a dashboard linking to lists.

A record's pages and actions have helpers too, typed by the resource, so a job or a test spells
no record path either. [`tablo::url`](https://y-l.fr/tablo/nightly/api/tablo/url/index.html) lists them and when each answers `None`.

## The shell layout

The panel frames every page under its prefix in the shell — sidebar, topbar and notification
toasts — with a layout it registers itself, so declare no `#[layout]` of your own at the prefix.

- **The sidebar** collapses to a rail of icons, each naming its entry on hover; below `md` it opens
  as a sheet instead.
- **The topbar** holds the sidebar's toggle and a trail to the page: the sidebar entry the page is
  under, then `Create`, `View` or `Edit` on a resource's pages, the entry linking back to its list.
  Under an entry that is not a resource, such as the dashboard, a deeper page names its last path
  segment.
- **Error pages.** A page load that fails with 404, 403 or 500 shows the failure inside the shell,
  with the trail ending in it and a link home, under the browser's URL and with the error's
  status. A request with another method, such as a form POST, keeps the bare status. A
  signed-in user the panel refuses outright sees a standalone 403 page with Sign out instead.
- **The user menu,** in the sidebar's footer, shows the signed-in user's initials, their
  `display_name` and the `subtitle` their `PanelUser` returns, such as an email. It opens the
  theme choice, a Profile link when the panel has a
  [profile page](./policy-auth-tenancy.md#profile), the entries `Panel::user_menu_item` adds, and
  Sign out. A panel with
  `Auth::disabled()` has no user to show, so its footer offers the theme choice alone.

To change the frame, pass your own layout function to `Panel::layout`. `Panel::layout_shell` is
the shipped one, so a layout that keeps the shell calls it around its own markup; one that does not
call it replaces the shell entirely:

```rust
{{#include ../../../examples/guide/src/panel_routing.rs:panel-custom-layout}}
```

## Sidebar

Every resource and page gets one sidebar entry, labelled with its plural label (a page's
`label`) and linked to its list or page URL. The sidebar lists an entry only when the
request's user may open it: a resource's when its policy allows `ViewAny`, a page's when
its `PageDef::access` answers `true`. So a user never sees a link that would answer 403.

A resource sets the order and an icon on its def, and the panel resolves the URL:

```rust
impl Resource for UserResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:user-navigation}}
    }
}
```

A page sets the same `icon`, `navigation_order` and `navigation_group` on its `PageDef`.
[`NavigationItem::at(label, url)`](https://y-l.fr/tablo/nightly/api/tablo/struct.NavigationItem.html#method.at) links elsewhere, such
as to a list with a filter applied, from `ResourceDef::navigation` or `PageDef::navigation`.

`tablo::ui::icons` names the Lucide icons the panel draws, its navigation entries included. An
entry that wants another one stages the set its app's own `iconify_icon!` resolves against: in the
app's `build.rs`,

```rust
fn main() {
    topcoat::icon::iconify::BuildConfig::new()
        .icon_set("lucide")
        .stage()
        .expect("the Lucide set stages");
}
```

and in the app, a constant beside the def that uses it:

```rust
use topcoat::icon::{IconData, iconify::iconify_icon};

pub const PACKAGE: IconData = iconify_icon!("lucide:package");
```

The app's manifest needs `topcoat` with the `icon-iconify` feature in `[dependencies]`, where the
macro reads the staged set, and in `[build-dependencies]`, where the build script stages it. The
first build downloads the set through the shared cache. `.cache_dir("assets/iconify")` beside
`.icon_set_version("lucide", "1.2.137")` stages a committed `lucide.json` and its `lucide.version`
sidecar offline instead, since a pinned set is reused only while the two agree.

`navigation_group("Content")` on either def renders the entry in a labelled sidebar group, after
the ungrouped entries.

An entry may carry a badge after its label. `NavigationItem::badge("New")` sets a fixed one;
`ResourceDef::navigation_badge(Post::fields().status().eq(PostStatus::Draft))` counts the rows of
the resource's [`scoped_query`](./data-access.md) that the filter matches, so each tenant sees its
own count. Each counted badge is a query on every sidebar render:
[`navigation_badge`](https://y-l.fr/tablo/nightly/api/tablo/struct.ResourceDef.html#method.navigation_badge) says when it runs.

## Global search

The topbar holds a search box over every resource the panel mounts. As the user types, it lists
the first matches of each resource under its plural label, in the sidebar's order, and links a
results page at `{prefix}/-/search?q=` that lists more. Without JavaScript the box's form opens that
page.

A resource's search reads what the resource already declares. A record matches when each word of
the term is in one of its table's searchable columns or in its `record_title` column, which also
titles the result. The results are the records the resource's list shows the same user: its
policy, its tenancy, its trash and its [field access](./resources.md#field-access) apply as they do
there, and each result opens the record's detail page. `global_search` lists details under each
title:

```rust
impl Resource for UserResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:user-global-search}}
    }
}
```

`ResourceDef::without_global_search()` leaves one resource out, and
`Panel::without_global_search()` removes the box and the results page.
[`Panel::without_global_search`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.without_global_search)
and [`ResourceDef::global_search`](https://y-l.fr/tablo/nightly/api/tablo/struct.ResourceDef.html#method.global_search)
have the full rules.

## Pages

A page that is not a record list — a dashboard, a report, a settings screen — implements `Page`:

```rust
use tablo::{Page, Panel};
use topcoat::{Result, context::Cx, view::{View, view}};

{{#include ../../../examples/guide/src/panel_routing.rs:panel-reports-page}}
```

- **The def.** `Page::declare` returns a `PageDef`, as a resource's returns a `ResourceDef`: the
  slug, the sidebar entry, who may open the page and its header actions. `render` stays a trait
  method. `Panel::page_with::<P>(|def| …)` adjusts the def for one panel:

  ```rust
  {{#include ../../../examples/guide/src/panel_routing.rs:panel-page-def}}
  ```

- **Slug and label** default to the type name without its `Page` suffix: `ReportsPage` mounts at
  `reports` and is labelled "Reports". `PageDef::slug` and `PageDef::label` change them. Pages and
  resources share one slug namespace.
- **The home page.** `Panel::home::<P>()` mounts a page at the prefix itself instead of the
  redirect to the first resource. A panel has at most one.
- **Access.** With authentication on, the page renders only for a signed-in user with panel access,
  inside the shell layout. `PageDef::access(|cx| …)` narrows that: when it answers `false`
  the page answers 403 and leaves the sidebar. It is synchronous, like a policy, and reads the
  signed-in user with `auth::user::<U>(cx)`.
- **Actions.** `PageDef::header_action::<A>()` declares a [header action](./actions.md#header-actions),
  behind the same sign-in and `access` as the page. `header_action_buttons::<Self>(cx)` renders
  their buttons where the page places it, usually in `page_actions`.
- **Forms.** Any other form a page renders posts to an app `#[route]`; put that route under the
  panel prefix so the auth gate covers it. `access` does not cover that route, so the route
  asks `can_open::<P>(cx)` itself: the panel's sign-in and the page's `access`, as the page's
  `GET` checks them.

The `tablo::ui` composites give a page the same frame as the panel's own pages: `page` sets the
width and padding; `page_header` holds a `page_title`, an optional `page_description` and optional
`page_actions`; `page_content` holds the body. Inside it, use `card` for a panel of content and
`empty_state` for a region with nothing to show. They are styled by the design tokens (`--card`,
`--border`, `--primary`, …) that your `styles.css` overrides, so one theme restyles the panel's
pages and yours.

## Public pages

A page outside the panel prefix is an ordinary Topcoat `#[page]` on your router. The auth gate
covers only the panel prefix and `/_topcoat/runtime`, so such a page is public.

A panel handle's `document` renders the same HTML document as that panel's shell — head, assets,
favicon, theme — around your own markup, and its `url` gives the panel's prefix for a link to it.
A public page belongs to no panel, so it names the one whose shell it takes: keep the handle
`panel_handle("admin")` returns in the app context. Under a prefix, `Panel::document` takes the
request's own panel.

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

- `brand` names the panel in the sidebar header, in the topbar on narrow screens, and above the
  sign-in and sign-up cards, where the logo shows 40px tall. A brand without a logo shows its
  first letter. `favicon` links the icon of every page the panel renders, sign-in included;
  without it the browser asks for `/favicon.ico`.
- `dark_mode(true)` makes the panel start dark for a visitor who has not chosen a theme. The user
  menu offers light, dark and the system's theme; the choice is kept in the `tablo_theme` cookie
  and wins over this default. Under the system's theme, `tablo-ui`'s script mirrors the system's
  scheme into `tablo_scheme` for the server to render. Without the call the panel starts light.
- `login_hint` renders a muted line under the sign-in card, such as demo credentials.

### Theme tokens

The shell's colors and shadows are CSS custom properties, declared under `:root` for the light
theme and again under `.dark`. Your `styles.css` redeclares the ones it changes
([The stylesheet](./first-panel.md#the-stylesheet)); Tailwind reads each as a color of the same
name, such as `bg-primary` or `text-muted-foreground`, in the panel's markup and yours.

| Token | Paints |
| --- | --- |
| `--background`, `--foreground` | the page and its text |
| `--card`, `--card-foreground` | cards, tables, dialogs and the sign-in and sign-up cards |
| `--popover`, `--popover-foreground` | the option lists of selects and menus |
| `--muted`, `--muted-foreground` | subdued surfaces and secondary text |
| `--primary`, `--primary-foreground` | primary buttons and checked checkboxes |
| `--destructive`, `--destructive-foreground` | delete buttons, errors, and the `danger` [color](./tables.md#presenting-values) |
| `--success`, `--warning`, `--info` | the `success`, `warning` and `info` colors of badges and tinted cells |
| `--border` | borders and dividers |
| `--ring` | focus rings |
| `--shadow-xs`, `--shadow-sm` | the `shadow-xs` and `shadow-sm` shadows |
| `--sidebar`, `--sidebar-foreground` | the sidebar and its text |
| `--sidebar-primary`, `--sidebar-primary-foreground` | the sidebar's primary elements, `--primary` by default |
| `--sidebar-accent`, `--sidebar-accent-foreground` | the hovered and active sidebar entry |
| `--sidebar-border`, `--sidebar-ring` | the sidebar's borders and focus rings, `--border` and `--ring` by default |

The font is `--font-sans`, Geist by default, under Tailwind's `@theme`.

## Assets

Without assets the panel renders unstyled HTML without Topcoat's runtime or the shell's scripts:
every page and form still works, tables update by navigating, and the confirmation dialogs that
deletes and confirmatory actions go through do not open. To style it, install the app's Topcoat asset bundle on the router, then give the
panel the Tailwind stylesheet and the font the shell links:

```rust
{{#include ../../../examples/guide/src/panel_routing.rs:panel-geist}}

{{#include ../../../examples/guide/src/panel_routing.rs:panel-assets-body}}
```

`shell_assets` also makes the shell load Topcoat's runtime, which drives the live tables, the
confirmation dialogs and the variant choice, and `tablo-ui`'s scripts: searchable selects, repeater rows, and the
sidebar toggle and the theme choice. The stylesheet comes from `tablo_build::tailwind()` in the app's `build.rs`
([Your first panel](./first-panel.md#the-stylesheet)).

`AssetBundle::load()` reads the bundle from the `assets` directory beside the executable, which
`cargo build` does not write. `topcoat dev` builds the app, writes the bundle there and serves the
app; `topcoat asset bundle --release` writes it beside a release binary, and the two deploy
together.
