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
| `GET` | `/admin/users/{id}` | the [detail page](./detail-pages.md); 404 when the resource's view is empty |
| `POST` | `/admin/users/{id}/delete` | delete one record |
| `POST` | `/admin/users/-/bulk-delete` | delete the selected records |
| `GET` | `/admin/users/-/export` | the list as CSV |
| `GET`, `POST` | `/admin/users/-/create` | the create form ¹ |
| `GET`, `POST` | `/admin/users/{id}/edit` | the edit form ¹ |
| `GET` | `/admin/users/-/options` | option search for a searchable relationship select, and a dependent choice's options ¹ |
| `POST` | `/admin/users/{id}/-/actions/{action}` | run a record action on one record ² |
| `POST` | `/admin/users/-/actions/{action}` | run a header action, or a bulk action on the selection ³ |
| `GET` | `/admin/users/-/actions/{action}/options` | option search for an action input's relationship select ² ³ |
| `POST` | `/admin/users/{id}/-/relations/{relation}/-/actions/{action}` | attach a record to a many-to-many relation, or detach the selection ⁴ |
| `POST` | `/admin/users/{id}/-/relations/{relation}/{related}/-/actions/{action}` | detach one related record ⁴ |
| `GET` | `/admin/users/{id}/-/relations/{relation}/-/actions/{action}/options` | option search for the attach dialog ⁴ |

¹ Only for a resource whose `Form` is a record form; a list-only resource names `NoForm` and gets
none of these. See [Forms](./forms.md).
² Only when an action is placed on a row, the detail page or the edit page. See
[Actions](./actions.md).
³ Only when the resource has a header action, or an action placed on the bulk bar.
⁴ Only when the resource has a [many-to-many relation](./detail-pages.md#many-to-many-relations).

The `-` segment keeps the list's own routes, and the action and relation routes, apart from a
record's: a record whose key is `create` still has its detail page at `/admin/users/create`, and
one whose key is `actions` its `/admin/users/actions/edit`.

`{id}` is the record's primary key. Each route checks the resource's policy, so a registered
route is not an open one: see [Policy, auth, tenancy](./policy-auth-tenancy.md).

The panel also serves:

- `GET /admin`: the [home page](#pages), or a redirect to the first registered resource's list
  when there is none.
- `GET`, `POST /admin/login` and `POST /admin/logout`, unless authentication is disabled.
- `POST /admin/tenant`, the [tenant switcher](./policy-auth-tenancy.md#tenancy), unless
  authentication is disabled.
- `GET`, `POST /admin/register`, the [sign-up page](./policy-auth-tenancy.md#sign-up), when
  `Auth::registration` installs a registrar.
- `POST /admin/{page}/-/actions/{action}` for a page's [header actions](./actions.md#header-actions),
  and `/admin/-/actions/{action}` for the home page's.

## Builder reference

| Call | Effect | See |
| --- | --- | --- |
| `Panel::new(prefix)` | the panel at `/{prefix}` (`""` mounts at `/admin`) | |
| `resource::<R>()` | registers a resource's routes and sidebar entry | [Resources](./resources.md) |
| `page::<P>()` | registers a page at `/{prefix}/{slug}` | [Pages](#pages) |
| `home::<P>()` | registers a page at `/{prefix}` | [Pages](#pages) |
| `brand(Brand)` | sets the name and optional logo in the sidebar, the topbar, and above the sign-in and sign-up cards | [Branding](#branding-and-theme) |
| `dark_mode(bool)` | sets the theme a first-time visitor sees | [Branding](#branding-and-theme) |
| `login_hint(text)` | adds a line under the sign-in card | [Branding](#branding-and-theme) |
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
from the router, so they come first.

The first panel mounted also installs what every panel shares: cookies, sessions unless the
router already configures them, the gate over Topcoat's runtime endpoints, and Topcoat's
runtime layer with link prefetching off unless the router already set
those up. The runtime layer has no
path, so mount panels after your own pathless layers: a page re-run must reach those layers
already rewritten to a `GET`.

The builder calls never fail; `.panel(..)` returns a `MountError` listing every mistake
instead. It refuses a router with no `Db`, a `Db` missing the shipped auth models, two resources or pages
with one slug, a slug the panel routes itself
(`login`, `logout`, `register`, and `-`, which leads the home page's action routes), a slug that is not a single URL segment, a second home page, one page type registered twice, `shell_assets`
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
there, and the helper returns `None`. `tablo::panel::navigation::<R>(cx)` answers the same way
with `R`'s whole sidebar entry, its label, URL and icon, which suits a dashboard linking to lists.

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

Every resource and page gets one sidebar entry, labelled with its plural label (a page's
`label`) and linked to its list or page URL. The sidebar lists an entry only when the
request's user may open it: a resource's when its policy allows `ViewAny`, a page's when
its `PageDef::access` answers `true`. Entries sort by `order` (lower first,
default `0`); entries with the same `order` keep registration order, and the home page leads its
`order`.

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
`NavigationItem::at(label, url)` links elsewhere, from `ResourceDef::navigation` or
`PageDef::navigation`; the panel keeps that URL as written, and the def's `icon`,
`navigation_order` and `navigation_group` still apply to it. The entry whose URL is the longest
match for the current path is marked active.

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

`navigation_group("Content")` on either def renders the entry in a labelled sidebar group. The ungrouped entries render first, under "Navigation";
each group follows in the order of its first entry, and `order` sorts the entries within it.

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
  `reports` and is labelled "Reports"; `MediaLibraryPage` mounts at `media-library` and is
  labelled "Media library". `PageDef::slug` and `PageDef::label` change them. Pages and
  resources share one slug namespace.
- **The home page.** `Panel::home::<P>()` mounts a page at the prefix itself instead of the
  redirect to the first resource, with its def as `declare` returns it. A panel has at most one;
  a second fails `.panel(..)`, as does registering one page type twice.
- **Access.** With authentication on, the page renders only for a signed-in user with panel access,
  inside the shell layout. `PageDef::access(|cx| …)` narrows that: when it answers `false`
  the page answers 403 and leaves the sidebar. It is synchronous, like a policy, and reads the
  signed-in user with `auth::user::<U>(cx)`.
- **Actions.** `PageDef::header_action::<A>()` declares a [header action](./actions.md#header-actions),
  served at `{page}/-/actions/{NAME}` behind the same sign-in and `access` as the page, in a
  transaction the framework opens. `header_action_buttons::<Self>(cx)` renders their buttons where the
  page places it, usually in `page_actions`.
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

`Panel::document` renders the same HTML document as the panel's shell — head, assets, theme — around
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

- `brand` names the panel in the sidebar header, in the topbar on narrow screens, and above the
  sign-in and sign-up cards, where the logo shows 40px tall. A brand without a logo shows its
  first letter.
- `dark_mode(true)` makes the panel start dark for a visitor who has not chosen a theme. The theme
  toggle always renders, and a visitor's choice, light or dark, is kept in the `tablo_theme`
  cookie and wins over this default. Without the call the panel starts light.
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
| `--destructive`, `--destructive-foreground` | delete buttons and errors |
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
sidebar and theme toggles. Mounting refuses `shell_assets` on a
router with no asset bundle. The stylesheet comes from `tablo_build::tailwind()` in the app's `build.rs`
([Your first panel](./first-panel.md#the-stylesheet)).

`AssetBundle::load()` reads the bundle from the `assets` directory beside the executable, which
`cargo build` does not write. `topcoat dev` builds the app, writes the bundle there and serves the
app; `topcoat asset bundle --release` writes it beside a release binary, and the two deploy
together.
